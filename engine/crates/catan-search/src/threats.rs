//! Opponent immediate-win threat detection outside the trade-safety guard.
//!
//! The current-turn tactical solver proves wins while we still control the
//! turn. This module answers the symmetric question: after we pass control,
//! can an opponent force a win on their next main phase through builds,
//! awards, hidden victory points, or a likely production roll?

use colonist_catan_core::{
    Action, CITY_COST, DevCard, GameState, Phase, ROAD_COST, SETTLEMENT_COST,
};

use crate::eval::{largest_army_outlook, longest_road_outlook};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpponentThreatKind {
    ImmediateMainPhaseWin,
    LongestRoadWin,
    LargestArmyWin,
    RoadBuildingWin,
    YearOfPlentyWin,
    MonopolyWin,
    KnightWin,
    DevelopmentPurchaseWin,
    HiddenVictoryPointWin,
    ProductionEnabledWin,
    TradeEnabledWin,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OpponentThreat {
    pub opponent: u8,
    pub kind: OpponentThreatKind,
    pub blocking_vertices: Vec<u8>,
    pub blocking_edges: Vec<u8>,
    pub blocking_hexes: Vec<u8>,
}

fn main_phase_for(state: &GameState, player: u8) -> GameState {
    let mut result = state.clone();
    result.current_player = player;
    result.phase = Phase::Main;
    result.trade = None;
    result.trade_negotiation_round = 0;
    result
}

fn can_afford(state: &GameState, player: u8, cost: &[u8; 5]) -> bool {
    state.players[player as usize]
        .resources
        .iter()
        .zip(cost)
        .all(|(have, need)| have >= need)
}

fn settlement_sites(state: &GameState, player: u8) -> Vec<u8> {
    main_phase_for(state, player)
        .legal_actions()
        .into_iter()
        .filter_map(|action| match action {
            Action::BuildSettlement { vertex } => Some(vertex),
            _ => None,
        })
        .collect()
}

fn city_sites(state: &GameState, player: u8) -> Vec<u8> {
    main_phase_for(state, player)
        .legal_actions()
        .into_iter()
        .filter_map(|action| match action {
            Action::BuildCity { vertex } => Some(vertex),
            _ => None,
        })
        .collect()
}

fn road_edges(state: &GameState, player: u8) -> Vec<u8> {
    main_phase_for(state, player)
        .legal_actions()
        .into_iter()
        .filter_map(|action| match action {
            Action::BuildRoad { edge } => Some(edge),
            _ => None,
        })
        .collect()
}

fn hidden_vp(state: &GameState, player: u8) -> u8 {
    state.players[player as usize].development[DevCard::VictoryPoint.index()]
}

fn award_swing_bonus(state: &GameState, player: u8) -> u8 {
    let mut bonus = 0u8;
    let road = longest_road_outlook(state, player);
    if state.longest_road_holder != Some(player) && road.acquire >= 0.92 {
        bonus = bonus.saturating_add(2);
    }
    let army = largest_army_outlook(state, player);
    if state.largest_army_holder != Some(player) && army.acquire >= 0.92 {
        bonus = bonus.saturating_add(2);
    }
    bonus
}

fn direct_win_after_progress(state: &GameState, opponent: u8) -> bool {
    if state.winner() == Some(opponent) {
        return true;
    }
    if state.phase != Phase::Main || state.current_player != opponent {
        return false;
    }
    state.legal_actions().into_iter().any(|action| {
        matches!(
            action,
            Action::BuildRoad { .. } | Action::BuildSettlement { .. } | Action::BuildCity { .. }
        ) && {
            let mut next = state.clone();
            next.apply(&action).is_ok() && next.winner() == Some(opponent)
        }
    })
}

fn chance_tail_win_probability(state: &GameState, opponent: u8) -> f32 {
    let actions = state.legal_actions();
    let total_weight = actions
        .iter()
        .map(|action| state.chance_weight(action) as u32)
        .sum::<u32>();
    if total_weight == 0 {
        return 0.0;
    }
    let winning_weight = actions
        .into_iter()
        .filter_map(|action| {
            let weight = state.chance_weight(&action) as u32;
            if weight == 0 {
                return None;
            }
            let mut next = state.clone();
            if next.apply(&action).is_err() {
                return None;
            }
            (direct_win_after_progress(&next, opponent)).then_some(weight)
        })
        .sum::<u32>();
    winning_weight as f32 / total_weight as f32
}

fn progress_action_win_probability(state: &GameState, opponent: u8, action: &Action) -> f32 {
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return 0.0;
    }
    if direct_win_after_progress(&next, opponent) {
        return 1.0;
    }
    match next.phase {
        Phase::DevelopmentChance | Phase::ResolveSteal { .. } => {
            chance_tail_win_probability(&next, opponent)
        }
        _ => 0.0,
    }
}

