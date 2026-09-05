use std::collections::HashMap;

use colonist_catan_core::{Action, GameState, NodeKind, Phase};

use crate::eval::{public_strategic_utility, strategic_utility};
use crate::policy::{rank_with_class_quotas, trade_acceptance_probability};

#[derive(Clone, Debug)]
pub struct TurnPlan {
    pub first_action: Action,
    pub actions: Vec<Action>,
    pub value: f32,
    pub nodes: u32,
    pub completion_mass: f32,
    /// Probability mass whose next material build/award/win occurs before
    /// control leaves the root player's turn.
    pub decisive_completion_mass: f32,
    /// Expected opponent decision windows before the next material
    /// build/award/win or, when the current-turn planner cannot reach one,
    /// before control returns to the root player. This is diagnostic only.
    pub response_windows: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct TurnPlanConfig {
    pub maximum_actions: u8,
    pub maximum_nodes: u32,
    pub branch_cap: usize,
    pub root_cap: usize,
}

impl Default for TurnPlanConfig {
    fn default() -> Self {
        Self {
            maximum_actions: 12,
            maximum_nodes: 4_000,
            branch_cap: 14,
            root_cap: 28,
        }
    }
}

#[derive(Clone)]
struct PlanValue {
    value: f32,
    actions: Vec<Action>,
    completion_mass: f32,
    decisive_completion_mass: f32,
    response_window_mass: f32,
    weighted_response_windows: f32,
}

struct Planner {
    root: u8,
    config: TurnPlanConfig,
    nodes: u32,
    node_limit: u32,
    memo: HashMap<(u64, u8, u8), PlanValue>,
}

impl Planner {
    fn endpoint_value(&self, state: &GameState) -> f32 {
        let own = strategic_utility(state, self.root);
        let strongest_opponent = (0..state.board.num_players)
            .filter(|player| *player != self.root)
            .map(|player| public_strategic_utility(state, player))
            .fold(f32::NEG_INFINITY, f32::max);
        own - strongest_opponent.max(0.0) * 0.24
    }

    fn opponent_turn_windows(&self, state: &GameState) -> f32 {
        if state.current_player == self.root {
            return 0.0;
        }
        let players = state.board.num_players.max(1);
        let mut player = state.current_player;
        let mut windows = 0u8;
        while player != self.root && windows < players {
            windows = windows.saturating_add(1);
            player = (player + 1) % players;
        }
        windows as f32
    }

    fn materially_decisive_transition(
        &self,
        before: &GameState,
        after: &GameState,
        action: &Action,
    ) -> bool {
        after.winner() == Some(self.root)
            || matches!(
                action,
                Action::BuildSettlement { .. } | Action::BuildCity { .. }
            )
            || (before.longest_road_holder != Some(self.root)
                && after.longest_road_holder == Some(self.root))
            || (before.largest_army_holder != Some(self.root)
                && after.largest_army_holder == Some(self.root))
    }

