use std::collections::HashSet;

use colonist_catan_core::{Action, GameState, Phase, Resource};

use crate::threats::progress_threat_kind;

/// Concrete, near-term opponent outcomes attributable to a domestic trade.
/// `MaterialBuild` is diagnostic risk; the other variants can participate in
/// the near-certain hard-veto posterior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DomesticTradeThreat {
    DirtyMonopoly,
    ImmediateWin,
    AwardSwing,
    ContestedSettlement,
    MaterialBuild,
}

/// Posterior evidence stays visible even when it is below the categorical
/// safety threshold. Search may value that risk normally; only `hard_veto`
/// removes a root from strategic competition.
#[derive(Clone, Copy, Debug, Default)]
pub struct DomesticTradeAssessment {
    pub threat: Option<DomesticTradeThreat>,
    pub posterior: f32,
    pub dirty_monopoly_posterior: f32,
    pub hard_veto_posterior: f32,
    pub hard_veto: bool,
}

/// Keep ordinary uncertain opponent plans in strategic search. This guard is
/// reserved for threats supported by essentially the entire weighted belief.
pub const HARD_VETO_POSTERIOR: f32 = 0.99;

const TACTICAL_ACTION_DEPTH: u8 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ThreatKey {
    ImmediateWin,
    AwardSwing,
    ContestedSettlement(u8),
    SettlementBuild(u8),
    CityBuild(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ProgressOrigin {
    Other,
    Monopoly(Resource),
}

#[derive(Default)]
struct TacticalThreats {
    keys: HashSet<ThreatKey>,
    monopoly_paths: Vec<(ThreatKey, Resource)>,
}

impl TacticalThreats {
    fn insert(&mut self, key: ThreatKey, origin: Option<ProgressOrigin>) {
        self.keys.insert(key);
        if let Some(ProgressOrigin::Monopoly(resource)) = origin
            && !self.monopoly_paths.contains(&(key, resource))
        {
            self.monopoly_paths.push((key, resource));
        }
    }
}

fn is_trade_candidate(action: &Action) -> bool {
    matches!(
        action,
        Action::OfferTrade { .. }
            | Action::RespondTrade { accept: true }
            | Action::CounterTrade { .. }
            | Action::ConfirmTrade { .. }
    )
}

fn is_trade_tail_action(action: &Action) -> bool {
    matches!(
        action,
        Action::RespondTrade { .. } | Action::ConfirmTrade { .. } | Action::CancelTrade
    )
}

fn resolve_without_exchange(state: &GameState) -> Option<GameState> {
    let mut resolved = state.clone();
    if resolved.phase != Phase::TradeResponses {
        return (resolved.phase == Phase::Main).then_some(resolved);
    }
    let limit = resolved.board.num_players.saturating_add(2);
    for _ in 0..limit {
        if resolved.phase == Phase::Main {
            return Some(resolved);
        }
        let legal = resolved.legal_actions();
        let action = legal
            .iter()
            .find(|action| matches!(action, Action::CancelTrade))
            .or_else(|| {
                legal
                    .iter()
                    .find(|action| matches!(action, Action::RespondTrade { accept: false }))
            })?
            .clone();
        resolved.apply(&action).ok()?;
    }
    (resolved.phase == Phase::Main).then_some(resolved)
}

fn resolved_exchange_states(state: &GameState, action: &Action, protected: u8) -> Vec<GameState> {
    fn visit(
        original: &GameState,
        state: &GameState,
        protected: u8,
        remaining: u8,
        exchanged: bool,
        outcomes: &mut Vec<GameState>,
        seen: &mut HashSet<(u64, u8, bool)>,
    ) {
        if !seen.insert((state.state_hash(), remaining, exchanged)) {
            return;
        }
        if state.phase == Phase::Main {
            if exchanged
                && state.players[protected as usize].resources
                    != original.players[protected as usize].resources
            {
                outcomes.push(state.clone());
            }
            return;
        }
        if state.phase != Phase::TradeResponses || remaining == 0 {
            return;
        }
        for tail in state
            .legal_actions()
            .into_iter()
            .filter(is_trade_tail_action)
        {
            let confirmed = matches!(tail, Action::ConfirmTrade { .. });
            let mut next = state.clone();
            if next.apply(&tail).is_ok() {
                visit(
                    original,
                    &next,
                    protected,
                    remaining - 1,
                    exchanged || confirmed,
                    outcomes,
                    seen,
                );
            }
        }
    }

    let mut next = state.clone();
    if next.apply(action).is_err() {
        return Vec::new();
    }
    let mut outcomes = Vec::new();
    let mut seen = HashSet::new();
    let confirmed = matches!(action, Action::ConfirmTrade { .. });
    let limit = state.board.num_players.saturating_add(2);
    visit(
        state,
        &next,
        protected,
        limit,
        confirmed,
        &mut outcomes,
        &mut seen,
    );
    outcomes
}

fn is_build(action: &Action) -> bool {
    matches!(
        action,
        Action::BuildRoad { .. } | Action::BuildSettlement { .. } | Action::BuildCity { .. }
    )
}

fn progress_origin(action: &Action) -> Option<ProgressOrigin> {
    progress_threat_kind(action)?;
    Some(match action {
        Action::PlayMonopoly { resource } => ProgressOrigin::Monopoly(*resource),
        _ => ProgressOrigin::Other,
    })
}

fn contested_settlement(public_state: &GameState, vertex: u8, protected: u8, attacker: u8) -> bool {
    let Some(candidate) = public_state.board.vertices.get(vertex as usize) else {
        return false;
    };
    if public_state.buildings[vertex as usize].is_some()
        || candidate
            .adjacent_vertices
            .iter()
            .any(|neighbor| public_state.buildings[*neighbor as usize].is_some())
    {
        return false;
    }
    let connected = |player| {
        candidate
            .adjacent_edges
            .iter()
            .any(|edge| public_state.roads[*edge as usize] == Some(player))
    };
    connected(protected) && connected(attacker)
}

fn record_threats(
    baseline: &GameState,
    next: &GameState,
    action: &Action,
    protected: u8,
    attacker: u8,
    origin: Option<ProgressOrigin>,
    result: &mut TacticalThreats,
) {
    if next.winner() == Some(attacker) {
        result.insert(ThreatKey::ImmediateWin, origin);
    }
    let attacker_gained_award = (baseline.longest_road_holder != Some(attacker)
        && next.longest_road_holder == Some(attacker))
        || (baseline.largest_army_holder != Some(attacker)
            && next.largest_army_holder == Some(attacker));
    let protected_lost_award = (baseline.longest_road_holder == Some(protected)
        && next.longest_road_holder != Some(protected))
        || (baseline.largest_army_holder == Some(protected)
            && next.largest_army_holder != Some(protected));
    if attacker_gained_award || protected_lost_award {
        result.insert(ThreatKey::AwardSwing, origin);
    }
    match action {
        Action::BuildSettlement { vertex } => {
            if contested_settlement(baseline, *vertex, protected, attacker) {
                result.insert(ThreatKey::ContestedSettlement(*vertex), origin);
            } else {
                result.insert(ThreatKey::SettlementBuild(*vertex), origin);
            }
        }
        Action::BuildCity { vertex } => {
            result.insert(ThreatKey::CityBuild(*vertex), origin);
        }
        _ => {}
    }
}

fn reachable_tactical_threats(
    root: &GameState,
    public_baseline: &GameState,
    protected: u8,
    attacker: u8,
) -> TacticalThreats {
    fn visit(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        attacker: u8,
        depth: u8,
        origin: Option<ProgressOrigin>,
        threats: &mut TacticalThreats,
        seen: &mut HashSet<(u64, u8, Option<ProgressOrigin>)>,
    ) {
        if depth >= TACTICAL_ACTION_DEPTH
            || state.phase != Phase::Main
            || state.current_player != attacker
            || !seen.insert((state.state_hash(), depth, origin))
        {
            return;
        }
        for action in state.legal_actions() {
            let action_origin = progress_origin(&action);
            if !is_build(&action) && action_origin.is_none() {
                continue;
            }
            let mut next = state.clone();
            if next.apply(&action).is_err() {
                continue;
            }
            let path_origin = origin.or(action_origin);
            record_threats(
                public_baseline,
                &next,
                &action,
                protected,
                attacker,
                path_origin,
                threats,
            );
            if !next.is_terminal() {
                visit(
                    &next,
                    public_baseline,
                    protected,
                    attacker,
                    depth + 1,
                    path_origin,
                    threats,
                    seen,
                );
            }
        }
    }

    let mut threats = TacticalThreats::default();
    visit(
        root,
        public_baseline,
        protected,
        attacker,
        0,
        None,
        &mut threats,
        &mut HashSet::new(),
    );
    threats
}

fn reclaimable_resource(state: &GameState, attacker: u8, resource: Resource) -> u16 {
    state
        .players
        .iter()
        .enumerate()
        .filter(|(player, _)| *player != attacker as usize)
        .map(|(_, player)| player.resources[resource.index()] as u16)
        .sum()
}

fn strongest(threats: &HashSet<ThreatKey>, dirty_monopoly: bool) -> Option<DomesticTradeThreat> {
    if dirty_monopoly {
        Some(DomesticTradeThreat::DirtyMonopoly)
    } else if threats.contains(&ThreatKey::ImmediateWin) {
        Some(DomesticTradeThreat::ImmediateWin)
    } else if threats.contains(&ThreatKey::AwardSwing) {
        Some(DomesticTradeThreat::AwardSwing)
    } else if threats
        .iter()
        .any(|threat| matches!(threat, ThreatKey::ContestedSettlement(_)))
    {
        Some(DomesticTradeThreat::ContestedSettlement)
    } else if threats.iter().any(|threat| {
        matches!(
            threat,
            ThreatKey::SettlementBuild(_) | ThreatKey::CityBuild(_)
        )
    }) {
        Some(DomesticTradeThreat::MaterialBuild)
    } else {
        None
    }
}

/// Assess one fully specified hidden world. The candidate is advanced through
/// the real response/confirmation protocol with `GameState::apply()`. Only the
/// actual post-resolution `current_player` receives a same-turn tactical probe.
pub fn domestic_trade_threat(state: &GameState, action: &Action) -> Option<DomesticTradeThreat> {
    if !is_trade_candidate(action) {
        return None;
    }
    let protected = state.actor();
    let before = resolve_without_exchange(state)?;
    let mut newly_enabled = HashSet::new();
    let mut dirty_monopoly = false;

    for after in resolved_exchange_states(state, action, protected) {
        if after.phase != Phase::Main
            || after.current_player == protected
            || before.current_player != after.current_player
        {
            continue;
        }
        let attacker = after.current_player;
        let before_threats = reachable_tactical_threats(&before, &before, protected, attacker);
        let after_threats = reachable_tactical_threats(&after, &before, protected, attacker);
        let new_keys = after_threats
            .keys
            .difference(&before_threats.keys)
            .copied()
            .collect::<Vec<_>>();
        for key in &new_keys {
            newly_enabled.insert(*key);
            for (monopoly_key, resource) in &after_threats.monopoly_paths {
                if monopoly_key == key
                    && reclaimable_resource(&after, attacker, *resource)
                        > reclaimable_resource(&before, attacker, *resource)
                {
                    dirty_monopoly = true;
                }
            }
        }
    }

    strongest(&newly_enabled, dirty_monopoly)
}

/// Aggregate the safety evidence over a weighted hidden-state belief without
/// collapsing sub-threshold malicious-trade risk into a categorical veto.
pub fn belief_domestic_trade_assessment<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    action: &Action,
) -> DomesticTradeAssessment {
    if !is_trade_candidate(action) {
        return DomesticTradeAssessment::default();
    }
    let worlds = worlds.into_iter().collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| weight.max(0.0))
        .sum::<f32>();
    if total <= f32::EPSILON {
        return DomesticTradeAssessment::default();
    }
    let mut mass = [0.0_f32; 5];
    for (state, weight) in worlds {
        let weight = weight.max(0.0) / total;
        match domestic_trade_threat(state, action) {
            Some(DomesticTradeThreat::DirtyMonopoly) => mass[0] += weight,
            Some(DomesticTradeThreat::ImmediateWin) => mass[1] += weight,
            Some(DomesticTradeThreat::AwardSwing) => mass[2] += weight,
            Some(DomesticTradeThreat::ContestedSettlement) => mass[3] += weight,
            Some(DomesticTradeThreat::MaterialBuild) => mass[4] += weight,
            None => {}
        }
    }
    let posterior = mass.iter().sum::<f32>().clamp(0.0, 1.0);
    let threat = mass
        .iter()
        .enumerate()
        .max_by(|left, right| left.1.total_cmp(right.1))
        .filter(|(_, value)| **value > f32::EPSILON)
        .map(|(index, _)| match index {
            0 => DomesticTradeThreat::DirtyMonopoly,
            1 => DomesticTradeThreat::ImmediateWin,
            2 => DomesticTradeThreat::AwardSwing,
            3 => DomesticTradeThreat::ContestedSettlement,
            _ => DomesticTradeThreat::MaterialBuild,
        });
    let hard_veto_posterior = mass[..4].iter().sum::<f32>().clamp(0.0, 1.0);
    DomesticTradeAssessment {
        threat,
        posterior,
        dirty_monopoly_posterior: mass[0].clamp(0.0, 1.0),
        hard_veto_posterior,
        hard_veto: hard_veto_posterior + 1e-6 >= HARD_VETO_POSTERIOR,
    }
}

