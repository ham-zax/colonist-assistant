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
    pub completed: bool,
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
    completed: bool,
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
            return PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completed: state.is_terminal() || state.current_player != self.root,
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
                completed: false,
            };
        }
        let result = match state.node_kind() {
            NodeKind::Terminal => PlanValue {
                value: self.endpoint_value(state),
                actions: Vec::new(),
                completed: true,
            },
            NodeKind::Chance => {
                let total = legal
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(1.0);
                let mut value = 0.0;
                let mut representative = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completed: false,
                };
                for action in legal {
                    if self.nodes >= self.node_limit {
                        break;
                    }
                    let probability = state.chance_weight(&action) as f32 / total;
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let child = self.visit(&next, depth + 1);
                    value += child.value * probability;
                    if child.value > representative.value {
                        representative = child;
                    }
                }
                PlanValue {
                    value,
                    actions: representative.actions,
                    completed: representative.completed,
                }
            }
            NodeKind::Decision { actor } if actor == self.root => {
                let ranked = rank_with_class_quotas(state, &legal, actor, self.config.branch_cap);
                let mut best = PlanValue {
                    value: f32::NEG_INFINITY,
                    actions: Vec::new(),
                    completed: false,
                };
                for (action, _) in ranked {
                    if self.nodes >= self.node_limit {
                        break;
                    }
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let mut child = self.visit(&next, depth + 1);
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
                        completed: false,
                    }
                }
            }
            NodeKind::Decision { actor } => {
                // The only non-root decisions inside a root player's turn are
                // domestic-trade replies. They are stochastic opponent-policy
                // outcomes, not adversarial omniscient choices.
                let mut weighted = 0.0;
                let mut mass = 0.0;
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
                for action in legal {
                    if self.nodes >= self.node_limit {
                        break;
                    }
                    let probability = match action {
                        Action::RespondTrade { accept: true } => accept_probability,
                        Action::RespondTrade { accept: false } => {
                            1.0 - accept_probability - counter_mass
                        }
                        Action::CounterTrade { .. } => counter_mass / counter_count.max(1) as f32,
                        _ => 1.0,
                    };
                    if probability <= 0.0 {
                        continue;
                    }
                    let mut next = state.clone();
                    if next.apply(&action).is_err() {
                        continue;
                    }
                    let child = self.visit(&next, depth + 1);
                    weighted += child.value * probability;
                    mass += probability;
                }
                PlanValue {
                    value: if mass > 0.0 {
                        weighted / mass
                    } else {
                        self.endpoint_value(state)
                    },
                    actions: Vec::new(),
                    completed: false,
                }
            }
        };
        // A budget-truncated value is only a lower-quality bound. Caching it
        // would let a later root action inherit an incomplete continuation
        // merely because it reached the same state after its fair budget slice.
        if result.completed {
            self.memo
                .insert((state.state_hash(), depth), result.clone());
        }
        result
    }
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
    let ranked = rank_with_class_quotas(state, &legal, root, config.root_cap);
    let mut planner = Planner {
        root,
        config,
        nodes: 0,
        node_limit: 0,
        memo: HashMap::new(),
    };
    let root_count = ranked.len().max(1) as u32;
    let per_root_budget = (planner.config.maximum_nodes / root_count)
        .max(48)
        .min(planner.config.maximum_nodes);
    let mut plans = Vec::new();
    for (action, _) in ranked {
        if planner.nodes >= planner.config.maximum_nodes {
            break;
        }
        // Give every root action an opportunity to expose its coherent
        // continuation. Without a local ceiling, the first broad action can
        // consume the entire budget and silently starve roads, trades, or
        // builds that appear later in the policy ordering.
        planner.node_limit = planner
            .nodes
            .saturating_add(per_root_budget)
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
            completed: result.completed,
        });
    }
    plans.sort_by(|left, right| right.value.total_cmp(&left.value));
    plans
}

pub(crate) fn plan_adjusted_priors(
    state: &GameState,
    ranked: &mut [(Action, f32)],
    maximum_nodes: u32,
) {
    if !matches!(state.phase, Phase::PreRoll | Phase::Main) || ranked.is_empty() {
        return;
    }
    let plans = plan_current_turn(
        state,
        TurnPlanConfig {
            maximum_nodes,
            root_cap: ranked.len().max(12),
            ..TurnPlanConfig::default()
        },
    );
    let Some(minimum) = plans.iter().map(|plan| plan.value).reduce(f32::min) else {
        return;
    };
    let Some(maximum) = plans.iter().map(|plan| plan.value).reduce(f32::max) else {
        return;
    };
    for (action, prior) in ranked.iter_mut() {
        if let Some(plan) = plans.iter().find(|plan| plan.first_action == *action) {
            let normalized = if maximum > minimum {
                (plan.value - minimum) / (maximum - minimum)
            } else {
                0.5
            };
            // Preserve a small policy prior while letting a coherent endpoint
            // dominate shallow inventory rewards.
            *prior = (*prior * 0.24 + 0.04 + normalized * 0.72).max(0.0001);
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