    fn visit(&mut self, state: &GameState, depth: u8, maritime_received: u8) -> PlanValue {
        if self.nodes >= self.node_limit
            || depth >= self.config.maximum_actions
            || state.is_terminal()
            || (state.current_player != self.root && !matches!(state.phase, Phase::TradeResponses))
        {
            let completed_endpoint = state.is_terminal()
                || (state.current_player != self.root
                    && !matches!(state.phase, Phase::TradeResponses));
            let completed_mass = if completed_endpoint { 1.0 } else { 0.0 };
            let terminal_win = state.is_terminal() && state.winner() == Some(self.root);
            return PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: completed_mass,
                decisive_completion_mass: if terminal_win { 1.0 } else { 0.0 },
                response_window_mass: completed_mass,
                weighted_response_windows: if completed_endpoint && !state.is_terminal() {
                    self.opponent_turn_windows(state)
                } else {
                    0.0
                },
            };
        }
        self.nodes += 1;
        if let Some(value) = self.memo.get(&(state.state_hash(), depth, maritime_received)) {
            return value.clone();
        }
        let legal = state.legal_actions();
        if legal.is_empty() {
            return PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: 0.0,
                decisive_completion_mass: 0.0,
                response_window_mass: 0.0,
                weighted_response_windows: 0.0,
            };
        }
        let result = match state.node_kind() {
            NodeKind::Terminal => PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: 1.0,
                decisive_completion_mass: if state.winner() == Some(self.root) {
                    1.0
                } else {
                    0.0
                },
                response_window_mass: 1.0,
                weighted_response_windows: 0.0,
            },
            NodeKind::Chance => {
                let total = legal
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(1.0);
                let mut value = 0.0;
                let mut completion_mass = 0.0;
                let mut decisive_completion_mass = 0.0;
                let mut response_window_mass = 0.0;
                let mut weighted_response_windows = 0.0;
                let mut representative = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
                    decisive_completion_mass: 0.0,
                    response_window_mass: 0.0,
                    weighted_response_windows: 0.0,
                };
                for action in legal {
                    let probability = state.chance_weight(&action) as f32 / total;
                    if probability <= 0.0 {
                        continue;
                    }
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let child = if self.nodes < self.node_limit {
                        self.visit(&next, depth + 1, next_maritime_mask(maritime_received, &action))
                    } else {
                        PlanValue {
                            value: self.endpoint_value(&next),
                            actions: Vec::new(),
                            completion_mass: 0.0,
                            decisive_completion_mass: 0.0,
                            response_window_mass: 0.0,
                            weighted_response_windows: 0.0,
                        }
                    };
                    value += child.value * probability;
                    completion_mass += child.completion_mass * probability;
                    decisive_completion_mass += child.decisive_completion_mass * probability;
                    response_window_mass += child.response_window_mass * probability;
                    weighted_response_windows += child.weighted_response_windows * probability;
                    if child.value > representative.value {
                        // Keep the actual chance/response transition in the
                        // representative line. Otherwise replay tries to build
                        // while still in the chance or negotiation phase.
                        representative = child;
                        representative.actions.insert(0, action);
                    }
                }
                PlanValue {
                    value,
                    actions: representative.actions,
                    completion_mass: completion_mass.clamp(0.0, 1.0),
                    decisive_completion_mass: decisive_completion_mass.clamp(0.0, 1.0),
                    response_window_mass: response_window_mass.clamp(0.0, 1.0),
                    weighted_response_windows,
                }
            }
            NodeKind::Decision { actor } if actor == self.root => {
                let ranked = rank_with_class_quotas(state, &legal, actor, self.config.branch_cap);
                let mut best = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
                    decisive_completion_mass: 0.0,
                    response_window_mass: 0.0,
                    weighted_response_windows: 0.0,
                };
                for (action, _) in ranked {
                    if matches!(&action, Action::MaritimeTrade { give, .. }
                        if maritime_received & (1 << give.index()) != 0) {
                        continue;
                    }
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let decisive_now = self.materially_decisive_transition(state, &next, &action);
                    let mut child = if self.nodes < self.node_limit {
                        self.visit(&next, depth + 1, next_maritime_mask(maritime_received, &action))
                    } else if next.is_terminal()
                        || (next.current_player != self.root
                            && !matches!(next.phase, Phase::TradeResponses))
                    {
                        // Recognizing a directly completed child does not spend
                        // another recursive node. This is the endpoint that the
                        // per-root response-path floor is designed to reach.
                        PlanValue {
                            value: self.endpoint_value(&next),
                            actions: Vec::new(),
                            completion_mass: 1.0,
                            decisive_completion_mass: if next.is_terminal()
                                && next.winner() == Some(self.root)
                            {
                                1.0
                            } else {
                                0.0
                            },
                            response_window_mass: 1.0,
                            weighted_response_windows: if next.is_terminal() {
                                0.0
                            } else {
                                self.opponent_turn_windows(&next)
                            },
                        }
                    } else {
                        continue;
                    };
                    if decisive_now {
                        child.decisive_completion_mass = 1.0;
                        child.response_window_mass = 1.0;
                        child.weighted_response_windows = 0.0;
                    }
                    if child.value > best.value {
                        child.actions.insert(0, action);
                        best = child;
                    }
                }
                if best.value.is_finite() {
                    best
                } else {
                    PlanValue {
                        value: self.endpoint_value(state),
                        actions: Vec::new(),
                        completion_mass: 0.0,
                        decisive_completion_mass: 0.0,
                        response_window_mass: 0.0,
                        weighted_response_windows: 0.0,
                    }
                }
            }
            NodeKind::Decision { actor } => {
                // The only non-root decisions inside a root player's turn are
                // domestic-trade replies. They are stochastic opponent-policy
                // outcomes, not adversarial omniscient choices.
                let mut weighted = 0.0;
                let mut completion_mass = 0.0;
                let mut decisive_completion_mass = 0.0;
                let mut response_window_mass = 0.0;
                let mut weighted_response_windows = 0.0;
                let mut representative = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
                    decisive_completion_mass: 0.0,
                    response_window_mass: 0.0,
                    weighted_response_windows: 0.0,
                };
                let accept_probability = trade_acceptance_probability(state, actor);
                let counter_count = legal
                    .iter()
                    .filter(|action| matches!(action, Action::CounterTrade { .. }))
                    .count();
                let counter_mass = if counter_count > 0 {
                    (1.0 - accept_probability) * 0.18
                } else {
                    0.0
                };
                let mut weighted_actions = legal
                    .into_iter()
                    .filter_map(|action| {
                        let probability = match action {
                            Action::RespondTrade { accept: true } => accept_probability,
                            Action::RespondTrade { accept: false } => {
                                1.0 - accept_probability - counter_mass
                            }
                            Action::CounterTrade { .. } => {
                                counter_mass / counter_count.max(1) as f32
                            }
                            _ => 1.0,
                        };
                        (probability > 0.0).then_some((action, probability))
                    })
                    .collect::<Vec<_>>();
                // A tiny live planner budget must reach at least one accepted
                // response path before spending the entire slice on the first
                // lexicographic decline/counter branch. Fully explored values
                // are unchanged because probabilities remain authoritative.
                weighted_actions.sort_by(|left, right| {
                    let priority = |action: &Action| match action {
                        Action::RespondTrade { accept: true } => 3u8,
                        Action::CounterTrade { .. } => 2,
                        Action::RespondTrade { accept: false } => 1,
                        _ => 0,
                    };
                    priority(&right.0)
                        .cmp(&priority(&left.0))
                        .then_with(|| right.1.total_cmp(&left.1))
                });
                let total_probability = weighted_actions
                    .iter()
                    .map(|(_, probability)| *probability)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                for (action, raw_probability) in weighted_actions {
                    let probability = raw_probability / total_probability;
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let child = if self.nodes < self.node_limit {
                        self.visit(&next, depth + 1, next_maritime_mask(maritime_received, &action))
                    } else {
                        PlanValue {
                            value: self.endpoint_value(&next),
                            actions: Vec::new(),
                            completion_mass: 0.0,
                            decisive_completion_mass: 0.0,
                            response_window_mass: 0.0,
                            weighted_response_windows: 0.0,
                        }
                    };
                    weighted += child.value * probability;
                    completion_mass += child.completion_mass * probability;
                    decisive_completion_mass += child.decisive_completion_mass * probability;
                    response_window_mass += child.response_window_mass * probability;
                    // Reaching this branch consumed one real opponent decision
                    // opportunity inside the root player's current turn.
                    weighted_response_windows += (child.weighted_response_windows
                        + child.response_window_mass)
                        * probability;
                    if child.value > representative.value {
                        // Keep the actual chance/response transition in the
                        // representative line. Otherwise replay tries to build
                        // while still in the chance or negotiation phase.
                        representative = child;
                        representative.actions.insert(0, action);
                    }
                }
                PlanValue {
                    value: if weighted.is_finite() {
                        weighted
                    } else {
                        self.endpoint_value(state)
                    },
                    actions: representative.actions,
                    completion_mass: completion_mass.clamp(0.0, 1.0),
                    decisive_completion_mass: decisive_completion_mass.clamp(0.0, 1.0),
                    response_window_mass: response_window_mass.clamp(0.0, 1.0),
                    weighted_response_windows,
                }
            }
        };
        // A budget-truncated value is only a lower-quality bound. Caching it
        // would let a later root action inherit an incomplete continuation
        // merely because it reached the same state after its fair budget slice.
        if self.nodes < self.node_limit {
            self.memo
                .insert((state.state_hash(), depth, maritime_received), result.clone());
        }
        result
    }
}

