use std::collections::{HashMap, HashSet};

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
enum ProgressChoice {
    BuyDevelopment,
    Knight { hex: u8, victim: Option<u8> },
    RoadBuilding { first: u8, second: Option<u8> },
    YearOfPlenty { first: Resource, second: Resource },
    Monopoly(Resource),
}

#[derive(Clone, Default)]
struct TacticalThreats {
    keys: HashMap<ThreatKey, f32>,
    non_progress_paths: HashMap<ThreatKey, f32>,
    progress_paths: HashMap<(ThreatKey, ProgressChoice), f32>,
}

impl TacticalThreats {
    fn insert(&mut self, key: ThreatKey, origin: Option<ProgressChoice>, probability: f32) {
        let probability = probability.clamp(0.0, 1.0);
        if probability <= f32::EPSILON {
            return;
        }
        Self::insert_max(&mut self.keys, key, probability);
        if let Some(choice) = origin {
            Self::insert_max(&mut self.progress_paths, (key, choice), probability);
        } else {
            Self::insert_max(&mut self.non_progress_paths, key, probability);
        }
    }

    fn insert_max<K: Eq + std::hash::Hash + Copy>(
        values: &mut HashMap<K, f32>,
        key: K,
        probability: f32,
    ) {
        values
            .entry(key)
            .and_modify(|existing| *existing = (*existing).max(probability))
            .or_insert(probability);
    }

    fn merge_max(&mut self, other: &Self) {
        for (&key, &probability) in &other.keys {
            Self::insert_max(&mut self.keys, key, probability);
        }
        for (&key, &probability) in &other.non_progress_paths {
            Self::insert_max(&mut self.non_progress_paths, key, probability);
        }
        for (&key, &probability) in &other.progress_paths {
            Self::insert_max(&mut self.progress_paths, key, probability);
        }
    }

