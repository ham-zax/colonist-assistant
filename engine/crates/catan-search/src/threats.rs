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

fn opponent_can_win_main_phase(state: &GameState, opponent: u8) -> Option<OpponentThreat> {
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
        if opponent_can_win_main_phase(&next, opponent).is_some() {
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
        if opponent_can_win_main_phase(&next, opponent).is_some() {
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
        if let Some(threat) = opponent_can_win_main_phase(state, opponent) {
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
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::{OpponentThreatKind, detect_opponent_threats, main_phase_for};

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