fn next_maritime_mask(previous: u8, action: &Action) -> u8 {
    match action {
        Action::MaritimeTrade { receive, .. } => previous | (1 << receive.index()),
        _ => 0,
    }
}

#[cfg(test)]
fn maritime_transshipment(first: &Action, second: &Action) -> bool {
    matches!(
        (first, second),
        (
            Action::MaritimeTrade { receive, .. },
            Action::MaritimeTrade { give, .. }
        ) if receive == give
    )
}

fn is_domestic_trade(action: &Action) -> bool {
    matches!(
        action,
        Action::OfferTrade { .. } | Action::CounterTrade { .. }
    )
}

fn retained_planner_roots(
    state: &GameState,
    legal: &[Action],
    root: u8,
    config: &TurnPlanConfig,
) -> Vec<(Action, f32)> {
    let quota_ranked = rank_with_class_quotas(state, legal, root, config.root_cap);
    if quota_ranked.is_empty() {
        return Vec::new();
    }
    let per_root_floor = state.board.num_players as u32 + 1;
    let planner_root_cap = quota_ranked
        .len()
        .min((config.maximum_nodes / per_root_floor.max(1)).max(1) as usize);
    let mut retained = Vec::with_capacity(planner_root_cap);
    if let Some(trade) = quota_ranked
        .iter()
        .find(|(action, _)| is_domestic_trade(action))
    {
        retained.push(trade.clone());
    }
    for candidate in quota_ranked {
        if retained.len() >= planner_root_cap {
            break;
        }
        if !retained.iter().any(|(action, _)| action == &candidate.0) {
            retained.push(candidate);
        }
    }
    retained
}