    fn add_weighted(&mut self, other: &Self, weight: f32) {
        if weight <= f32::EPSILON {
            return;
        }
        for (&key, &probability) in &other.keys {
            let entry = self.keys.entry(key).or_default();
            *entry = (*entry + probability * weight).clamp(0.0, 1.0);
        }
        for (&key, &probability) in &other.non_progress_paths {
            let entry = self.non_progress_paths.entry(key).or_default();
            *entry = (*entry + probability * weight).clamp(0.0, 1.0);
        }
        for (&key, &probability) in &other.progress_paths {
            let entry = self.progress_paths.entry(key).or_default();
            *entry = (*entry + probability * weight).clamp(0.0, 1.0);
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

fn progress_origin(action: &Action) -> Option<ProgressChoice> {
    progress_threat_kind(action)?;
    Some(match action {
        Action::BuyDevelopment => ProgressChoice::BuyDevelopment,
        Action::PlayKnight { hex, victim } => ProgressChoice::Knight {
            hex: *hex,
            victim: *victim,
        },
        Action::PlayRoadBuilding { first, second } => ProgressChoice::RoadBuilding {
            first: *first,
            second: *second,
        },
        Action::PlayYearOfPlenty { first, second } => ProgressChoice::YearOfPlenty {
            first: *first,
            second: *second,
        },
        Action::PlayMonopoly { resource } => ProgressChoice::Monopoly(*resource),
        _ => return None,
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
    origin: Option<ProgressChoice>,
    probability: f32,
    result: &mut TacticalThreats,
) {
    if next.winner() == Some(attacker) {
        result.insert(ThreatKey::ImmediateWin, origin, probability);
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
        result.insert(ThreatKey::AwardSwing, origin, probability);
    }
    match action {
        Action::BuildSettlement { vertex } => {
            if contested_settlement(baseline, *vertex, protected, attacker) {
                result.insert(ThreatKey::ContestedSettlement(*vertex), origin, probability);
            } else {
                result.insert(ThreatKey::SettlementBuild(*vertex), origin, probability);
            }
        }
        Action::BuildCity { vertex } => {
            result.insert(ThreatKey::CityBuild(*vertex), origin, probability);
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
    fn chance_tail(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        attacker: u8,
        depth: u8,
        origin: Option<ProgressChoice>,
        seen: &mut HashSet<(u64, u8, Option<ProgressChoice>)>,
    ) -> TacticalThreats {
        let actions = state.legal_actions();
        let total_weight = actions
            .iter()
            .map(|action| state.chance_weight(action) as u32)
            .sum::<u32>();
        if total_weight == 0 {
            return TacticalThreats::default();
        }

        let mut threats = TacticalThreats::default();
        for action in actions {
            let weight = state.chance_weight(&action) as u32;
            if weight == 0 {
                continue;
            }
            let mut next = state.clone();
            if next.apply(&action).is_err() {
                continue;
            }
            let mut branch = TacticalThreats::default();
            record_threats(
                public_baseline,
                &next,
                &action,
                protected,
                attacker,
                origin,
                1.0,
                &mut branch,
            );
            if !next.is_terminal() {
                let continuation = match next.phase {
                    Phase::Main => visit(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth,
                        origin,
                        seen,
                    ),
                    Phase::DevelopmentChance | Phase::ResolveSteal { .. } => chance_tail(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth,
                        origin,
                        seen,
                    ),
                    _ => TacticalThreats::default(),
                };
                branch.merge_max(&continuation);
            }
            threats.add_weighted(&branch, weight as f32 / total_weight as f32);
        }
        threats
    }

    fn visit(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        attacker: u8,
        depth: u8,
        origin: Option<ProgressChoice>,
        seen: &mut HashSet<(u64, u8, Option<ProgressChoice>)>,
    ) -> TacticalThreats {
        if depth >= TACTICAL_ACTION_DEPTH
            || state.phase != Phase::Main
            || state.current_player != attacker
        {
            return TacticalThreats::default();
        }
        let seen_key = (state.state_hash(), depth, origin);
        if !seen.insert(seen_key) {
            return TacticalThreats::default();
        }

        let mut threats = TacticalThreats::default();
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
            let mut branch = TacticalThreats::default();
            record_threats(
                public_baseline,
                &next,
                &action,
                protected,
                attacker,
                path_origin,
                1.0,
                &mut branch,
            );
            if !next.is_terminal() {
                let continuation = match next.phase {
                    Phase::Main => visit(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth + 1,
                        path_origin,
                        seen,
                    ),
                    Phase::DevelopmentChance | Phase::ResolveSteal { .. } => chance_tail(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth + 1,
                        path_origin,
                        seen,
                    ),
                    _ => TacticalThreats::default(),
                };
                branch.merge_max(&continuation);
            }
            threats.merge_max(&branch);
        }
        seen.remove(&seen_key);
        threats
    }

    visit(
        root,
        public_baseline,
        protected,
        attacker,
        0,
        None,
        &mut HashSet::new(),
    )
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

fn probability_delta<K: Eq + std::hash::Hash>(
    after: &HashMap<K, f32>,
    before: &HashMap<K, f32>,
    key: &K,
) -> f32 {
    (after.get(key).copied().unwrap_or(0.0) - before.get(key).copied().unwrap_or(0.0)).max(0.0)
}

fn hard_threat_key(key: ThreatKey) -> bool {
    matches!(
        key,
        ThreatKey::ImmediateWin | ThreatKey::AwardSwing | ThreatKey::ContestedSettlement(_)
    )
}

fn strongest(
    threats: &HashMap<ThreatKey, f32>,
    dirty_monopoly_probability: f32,
) -> Option<DomesticTradeThreat> {
    if dirty_monopoly_probability > f32::EPSILON {
        Some(DomesticTradeThreat::DirtyMonopoly)
    } else if threats
        .get(&ThreatKey::ImmediateWin)
        .copied()
        .unwrap_or(0.0)
        > f32::EPSILON
    {
        Some(DomesticTradeThreat::ImmediateWin)
    } else if threats.get(&ThreatKey::AwardSwing).copied().unwrap_or(0.0) > f32::EPSILON {
        Some(DomesticTradeThreat::AwardSwing)
    } else if threats.iter().any(|(threat, probability)| {
        *probability > f32::EPSILON && matches!(threat, ThreatKey::ContestedSettlement(_))
    }) {
        Some(DomesticTradeThreat::ContestedSettlement)
    } else if threats.iter().any(|(threat, probability)| {
        *probability > f32::EPSILON
            && matches!(
                threat,
                ThreatKey::SettlementBuild(_) | ThreatKey::CityBuild(_)
            )
    }) {
        Some(DomesticTradeThreat::MaterialBuild)
    } else {
        None
    }
}

fn threat_probability(
    threats: &HashMap<ThreatKey, f32>,
    threat: DomesticTradeThreat,
    dirty_monopoly_probability: f32,
) -> f32 {
    match threat {
        DomesticTradeThreat::DirtyMonopoly => dirty_monopoly_probability,
        DomesticTradeThreat::ImmediateWin => threats
            .get(&ThreatKey::ImmediateWin)
            .copied()
            .unwrap_or(0.0),
        DomesticTradeThreat::AwardSwing => {
            threats.get(&ThreatKey::AwardSwing).copied().unwrap_or(0.0)
        }
        DomesticTradeThreat::ContestedSettlement => threats
            .iter()
            .filter_map(|(key, probability)| {
                matches!(key, ThreatKey::ContestedSettlement(_)).then_some(*probability)
            })
            .fold(0.0_f32, f32::max),
        DomesticTradeThreat::MaterialBuild => threats
            .iter()
            .filter_map(|(key, probability)| {
                matches!(key, ThreatKey::SettlementBuild(_) | ThreatKey::CityBuild(_))
                    .then_some(*probability)
            })
            .fold(0.0_f32, f32::max),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum HardPolicyChoice {
    NonProgress,
    Progress(ProgressChoice),
}

#[derive(Clone)]
struct HardChoiceEvidence {
    attacker: u8,
    observation: u64,
    hard_probabilities: HashMap<HardPolicyChoice, f32>,
}

#[derive(Default)]
struct WorldTradeEvidence {
    threat: Option<DomesticTradeThreat>,
    threat_probability: f32,
    dirty_monopoly_probability: f32,
    hard_choice: Option<HardChoiceEvidence>,
}

fn domestic_trade_evidence(state: &GameState, action: &Action) -> WorldTradeEvidence {
    if !is_trade_candidate(action) {
        return WorldTradeEvidence::default();
    }
    let protected = state.actor();
    let Some(before) = resolve_without_exchange(state) else {
        return WorldTradeEvidence::default();
    };
    let mut newly_enabled = HashMap::<ThreatKey, f32>::new();
    let mut dirty_monopoly_probability = 0.0_f32;
    let mut hard_contexts = Vec::<HardChoiceEvidence>::new();

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

        for (&key, &probability) in &after_threats.keys {
            let delta = probability_delta(&after_threats.keys, &before_threats.keys, &key);
            if delta > f32::EPSILON {
                TacticalThreats::insert_max(&mut newly_enabled, key, delta);
            }
            debug_assert!(probability >= delta);
        }

        let mut hard_probabilities = HashMap::<HardPolicyChoice, f32>::new();
        for &key in after_threats.non_progress_paths.keys() {
            let delta = probability_delta(
                &after_threats.non_progress_paths,
                &before_threats.non_progress_paths,
                &key,
            );
            if delta > f32::EPSILON && hard_threat_key(key) {
                TacticalThreats::insert_max(
                    &mut hard_probabilities,
                    HardPolicyChoice::NonProgress,
                    delta,
                );
            }
        }
        for (&(key, choice), _) in &after_threats.progress_paths {
            let delta = probability_delta(
                &after_threats.progress_paths,
                &before_threats.progress_paths,
                &(key, choice),
            );
            if delta <= f32::EPSILON {
                continue;
            }
            let dirty = match choice {
                ProgressChoice::Monopoly(resource) => {
                    reclaimable_resource(&after, attacker, resource)
                        > reclaimable_resource(&before, attacker, resource)
                }
                _ => false,
            };
            if dirty {
                dirty_monopoly_probability = dirty_monopoly_probability.max(delta);
            }
            if dirty || hard_threat_key(key) {
                TacticalThreats::insert_max(
                    &mut hard_probabilities,
                    HardPolicyChoice::Progress(choice),
                    delta,
                );
            }
        }
        if !hard_probabilities.is_empty() {
            let observation = after.observation_hash(attacker);
            if let Some(existing) = hard_contexts
                .iter_mut()
                .find(|context| context.attacker == attacker && context.observation == observation)
            {
                for (&choice, &probability) in &hard_probabilities {
                    TacticalThreats::insert_max(
                        &mut existing.hard_probabilities,
                        choice,
                        probability,
                    );
                }
            } else {
                hard_contexts.push(HardChoiceEvidence {
                    attacker,
                    observation,
                    hard_probabilities,
                });
            }
        }
    }

    let threat = strongest(&newly_enabled, dirty_monopoly_probability);
    let threat_probability = threat.map_or(0.0, |threat| {
        threat_probability(&newly_enabled, threat, dirty_monopoly_probability)
    });
    let hard_choice = hard_contexts.into_iter().max_by(|left, right| {
        let left_probability = left
            .hard_probabilities
            .values()
            .copied()
            .fold(0.0_f32, f32::max);
        let right_probability = right
            .hard_probabilities
            .values()
            .copied()
            .fold(0.0_f32, f32::max);
        left_probability
            .total_cmp(&right_probability)
            .then_with(|| left.attacker.cmp(&right.attacker))
            .then_with(|| left.observation.cmp(&right.observation))
    });

    WorldTradeEvidence {
        threat,
        threat_probability,
        dirty_monopoly_probability,
        hard_choice,
    }
}

/// Assess one fully specified hidden world. The candidate is advanced through
/// the real response/confirmation protocol with `GameState::apply()`. Only the
/// actual post-resolution `current_player` receives a same-turn tactical probe.
pub fn domestic_trade_threat(state: &GameState, action: &Action) -> Option<DomesticTradeThreat> {
    domestic_trade_evidence(state, action).threat
}

/// Aggregate the safety evidence over a weighted hidden-state belief without
/// collapsing sub-threshold malicious-trade risk into a categorical veto.
/// Hard evidence is aggregated by the acting opponent's observation and the
/// concrete progress-card policy choice, so indistinguishable worlds cannot
/// select different hidden-state-dependent Knight, YOP, or Monopoly actions.
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

    struct ObservationHardMass {
        attacker: u8,
        observation: u64,
        choice_mass: HashMap<HardPolicyChoice, f32>,
    }

    let mut mass = [0.0_f32; 5];
    let mut dirty_monopoly_posterior = 0.0_f32;
    let mut observation_hard_mass = Vec::<ObservationHardMass>::new();
    for (state, weight) in worlds {
        let weight = weight.max(0.0) / total;
        if weight <= f32::EPSILON {
            continue;
        }
        let evidence = domestic_trade_evidence(state, action);
        if let Some(threat) = evidence.threat {
            let index = match threat {
                DomesticTradeThreat::DirtyMonopoly => 0,
                DomesticTradeThreat::ImmediateWin => 1,
                DomesticTradeThreat::AwardSwing => 2,
                DomesticTradeThreat::ContestedSettlement => 3,
                DomesticTradeThreat::MaterialBuild => 4,
            };
            mass[index] += weight * evidence.threat_probability;
        }
        dirty_monopoly_posterior += weight * evidence.dirty_monopoly_probability;

        if let Some(choice) = evidence.hard_choice {
            let group = if let Some(group) = observation_hard_mass.iter_mut().find(|group| {
                group.attacker == choice.attacker && group.observation == choice.observation
            }) {
                group
            } else {
                observation_hard_mass.push(ObservationHardMass {
                    attacker: choice.attacker,
                    observation: choice.observation,
                    choice_mass: HashMap::new(),
                });
                observation_hard_mass.last_mut().unwrap()
            };
            for (policy_choice, probability) in choice.hard_probabilities {
                *group.choice_mass.entry(policy_choice).or_default() += weight * probability;
            }
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
    let hard_veto_posterior = observation_hard_mass
        .iter()
        .map(|group| group.choice_mass.values().copied().fold(0.0_f32, f32::max))
        .sum::<f32>()
        .clamp(0.0, 1.0);
    DomesticTradeAssessment {
        threat,
        posterior,
        dirty_monopoly_posterior: dirty_monopoly_posterior.clamp(0.0, 1.0),
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
