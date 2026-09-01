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
}

struct Planner {
    root: u8,
    config: TurnPlanConfig,
    nodes: u32,
    node_limit: u32,
    memo: HashMap<(u64, u8), PlanValue>,
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

    fn visit(&mut self, state: &GameState, depth: u8) -> PlanValue {
        if self.nodes >= self.node_limit
            || depth >= self.config.maximum_actions
            || state.is_terminal()
            || (state.current_player != self.root && !matches!(state.phase, Phase::TradeResponses))
        {
            let completed_endpoint = state.is_terminal()
                || (state.current_player != self.root
                    && !matches!(state.phase, Phase::TradeResponses));
            return PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: if completed_endpoint { 1.0 } else { 0.0 },
            };
        }
        self.nodes += 1;
        if let Some(value) = self.memo.get(&(state.state_hash(), depth)) {
            return value.clone();
        }
        let legal = state.legal_actions();
        if legal.is_empty() {
            return PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: 0.0,
            };
        }
        let result = match state.node_kind() {
            NodeKind::Terminal => PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completion_mass: 1.0,
            },
            NodeKind::Chance => {
                let total = legal
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(1.0);
                let mut value = 0.0;
                let mut completion_mass = 0.0;
                let mut representative = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
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
                        self.visit(&next, depth + 1)
                    } else {
                        PlanValue {
                            value: self.endpoint_value(&next),
                            actions: Vec::new(),
                            completion_mass: 0.0,
                        }
                    };
                    value += child.value * probability;
                    completion_mass += child.completion_mass * probability;
                    if child.value > representative.value {
                        representative = child;
                    }
                }
                PlanValue {
                    value,
                    actions: representative.actions,
                    completion_mass: completion_mass.clamp(0.0, 1.0),
                }
            }
            NodeKind::Decision { actor } if actor == self.root => {
                let ranked = rank_with_class_quotas(state, &legal, actor, self.config.branch_cap);
                let mut best = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
                };
                for (action, _) in ranked {
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let mut child = if self.nodes < self.node_limit {
                        self.visit(&next, depth + 1)
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
                        }
                    } else {
                        continue;
                    };
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
                    }
                }
            }
            NodeKind::Decision { actor } => {
                // The only non-root decisions inside a root player's turn are
                // domestic-trade replies. They are stochastic opponent-policy
                // outcomes, not adversarial omniscient choices.
                let mut weighted = 0.0;
                let mut completion_mass = 0.0;
                let mut representative = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completion_mass: 0.0,
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
                let weighted_actions = legal
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
                        self.visit(&next, depth + 1)
                    } else {
                        PlanValue {
                            value: self.endpoint_value(&next),
                            actions: Vec::new(),
                            completion_mass: 0.0,
                        }
                    };
                    weighted += child.value * probability;
                    completion_mass += child.completion_mass * probability;
                    if child.value > representative.value {
                        representative = child;
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
                }
            }
        };
        // A budget-truncated value is only a lower-quality bound. Caching it
        // would let a later root action inherit an incomplete continuation
        // merely because it reached the same state after its fair budget slice.
        if result.completion_mass >= 1.0 - 1e-6 {
            self.memo
                .insert((state.state_hash(), depth), result.clone());
        }
        result
    }
}

fn is_domestic_trade(action: &Action) -> bool {
    matches!(action, Action::OfferTrade { .. } | Action::CounterTrade { .. })
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
    let planner_root_cap = quota_ranked.len().min(
        (config.maximum_nodes / per_root_floor.max(1))
            .max(1) as usize,
    );
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
    let total_prior = ranked
        .iter()
        .map(|(_, prior)| prior.max(0.0))
        .sum::<f32>();
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
        let mut result = planner.visit(&next, 1);
        result.actions.insert(0, action.clone());
        plans.push(TurnPlan {
            first_action: action,
            actions: result.actions,
            value: result.value,
            nodes: planner.nodes - before,
            completion_mass: result.completion_mass.clamp(0.0, 1.0),
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

    use super::{TurnPlanConfig, plan_current_turn};

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
    fn domestic_trade_planner_gets_completion_mass_under_live_belief_budget() {
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
            .find(|plan| {
                matches!(
                    plan.first_action,
                    Action::OfferTrade { give, receive, .. }
                        if give == [1, 1, 1, 0, 0]
                            && receive == [0, 0, 0, 1, 0]
                )
            })
            .expect("live planner allocation must retain the settlement-unlocking trade");
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