fn planner_root_budgets(
    ranked: &[(Action, f32)],
    maximum_nodes: u32,
    per_root_floor: u32,
) -> Vec<u32> {
    if ranked.is_empty() {
        return Vec::new();
    }
    let count = ranked.len() as u32;
    let floor = per_root_floor.min(maximum_nodes / count.max(1));
    let mut budgets = vec![floor; ranked.len()];
    let mut remaining = maximum_nodes.saturating_sub(floor.saturating_mul(count));
    if remaining == 0 {
        return budgets;
    }
    let total_prior = ranked.iter().map(|(_, prior)| prior.max(0.0)).sum::<f32>();
    if total_prior > f32::EPSILON {
        for (index, (_, prior)) in ranked.iter().enumerate() {
            if remaining == 0 {
                break;
            }
            let share = ((maximum_nodes.saturating_sub(floor.saturating_mul(count)) as f32)
                * prior.max(0.0)
                / total_prior)
                .floor() as u32;
            let granted = share.min(remaining);
            budgets[index] = budgets[index].saturating_add(granted);
            remaining -= granted;
        }
    }
    let mut index = 0usize;
    while remaining > 0 {
        budgets[index] = budgets[index].saturating_add(1);
        remaining -= 1;
        index = (index + 1) % budgets.len();
    }
    budgets
}