/// Compatibility seam for existing exact/search safety callers. Only the
/// near-certain classification is returned here; use the assessment API for
/// measured sub-threshold risk and provenance.
pub fn belief_domestic_trade_threat<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    action: &Action,
) -> Option<DomesticTradeThreat> {
    let assessment = belief_domestic_trade_assessment(worlds, action);
    assessment.hard_veto.then_some(assessment.threat).flatten()
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, Building, DevCard, GameState, Phase};

    use super::{
        DomesticTradeThreat, belief_domestic_trade_assessment, belief_domestic_trade_threat,
        domestic_trade_threat,
    };

    fn dirty_monopoly_response_state() -> (GameState, Action) {
        let mut state = GameState::standard(401, 3);
        state.phase = Phase::Main;
        state.current_player = 1;
        state.buildings.fill(None);
        state.roads.fill(None);
        state.buildings[0] = Some(Building::Settlement(1));
        state.players[1].public_victory_points = 9;
        state.players[1].resources = [0, 0, 0, 1, 2];
        state.players[1].development[DevCard::Monopoly.index()] = 1;
        state.players[0].resources = [0, 0, 0, 1, 0];
        state.players[2].resources = [0, 0, 0, 0, 1];
        let offer = Action::OfferTrade {
            recipients: 1 << 0,
            give: [0, 0, 0, 0, 1],
            receive: [0, 0, 0, 1, 0],
        };
        state.apply(&offer).unwrap();
        assert_eq!(state.phase, Phase::TradeResponses);
        assert_eq!(state.current_player, 1);
        assert_eq!(state.actor(), 0);
        (state, Action::RespondTrade { accept: true })
    }

    #[test]
    fn accepting_current_players_offer_vetoes_near_certain_dirty_monopoly() {
        let (state, accept) = dirty_monopoly_response_state();
        assert_eq!(
            domestic_trade_threat(&state, &accept),
            Some(DomesticTradeThreat::DirtyMonopoly)
        );
        let assessment = belief_domestic_trade_assessment([(&state, 1.0)], &accept);
        assert_eq!(assessment.threat, Some(DomesticTradeThreat::DirtyMonopoly));
        assert!((assessment.posterior - 1.0).abs() < 1e-6);
        assert!((assessment.dirty_monopoly_posterior - 1.0).abs() < 1e-6);
        assert!((assessment.hard_veto_posterior - 1.0).abs() < 1e-6);
        assert!(assessment.hard_veto);
        assert_eq!(
            belief_domestic_trade_threat([(&state, 1.0)], &accept),
            Some(DomesticTradeThreat::DirtyMonopoly)
        );
    }

    #[test]
    fn low_monopoly_posterior_stays_in_ordinary_search() {
        let (dangerous, accept) = dirty_monopoly_response_state();
        let mut safe = dangerous.clone();
        safe.players[1].development[DevCard::Monopoly.index()] = 0;
        safe.players[1].development[DevCard::RoadBuilding.index()] = 1;

        let assessment =
            belief_domestic_trade_assessment([(&dangerous, 0.50), (&safe, 0.50)], &accept);
        assert!((assessment.posterior - 0.50).abs() < 1e-6);
        assert!((assessment.dirty_monopoly_posterior - 0.50).abs() < 1e-6);
        assert!((assessment.hard_veto_posterior - 0.50).abs() < 1e-6);
        assert!(!assessment.hard_veto);
        assert_eq!(
            belief_domestic_trade_threat([(&dangerous, 0.50), (&safe, 0.50)], &accept),
            None
        );
    }

    #[test]
    fn near_certain_material_build_stays_in_ordinary_search() {
        let (mut state, accept) = dirty_monopoly_response_state();
        state.players[1].public_victory_points = 4;
        state.players[1].development[DevCard::Monopoly.index()] = 0;
        state.players[1].resources = [0, 0, 0, 1, 4];

        let assessment = belief_domestic_trade_assessment([(&state, 1.0)], &accept);
        assert_eq!(assessment.threat, Some(DomesticTradeThreat::MaterialBuild));
        assert!((assessment.posterior - 1.0).abs() < 1e-6);
        assert!(assessment.hard_veto_posterior <= 1e-6);
        assert!(!assessment.hard_veto);
        assert_eq!(belief_domestic_trade_threat([(&state, 1.0)], &accept), None);
    }

    #[test]
    fn preexisting_winning_conversion_is_not_reclassified_as_trade_created() {
        let (mut state, accept) = dirty_monopoly_response_state();
        state.players[1].resources[3] = 2;
        assert_eq!(domestic_trade_threat(&state, &accept), None);
    }

    #[test]
    fn counteroffer_creator_does_not_receive_fictional_main_phase() {
        let mut state = GameState::standard(403, 3);
        state.phase = Phase::Main;
        state.current_player = 1;
        state.buildings.fill(None);
        state.roads.fill(None);
        state.buildings[0] = Some(Building::Settlement(0));
        state.players[0].public_victory_points = 9;
        state.players[0].resources = [0, 0, 1, 1, 3];
        state.players[0].development[DevCard::Monopoly.index()] = 1;
        state.players[1].resources = [1, 0, 0, 1, 0];
        state
            .apply(&Action::OfferTrade {
                recipients: 1 << 0,
                give: [1, 0, 0, 0, 0],
                receive: [0, 0, 1, 0, 0],
            })
            .unwrap();
        assert_eq!(state.actor(), 0);
        let counter = Action::CounterTrade {
            give: [0, 0, 0, 0, 1],
            receive: [0, 0, 0, 1, 0],
        };
        assert_eq!(domestic_trade_threat(&state, &counter), None);

        let mut resolved = state.clone();
        resolved.apply(&counter).unwrap();
        assert_eq!(resolved.trade.unwrap().creator, 0);
        assert_eq!(resolved.current_player, 1);
        assert_eq!(resolved.actor(), 1);
        resolved
            .apply(&Action::RespondTrade { accept: true })
            .unwrap();
        assert_eq!(resolved.actor(), 2);
        resolved
            .apply(&Action::RespondTrade { accept: false })
            .unwrap();
        assert_eq!(resolved.actor(), 0);
        resolved
            .apply(&Action::ConfirmTrade { partner: 1 })
            .unwrap();
        assert_eq!(resolved.phase, Phase::Main);
        assert_eq!(resolved.current_player, 1);
    }

    #[test]
    fn own_turn_offer_does_not_hand_the_recipient_an_immediate_main_phase() {
        let mut state = GameState::standard(409, 3);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.buildings.fill(None);
        state.buildings[0] = Some(Building::Settlement(1));
        state.players[1].public_victory_points = 9;
        state.players[0].resources = [0, 1, 0, 0, 0];
        state.players[1].resources = [0, 0, 1, 2, 2];
        let offer = Action::OfferTrade {
            recipients: 1 << 1,
            give: [0, 1, 0, 0, 0],
            receive: [0, 0, 1, 0, 0],
        };
        assert_eq!(domestic_trade_threat(&state, &offer), None);
    }
}