fn progress_threat_kind(action: &Action) -> Option<OpponentThreatKind> {
    match action {
        Action::PlayRoadBuilding { .. } => Some(OpponentThreatKind::RoadBuildingWin),
        Action::PlayYearOfPlenty { .. } => Some(OpponentThreatKind::YearOfPlentyWin),
        Action::PlayMonopoly { .. } => Some(OpponentThreatKind::MonopolyWin),
        Action::PlayKnight { .. } => Some(OpponentThreatKind::KnightWin),
        Action::BuyDevelopment => Some(OpponentThreatKind::DevelopmentPurchaseWin),
        _ => None,
    }
}

fn best_progress_threat(
    state: &GameState,
    opponent: u8,
) -> Option<(OpponentThreatKind, f32)> {
    let probe = main_phase_for(state, opponent);
    probe
        .legal_actions()
        .into_iter()
        .filter_map(|action| {
            let kind = progress_threat_kind(&action)?;
            let probability = progress_action_win_probability(&probe, opponent, &action);
            (probability > f32::EPSILON).then_some((kind, probability))
        })
        .max_by(|left, right| left.1.total_cmp(&right.1))
}

fn opponent_can_win_main_phase(
    state: &GameState,
    opponent: u8,
    include_progress_cards: bool,
) -> Option<OpponentThreat> {
    let target = state.victory_target;
    // victory_points() already includes held VP cards in this rules engine.
    let base = state.players[opponent as usize].victory_points();
    if base >= target {
        return Some(OpponentThreat {
            opponent,
            kind: OpponentThreatKind::ImmediateMainPhaseWin,
            blocking_vertices: Vec::new(),
            blocking_edges: Vec::new(),
            blocking_hexes: Vec::new(),
        });
    }

    let settlements = settlement_sites(state, opponent);
    let cities = city_sites(state, opponent);
    let can_settle = !settlements.is_empty() && can_afford(state, opponent, &SETTLEMENT_COST);
    let can_city = !cities.is_empty() && can_afford(state, opponent, &CITY_COST);
    let with_builds = base
        .saturating_add(u8::from(can_settle))
        .saturating_add(u8::from(can_city));
    if with_builds >= target {
        return Some(OpponentThreat {
            opponent,
            kind: OpponentThreatKind::ImmediateMainPhaseWin,
            blocking_vertices: settlements.into_iter().chain(cities).collect(),
            blocking_edges: road_edges(state, opponent),
            blocking_hexes: Vec::new(),
        });
    }

    let probe = main_phase_for(state, opponent);
    if can_afford(&probe, opponent, &ROAD_COST) {
        let before_holder = probe.longest_road_holder;
        for edge in road_edges(&probe, opponent) {
            let mut next = probe.clone();
            if next.apply(&Action::BuildRoad { edge }).is_ok()
                && next.longest_road_holder == Some(opponent)
                && before_holder != Some(opponent)
                && next.players[opponent as usize].victory_points() >= target
            {
                return Some(OpponentThreat {
                    opponent,
                    kind: OpponentThreatKind::LongestRoadWin,
                    blocking_vertices: Vec::new(),
                    blocking_edges: vec![edge],
                    blocking_hexes: Vec::new(),
                });
            }
        }
    }

    let army = largest_army_outlook(state, opponent);
    if state.largest_army_holder != Some(opponent)
        && army.acquire >= 0.95
        && base.saturating_add(2) >= target
    {
        return Some(OpponentThreat {
            opponent,
            kind: OpponentThreatKind::LargestArmyWin,
            blocking_vertices: Vec::new(),
            blocking_edges: Vec::new(),
            blocking_hexes: Vec::new(),
        });
    }

    let award_bonus = award_swing_bonus(state, opponent);
    if base
        .saturating_add(award_bonus)
        .saturating_add(u8::from(can_settle))
        .saturating_add(u8::from(can_city))
        >= target
    {
        return Some(OpponentThreat {
            opponent,
            kind: if state.longest_road_holder != Some(opponent) && award_bonus >= 2 {
                OpponentThreatKind::LongestRoadWin
            } else {
                OpponentThreatKind::LargestArmyWin
            },
            blocking_vertices: settlements.into_iter().chain(cities).collect(),
            blocking_edges: road_edges(state, opponent),
            blocking_hexes: Vec::new(),
        });
    }

    if include_progress_cards
        && let Some((kind, probability)) = best_progress_threat(state, opponent)
        && probability >= 1.0 - 1e-6
    {
        return Some(OpponentThreat {
            opponent,
            kind,
            blocking_vertices: Vec::new(),
            blocking_edges: Vec::new(),
            blocking_hexes: Vec::new(),
        });
    }

    // Public score can hide that the opponent already counts VP cards in
    // victory_points(); surface a dedicated label when those cards are the
    // decisive margin against the public board score.
    let public = state.players[opponent as usize].public_victory_points;
    let hidden = hidden_vp(state, opponent);
    if public < target && public.saturating_add(hidden) >= target {
        return Some(OpponentThreat {
            opponent,
            kind: OpponentThreatKind::HiddenVictoryPointWin,
            blocking_vertices: settlements.into_iter().chain(cities).collect(),
            blocking_edges: Vec::new(),
            blocking_hexes: Vec::new(),
        });
    }

    None
}