/// Enumerates complete current-turn endpoints. Search still executes one
/// state-validated action at a time, but roads, trades, development plays, and
/// builds receive the value of the best coherent continuation they enable.
#[cfg(test)]
fn contains_dominated_maritime_sequence(state: &GameState, actions: &[Action]) -> bool {
    let root = state.actor();
    let mut cursor = state.clone();
    let mut maritime_received = 0u8;
    let mut prior_hands = vec![cursor.players[root as usize].resources];
    for action in actions {
        if matches!(action, Action::MaritimeTrade { give, .. }
            if maritime_received & (1 << give.index()) != 0) {
            return true;
        }
        maritime_received = next_maritime_mask(maritime_received, action);
        if cursor.apply(action).is_err() {
            return true;
        }
        if matches!(action, Action::MaritimeTrade { .. }) {
            let hand = cursor.players[root as usize].resources;
            if prior_hands.iter().any(|prior| {
                prior.iter().zip(hand).all(|(before, after)| *before >= after)
                    && prior != &hand
            }) {
                return true;
            }
            prior_hands.push(hand);
        } else {
            // A build or other action changes the strategic state, so only
            // compare maritime transformations inside the next uninterrupted
            // conversion segment.
            prior_hands.clear();
            prior_hands.push(cursor.players[root as usize].resources);
        }
    }
    false
}

pub fn plan_current_turn(state: &GameState, config: TurnPlanConfig) -> Vec<TurnPlan> {
    if !matches!(state.phase, Phase::PreRoll | Phase::Main) {
        return Vec::new();
    }
    let root = state.actor();
    let legal = state.legal_actions();
    let ranked = retained_planner_roots(state, &legal, root, &config);
    let per_root_floor = state.board.num_players as u32 + 1;
    let budgets = planner_root_budgets(&ranked, config.maximum_nodes, per_root_floor);
    let mut planner = Planner {
        root,
        config,
        nodes: 0,
        node_limit: 0,
        memo: HashMap::new(),
    };
    let mut plans = Vec::new();
    for ((action, _), budget) in ranked.into_iter().zip(budgets) {
        if planner.nodes >= planner.config.maximum_nodes {
            break;
        }
        // Give every retained root the configured response-path floor, then
        // distribute remaining live-budget nodes by ranked prior. A reserved
        // domestic-trade representative therefore has enough room to traverse
        // recipient replies and reach a returned Main-phase continuation.
        planner.node_limit = planner
            .nodes
            .saturating_add(budget)
            .min(planner.config.maximum_nodes);
        let before = planner.nodes;
        let mut next = state.clone();
        if next.apply(&action).is_err() {
            continue;
        }
        let decisive_now = planner.materially_decisive_transition(state, &next, &action);
        let mut result = planner.visit(&next, 1, next_maritime_mask(0, &action));
        if decisive_now {
            result.decisive_completion_mass = 1.0;
            result.response_window_mass = 1.0;
            result.weighted_response_windows = 0.0;
        }
        result.actions.insert(0, action.clone());
        let response_windows = (result.response_window_mass > f32::EPSILON)
            .then_some((result.weighted_response_windows / result.response_window_mass).max(0.0));
        plans.push(TurnPlan {
            first_action: action,
            actions: result.actions,
            value: result.value,
            nodes: planner.nodes - before,
            completion_mass: result.completion_mass.clamp(0.0, 1.0),
            decisive_completion_mass: result.decisive_completion_mass.clamp(0.0, 1.0),
            response_windows,
        });
    }
    plans.sort_by(|left, right| right.value.total_cmp(&left.value));
    plans
}

