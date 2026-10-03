use std::cell::Cell;
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

/// Interruption is sticky: an incomplete probe must never become safe evidence.
struct ProbeControl<'a> {
    stop: &'a dyn Fn() -> bool,
    interrupted: Cell<bool>,
}

impl<'a> ProbeControl<'a> {
    fn new(stop: &'a dyn Fn() -> bool) -> Self {
        Self {
            stop,
            interrupted: Cell::new(false),
        }
    }

    fn stopped(&self) -> bool {
        if self.interrupted.get() || (self.stop)() {
            self.interrupted.set(true);
            true
        } else {
            false
        }
    }
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum ProgressPath {
    Single(ProgressChoice),
    Multiple { monopoly: Option<Resource> },
}

#[derive(Clone, Default)]
struct TacticalThreats {
    keys: HashMap<ThreatKey, f32>,
    non_progress_paths: HashMap<ThreatKey, f32>,
    progress_paths: HashMap<(ThreatKey, ProgressPath), f32>,
}

impl TacticalThreats {
    fn insert(&mut self, key: ThreatKey, origin: Option<ProgressPath>, probability: f32) {
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

#[cfg(test)]
fn resolved_exchange_states(state: &GameState, action: &Action, protected: u8) -> Vec<GameState> {
    resolved_exchange_states_controlled(state, action, protected, &ProbeControl::new(&|| false))
}

fn resolved_exchange_states_controlled(
    state: &GameState,
    action: &Action,
    protected: u8,
    control: &ProbeControl<'_>,
) -> Vec<GameState> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        original: &GameState,
        state: &GameState,
        protected: u8,
        remaining: u8,
        exchanged: bool,
        outcomes: &mut Vec<GameState>,
        seen: &mut HashSet<(u64, u8, bool)>,
        control: &ProbeControl<'_>,
    ) {
        if control.stopped() {
            return;
        }
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
                    control,
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
        control,
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

fn monopoly_resource(choice: ProgressChoice) -> Option<Resource> {
    match choice {
        ProgressChoice::Monopoly(resource) => Some(resource),
        _ => None,
    }
}

fn extend_progress_path(
    path: Option<ProgressPath>,
    choice: Option<ProgressChoice>,
) -> Option<ProgressPath> {
    match (path, choice) {
        (None, Some(choice)) => Some(ProgressPath::Single(choice)),
        (Some(ProgressPath::Single(first)), Some(choice)) => Some(ProgressPath::Multiple {
            monopoly: monopoly_resource(first).or_else(|| monopoly_resource(choice)),
        }),
        (Some(ProgressPath::Multiple { monopoly }), Some(choice)) => Some(ProgressPath::Multiple {
            monopoly: monopoly.or_else(|| monopoly_resource(choice)),
        }),
        (path, None) => path,
    }
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

#[allow(clippy::too_many_arguments)]
fn record_threats(
    baseline: &GameState,
    next: &GameState,
    action: &Action,
    protected: u8,
    attacker: u8,
    origin: Option<ProgressPath>,
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

type TacticalMemoKey = (u64, u8, Option<ProgressPath>);

/// Completed tactical subtrees for one `reachable_tactical_threats` probe.
/// Without `reuse`, entries only guard the current path (the unmemoized
/// reference behaviour that tests compare against).
struct TacticalMemo {
    reuse: bool,
    visit: HashMap<TacticalMemoKey, TacticalThreats>,
    chance: HashMap<TacticalMemoKey, TacticalThreats>,
    #[cfg(test)]
    hits: usize,
}

impl TacticalMemo {
    fn new(reuse: bool) -> Self {
        Self {
            reuse,
            visit: HashMap::new(),
            chance: HashMap::new(),
            #[cfg(test)]
            hits: 0,
        }
    }
}

#[cfg(test)]
fn reachable_tactical_threats_with(
    root: &GameState,
    public_baseline: &GameState,
    protected: u8,
    attacker: u8,
    monopoly_gain_penalty: Option<(Resource, u8)>,
    memo: &mut TacticalMemo,
) -> TacticalThreats {
    reachable_tactical_threats_controlled(
        root,
        public_baseline,
        protected,
        attacker,
        monopoly_gain_penalty,
        memo,
        &ProbeControl::new(&|| false),
    )
}

#[allow(clippy::too_many_arguments)]
fn reachable_tactical_threats_controlled(
    root: &GameState,
    public_baseline: &GameState,
    protected: u8,
    attacker: u8,
    monopoly_gain_penalty: Option<(Resource, u8)>,
    memo: &mut TacticalMemo,
    control: &ProbeControl<'_>,
) -> TacticalThreats {
    #[allow(clippy::too_many_arguments)]
    fn chance_tail(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        attacker: u8,
        depth: u8,
        origin: Option<ProgressPath>,
        monopoly_gain_penalty: Option<(Resource, u8)>,
        memo: &mut TacticalMemo,
        control: &ProbeControl<'_>,
    ) -> TacticalThreats {
        if control.stopped() {
            return TacticalThreats::default();
        }
        let memo_key = (state.state_hash(), depth, origin);
        if memo.reuse
            && let Some(threats) = memo.chance.get(&memo_key)
        {
            #[cfg(test)]
            {
                memo.hits += 1;
            }
            return threats.clone();
        }
        let actions = state.legal_actions();
        let total_weight = actions
            .iter()
            .map(|action| state.chance_weight(action))
            .sum::<u64>();
        if total_weight == 0 {
            return TacticalThreats::default();
        }

        let mut threats = TacticalThreats::default();
        for action in actions {
            if control.stopped() {
                return TacticalThreats::default();
            }
            let weight = state.chance_weight(&action);
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
                        monopoly_gain_penalty,
                        memo,
                        control,
                    ),
                    Phase::DevelopmentChance | Phase::ResolveSteal { .. } => chance_tail(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth,
                        origin,
                        monopoly_gain_penalty,
                        memo,
                        control,
                    ),
                    _ => TacticalThreats::default(),
                };
                branch.merge_max(&continuation);
            }
            threats.add_weighted(&branch, weight as f32 / total_weight as f32);
        }
        if control.interrupted.get() {
            return TacticalThreats::default();
        }
        if memo.reuse {
            memo.chance.insert(memo_key, threats.clone());
        }
        threats
    }

    #[allow(clippy::too_many_arguments)]
    fn visit(
        state: &GameState,
        public_baseline: &GameState,
        protected: u8,
        attacker: u8,
        depth: u8,
        origin: Option<ProgressPath>,
        monopoly_gain_penalty: Option<(Resource, u8)>,
        memo: &mut TacticalMemo,
        control: &ProbeControl<'_>,
    ) -> TacticalThreats {
        if control.stopped() {
            return TacticalThreats::default();
        }
        if depth >= TACTICAL_ACTION_DEPTH
            || state.phase != Phase::Main
            || state.current_player != attacker
        {
            return TacticalThreats::default();
        }
        // Results depend only on this key, so transposed build/progress orders
        // share one expansion. The empty placeholder keeps the old cycle guard.
        let memo_key = (state.state_hash(), depth, origin);
        if let Some(threats) = memo.visit.get(&memo_key) {
            #[cfg(test)]
            {
                memo.hits += 1;
            }
            return threats.clone();
        }
        memo.visit.insert(memo_key, TacticalThreats::default());

        let mut threats = TacticalThreats::default();
        for action in state.legal_actions() {
            if control.stopped() {
                return TacticalThreats::default();
            }
            let action_origin = progress_origin(&action);
            if !is_build(&action) && action_origin.is_none() {
                continue;
            }
            let mut next = state.clone();
            if next.apply(&action).is_err() {
                continue;
            }
            // Causal counterfactual: remove only the extra Monopoly haul created
            // by the trade, after the action so earlier action legality is unchanged.
            if let (Action::PlayMonopoly { resource }, Some((penalized_resource, penalty))) =
                (&action, monopoly_gain_penalty)
                && *resource == penalized_resource
            {
                let held = &mut next.players[attacker as usize].resources[resource.index()];
                debug_assert!(*held >= penalty);
                *held = held.saturating_sub(penalty);
            }
            let path_origin = extend_progress_path(origin, action_origin);
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
                        monopoly_gain_penalty,
                        memo,
                        control,
                    ),
                    Phase::DevelopmentChance | Phase::ResolveSteal { .. } => chance_tail(
                        &next,
                        public_baseline,
                        protected,
                        attacker,
                        depth + 1,
                        path_origin,
                        monopoly_gain_penalty,
                        memo,
                        control,
                    ),
                    _ => TacticalThreats::default(),
                };
                branch.merge_max(&continuation);
            }
            threats.merge_max(&branch);
        }
        if control.interrupted.get() {
            return TacticalThreats::default();
        }
        if memo.reuse {
            memo.visit.insert(memo_key, threats.clone());
        } else {
            memo.visit.remove(&memo_key);
        }
        threats
    }

    visit(
        root,
        public_baseline,
        protected,
        attacker,
        0,
        None,
        monopoly_gain_penalty,
        memo,
        control,
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

fn domestic_trade_evidence(
    state: &GameState,
    action: &Action,
    control: &ProbeControl<'_>,
) -> WorldTradeEvidence {
    if !is_trade_candidate(action) {
        return WorldTradeEvidence::default();
    }
    if control.stopped() {
        return WorldTradeEvidence::default();
    }
    let protected = state.actor();
    let Some(before) = resolve_without_exchange(state) else {
        return WorldTradeEvidence::default();
    };
    let mut newly_enabled = HashMap::<ThreatKey, f32>::new();
    let mut dirty_monopoly_probability = 0.0_f32;
    let mut hard_contexts = Vec::<HardChoiceEvidence>::new();
    // Every retained outcome keeps `before.current_player` as the attacker, so
    // the no-exchange baseline is shared by all of them.
    let attacker = before.current_player;
    let mut before_threats = None;

    for after in resolved_exchange_states_controlled(state, action, protected, control) {
        if after.phase != Phase::Main
            || after.current_player == protected
            || before.current_player != after.current_player
        {
            continue;
        }
        if control.stopped() {
            return WorldTradeEvidence::default();
        }
        debug_assert_eq!(after.current_player, attacker);
        let before_threats = before_threats.get_or_insert_with(|| {
            reachable_tactical_threats_controlled(
                &before,
                &before,
                protected,
                attacker,
                None,
                &mut TacticalMemo::new(true),
                control,
            )
        });
        let after_threats = reachable_tactical_threats_controlled(
            &after,
            &before,
            protected,
            attacker,
            None,
            &mut TacticalMemo::new(true),
            control,
        );
        let mut monopoly_counterfactuals = HashMap::<Resource, TacticalThreats>::new();
        for resource in Resource::ALL {
            let appears_in_path = after_threats.progress_paths.keys().any(|(_, path)| {
                matches!(
                    path,
                    ProgressPath::Single(ProgressChoice::Monopoly(found))
                        | ProgressPath::Multiple {
                            monopoly: Some(found)
                        } if *found == resource
                )
            });
            if !appears_in_path {
                continue;
            }
            let gain = reclaimable_resource(&after, attacker, resource)
                .saturating_sub(reclaimable_resource(&before, attacker, resource));
            if gain == 0 {
                continue;
            }
            let penalty = gain.min(u8::MAX as u16) as u8;
            monopoly_counterfactuals.insert(
                resource,
                reachable_tactical_threats_controlled(
                    &after,
                    &before,
                    protected,
                    attacker,
                    Some((resource, penalty)),
                    &mut TacticalMemo::new(true),
                    control,
                ),
            );
        }

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
        for &(key, path) in after_threats.progress_paths.keys() {
            let delta = probability_delta(
                &after_threats.progress_paths,
                &before_threats.progress_paths,
                &(key, path),
            );
            if delta <= f32::EPSILON {
                continue;
            }
            let monopoly = match path {
                ProgressPath::Single(choice) => monopoly_resource(choice),
                ProgressPath::Multiple { monopoly } => monopoly,
            };
            let dirty_probability = monopoly
                .and_then(|resource| monopoly_counterfactuals.get(&resource))
                .map_or(0.0, |counterfactual| {
                    let trade_enabled =
                        probability_delta(&after_threats.keys, &before_threats.keys, &key);
                    let monopoly_gain_enabled =
                        probability_delta(&after_threats.keys, &counterfactual.keys, &key);
                    delta.min(trade_enabled).min(monopoly_gain_enabled)
                });
            if dirty_probability > f32::EPSILON {
                dirty_monopoly_probability = dirty_monopoly_probability.max(dirty_probability);
            }
            if let ProgressPath::Single(choice) = path {
                let hard_probability = if hard_threat_key(key) {
                    delta
                } else {
                    dirty_probability
                };
                if hard_probability > f32::EPSILON {
                    TacticalThreats::insert_max(
                        &mut hard_probabilities,
                        HardPolicyChoice::Progress(choice),
                        hard_probability,
                    );
                }
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
    domestic_trade_evidence(state, action, &ProbeControl::new(&|| false)).threat
}

/// Aggregate the safety evidence over a weighted hidden-state belief without
/// collapsing sub-threshold malicious-trade risk into a categorical veto.
/// Hard evidence is aggregated by the acting opponent's observation and one
/// concrete progress-card policy choice. Multi-progress tactical lines remain
/// diagnostic but are conservatively excluded from categorical hard-veto mass.
pub fn belief_domestic_trade_assessment<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    action: &Action,
) -> DomesticTradeAssessment {
    belief_domestic_trade_assessment_controlled(worlds, action, &|| false)
        .expect("an unbounded safety probe cannot be interrupted")
}

/// `None` means incomplete, never a zero-risk assessment. Callers must exclude
/// the candidate or retry with more time before selecting it.
pub(crate) fn belief_domestic_trade_assessment_controlled<'a>(
    worlds: impl IntoIterator<Item = (&'a GameState, f32)>,
    action: &Action,
    stop: &dyn Fn() -> bool,
) -> Option<DomesticTradeAssessment> {
    let control = ProbeControl::new(stop);
    if !is_trade_candidate(action) {
        return Some(DomesticTradeAssessment::default());
    }
    let worlds = worlds.into_iter().collect::<Vec<_>>();
    let total = worlds
        .iter()
        .map(|(_, weight)| weight.max(0.0))
        .sum::<f32>();
    if total <= f32::EPSILON {
        return Some(DomesticTradeAssessment::default());
    }

    // Negotiation tails preserve the turn owner. When every world has that
    // owner acting, no opponent receives the same-turn probe this guard checks.
    if worlds
        .iter()
        .filter(|(_, weight)| *weight > 0.0)
        .all(|(state, _)| state.actor() == state.current_player)
    {
        return Some(DomesticTradeAssessment::default());
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
        if control.stopped() {
            return None;
        }
        let evidence = domestic_trade_evidence(state, action, &control);
        if control.stopped() {
            return None;
        }
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
    Some(DomesticTradeAssessment {
        threat,
        posterior,
        dirty_monopoly_posterior: dirty_monopoly_posterior.clamp(0.0, 1.0),
        hard_veto_posterior,
        hard_veto: hard_veto_posterior + 1e-6 >= HARD_VETO_POSTERIOR,
    })
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

    use colonist_catan_core::SplitMix64;

    use super::{
        DomesticTradeThreat, TacticalMemo, belief_domestic_trade_assessment,
        belief_domestic_trade_threat, domestic_trade_threat, reachable_tactical_threats_with,
    };

    #[test]
    fn memoized_tactical_probe_matches_unmemoized_reference() {
        let mut compared = 0;
        let mut cache_hits = 0;
        let mut nonempty = 0;
        let mut rich_compared = 0;
        for seed in 0..6_u64 {
            let mut state = GameState::standard(9_300_001 + seed, 4);
            let mut rng = SplitMix64::new(seed);
            for step in 0..400 {
                if state.is_terminal() {
                    break;
                }
                if state.phase == Phase::Main && step % 7 == 0 {
                    let attacker = state.current_player;
                    let protected = (attacker + 1) % 4;
                    // Also probe a copy where the attacker holds spare bank cards and
                    // unplayed progress cards, so transpositions actually occur.
                    let mut rich = state.clone();
                    for resource in 0..5 {
                        let moved = rich.bank[resource].min(3);
                        rich.bank[resource] -= moved;
                        rich.players[attacker as usize].resources[resource] += moved;
                    }
                    let hand = &mut rich.players[attacker as usize];
                    hand.played_development_this_turn = false;
                    for card in [DevCard::Knight, DevCard::RoadBuilding, DevCard::Monopoly] {
                        hand.development[card.index()] += 1;
                    }
                    // A few rich states cover cache reuse without making the
                    // regression suite expand hundreds of expensive subtrees.
                    let probes =
                        std::iter::once(&state).chain((rich_compared < 3).then_some(&rich));
                    for probe in probes {
                        if std::ptr::eq(probe, &rich) {
                            rich_compared += 1;
                        }
                        let mut memo = TacticalMemo::new(true);
                        let fast = reachable_tactical_threats_with(
                            probe, &state, protected, attacker, None, &mut memo,
                        );
                        let reference = reachable_tactical_threats_with(
                            probe,
                            &state,
                            protected,
                            attacker,
                            None,
                            &mut TacticalMemo::new(false),
                        );
                        assert_eq!(fast.keys, reference.keys, "seed {seed} step {step}");
                        assert_eq!(fast.non_progress_paths, reference.non_progress_paths);
                        assert_eq!(fast.progress_paths, reference.progress_paths);
                        cache_hits += memo.hits;
                        nonempty += usize::from(!fast.keys.is_empty());
                        compared += 1;
                    }
                }
                let actions = state.legal_actions();
                let action = actions[rng.range(actions.len())].clone();
                state.apply(&action).unwrap();
            }
        }
        assert!(compared >= 20, "only {compared} states compared");
        assert!(
            cache_hits > 0,
            "fixture must exercise completed subtree reuse"
        );
        assert!(nonempty > 0, "fixture must preserve real tactical threats");
    }

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
    fn timed_belief_search_rejects_an_unverified_trade() {
        let (mut state, accept) = dirty_monopoly_response_state();
        state.players[1].public_victory_points = 2;
        state.players[1].resources = [4; 5];
        state.players[1].development = [1, 0, 1, 1, 1];
        let report = crate::depth::search_weighted_belief_maxn_bounded_timed(
            &[crate::BeliefParticle { state, weight: 1.0 }],
            3,
            12,
            8_000,
            1,
        )
        .unwrap();
        assert!(report.deadline_reached);
        assert_eq!(report.chosen, Some(Action::RespondTrade { accept: false }));
        assert!(
            report
                .provenance
                .pruned_roots
                .iter()
                .any(|root| root.action == accept
                    && root.reason == crate::depth::RootPruneReason::TradeSafetyIncomplete)
        );
        assert!(!report.actions.iter().any(|root| root.action == accept));
    }

    #[test]
    fn interrupted_safety_probe_never_returns_partial_safe_evidence() {
        use std::cell::Cell;
        let (state, accept) = dirty_monopoly_response_state();
        let checkpoints = Cell::new(0);
        let full =
            super::belief_domestic_trade_assessment_controlled([(&state, 1.0)], &accept, &|| {
                checkpoints.set(checkpoints.get() + 1);
                false
            })
            .unwrap();
        assert!(full.hard_veto);
        assert_eq!(full.threat, Some(super::DomesticTradeThreat::DirtyMonopoly));
        let total = checkpoints.get();
        assert!(total > 10, "fixture must exercise recursive checkpoints");
        for stop_at in [1, 5, total / 2, total - 1] {
            let calls = Cell::new(0);
            let partial = super::belief_domestic_trade_assessment_controlled(
                [(&state, 1.0), (&state, 1.0)],
                &accept,
                &|| {
                    calls.set(calls.get() + 1);
                    // A one-shot cancellation remains sticky through unwinding.
                    calls.get() == stop_at
                },
            );
            assert!(
                partial.is_none(),
                "checkpoint {stop_at} leaked partial evidence"
            );
            assert_eq!(calls.get(), stop_at, "interrupted work must not resume");
        }
        let reject = super::belief_domestic_trade_assessment_controlled(
            [(&state, 1.0)],
            &Action::RespondTrade { accept: false },
            &|| true,
        )
        .expect("rejecting a trade needs no tactical probe");
        assert!(!reject.hard_veto);
        assert_eq!(reject.threat, None);
    }

    #[test]
    fn memoized_probe_preserves_distinct_road_building_origins() {
        let mut state = GameState::standard(401, 3);
        state.phase = Phase::Main;
        state.buildings[0] = Some(Building::Settlement(0));
        state.players[0].resources = [0, 0, 0, 2, 3];
        state.players[0].development[DevCard::RoadBuilding.index()] = 1;
        let actions = state.legal_actions();
        let (first, second) = actions
            .iter()
            .find_map(|action| {
                if let Action::PlayRoadBuilding {
                    first,
                    second: Some(second),
                } = action
                {
                    actions
                        .contains(&Action::PlayRoadBuilding {
                            first: *second,
                            second: Some(*first),
                        })
                        .then_some((*first, *second))
                } else {
                    None
                }
            })
            .expect("two independently legal roads");
        let mut forward = state.clone();
        forward
            .apply(&Action::PlayRoadBuilding {
                first,
                second: Some(second),
            })
            .unwrap();
        let mut reverse = state.clone();
        reverse
            .apply(&Action::PlayRoadBuilding {
                first: second,
                second: Some(first),
            })
            .unwrap();
        assert_eq!(forward.state_hash(), reverse.state_hash());

        let fast = reachable_tactical_threats_with(
            &state,
            &state,
            1,
            0,
            None,
            &mut TacticalMemo::new(true),
        );
        let reference = reachable_tactical_threats_with(
            &state,
            &state,
            1,
            0,
            None,
            &mut TacticalMemo::new(false),
        );
        assert_eq!(fast.keys, reference.keys);
        assert_eq!(fast.non_progress_paths, reference.non_progress_paths);
        assert_eq!(fast.progress_paths, reference.progress_paths);
        for (first, second) in [(first, second), (second, first)] {
            assert_eq!(
                fast.progress_paths.get(&(
                    super::ThreatKey::CityBuild(0),
                    super::ProgressPath::Single(super::ProgressChoice::RoadBuilding {
                        first,
                        second: Some(second)
                    }),
                )),
                Some(&1.0)
            );
        }
    }

    #[test]
    fn memoized_probe_preserves_monopoly_counterfactual_against_pre_trade_baseline() {
        let (state, accept) = dirty_monopoly_response_state();
        let baseline = super::resolve_without_exchange(&state).unwrap();
        let after = super::resolved_exchange_states(&state, &accept, 0)
            .into_iter()
            .next()
            .unwrap();
        assert_ne!(after.state_hash(), baseline.state_hash());
        for penalty in [None, Some((colonist_catan_core::Resource::Ore, 1))] {
            let fast = reachable_tactical_threats_with(
                &after,
                &baseline,
                0,
                1,
                penalty,
                &mut TacticalMemo::new(true),
            );
            let reference = reachable_tactical_threats_with(
                &after,
                &baseline,
                0,
                1,
                penalty,
                &mut TacticalMemo::new(false),
            );
            assert_eq!(fast.keys, reference.keys);
            assert_eq!(fast.non_progress_paths, reference.non_progress_paths);
            assert_eq!(fast.progress_paths, reference.progress_paths);
            assert_eq!(
                fast.keys.contains_key(&super::ThreatKey::ImmediateWin),
                penalty.is_none()
            );
        }
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