fn production_enabled_win(state: &GameState, opponent: u8) -> Option<OpponentThreat> {
    let target = state.victory_target;
    let base = state.players[opponent as usize].victory_points();
    if base.saturating_add(1) < target.saturating_sub(1) {
        return None;
    }
    let mut blocking_hexes = Vec::new();
    for (hex_index, hex) in state.board.hexes.iter().enumerate() {
        let Some(resource) = hex.resource else {
            continue;
        };
        if hex.number == 0 || hex.number == 7 {
            continue;
        }
        let touches = state
            .buildings
            .iter()
            .enumerate()
            .any(|(vertex, building)| {
                building.is_some_and(|piece| piece.player() == opponent)
                    && state.board.vertices[vertex]
                        .adjacent_hexes
                        .contains(&(hex_index as u8))
            });
        if !touches {
            continue;
        }
        let mut next = state.clone();
        next.players[opponent as usize].resources[resource.index()] =
            next.players[opponent as usize].resources[resource.index()].saturating_add(1);
        if opponent_can_win_main_phase(&next, opponent, false).is_some() {
            blocking_hexes.push(hex_index as u8);
        }
    }
    if blocking_hexes.is_empty() {
        None
    } else {
        Some(OpponentThreat {
            opponent,
            kind: OpponentThreatKind::ProductionEnabledWin,
            blocking_vertices: Vec::new(),
            blocking_edges: Vec::new(),
            blocking_hexes,
        })
    }
}

fn trade_enabled_win(state: &GameState, opponent: u8) -> Option<OpponentThreat> {
    let target = state.victory_target;
    let base = state.players[opponent as usize].victory_points();
    if base.saturating_add(2) < target {
        return None;
    }
    // Probe one-card gifts that complete a settlement or city recipe.
    for resource in 0..5u8 {
        let mut next = state.clone();
        next.players[opponent as usize].resources[resource as usize] =
            next.players[opponent as usize].resources[resource as usize].saturating_add(1);
        if opponent_can_win_main_phase(&next, opponent, false).is_some() {
            let settlements = settlement_sites(state, opponent);
            let cities = city_sites(state, opponent);
            return Some(OpponentThreat {
                opponent,
                kind: OpponentThreatKind::TradeEnabledWin,
                blocking_vertices: settlements.into_iter().chain(cities).collect(),
                blocking_edges: road_edges(state, opponent),
                blocking_hexes: Vec::new(),
            });
        }
    }
    None
}