pub(crate) fn plan_adjusted_priors_with_plans(
    state: &GameState,
    ranked: &mut [(Action, f32)],
    maximum_nodes: u32,
) -> Vec<TurnPlan> {
    if !matches!(state.phase, Phase::PreRoll | Phase::Main) || ranked.is_empty() {
        return Vec::new();
    }
    let plans = plan_current_turn(
        state,
        TurnPlanConfig {
            maximum_nodes,
            // Preserve semantic-family representatives, but do not spread a
            // tiny live prior budget over every parameterized road/trade.
            root_cap: ranked.len().min(14),
            ..TurnPlanConfig::default()
        },
    );
    let completed = plans
        .iter()
        .filter(|plan| plan.completion_mass > 0.0)
        .collect::<Vec<_>>();
    let Some(minimum) = completed.iter().map(|plan| plan.value).reduce(f32::min) else {
        return plans;
    };
    let Some(maximum) = completed.iter().map(|plan| plan.value).reduce(f32::max) else {
        return plans;
    };
    for (action, prior) in ranked.iter_mut() {
        if let Some(plan) = completed.iter().find(|plan| plan.first_action == *action) {
            let normalized = if maximum > minimum {
                (plan.value - minimum) / (maximum - minimum)
            } else {
                0.5
            };
            // Scale the configured planner influence by the probability mass
            // that actually reached a coherent current-turn endpoint.
            let blend = 0.60 * plan.completion_mass.clamp(0.0, 1.0);
            *prior = (*prior * (1.0 - blend) + normalized * blend).max(0.0001);
        }
    }
    let total = ranked
        .iter()
        .map(|(_, prior)| *prior)
        .sum::<f32>()
        .max(0.0001);
    for (_, prior) in ranked.iter_mut() {
        *prior /= total;
    }
    ranked.sort_by(|left, right| right.1.total_cmp(&left.1));
    plans
}