/// Detect high-priority opponent win threats from the protected player's view.
pub fn detect_opponent_threats(state: &GameState, protected: u8) -> Vec<OpponentThreat> {
    let mut threats = Vec::new();
    for opponent in 0..state.board.num_players {
        if opponent == protected {
            continue;
        }
        if let Some(threat) = opponent_can_win_main_phase(state, opponent, true) {
            threats.push(threat);
            continue;
        }
        if let Some(threat) = trade_enabled_win(state, opponent) {
            threats.push(threat);
            continue;
        }
        if let Some(threat) = production_enabled_win(state, opponent) {
            threats.push(threat);
        }
    }
    threats
}

/// A verified immediate loss is stricter than the broader strategic threat
/// list: an opponent can already win from a Main-phase hand/board state in
/// this exact particle without assuming another production roll or trade.
pub fn has_verified_immediate_opponent_win(state: &GameState, protected: u8) -> bool {
    if state.is_terminal() {
        return false;
    }
    (0..state.board.num_players).any(|opponent| {
        opponent != protected && opponent_can_win_main_phase(state, opponent, true).is_some()
    })
}

fn has_verified_immediate_opponent_win_after_transition(state: &GameState, protected: u8) -> bool {
    if state.is_terminal() {
        return false;
    }
    let next_turn_player = state.current_player;
    if next_turn_player == protected {
        return false;
    }
    opponent_can_win_main_phase(state, next_turn_player, true).is_some()
}

pub fn posterior_immediate_threat_weight<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    protected: u8,
) -> f32 {
    let worlds = worlds
        .into_iter()
        .filter(|(_, weight)| *weight > f32::EPSILON)
        .collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| *weight)
        .sum::<f32>()
        .max(f32::EPSILON);
    worlds
        .into_iter()
        .filter(|(state, _)| has_verified_immediate_opponent_win(state, protected))
        .map(|(_, weight)| weight / total)
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

/// Posterior expected immediate tactical loss probability. Strict proofs stay
/// in `posterior_immediate_threat_weight`; chance tails such as a VP development
/// draw contribute only their weighted probability here.
pub fn posterior_expected_tactical_threat_weight<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    protected: u8,
) -> f32 {
    let worlds = worlds
        .into_iter()
        .filter(|(_, weight)| *weight > f32::EPSILON)
        .collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| *weight)
        .sum::<f32>()
        .max(f32::EPSILON);
    worlds
        .into_iter()
        .map(|(state, weight)| {
            let probability = (0..state.board.num_players)
                .filter(|opponent| *opponent != protected)
                .map(|opponent| {
                    if opponent_can_win_main_phase(state, opponent, true).is_some() {
                        1.0
                    } else {
                        best_progress_threat(state, opponent)
                            .map(|(_, probability)| probability)
                            .unwrap_or(0.0)
                    }
                })
                .fold(0.0_f32, f32::max);
            probability * (weight / total)
        })
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

/// Posterior mass in which this root action still leaves a verified immediate
/// opponent win. The action is verified after transition in every world where
/// it is legal. If it is unavailable in one world, it cannot count as an escape
/// from a threat already present in that world.
pub fn forced_loss_weight<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    protected: u8,
    action: &Action,
) -> f32 {
    let worlds = worlds
        .into_iter()
        .filter(|(_, weight)| *weight > f32::EPSILON)
        .collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| *weight)
        .sum::<f32>()
        .max(f32::EPSILON);
    worlds
        .into_iter()
        .map(|(state, weight)| {
            let mut next = state.clone();
            let loses = if next.apply(action).is_ok() {
                next.winner() != Some(protected)
                    && has_verified_immediate_opponent_win_after_transition(&next, protected)
            } else {
                has_verified_immediate_opponent_win(state, protected)
            };
            if loses { weight / total } else { 0.0 }
        })
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

/// Prefer root actions that deny an active opponent win threat.
pub fn action_blocks_threat(state: &GameState, action: &Action, threat: &OpponentThreat) -> bool {
    match action {
        Action::BuildSettlement { vertex } | Action::PlaceSettlement { vertex } => {
            threat.blocking_vertices.contains(vertex)
        }
        Action::BuildRoad { edge } | Action::PlaceRoad { edge } => {
            threat.blocking_edges.contains(edge)
                || state.board.edges[*edge as usize]
                    .vertices
                    .iter()
                    .any(|vertex| threat.blocking_vertices.contains(vertex))
        }
        Action::MoveRobber { hex, .. } | Action::PlayKnight { hex, .. } => {
            threat.blocking_hexes.contains(hex)
                || matches!(
                    threat.kind,
                    OpponentThreatKind::ProductionEnabledWin
                        | OpponentThreatKind::ImmediateMainPhaseWin
                        | OpponentThreatKind::HiddenVictoryPointWin
                )
        }
        Action::OfferTrade { .. }
        | Action::CounterTrade { .. }
        | Action::RespondTrade { accept: true } => false,
        Action::RespondTrade { accept: false } | Action::CancelTrade => {
            matches!(
                threat.kind,
                OpponentThreatKind::TradeEnabledWin
                    | OpponentThreatKind::ImmediateMainPhaseWin
                    | OpponentThreatKind::HiddenVictoryPointWin
            )
        }
        Action::EndTurn => false,
        _ => false,
    }
}

/// Insert threat-blocking actions ahead of ordinary relevance quotas.
/// Force coarse threat blockers into the front of a ranked root.
///
/// Disabled in production MaxN until the detector aggregates over the
/// posterior and verifies post-apply that a candidate removes a real win.
#[allow(dead_code)]
pub fn force_threat_blocking_actions(
    state: &GameState,
    actor: u8,
    ranked: &[(Action, f32)],
    selected: &mut Vec<(Action, f32)>,
) {
    let threats = detect_opponent_threats(state, actor);
    if threats.is_empty() {
        return;
    }
    for threat in &threats {
        for candidate in ranked {
            if action_blocks_threat(state, &candidate.0, threat)
                && !selected.iter().any(|(action, _)| action == &candidate.0)
            {
                selected.insert(0, candidate.clone());
                break;
            }
        }
    }
}

#[cfg(test)]
pub(crate) fn forced_blocker_fixture() -> (GameState, Action) {
    let mut state = GameState::standard(419, 3);
    while matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        let action = state.legal_actions()[0].clone();
        state.apply(&action).unwrap();
    }
    state.phase = Phase::Main;
    state.current_player = 0;
    state.victory_target = 10;
    state.roads.fill(None);
    state.longest_road_holder = None;
    state.largest_army_holder = None;
    for player in &mut state.players {
        player.has_longest_road = false;
        player.has_largest_army = false;
        player.development = [0; 5];
        player.resources = [0; 5];
    }
    state.players[0].resources = [1, 1, 1, 1, 0];
    state.players[1].resources = [1, 1, 1, 1, 0];
    state.players[1].public_victory_points = 9;
    state.players[0].settlements_left = state.players[0].settlements_left.max(1);
    state.players[1].settlements_left = state.players[1].settlements_left.max(1);

    for vertex in 0..state.board.vertices.len() as u8 {
        if state.buildings[vertex as usize].is_some() {
            continue;
        }
        let incident = state
            .board
            .edges
            .iter()
            .enumerate()
            .filter_map(|(edge, topology)| {
                topology.vertices.contains(&vertex).then_some(edge as u8)
            })
            .collect::<Vec<_>>();
        if incident.len() < 2 {
            continue;
        }
        for root_edge_index in 0..incident.len() {
            for opponent_edge_index in 0..incident.len() {
                if root_edge_index == opponent_edge_index {
                    continue;
                }
                state.roads.fill(None);
                state.roads[incident[root_edge_index] as usize] = Some(0);
                state.roads[incident[opponent_edge_index] as usize] = Some(1);
                let blocker = Action::BuildSettlement { vertex };
                if !state.legal_actions().contains(&blocker) {
                    continue;
                }
                let opponent_sites = settlement_sites(&state, 1);
                if opponent_sites != [vertex] {
                    continue;
                }
                if !has_verified_immediate_opponent_win(&state, 0) {
                    continue;
                }
                let mut blocked = state.clone();
                blocked.apply(&blocker).unwrap();
                if !has_verified_immediate_opponent_win(&blocked, 0) {
                    return (state, blocker);
                }
            }
        }
    }
    panic!("standard board must expose one contested settlement blocker fixture");
}