pub(crate) fn plan_adjusted_priors(
    state: &GameState,
    ranked: &mut [(Action, f32)],
    maximum_nodes: u32,
) {
    let _ = plan_adjusted_priors_with_plans(state, ranked, maximum_nodes);
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase, Resource};

    use super::{TurnPlanConfig, maritime_transshipment, plan_current_turn};

    fn main_state_with_settlement_frontier(seed: u64) -> GameState {
        let mut state = GameState::standard(seed, 2);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.player_trades_enabled = false;
        state.domestic_trade_disabled = 1;
        state.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        state.bank = [19; 5];
        let mut frontier = vec![state];
        for _ in 0..=4 {
            let mut next_frontier = Vec::new();
            for mut candidate in frontier {
                candidate.players[0].resources = [20; 5];
                if candidate
                    .legal_actions()
                    .iter()
                    .any(|action| matches!(action, Action::BuildSettlement { .. }))
                {
                    candidate.players[0].resources = [0; 5];
                    candidate.bank = [19; 5];
                    return candidate;
                }
                for road in candidate
                    .legal_actions()
                    .into_iter()
                    .filter(|action| matches!(action, Action::BuildRoad { .. }))
                {
                    let mut next = candidate.clone();
                    if next.apply(&road).is_ok() {
                        next_frontier.push(next);
                    }
                }
            }
            frontier = next_frontier;
        }
        panic!("the fixture must expose an immediate settlement site");
    }

    fn set_root_hand(state: &mut GameState, hand: [u8; 5]) {
        state.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        state.bank = std::array::from_fn(|resource| 19 - hand[resource]);
        state.players[0].resources = hand;
    }

    #[test]
    fn road_and_settlement_are_evaluated_as_one_turn_plan() {
        let mut state = GameState::standard(7, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        for resource in Resource::ALL {
            let amount = if matches!(resource, Resource::Ore) {
                0
            } else {
                2
            };
            state.bank[resource.index()] -= amount;
            state.players[0].resources[resource.index()] += amount;
        }
        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 2_000,
                ..TurnPlanConfig::default()
            },
        );
        assert!(!plans.is_empty());
        assert!(plans.iter().any(|plan| {
            matches!(plan.first_action, Action::BuildRoad { .. })
                && plan
                    .actions
                    .iter()
                    .any(|action| matches!(action, Action::BuildSettlement { .. }))
        }));
    }

    #[test]
    fn settlement_one_road_away_is_ranked_as_a_complete_conversion() {
        let mut state = GameState::standard(7, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [2, 2, 1, 1, 1];
        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 5_000,
                ..TurnPlanConfig::default()
            },
        );
        let best_expansion = plans.iter().find(|plan| {
            matches!(plan.first_action, Action::BuildRoad { .. })
                && plan
                    .actions
                    .iter()
                    .any(|action| matches!(action, Action::BuildSettlement { .. }))
        });
        let development = plans
            .iter()
            .find(|plan| matches!(plan.first_action, Action::BuyDevelopment));
        assert!(best_expansion.is_some());
        assert!(
            development
                .is_none_or(|development| { best_expansion.unwrap().value > development.value }),
            "a live road-to-settlement conversion must beat inventory-only dev value",
        );
    }

    #[test]
    fn domestic_trade_planner_gets_response_completion_mass_under_live_belief_budget() {
        let mut state = GameState::standard(211, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [2, 2, 2, 0, 0];
        for opponent in 1..4 {
            state.players[opponent].resources = [0, 0, 0, 2, 0];
        }

        let live_planner_nodes = (4_000_u32 / 12).clamp(300, 4_000);
        let per_particle_nodes = (live_planner_nodes / 12).max(1);
        assert_eq!(per_particle_nodes, 27);
        let live = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: per_particle_nodes,
                root_cap: 14,
                ..TurnPlanConfig::default()
            },
        );
        let trade = live
            .iter()
            .filter(|plan| matches!(plan.first_action, Action::OfferTrade { .. }))
            .max_by(|left, right| left.completion_mass.total_cmp(&right.completion_mass))
            .expect("live planner allocation must retain a material domestic-trade root");
        assert!(trade.completion_mass > 0.0);
        assert!(trade.completion_mass <= 1.0);
        assert!(live.iter().map(|plan| plan.nodes).sum::<u32>() <= per_particle_nodes);

        let expanded = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: per_particle_nodes * 2,
                root_cap: 14,
                ..TurnPlanConfig::default()
            },
        );
        let same_trade = expanded
            .iter()
            .find(|plan| plan.first_action == trade.first_action)
            .expect("increasing planner budget must retain the live trade root");
        assert!(same_trade.completion_mass + 1e-6 >= trade.completion_mass);
    }

    #[test]
    fn closeout_diagnostics_distinguish_same_turn_completion_from_response_windows() {
        let mut state = GameState::standard(421, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 0, 0, 2, 3];

        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 4_000,
                root_cap: 64,
                ..TurnPlanConfig::default()
            },
        );
        let city = plans
            .iter()
            .find(|plan| matches!(plan.first_action, Action::BuildCity { .. }))
            .expect("city conversion must be planned");
        assert!(city.decisive_completion_mass > 0.99);
        assert_eq!(city.response_windows, Some(0.0));

        let end_turn = plans
            .iter()
            .find(|plan| matches!(plan.first_action, Action::EndTurn))
            .expect("end turn remains a planner root");
        assert!(end_turn.decisive_completion_mass <= 1e-6);
        assert_eq!(end_turn.response_windows, Some(2.0));
    }

    #[test]
    fn direct_maritime_trade_that_closes_a_settlement_is_retained() {
        let mut state = main_state_with_settlement_frontier(607);
        set_root_hand(&mut state, [1, 1, 0, 1, 4]);
        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 4_000,
                root_cap: 28,
                ..TurnPlanConfig::default()
            },
        );
        let closing_trade = plans
            .iter()
            .find(|plan| {
                matches!(
                    plan.first_action,
                    Action::MaritimeTrade {
                        give: Resource::Ore,
                        receive: Resource::Wool,
                        ratio: 4,
                    }
                ) && plan
                    .actions
                    .iter()
                    .any(|action| matches!(action, Action::BuildSettlement { .. }))
            })
            .expect("the one-trade settlement conversion must survive planning");
        assert!(closing_trade.completion_mass > 0.0);
        assert!(closing_trade.decisive_completion_mass > 0.99);
    }

    #[test]
    fn immediate_city_dominates_maritime_churn() {
        let mut state = main_state_with_settlement_frontier(613);
        set_root_hand(&mut state, [0, 0, 0, 3, 5]);
        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 4_000,
                root_cap: 28,
                ..TurnPlanConfig::default()
            },
        );
        assert!(matches!(
            plans.first().map(|plan| &plan.first_action),
            Some(Action::BuildCity { .. })
        ));
        assert!(plans.iter().all(|plan| {
            plan.actions
                .windows(2)
                .all(|actions| !maritime_transshipment(&actions[0], &actions[1]))
        }));
    }

    #[test]
    fn maritime_transshipment_matches_the_observed_ore_grain_wool_waste_chain() {
        let ore_to_grain = Action::MaritimeTrade {
            give: Resource::Ore,
            receive: Resource::Grain,
            ratio: 4,
        };
        let grain_to_wool = Action::MaritimeTrade {
            give: Resource::Grain,
            receive: Resource::Wool,
            ratio: 4,
        };
        assert!(maritime_transshipment(&ore_to_grain, &grain_to_wool));
    }

    #[test]
    fn twelve_cards_with_legal_conversions_do_not_end_the_turn() {
        let mut state = GameState::standard(13, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [3, 3, 2, 2, 2];
        let plans = plan_current_turn(
            &state,
            TurnPlanConfig {
                maximum_nodes: 4_000,
                ..TurnPlanConfig::default()
            },
        );
        assert!(!matches!(
            plans.first().map(|plan| &plan.first_action),
            Some(Action::EndTurn)
        ));
    }
}