#[cfg(test)]
pub(crate) fn winning_road_over_blocker_fixture() -> (GameState, Action, Action) {
    fn extend_path(
        state: &GameState,
        current_vertex: u8,
        forbidden_vertices: &[bool],
        seen_vertices: &mut [bool],
        path: &mut Vec<u8>,
    ) -> bool {
        if path.len() == 5 {
            return true;
        }
        for edge in state.board.vertices[current_vertex as usize]
            .adjacent_edges
            .iter()
            .copied()
        {
            if state.roads[edge as usize].is_some() || path.contains(&edge) {
                continue;
            }
            let [left, right] = state.board.edges[edge as usize].vertices;
            let next_vertex = if left == current_vertex {
                right
            } else if right == current_vertex {
                left
            } else {
                continue;
            };
            if forbidden_vertices[next_vertex as usize] || seen_vertices[next_vertex as usize] {
                continue;
            }
            path.push(edge);
            seen_vertices[next_vertex as usize] = true;
            if extend_path(state, next_vertex, forbidden_vertices, seen_vertices, path) {
                return true;
            }
            seen_vertices[next_vertex as usize] = false;
            path.pop();
        }
        false
    }

    let (state, blocker) = forced_blocker_fixture();
    let blocker_vertex = match blocker {
        Action::BuildSettlement { vertex } => vertex,
        _ => unreachable!("forced blocker fixture must use a settlement"),
    };
    let mut forbidden_vertices = vec![false; state.board.vertices.len()];
    forbidden_vertices[blocker_vertex as usize] = true;
    for (edge, owner) in state.roads.iter().enumerate() {
        if owner.is_some() {
            for vertex in state.board.edges[edge].vertices {
                forbidden_vertices[vertex as usize] = true;
            }
        }
    }

    for (first_edge, topology) in state.board.edges.iter().enumerate() {
        if state.roads[first_edge].is_some()
            || topology
                .vertices
                .iter()
                .any(|vertex| forbidden_vertices[*vertex as usize])
        {
            continue;
        }
        for [start, current] in [
            topology.vertices,
            [topology.vertices[1], topology.vertices[0]],
        ] {
            let mut path = vec![first_edge as u8];
            let mut seen_vertices = vec![false; state.board.vertices.len()];
            seen_vertices[start as usize] = true;
            seen_vertices[current as usize] = true;
            if !extend_path(
                &state,
                current,
                &forbidden_vertices,
                &mut seen_vertices,
                &mut path,
            ) {
                continue;
            }

            let mut candidate_state = state.clone();
            for edge in path.iter().take(4) {
                candidate_state.roads[*edge as usize] = Some(0);
            }
            for player in 0..candidate_state.board.num_players {
                candidate_state.players[player as usize].roads_left = 15_u8.saturating_sub(
                    candidate_state
                        .roads
                        .iter()
                        .filter(|owner| **owner == Some(player))
                        .count() as u8,
                );
                candidate_state.players[player as usize].public_victory_points = candidate_state
                    .buildings
                    .iter()
                    .flatten()
                    .filter(|building| building.player() == player)
                    .map(|building| building.production_multiplier())
                    .sum();
                candidate_state.players[player as usize].development = [0; 5];
            }
            let vp = DevCard::VictoryPoint.index();
            candidate_state.players[0].development[vp] = 1;
            candidate_state.players[1].development[vp] = 2;
            candidate_state.development_deck[vp] = 2;
            candidate_state.victory_target = 5;
            candidate_state.players[0].resources = [1, 1, 1, 1, 0];
            candidate_state.players[1].resources = [1, 1, 1, 1, 0];
            for player in candidate_state.players.iter_mut().skip(2) {
                player.resources = [0; 5];
            }
            for resource in 0..5 {
                candidate_state.bank[resource] = 19_u8.saturating_sub(
                    candidate_state
                        .players
                        .iter()
                        .map(|player| player.resources[resource])
                        .sum(),
                );
            }
            candidate_state.players[0].has_longest_road = false;
            candidate_state.longest_road_holder = None;
            let winning_road = Action::BuildRoad { edge: path[4] };
            if candidate_state.validate().is_err()
                || candidate_state.longest_road_length(0) != 4
                || !candidate_state.legal_actions().contains(&winning_road)
                || !candidate_state.legal_actions().contains(&blocker)
                || !has_verified_immediate_opponent_win(&candidate_state, 0)
            {
                continue;
            }
            let mut won = candidate_state.clone();
            if won.apply(&winning_road).is_err() || won.winner() != Some(0) {
                continue;
            }
            let mut blocked = candidate_state.clone();
            if blocked.apply(&blocker).is_err() || has_verified_immediate_opponent_win(&blocked, 0)
            {
                continue;
            }
            return (candidate_state, winning_road, blocker);
        }
    }
    panic!("standard board must expose a winning-road plus forced-blocker fixture");
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::{
        OpponentThreatKind, detect_opponent_threats, forced_blocker_fixture, forced_loss_weight,
        has_verified_immediate_opponent_win, main_phase_for, posterior_immediate_threat_weight,
        winning_road_over_blocker_fixture,
    };

    #[test]
    fn threat_winning_terminal_has_zero_forced_loss() {
        let (state, winning_road, _) = winning_road_over_blocker_fixture();
        assert_eq!(
            forced_loss_weight(std::iter::once((&state, 1.0)), 0, &winning_road),
            0.0,
        );
        let mut won = state.clone();
        won.apply(&winning_road).unwrap();
        assert_eq!(won.winner(), Some(0));
        assert!(won.is_terminal());
        assert!(!has_verified_immediate_opponent_win(&won, 0));
    }

    #[test]
    fn threat_posterior_verifies_f8_blocker_after_transition() {
        let (state, blocker) = forced_blocker_fixture();
        let worlds = std::iter::once((&state, 1.0));
        assert_eq!(posterior_immediate_threat_weight(worlds, 0), 1.0);
        assert_eq!(
            forced_loss_weight(std::iter::once((&state, 1.0)), 0, &Action::EndTurn),
            1.0,
        );
        assert_eq!(
            forced_loss_weight(std::iter::once((&state, 1.0)), 0, &blocker),
            0.0,
        );
    }

    #[test]
    fn threat_uncertainty_remains_probability_mass_not_a_hard_veto() {
        let (threatened, blocker) = forced_blocker_fixture();
        let mut safe = threatened.clone();
        safe.players[1].public_victory_points = 8;
        let end_risk = forced_loss_weight([(&threatened, 0.5), (&safe, 0.5)], 0, &Action::EndTurn);
        let block_risk = forced_loss_weight([(&threatened, 0.5), (&safe, 0.5)], 0, &blocker);
        assert!((end_risk - 0.5).abs() <= 1e-6);
        assert_eq!(block_risk, 0.0);
        assert!(end_risk < 1.0 - 1e-6);
    }

    #[test]
    fn detects_opponent_settlement_win_threat_when_site_exists() {
        let mut state = GameState::standard(401, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.victory_target = 10;
        state.players[1].public_victory_points = 9;
        state.players[1].resources = [1, 1, 1, 1, 0];
        state.players[1].settlements_left = state.players[1].settlements_left.max(1);
        let probe = main_phase_for(&state, 1);
        let has_settlement = probe
            .legal_actions()
            .iter()
            .any(|action| matches!(action, Action::BuildSettlement { .. }));
        let threats = detect_opponent_threats(&state, 0);
        if has_settlement {
            assert!(
                threats.iter().any(|threat| {
                    threat.opponent == 1
                        && matches!(
                            threat.kind,
                            OpponentThreatKind::ImmediateMainPhaseWin
                                | OpponentThreatKind::HiddenVictoryPointWin
                        )
                }),
                "affordable settlement to the victory target must register as a threat"
            );
        }
    }
}