#[cfg(test)]
mod maritime_dominance_tests {
    use super::*;
    use colonist_catan_core::{Phase, Resource};

    fn main_state() -> GameState {
        let mut state = GameState::standard(7, 2);
        while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).expect("legal setup action");
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state
    }

    #[test]
    fn detects_reselling_a_card_acquired_in_the_same_maritime_segment() {
        let mut state = main_state();
        let old = state.players[0].resources;
        let hand = [0, 0, 0, 3, 5];
        for resource in 0..5 {
            state.bank[resource] = state.bank[resource].saturating_add(old[resource]);
            state.bank[resource] = state.bank[resource].saturating_sub(hand[resource]);
        }
        state.players[0].resources = hand;
        let actions = [
            Action::MaritimeTrade {
                give: Resource::Ore,
                receive: Resource::Grain,
                ratio: 4,
            },
            Action::MaritimeTrade {
                give: Resource::Grain,
                receive: Resource::Wool,
                ratio: 4,
            },
        ];

        assert!(contains_dominated_maritime_sequence(&state, &actions));
    }

    #[test]
    fn detects_a_card_destroying_inverse_maritime_segment() {
        let mut state = main_state();
        let old = state.players[0].resources;
        let hand = [0, 0, 0, 3, 5];
        for resource in 0..5 {
            state.bank[resource] = state.bank[resource].saturating_add(old[resource]);
            state.bank[resource] = state.bank[resource].saturating_sub(hand[resource]);
        }
        state.players[0].resources = hand;
        let actions = [
            Action::MaritimeTrade {
                give: Resource::Ore,
                receive: Resource::Grain,
                ratio: 4,
            },
            Action::MaritimeTrade {
                give: Resource::Grain,
                receive: Resource::Ore,
                ratio: 4,
            },
        ];

        assert!(contains_dominated_maritime_sequence(&state, &actions));
    }
}
