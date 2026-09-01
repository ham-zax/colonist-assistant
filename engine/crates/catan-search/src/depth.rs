use colonist_catan_core::{Action, GameState, NodeKind, Phase};

use crate::deadline::CooperativeDeadline;
use crate::eval::evaluate;
use crate::exact::{ExactActionFamily, solve_exact_belief};
use crate::mcts::BeliefParticle;
use crate::opening::opening_adjusted_priors;
use crate::opening::{OpeningConfig, solve_opening};
use crate::planner::plan_adjusted_priors;
use crate::policy::{
    allocate_root_node_budgets, normalize_observed_priors, normalize_priors,
    order_scored_with_state_quotas, rank_with_class_quotas, truncate_root_preserving_end_turn,
};
use crate::shared::{coalesce_identical_particles, select_experimental_strategic_particles};
use crate::threats::{forced_loss_weight, posterior_immediate_threat_weight};
use crate::trade_safety::belief_domestic_trade_threat;

// Convenience APIs must remain safe in UI/tests. Production callers that
// explicitly want a larger budget use the `_bounded` variants.
const DEFAULT_DEPTH_NODE_BUDGET: u32 = 8_000;
const DOMESTIC_OFFER_FRICTION: f32 = 0.006;
const COUNTEROFFER_FRICTION: f32 = 0.004;

#[derive(Clone, Debug)]
pub struct DepthActionValue {
    pub action: Action,
    pub value: [f32; 4],
    pub legal_weight: f32,
    pub lower_confidence_value: [f32; 4],
}

#[derive(Clone, Debug)]
pub struct DepthSearchResult {
    pub chosen: Option<Action>,
    pub value: [f32; 4],
    pub actions: Vec<DepthActionValue>,
    pub nodes: u32,
    pub cutoffs: u32,
    pub depth: u8,
    pub deadline_reached: bool,
}

#[derive(Clone, Debug)]
pub struct BeliefDepthResult {
    pub chosen: Option<Action>,
    pub value: [f32; 4],
    pub actions: Vec<DepthActionValue>,
    pub nodes: u32,
    pub cutoffs: u32,
    pub depth: u8,
    /// Exact-distinct states actually searched after lossless coalescing.
    pub particles: usize,
    /// Weighted particles supplied to this Rust belief search before coalescing.
    pub posterior_particles: usize,
    pub deadline_reached: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BeliefDepthConfig {
    pub maximum_depth: u8,
    pub branch_cap: usize,
    pub maximum_nodes: u32,
    pub time_budget_ms: u32,
    /// Finite values opt into the legacy lossy coreset for arena/benchmark
    /// experiments. Production bounded-search entry points always use
    /// `usize::MAX`, leaving only exact-identical coalescing active.
    pub strategic_particle_limit: usize,
}

impl BeliefDepthConfig {
    fn normalized(self) -> Self {
        Self {
            maximum_depth: self.maximum_depth,
            branch_cap: self.branch_cap.max(1),
            maximum_nodes: self.maximum_nodes.max(1),
            time_budget_ms: self.time_budget_ms,
            strategic_particle_limit: self.strategic_particle_limit.max(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepthBeliefError {
    Empty,
    PublicStateMismatch,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaTimeBudgetUnsupported,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaOpeningUnsupported,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaEvaluationFailed,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaBatchLengthMismatch,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaEvaluatorLockPoisoned,
}

#[derive(Clone, Copy)]
enum Algorithm {
    MaxN,
    Paranoid { root: u8 },
}

fn apply_action_friction(value: &mut [f32; 4], state: &GameState, action: &Action, actor: u8) {
    let (base, give, receive) = match action {
        Action::OfferTrade { give, receive, .. } => {
            (DOMESTIC_OFFER_FRICTION, Some(give), Some(receive))
        }
        Action::CounterTrade { give, receive } => {
            (COUNTEROFFER_FRICTION, Some(give), Some(receive))
        }
        _ => (0.0, None, None),
    };
    let is_hand_safety_conversion = give.zip(receive).is_some_and(|(give, receive)| {
        state.players[actor as usize].resource_total() > state.card_discard_limit
            && give.iter().sum::<u8>() > receive.iter().sum::<u8>()
    });
    let friction = if is_hand_safety_conversion {
        base * 0.25
    } else {
        base
    };
    value[actor as usize] = (value[actor as usize] - friction).max(0.0);
}

fn allocate_weighted_node_budgets(weights: &[f32], total_nodes: u32) -> Vec<u32> {
    if weights.is_empty() {
        return Vec::new();
    }
    let mut budgets = vec![0_u32; weights.len()];
    if total_nodes == 0 {
        return budgets;
    }
    let mut order = (0..weights.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        weights[*right]
            .max(0.0)
            .total_cmp(&weights[*left].max(0.0))
            .then_with(|| left.cmp(right))
    });
    let guaranteed = (total_nodes as usize).min(weights.len());
    for index in order.iter().take(guaranteed) {
        budgets[*index] = 1;
    }
    let mut remaining = total_nodes.saturating_sub(guaranteed as u32);
    if remaining == 0 {
        return budgets;
    }
    let total_weight = weights
        .iter()
        .map(|weight| weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let distributable = remaining;
    for (index, weight) in weights.iter().enumerate() {
        if remaining == 0 {
            break;
        }
        let share = ((distributable as f32) * weight.max(0.0) / total_weight).floor() as u32;
        let granted = share.min(remaining);
        budgets[index] = budgets[index].saturating_add(granted);
        remaining -= granted;
    }
    let mut cursor = 0usize;
    while remaining > 0 {
        let index = order[cursor % order.len()];
        budgets[index] = budgets[index].saturating_add(1);
        remaining -= 1;
        cursor += 1;
    }
    budgets
}

fn canonicalize_equal_prior_siblings(ranked: &mut [(Action, f32)]) {
    let mut start = 0usize;
    while start < ranked.len() {
        let prior = ranked[start].1;
        let mut end = start + 1;
        while end < ranked.len() && (ranked[end].1 - prior).abs() <= 1e-9 {
            end += 1;
        }
        ranked[start..end].sort_by(|left, right| {
            format!("{:?}", left.0).cmp(&format!("{:?}", right.0))
        });
        start = end;
    }
}

fn recursive_observation_policy(
    state: &GameState,
    actions: &[Action],
    actor: u8,
    branch_cap: usize,
) -> Vec<(Action, f32)> {
    let observed = state.observed_state(actor);
    let observed_ranked = normalize_observed_priors(state, actions, actor);
    let mut ranked = order_scored_with_state_quotas(&observed, actor, observed_ranked);
    ranked = truncate_root_preserving_end_turn(ranked, branch_cap.max(1));
    if ranked.is_empty() {
        ranked = actions
            .iter()
            .cloned()
            .map(|action| (action, 1.0))
            .collect();
    }
    canonicalize_equal_prior_siblings(&mut ranked);
    ranked.truncate(3.min(ranked.len()));
    let mass = ranked
        .iter()
        .map(|(_, prior)| prior.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    for (_, prior) in &mut ranked {
        *prior = prior.max(0.0) / mass;
    }
    ranked
}

struct Searcher {
    algorithm: Algorithm,
    maximum_depth: u8,
    maximum_nodes: u32,
    node_limit: u32,
    branch_cap: usize,
    nodes: u32,
    cutoffs: u32,
    deepest_depth: u8,
    deadline: CooperativeDeadline,
    deadline_reached: bool,
    /// Weighted-belief production search must select every recursive action
    /// distribution from the acting player's information set. Perfect-
    /// information diagnostic search keeps this false intentionally.
    observation_safe_recursive: bool,
}

fn normalize_belief_root_priors(
    particles: &[BeliefParticle],
    actor: u8,
    planner_nodes: u32,
) -> Vec<(Action, f32)> {
    struct Aggregate {
        action: Action,
        prior: f32,
        quota_score: f32,
    }

    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let positive_particles = particles
        .iter()
        .filter(|particle| particle.weight > 0.0)
        .count()
        .max(1) as u32;
    let per_particle_planner_nodes = (planner_nodes / positive_particles).max(1);
    let mut aggregate = Vec::<Aggregate>::new();

    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        let legal = particle.state.legal_actions();
        let mut ranked = normalize_priors(&particle.state, &legal, actor);
        plan_adjusted_priors(&particle.state, &mut ranked, per_particle_planner_nodes);
        let ordered = order_scored_with_state_quotas(&particle.state, actor, ranked);
        let rank_scale = ordered.len().max(1) as f32;
        for (position, (action, prior)) in ordered.into_iter().enumerate() {
            let quota_score = (rank_scale - position as f32) / rank_scale;
            if let Some(existing) = aggregate
                .iter_mut()
                .find(|candidate| candidate.action == action)
            {
                existing.prior += prior * weight;
                existing.quota_score += quota_score * weight;
            } else {
                aggregate.push(Aggregate {
                    action,
                    prior: prior * weight,
                    quota_score: quota_score * weight,
                });
            }
        }
    }

    let prior_mass = aggregate
        .iter()
        .map(|candidate| candidate.prior)
        .sum::<f32>()
        .max(f32::EPSILON);
    for candidate in &mut aggregate {
        candidate.prior /= prior_mass;
    }
    aggregate.sort_by(|left, right| {
        right
            .quota_score
            .total_cmp(&left.quota_score)
            .then_with(|| right.prior.total_cmp(&left.prior))
            .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
    });
    aggregate
        .into_iter()
        .map(|candidate| (candidate.action, candidate.prior))
        .collect()
}

impl Searcher {
    fn visit_ranked_decision(
        &mut self,
        state: &GameState,
        actor: u8,
        mut ranked: Vec<(Action, f32)>,
        depth: u8,
        actions_in_turn: u8,
        mut alpha: f32,
        mut beta: f32,
        subtree_limit: u32,
    ) -> ([f32; 4], Option<Action>) {
        canonicalize_equal_prior_siblings(&mut ranked);
        let remaining = subtree_limit.saturating_sub(self.nodes);
        if remaining == 0 || ranked.is_empty() {
            return (evaluate(state), None);
        }
        ranked.truncate(ranked.len().min(remaining as usize));
        let maximize_root = match self.algorithm {
            Algorithm::MaxN => true,
            Algorithm::Paranoid { root } => actor == root,
        };
        let component = match self.algorithm {
            Algorithm::MaxN => actor as usize,
            Algorithm::Paranoid { root } => root as usize,
        };
        let mut best = [0.0; 4];
        let mut chosen = None;
        let mut best_scalar = if maximize_root {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
        let budgets = allocate_root_node_budgets(ranked.len(), remaining);
        let mut carry = 0_u32;
        for (index, (action, _)) in ranked.into_iter().enumerate() {
            let allowance = budgets
                .get(index)
                .copied()
                .unwrap_or(0)
                .saturating_add(carry);
            let before = self.nodes;
            let child_limit = self
                .nodes
                .saturating_add(allowance)
                .min(subtree_limit);
            let mut next = state.clone();
            next.apply(&action)
                .expect("ranked depth-search action must transition");
            let completed_turn =
                next.turn != state.turn || next.current_player != state.current_player;
            let mut child = if allowance > 0 && self.nodes < child_limit {
                self.visit(
                    &next,
                    depth + u8::from(completed_turn),
                    if completed_turn {
                        0
                    } else {
                        actions_in_turn.saturating_add(1)
                    },
                    alpha,
                    beta,
                    child_limit,
                )
            } else {
                evaluate(&next)
            };
            let used = self.nodes.saturating_sub(before);
            carry = allowance.saturating_sub(used);
            if self.deadline_reached {
                return (evaluate(state), None);
            }
            apply_action_friction(&mut child, state, &action, actor);
            let scalar = child[component];
            let improves = if maximize_root {
                scalar > best_scalar
            } else {
                scalar < best_scalar
            };
            if improves {
                best_scalar = scalar;
                best = child;
                chosen = Some(action);
            }
            if let Algorithm::Paranoid { .. } = self.algorithm {
                if maximize_root {
                    alpha = alpha.max(best_scalar);
                } else {
                    beta = beta.min(best_scalar);
                }
                if beta <= alpha {
                    self.cutoffs += 1;
                    break;
                }
            }
        }
        if best_scalar.is_finite() {
            (best, chosen)
        } else {
            (evaluate(state), None)
        }
    }

    fn visit(
        &mut self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        alpha: f32,
        beta: f32,
        subtree_limit: u32,
    ) -> [f32; 4] {
        let subtree_limit = subtree_limit.min(self.node_limit).min(self.maximum_nodes);
        if self.nodes >= subtree_limit {
            return evaluate(state);
        }
        if self.deadline.expired_at_checkpoint(self.nodes, 8) {
            self.deadline_reached = true;
            return evaluate(state);
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() || depth >= self.maximum_depth || actions_in_turn >= 18 {
            return evaluate(state);
        }
        let actions = state.legal_actions();
        if actions.is_empty() {
            return evaluate(state);
        }
        match state.node_kind() {
            NodeKind::Terminal => evaluate(state),
            NodeKind::Chance => {
                let total = actions
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                let weighted_actions = actions
                    .into_iter()
                    .filter_map(|action| {
                        let weight = state.chance_weight(&action) as f32 / total;
                        (weight > 0.0).then_some((action, weight))
                    })
                    .collect::<Vec<_>>();
                let remaining = subtree_limit.saturating_sub(self.nodes);
                let weights = weighted_actions
                    .iter()
                    .map(|(_, weight)| *weight)
                    .collect::<Vec<_>>();
                let budgets = allocate_weighted_node_budgets(&weights, remaining);
                let mut carry = 0_u32;
                let mut expected = [0.0; 4];
                for (index, (action, weight)) in weighted_actions.into_iter().enumerate() {
                    let allowance = budgets.get(index).copied().unwrap_or(0).saturating_add(carry);
                    let before = self.nodes;
                    let child_limit = self
                        .nodes
                        .saturating_add(allowance)
                        .min(subtree_limit);
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("legal chance action must transition");
                    let child = if allowance > 0 && self.nodes < child_limit {
                        self.visit(
                            &next,
                            depth,
                            actions_in_turn.saturating_add(1),
                            alpha,
                            beta,
                            child_limit,
                        )
                    } else {
                        evaluate(&next)
                    };
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                    if self.deadline_reached {
                        return evaluate(state);
                    }
                    for player in 0..4 {
                        expected[player] += child[player] * weight;
                    }
                }
                expected
            }
            NodeKind::Decision { actor } => {
                // Legality from the exact particle; candidate ordering from the
                // acting player's observation only. Determinized MaxN still
                // evaluates leaves with exact hands, but opponents no longer
                // prioritize actions using third-party hidden identities.
                let remaining = subtree_limit.saturating_sub(self.nodes);
                if remaining == 0 {
                    return evaluate(state);
                }
                let observation_safe = self.observation_safe_recursive;
                let mut ranked = if observation_safe {
                    recursive_observation_policy(state, &actions, actor, self.branch_cap)
                } else {
                    let observed_ranked = normalize_observed_priors(state, &actions, actor);
                    let mut ranked = order_scored_with_state_quotas(
                        &state.observed_state(actor),
                        actor,
                        observed_ranked,
                    );
                    ranked = truncate_root_preserving_end_turn(ranked, self.branch_cap);
                    if ranked.is_empty() {
                        ranked = rank_with_class_quotas(state, &actions, actor, self.branch_cap);
                    }
                    ranked
                };
                ranked.truncate(ranked.len().min(remaining as usize));
                // Observation-safe opponents evaluate a prior-weighted mixture
                // over the top observation-ranked actions. The mixture depends
                // only on the actor's observation, so indistinguishable worlds
                // share one strategy while still covering more than a single
                // greedy prior line.
                if observation_safe {
                    let budgets = allocate_root_node_budgets(ranked.len(), remaining);
                    let mut carry = 0_u32;
                    let mut expected = [0.0_f32; 4];
                    for (index, (action, weight)) in ranked.iter().enumerate() {
                        if *weight <= 0.0 {
                            continue;
                        }
                        let allowance = budgets
                            .get(index)
                            .copied()
                            .unwrap_or(0)
                            .saturating_add(carry);
                        let before = self.nodes;
                        let child_limit = self
                            .nodes
                            .saturating_add(allowance)
                            .min(subtree_limit);
                        let mut next = state.clone();
                        next.apply(action)
                            .expect("observation-policy action must transition");
                        let completed_turn =
                            next.turn != state.turn || next.current_player != state.current_player;
                        let mut child = if allowance > 0 && self.nodes < child_limit {
                            self.visit(
                                &next,
                                depth + u8::from(completed_turn),
                                if completed_turn {
                                    0
                                } else {
                                    actions_in_turn.saturating_add(1)
                                },
                                alpha,
                                beta,
                                child_limit,
                            )
                        } else {
                            evaluate(&next)
                        };
                        let used = self.nodes.saturating_sub(before);
                        carry = allowance.saturating_sub(used);
                        if self.deadline_reached {
                            return evaluate(state);
                        }
                        apply_action_friction(&mut child, state, action, actor);
                        for player in 0..4 {
                            expected[player] += child[player] * *weight;
                        }
                    }
                    return expected;
                }
                self.visit_ranked_decision(
                    state,
                    actor,
                    ranked,
                    depth,
                    actions_in_turn,
                    alpha,
                    beta,
                    subtree_limit,
                )
                .0
            }
        }
    }

    fn root(&mut self, state: &GameState) -> DepthSearchResult {
        let NodeKind::Decision { actor } = state.node_kind() else {
            return DepthSearchResult {
                chosen: None,
                value: evaluate(state),
                actions: Vec::new(),
                nodes: 1,
                cutoffs: 0,
                depth: 0,
                deadline_reached: false,
            };
        };
        let actions = state.legal_actions();
        let mut ranked = normalize_priors(state, &actions, actor);
        plan_adjusted_priors(
            state,
            &mut ranked,
            (self.maximum_nodes / 12).clamp(300, 4_000),
        );
        opening_adjusted_priors(
            state,
            &mut ranked,
            (self.maximum_nodes / 5).clamp(1_000, 18_000),
        );
        let ranked = order_scored_with_state_quotas(state, actor, ranked);
        // Threat forcing is disabled until it aggregates over the posterior and
        // verifies that a candidate actually removes a winning continuation.
        let mut ranked = truncate_root_preserving_end_turn(ranked, self.branch_cap);
        let safe_ranked = ranked
            .iter()
            .filter(|(action, _)| {
                belief_domestic_trade_threat(std::iter::once((state, 1.0)), action).is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        if !safe_ranked.is_empty() {
            ranked = safe_ranked;
        }
        let root_budgets = allocate_root_node_budgets(ranked.len(), self.maximum_nodes);
        let component = match self.algorithm {
            Algorithm::MaxN => actor as usize,
            Algorithm::Paranoid { root } => root as usize,
        };
        let maximize = match self.algorithm {
            Algorithm::MaxN => true,
            Algorithm::Paranoid { root } => actor == root,
        };
        let mut chosen = None;
        let mut value = [0.0; 4];
        let mut action_values = Vec::new();
        let mut best = if maximize {
            f32::NEG_INFINITY
        } else {
            f32::INFINITY
        };
        let mut alpha: f32 = 0.0;
        let mut beta: f32 = 1.0;
        for (index, (action, _)) in ranked.into_iter().enumerate() {
            if self.nodes >= self.maximum_nodes {
                break;
            }
            let per_root_budget = root_budgets.get(index).copied().unwrap_or(32).max(32);
            self.node_limit = self
                .nodes
                .saturating_add(per_root_budget)
                .min(self.maximum_nodes);
            let mut next = state.clone();
            next.apply(&action)
                .expect("ranked root action must transition");
            let completed_turn =
                next.turn != state.turn || next.current_player != state.current_player;
            let mut child = if self.deadline.has_elapsed() {
                self.deadline_reached = true;
                evaluate(&next)
            } else {
                self.visit(
                    &next,
                    u8::from(completed_turn),
                    if completed_turn { 0 } else { 1 },
                    alpha,
                    beta,
                    self.node_limit,
                )
            };
            apply_action_friction(&mut child, state, &action, actor);
            let scalar = child[component];
            action_values.push(DepthActionValue {
                action: action.clone(),
                value: child,
                legal_weight: 1.0,
                lower_confidence_value: child,
            });
            let improves = if maximize {
                scalar > best
            } else {
                scalar < best
            };
            if improves {
                best = scalar;
                chosen = Some(action);
                value = child;
            }
            if let Algorithm::Paranoid { .. } = self.algorithm {
                if maximize {
                    alpha = alpha.max(best);
                } else {
                    beta = beta.min(best);
                }
            }
        }
        DepthSearchResult {
            chosen,
            value,
            actions: action_values,
            nodes: self.nodes,
            cutoffs: self.cutoffs,
            depth: self.deepest_depth,
            deadline_reached: self.deadline_reached,
        }
    }
}

fn belief_search(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    paranoid: bool,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let config = config.normalized();
    let maximum_depth = config.maximum_depth;
    let branch_cap = config.branch_cap;
    let maximum_nodes = config.maximum_nodes;
    let deadline = CooperativeDeadline::start(config.time_budget_ms);
    let Some(first_particle) = particles.first() else {
        return Err(DepthBeliefError::Empty);
    };
    let first = &first_particle.state;
    let observer = first.actor();
    let public = first.public_hash();
    if particles
        .iter()
        .any(|particle| particle.state.public_hash() != public)
    {
        return Err(DepthBeliefError::PublicStateMismatch);
    }
    if matches!(
        first.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        // Setup is a public sequential snake draft. Particle aggregation is
        // unnecessary here; evaluate the complete settlement/road horizon on
        // the shared board geometry with the dedicated opening solver.
        let report = solve_opening(
            first,
            observer,
            OpeningConfig {
                maximum_nodes: maximum_nodes.max(1),
                root_width: 24,
                opponent_width: 4,
                time_budget_ms: config.time_budget_ms,
                opponent_maximizes: true,
                ..OpeningConfig::default()
            },
        );
        let minimum = report
            .actions
            .iter()
            .map(|candidate| candidate.value)
            .reduce(f32::min)
            .unwrap_or(0.0);
        let maximum = report
            .actions
            .iter()
            .map(|candidate| candidate.value)
            .reduce(f32::max)
            .unwrap_or(minimum);
        let baseline = evaluate(first);
        let count = first.board.num_players as usize;
        let chosen = report.chosen.clone();
        let opening_nodes = report.nodes;
        let opening_complete = report.complete;
        let action_values = report
            .actions
            .into_iter()
            .map(|candidate| {
                let normalized = if maximum > minimum {
                    (candidate.value - minimum) / (maximum - minimum)
                } else {
                    0.5
                };
                let root_share = 0.20 + normalized * 0.65;
                let other_mass = baseline
                    .iter()
                    .enumerate()
                    .take(count)
                    .filter(|(player, _)| *player != observer as usize)
                    .map(|(_, value)| *value)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                let mut value = baseline;
                value[observer as usize] = root_share;
                for player in 0..count {
                    if player != observer as usize {
                        value[player] = baseline[player] / other_mass * (1.0 - root_share);
                    }
                }
                DepthActionValue {
                    action: candidate.action,
                    value,
                    legal_weight: 1.0,
                    lower_confidence_value: value,
                }
            })
            .collect::<Vec<_>>();
        let value = action_values
            .iter()
            .find(|candidate| Some(&candidate.action) == chosen.as_ref())
            .map(|candidate| candidate.value)
            .unwrap_or(baseline);
        return Ok(BeliefDepthResult {
            chosen,
            value,
            actions: action_values,
            nodes: opening_nodes,
            cutoffs: u32::from(!opening_complete),
            depth: first
                .board
                .num_players
                .saturating_mul(2)
                .saturating_sub(first.setup_step),
            particles: particles.len(),
            posterior_particles: particles.len(),
            deadline_reached: report.deadline_reached,
        });
    }
    // Preserve every distinct WASM world in production. Exact-identical states
    // may be merged losslessly by summing their weights; finite configured
    // limits are reserved for explicit arena/benchmark experiments.
    let posterior_particles = particles.len();
    let posterior = particles;
    let coalesced_storage = coalesce_identical_particles(particles);
    let coalesced = coalesced_storage.as_slice();
    let strategic_storage;
    let particles = if coalesced.len() > config.strategic_particle_limit {
        strategic_storage = select_experimental_strategic_particles(
            coalesced,
            config.strategic_particle_limit,
        );
        strategic_storage.as_slice()
    } else {
        coalesced
    };
    struct Aggregate {
        action: Action,
        value: [f32; 4],
        covered_weight: f32,
        legal_weight: f32,
        lower_bound: [f32; 4],
    }
    struct RowEntry {
        action: Action,
        value: [f32; 4],
        legal: bool,
    }
    let mut aggregate = Vec::<Aggregate>::new();
    let mut nodes = 0;
    let mut cutoffs = 0;
    let mut depth = 0;
    let mut particles_searched = 0;
    let mut deadline_reached = false;
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let planner_nodes = (maximum_nodes / 12).clamp(300, 4_000);
    let mut root_scored = normalize_belief_root_priors(particles, observer, planner_nodes);
    root_scored.retain(|(action, _)| !root_exclusions.contains(action));
    let immediate_threat_weight = posterior_immediate_threat_weight(
        posterior
            .iter()
            .map(|particle| (&particle.state, particle.weight)),
        observer,
    );
    let verified_blockers = if immediate_threat_weight > f32::EPSILON {
        root_scored
            .iter()
            .filter(|(action, _)| {
                forced_loss_weight(
                    posterior
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    observer,
                    action,
                ) + 1e-6
                    < immediate_threat_weight
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let ordinarily_retained = truncate_root_preserving_end_turn(root_scored, branch_cap);
    let mut retained = Vec::with_capacity(branch_cap.max(1));
    for candidate in verified_blockers.into_iter().chain(ordinarily_retained) {
        if retained.len() >= branch_cap.max(1) {
            break;
        }
        if !retained.iter().any(|(action, _)| action == &candidate.0) {
            retained.push(candidate);
        }
    }
    let mut root_actions = retained
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    // Monopoly's five resource parameters share one strategic family slot.
    // Pick that representative over the complete posterior before MaxN spends
    // its root budget; a production-based public prior cannot know which
    // accumulated resource the opponents are actually holding.
    if let Some(monopoly_slot) = root_actions
        .iter()
        .position(|action| matches!(action, Action::PlayMonopoly { .. }))
    {
        let fallback = root_actions[monopoly_slot].clone();
        let replacement = solve_exact_belief(particles, ExactActionFamily::Monopoly)
            .chosen
            .unwrap_or(fallback);
        root_actions.retain(|action| !matches!(action, Action::PlayMonopoly { .. }));
        root_actions.insert(monopoly_slot.min(root_actions.len()), replacement);
    }
    let mut unique_root_actions = Vec::with_capacity(root_actions.len());
    for action in root_actions {
        if !unique_root_actions.contains(&action) {
            unique_root_actions.push(action);
        }
    }
    let mut root_actions = unique_root_actions;
    debug_assert!(root_actions.iter().enumerate().all(|(index, action)| {
        root_actions
            .iter()
            .skip(index + 1)
            .all(|other| other != action)
    }));
    debug_assert_eq!(
        root_actions
            .iter()
            .filter(|action| matches!(action, Action::PlayMonopoly { .. }))
            .count(),
        usize::from(
            root_actions
                .iter()
                .any(|action| matches!(action, Action::PlayMonopoly { .. }))
        ),
    );
    let safe_root_actions = root_actions
        .iter()
        .filter(|action| {
            belief_domestic_trade_threat(
                particles
                    .iter()
                    .map(|particle| (&particle.state, particle.weight)),
                action,
            )
            .is_none()
        })
        .cloned()
        .collect::<Vec<_>>();
    if !safe_root_actions.is_empty() {
        root_actions = safe_root_actions;
    }
    // Concentrate nodes on the leading root actions instead of giving every
    // particle/action pair the same tiny equal slice. Uniform fairness left
    // live search with ~7 nodes/action at 32 particles × 16 actions.
    let action_budgets = allocate_root_node_budgets(
        root_actions.len(),
        maximum_nodes / particles.len().max(1) as u32,
    );
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        particles_searched += 1;
        let mut row = Vec::<RowEntry>::with_capacity(root_actions.len());
        let mut row_deadline = deadline.has_elapsed();
        deadline_reached |= row_deadline;
        for (action_index, action) in root_actions.iter().enumerate() {
            let mut next = particle.state.clone();
            if next.apply(action).is_err() {
                // An unavailable action retains the no-action baseline rather
                // than disappearing from the denominator.
                row.push(RowEntry {
                    action: action.clone(),
                    value: evaluate(&particle.state),
                    legal: false,
                });
                continue;
            }
            let completed_turn = next.turn != particle.state.turn
                || next.current_player != particle.state.current_player;
            let nodes_for_action = action_budgets
                .get(action_index)
                .copied()
                .unwrap_or(1)
                .max(1);
            let mut searcher = Searcher {
                algorithm: if paranoid {
                    Algorithm::Paranoid { root: observer }
                } else {
                    Algorithm::MaxN
                },
                maximum_depth,
                maximum_nodes: nodes_for_action,
                node_limit: nodes_for_action,
                branch_cap: branch_cap.max(1),
                nodes: 0,
                cutoffs: 0,
                deepest_depth: 0,
                deadline: deadline.clone(),
                deadline_reached: false,
                observation_safe_recursive: true,
            };
            let mut candidate_value = if row_deadline {
                evaluate(&next)
            } else if searcher.nodes < searcher.node_limit {
                searcher.visit(
                    &next,
                    u8::from(completed_turn),
                    if completed_turn { 0 } else { 1 },
                    0.0,
                    1.0,
                    searcher.node_limit,
                )
            } else {
                evaluate(&next)
            };
            apply_action_friction(&mut candidate_value, &particle.state, action, observer);
            nodes += searcher.nodes;
            cutoffs += searcher.cutoffs;
            depth = depth.max(searcher.deepest_depth);
            if searcher.deadline_reached || deadline.has_elapsed() {
                row_deadline = true;
                deadline_reached = true;
            }
            row.push(RowEntry {
                action: action.clone(),
                value: candidate_value,
                legal: true,
            });
        }
        if row_deadline {
            // Do not let an early action in one hidden world retain a deeper
            // value than later actions merely because the wall clock expired
            // midway through that world's row. Keep completed earlier worlds,
            // but reduce every legal action in this row to the same one-ply
            // fallback.
            for entry in &mut row {
                if !entry.legal {
                    continue;
                }
                let mut next = particle.state.clone();
                next.apply(&entry.action)
                    .expect("row entry was legal before deadline fallback");
                entry.value = evaluate(&next);
                apply_action_friction(&mut entry.value, &particle.state, &entry.action, observer);
            }
        }
        for entry in row {
            if let Some(existing) = aggregate
                .iter_mut()
                .find(|candidate| candidate.action == entry.action)
            {
                for (sum, value) in existing.value.iter_mut().zip(entry.value) {
                    *sum += value * weight;
                }
                existing.covered_weight += weight;
                if entry.legal {
                    existing.legal_weight += weight;
                }
                for (bound, value) in existing.lower_bound.iter_mut().zip(entry.value) {
                    *bound = bound.min(value);
                }
            } else {
                aggregate.push(Aggregate {
                    action: entry.action,
                    value: entry.value.map(|value| value * weight),
                    covered_weight: weight,
                    legal_weight: if entry.legal { weight } else { 0.0 },
                    lower_bound: entry.value,
                });
            }
        }
    }
    deadline_reached |= deadline.has_elapsed();
    let actor = observer as usize;
    let mut actions = aggregate
        .into_iter()
        .map(|entry| DepthActionValue {
            action: entry.action,
            value: entry
                .value
                .map(|value| value / entry.covered_weight.max(f32::EPSILON)),
            legal_weight: entry.legal_weight.clamp(0.0, 1.0),
            lower_confidence_value: entry.lower_bound,
        })
        .collect::<Vec<_>>();
    actions.sort_by(|left, right| right.value[actor].total_cmp(&left.value[actor]));
    let mut chosen_index = 0usize;
    if let Some(leading) = actions.first() {
        let leading_loss = forced_loss_weight(
            posterior
                .iter()
                .map(|particle| (&particle.state, particle.weight)),
            observer,
            &leading.action,
        );
        if leading_loss >= 1.0 - 1e-6
            && let Some(escape_index) = actions.iter().position(|candidate| {
                forced_loss_weight(
                    posterior
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    observer,
                    &candidate.action,
                ) <= 1e-6
            })
        {
            chosen_index = escape_index;
        }
    }
    let chosen = actions
        .get(chosen_index)
        .map(|entry| entry.action.clone());
    let value = actions
        .get(chosen_index)
        .map(|entry| entry.value)
        .unwrap_or_else(|| evaluate(first));
    Ok(BeliefDepthResult {
        chosen,
        value,
        actions,
        nodes,
        cutoffs,
        depth,
        particles: particles_searched,
        posterior_particles,
        deadline_reached,
    })
}

pub fn search_maxn(state: &GameState, depth: u8, branch_cap: usize) -> DepthSearchResult {
    search_maxn_bounded(state, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

fn public_opening_result(
    state: &GameState,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
    paranoid: bool,
) -> DepthSearchResult {
    let particle = BeliefParticle {
        state: state.clone(),
        weight: 1.0,
    };
    let report = belief_search(
        &[particle],
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            strategic_particle_limit: 1,
        },
        paranoid,
        &[],
    )
    .expect("one public setup state is a valid belief");
    DepthSearchResult {
        chosen: report.chosen,
        value: report.value,
        actions: report.actions,
        nodes: report.nodes,
        cutoffs: report.cutoffs,
        depth: report.depth,
        deadline_reached: report.deadline_reached,
    }
}

pub fn search_maxn_bounded(
    state: &GameState,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> DepthSearchResult {
    search_maxn_bounded_timed(state, depth, branch_cap, maximum_nodes, 0)
}

pub fn search_maxn_bounded_timed(
    state: &GameState,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
) -> DepthSearchResult {
    if matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return public_opening_result(
            state,
            depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            false,
        );
    }
    Searcher {
        algorithm: Algorithm::MaxN,
        maximum_depth: depth,
        maximum_nodes: maximum_nodes.max(1),
        node_limit: maximum_nodes.max(1),
        branch_cap: branch_cap.max(1),
        nodes: 0,
        cutoffs: 0,
        deepest_depth: 0,
        deadline: CooperativeDeadline::start(time_budget_ms),
        deadline_reached: false,
        observation_safe_recursive: false,
    }
    .root(state)
}

pub fn search_paranoid(
    state: &GameState,
    root: u8,
    depth: u8,
    branch_cap: usize,
) -> DepthSearchResult {
    search_paranoid_bounded(state, root, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

pub fn search_paranoid_bounded(
    state: &GameState,
    root: u8,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> DepthSearchResult {
    search_paranoid_bounded_timed(state, root, depth, branch_cap, maximum_nodes, 0)
}

pub fn search_paranoid_bounded_timed(
    state: &GameState,
    root: u8,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
) -> DepthSearchResult {
    if matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return public_opening_result(
            state,
            depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            true,
        );
    }
    Searcher {
        algorithm: Algorithm::Paranoid { root },
        maximum_depth: depth,
        maximum_nodes: maximum_nodes.max(1),
        node_limit: maximum_nodes.max(1),
        branch_cap: branch_cap.max(1),
        nodes: 0,
        cutoffs: 0,
        deepest_depth: 0,
        deadline: CooperativeDeadline::start(time_budget_ms),
        deadline_reached: false,
        observation_safe_recursive: false,
    }
    .root(state)
}

pub fn search_belief_maxn(
    particles: &[GameState],
    depth: u8,
    branch_cap: usize,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_belief_maxn_bounded(particles, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

pub fn search_belief_maxn_bounded(
    particles: &[GameState],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let weighted = particles
        .iter()
        .cloned()
        .map(|state| BeliefParticle { state, weight: 1.0 })
        .collect::<Vec<_>>();
    search_weighted_belief_maxn_with_config(
        &weighted,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms: 0,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_maxn_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, config, false, &[])
}

pub fn search_weighted_belief_maxn_bounded(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_weighted_belief_maxn_with_config(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms: 0,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_maxn_bounded_timed(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_weighted_belief_maxn_bounded_timed_excluding(
        particles,
        depth,
        branch_cap,
        maximum_nodes,
        time_budget_ms,
        &[],
    )
}

pub fn search_weighted_belief_maxn_bounded_timed_excluding(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            strategic_particle_limit: usize::MAX,
        },
        false,
        root_exclusions,
    )
}

pub fn search_belief_paranoid(
    particles: &[GameState],
    depth: u8,
    branch_cap: usize,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_belief_paranoid_bounded(particles, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

pub fn search_belief_paranoid_bounded(
    particles: &[GameState],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let weighted = particles
        .iter()
        .cloned()
        .map(|state| BeliefParticle { state, weight: 1.0 })
        .collect::<Vec<_>>();
    search_weighted_belief_paranoid_with_config(
        &weighted,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms: 0,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_paranoid_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, config, true, &[])
}

pub fn search_weighted_belief_paranoid_bounded(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_weighted_belief_paranoid_with_config(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms: 0,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_paranoid_bounded_timed(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_weighted_belief_paranoid_bounded_timed_excluding(
        particles,
        depth,
        branch_cap,
        maximum_nodes,
        time_budget_ms,
        &[],
    )
}

pub fn search_weighted_belief_paranoid_bounded_timed_excluding(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    time_budget_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            strategic_particle_limit: usize::MAX,
        },
        true,
        root_exclusions,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredChild {
    node: usize,
    weight: f32,
    friction: Option<(u8, f32)>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
enum CudaDeferredNode {
    Leaf(usize),
    Weighted(Vec<CudaDeferredChild>),
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredTree {
    nodes: Vec<CudaDeferredNode>,
    leaves: Vec<GameState>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaDeferredTree {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            leaves: Vec::new(),
        }
    }

    fn leaf(&mut self, state: &GameState) -> usize {
        let leaf = self.leaves.len();
        self.leaves.push(state.clone());
        let node = self.nodes.len();
        self.nodes.push(CudaDeferredNode::Leaf(leaf));
        node
    }

    fn weighted(&mut self, children: Vec<CudaDeferredChild>) -> usize {
        let node = self.nodes.len();
        self.nodes.push(CudaDeferredNode::Weighted(children));
        node
    }

    fn backup(&self, node: usize, leaf_values: &[[f32; 4]]) -> [f32; 4] {
        match &self.nodes[node] {
            CudaDeferredNode::Leaf(leaf) => leaf_values[*leaf],
            CudaDeferredNode::Weighted(children) => {
                let mut expected = [0.0; 4];
                for child in children {
                    let mut value = self.backup(child.node, leaf_values);
                    if let Some((actor, friction)) = child.friction {
                        value[actor as usize] = (value[actor as usize] - friction).max(0.0);
                    }
                    for player in 0..4 {
                        expected[player] += value[player] * child.weight;
                    }
                }
                expected
            }
        }
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_action_friction(state: &GameState, action: &Action, actor: u8) -> f32 {
    let (base, give, receive) = match action {
        Action::OfferTrade { give, receive, .. } => {
            (DOMESTIC_OFFER_FRICTION, Some(give), Some(receive))
        }
        Action::CounterTrade { give, receive } => {
            (COUNTEROFFER_FRICTION, Some(give), Some(receive))
        }
        _ => (0.0, None, None),
    };
    let is_hand_safety_conversion = give.zip(receive).is_some_and(|(give, receive)| {
        state.players[actor as usize].resource_total() > state.card_discard_limit
            && give.iter().sum::<u8>() > receive.iter().sum::<u8>()
    });
    if is_hand_safety_conversion {
        base * 0.25
    } else {
        base
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredSearcher<'a> {
    tree: &'a mut CudaDeferredTree,
    maximum_depth: u8,
    maximum_nodes: u32,
    node_limit: u32,
    branch_cap: usize,
    nodes: u32,
    deepest_depth: u8,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaDeferredSearcher<'_> {
    fn visit(
        &mut self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        subtree_limit: u32,
    ) -> usize {
        let subtree_limit = subtree_limit.min(self.node_limit).min(self.maximum_nodes);
        if self.nodes >= subtree_limit {
            return self.tree.leaf(state);
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() || depth >= self.maximum_depth || actions_in_turn >= 18 {
            return self.tree.leaf(state);
        }
        let actions = state.legal_actions();
        if actions.is_empty() {
            return self.tree.leaf(state);
        }
        match state.node_kind() {
            NodeKind::Terminal => self.tree.leaf(state),
            NodeKind::Chance => {
                let total = actions
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                let weighted_actions = actions
                    .into_iter()
                    .filter_map(|action| {
                        let weight = state.chance_weight(&action) as f32 / total;
                        (weight > 0.0).then_some((action, weight))
                    })
                    .collect::<Vec<_>>();
                let remaining = subtree_limit.saturating_sub(self.nodes);
                let weights = weighted_actions
                    .iter()
                    .map(|(_, weight)| *weight)
                    .collect::<Vec<_>>();
                let budgets = allocate_weighted_node_budgets(&weights, remaining);
                let mut carry = 0_u32;
                let mut children = Vec::with_capacity(weighted_actions.len());
                for (index, (action, weight)) in weighted_actions.into_iter().enumerate() {
                    let allowance = budgets
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(carry);
                    let before = self.nodes;
                    let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("legal chance action must transition");
                    let node = if allowance > 0 && self.nodes < child_limit {
                        self.visit(&next, depth, actions_in_turn.saturating_add(1), child_limit)
                    } else {
                        self.tree.leaf(&next)
                    };
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                    children.push(CudaDeferredChild {
                        node,
                        weight,
                        friction: None,
                    });
                }
                self.tree.weighted(children)
            }
            NodeKind::Decision { actor } => {
                let remaining = subtree_limit.saturating_sub(self.nodes);
                if remaining == 0 {
                    return self.tree.leaf(state);
                }
                let mut ranked =
                    recursive_observation_policy(state, &actions, actor, self.branch_cap);
                ranked.truncate(ranked.len().min(remaining as usize));
                let budgets = allocate_root_node_budgets(ranked.len(), remaining);
                let mut carry = 0_u32;
                let mut children = Vec::with_capacity(ranked.len());
                for (index, (action, weight)) in ranked.into_iter().enumerate() {
                    if weight <= 0.0 {
                        continue;
                    }
                    let allowance = budgets
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(carry);
                    let before = self.nodes;
                    let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("observation-policy action must transition");
                    let completed_turn =
                        next.turn != state.turn || next.current_player != state.current_player;
                    let node = if allowance > 0 && self.nodes < child_limit {
                        self.visit(
                            &next,
                            depth + u8::from(completed_turn),
                            if completed_turn {
                                0
                            } else {
                                actions_in_turn.saturating_add(1)
                            },
                            child_limit,
                        )
                    } else {
                        self.tree.leaf(&next)
                    };
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                    children.push(CudaDeferredChild {
                        node,
                        weight,
                        friction: Some((actor, cuda_action_friction(state, &action, actor))),
                    });
                }
                self.tree.weighted(children)
            }
        }
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredRootEntry {
    action: Action,
    node: usize,
    friction: f32,
    legal: bool,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredRootRow {
    weight: f32,
    entries: Vec<CudaDeferredRootEntry>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_belief_search_with_batch(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
    evaluate_batch: &mut dyn FnMut(&[GameState]) -> Result<Vec<[f32; 4]>, DepthBeliefError>,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let config = config.normalized();
    if config.time_budget_ms != 0 {
        return Err(DepthBeliefError::CudaTimeBudgetUnsupported);
    }
    let Some(first_particle) = particles.first() else {
        return Err(DepthBeliefError::Empty);
    };
    let first = &first_particle.state;
    let observer = first.actor();
    let public = first.public_hash();
    if particles
        .iter()
        .any(|particle| particle.state.public_hash() != public)
    {
        return Err(DepthBeliefError::PublicStateMismatch);
    }
    if matches!(
        first.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return Err(DepthBeliefError::CudaOpeningUnsupported);
    }

    let maximum_depth = config.maximum_depth;
    let branch_cap = config.branch_cap;
    let maximum_nodes = config.maximum_nodes;
    let posterior_particles = particles.len();
    let posterior = particles;
    let coalesced_storage = coalesce_identical_particles(particles);
    let coalesced = coalesced_storage.as_slice();
    let strategic_storage;
    let particles = if coalesced.len() > config.strategic_particle_limit {
        strategic_storage =
            select_experimental_strategic_particles(coalesced, config.strategic_particle_limit);
        strategic_storage.as_slice()
    } else {
        coalesced
    };

    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let planner_nodes = (maximum_nodes / 12).clamp(300, 4_000);
    let mut root_scored = normalize_belief_root_priors(particles, observer, planner_nodes);
    root_scored.retain(|(action, _)| !root_exclusions.contains(action));
    let immediate_threat_weight = posterior_immediate_threat_weight(
        posterior
            .iter()
            .map(|particle| (&particle.state, particle.weight)),
        observer,
    );
    let verified_blockers = if immediate_threat_weight > f32::EPSILON {
        root_scored
            .iter()
            .filter(|(action, _)| {
                forced_loss_weight(
                    posterior
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    observer,
                    action,
                ) + 1e-6
                    < immediate_threat_weight
            })
            .cloned()
            .collect::<Vec<_>>()
    } else {
        Vec::new()
    };
    let ordinarily_retained = truncate_root_preserving_end_turn(root_scored, branch_cap);
    let mut retained = Vec::with_capacity(branch_cap.max(1));
    for candidate in verified_blockers.into_iter().chain(ordinarily_retained) {
        if retained.len() >= branch_cap.max(1) {
            break;
        }
        if !retained.iter().any(|(action, _)| action == &candidate.0) {
            retained.push(candidate);
        }
    }
    let mut root_actions = retained
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    if let Some(monopoly_slot) = root_actions
        .iter()
        .position(|action| matches!(action, Action::PlayMonopoly { .. }))
    {
        let fallback = root_actions[monopoly_slot].clone();
        let replacement = solve_exact_belief(particles, ExactActionFamily::Monopoly)
            .chosen
            .unwrap_or(fallback);
        root_actions.retain(|action| !matches!(action, Action::PlayMonopoly { .. }));
        root_actions.insert(monopoly_slot.min(root_actions.len()), replacement);
    }
    let mut unique_root_actions = Vec::with_capacity(root_actions.len());
    for action in root_actions {
        if !unique_root_actions.contains(&action) {
            unique_root_actions.push(action);
        }
    }
    let mut root_actions = unique_root_actions;
    let safe_root_actions = root_actions
        .iter()
        .filter(|action| {
            belief_domestic_trade_threat(
                particles
                    .iter()
                    .map(|particle| (&particle.state, particle.weight)),
                action,
            )
            .is_none()
        })
        .cloned()
        .collect::<Vec<_>>();
    if !safe_root_actions.is_empty() {
        root_actions = safe_root_actions;
    }

    let action_budgets = allocate_root_node_budgets(
        root_actions.len(),
        maximum_nodes / particles.len().max(1) as u32,
    );
    let mut tree = CudaDeferredTree::new();
    let mut root_rows = Vec::new();
    let mut nodes = 0_u32;
    let mut depth = 0_u8;
    let mut particles_searched = 0_usize;
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        particles_searched += 1;
        let mut entries = Vec::with_capacity(root_actions.len());
        for (action_index, action) in root_actions.iter().enumerate() {
            let mut next = particle.state.clone();
            if next.apply(action).is_err() {
                entries.push(CudaDeferredRootEntry {
                    action: action.clone(),
                    node: tree.leaf(&particle.state),
                    friction: 0.0,
                    legal: false,
                });
                continue;
            }
            let completed_turn = next.turn != particle.state.turn
                || next.current_player != particle.state.current_player;
            let nodes_for_action = action_budgets
                .get(action_index)
                .copied()
                .unwrap_or(1)
                .max(1);
            let mut searcher = CudaDeferredSearcher {
                tree: &mut tree,
                maximum_depth,
                maximum_nodes: nodes_for_action,
                node_limit: nodes_for_action,
                branch_cap: branch_cap.max(1),
                nodes: 0,
                deepest_depth: 0,
            };
            let node_limit = searcher.node_limit;
            let node = searcher.visit(
                &next,
                u8::from(completed_turn),
                if completed_turn { 0 } else { 1 },
                node_limit,
            );
            nodes += searcher.nodes;
            depth = depth.max(searcher.deepest_depth);
            entries.push(CudaDeferredRootEntry {
                action: action.clone(),
                node,
                friction: cuda_action_friction(&particle.state, action, observer),
                legal: true,
            });
        }
        root_rows.push(CudaDeferredRootRow { weight, entries });
    }

    let fallback_node =
        if root_rows.is_empty() || root_rows.iter().all(|row| row.entries.is_empty()) {
            Some(tree.leaf(first))
        } else {
            None
        };
    let leaf_values = evaluate_batch(&tree.leaves)?;
    if leaf_values.len() != tree.leaves.len() {
        return Err(DepthBeliefError::CudaBatchLengthMismatch);
    }

    struct Aggregate {
        action: Action,
        value: [f32; 4],
        covered_weight: f32,
        legal_weight: f32,
        lower_bound: [f32; 4],
    }
    let mut aggregate = Vec::<Aggregate>::new();
    for row in root_rows {
        for entry in row.entries {
            let mut value = tree.backup(entry.node, &leaf_values);
            if entry.legal {
                value[observer as usize] = (value[observer as usize] - entry.friction).max(0.0);
            }
            if let Some(existing) = aggregate
                .iter_mut()
                .find(|candidate| candidate.action == entry.action)
            {
                for (sum, component) in existing.value.iter_mut().zip(value) {
                    *sum += component * row.weight;
                }
                existing.covered_weight += row.weight;
                if entry.legal {
                    existing.legal_weight += row.weight;
                }
                for (bound, component) in existing.lower_bound.iter_mut().zip(value) {
                    *bound = bound.min(component);
                }
            } else {
                aggregate.push(Aggregate {
                    action: entry.action,
                    value: value.map(|component| component * row.weight),
                    covered_weight: row.weight,
                    legal_weight: if entry.legal { row.weight } else { 0.0 },
                    lower_bound: value,
                });
            }
        }
    }
    let actor = observer as usize;
    let mut actions = aggregate
        .into_iter()
        .map(|entry| DepthActionValue {
            action: entry.action,
            value: entry
                .value
                .map(|value| value / entry.covered_weight.max(f32::EPSILON)),
            legal_weight: entry.legal_weight.clamp(0.0, 1.0),
            lower_confidence_value: entry.lower_bound,
        })
        .collect::<Vec<_>>();
    actions.sort_by(|left, right| right.value[actor].total_cmp(&left.value[actor]));
    let mut chosen_index = 0usize;
    if let Some(leading) = actions.first() {
        let leading_loss = forced_loss_weight(
            posterior
                .iter()
                .map(|particle| (&particle.state, particle.weight)),
            observer,
            &leading.action,
        );
        if leading_loss >= 1.0 - 1e-6
            && let Some(escape_index) = actions.iter().position(|candidate| {
                forced_loss_weight(
                    posterior
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    observer,
                    &candidate.action,
                ) <= 1e-6
            })
        {
            chosen_index = escape_index;
        }
    }
    let chosen = actions.get(chosen_index).map(|entry| entry.action.clone());
    let value = actions
        .get(chosen_index)
        .map(|entry| entry.value)
        .or_else(|| fallback_node.map(|node| tree.backup(node, &leaf_values)))
        .expect("CUDA belief search must have a root value");
    Ok(BeliefDepthResult {
        chosen,
        value,
        actions,
        nodes,
        cutoffs: 0,
        depth,
        particles: particles_searched,
        posterior_particles,
        deadline_reached: false,
    })
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_belief_search(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let mut evaluate_batch = |states: &[GameState]| {
        evaluator
            .evaluate_batch(states)
            .map_err(|_| DepthBeliefError::CudaEvaluationFailed)
    };
    cuda_belief_search_with_batch(particles, config, root_exclusions, &mut evaluate_batch)
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_belief_search_mutex(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let mut evaluate_batch = |states: &[GameState]| {
        let result = {
            let mut evaluator = evaluator
                .lock()
                .map_err(|_| DepthBeliefError::CudaEvaluatorLockPoisoned)?;
            evaluator.evaluate_batch(states)
        };
        result.map_err(|_| DepthBeliefError::CudaEvaluationFailed)
    };
    cuda_belief_search_with_batch(particles, config, root_exclusions, &mut evaluate_batch)
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    cuda_belief_search(evaluator, particles, config, &[])
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_mutex(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    cuda_belief_search_mutex(evaluator, particles, config, &[])
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_excluding(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    cuda_belief_search(evaluator, particles, config, root_exclusions)
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_mutex_excluding(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    cuda_belief_search_mutex(evaluator, particles, config, root_exclusions)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use colonist_catan_core::{
        Action, DevCard, GameState, NodeKind, Phase, Resource, SplitMix64,
    };

    use super::{
        apply_action_friction, normalize_belief_root_priors, search_belief_maxn,
        search_belief_maxn_bounded, search_maxn, search_paranoid,
        search_weighted_belief_maxn_bounded, search_weighted_belief_maxn_bounded_timed,
    };
    use crate::mcts::BeliefParticle;

    fn advance_setup_and_roll(state: &mut GameState, rng: &mut SplitMix64) {
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.apply(&Action::Roll).unwrap();
        let roll = state.sample_chance(rng).unwrap();
        state.apply(&roll).unwrap();
        if state.phase == Phase::Discard {
            while state.phase == Phase::Discard {
                let discard = state.legal_actions()[0].clone();
                state.apply(&discard).unwrap();
            }
        }
        if state.phase == Phase::MoveRobber {
            let robber = state.legal_actions()[0].clone();
            state.apply(&robber).unwrap();
            if state.node_kind() == NodeKind::Chance {
                let steal = state.sample_chance(rng).unwrap();
                state.apply(&steal).unwrap();
            }
        }
    }

    fn recovered_turn_54_control() -> GameState {
        let mut state = GameState::standard(54, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.turn = 54;
        state.bank_is_public = true;
        // Recovered public bank [lumber, brick, wool, grain, ore].
        state.bank = [13, 15, 9, 13, 13];
        // Recovered hand sizes 9, 8, 8, 7. The opponent compositions below
        // are one conservation-exact completion of the public evidence; the
        // historical hidden identities were not retained in the repository.
        state.players[0].resources = [0, 4, 1, 3, 1];
        state.players[1].resources = [2, 0, 3, 1, 2];
        state.players[2].resources = [2, 0, 3, 1, 2];
        state.players[3].resources = [2, 0, 3, 1, 1];
        state.players[0].public_victory_points = 2;
        state.players[1].public_victory_points = 4;
        state.players[2].public_victory_points = 3;
        state.players[3].public_victory_points = 2;
        state
    }

    fn observation_swap_control(actor: u8) -> (GameState, GameState) {
        let mut left = GameState::standard(77, 4);
        while matches!(
            left.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = left.legal_actions()[0].clone();
            left.apply(&action).unwrap();
        }
        left.phase = Phase::Main;
        left.current_player = actor;
        left.bank = [19; 5];
        for player in &mut left.players {
            player.resources = [0; 5];
        }
        left.players[actor as usize].resources = [0, 0, 0, 0, 4];
        left.players[1].resources = [3, 0, 0, 0, 0];
        left.players[2].resources = [0, 3, 0, 0, 0];
        left.bank = [16, 16, 19, 19, 15];

        let mut right = left.clone();
        right.players[1].resources = [0, 3, 0, 0, 0];
        right.players[2].resources = [3, 0, 0, 0, 0];
        (left, right)
    }

    fn winning_position(players: u8) -> GameState {
        let mut state = GameState::standard(81, players);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.victory_target = 3;
        state.bank[Resource::Grain.index()] -= 2;
        state.players[0].resources[Resource::Grain.index()] += 2;
        state.bank[Resource::Ore.index()] -= 3;
        state.players[0].resources[Resource::Ore.index()] += 3;
        state
    }

    #[test]
    fn maxn_converts_an_immediate_win() {
        let state = winning_position(4);
        let result = search_maxn(&state, 2, 16);
        assert!(matches!(result.chosen, Some(Action::BuildCity { .. })));
        assert_eq!(result.value[0], 1.0);
    }

    #[test]
    fn paranoid_alpha_beta_converts_an_immediate_win() {
        let state = winning_position(2);
        let result = search_paranoid(&state, 0, 3, 16);
        assert!(matches!(result.chosen, Some(Action::BuildCity { .. })));
        assert_eq!(result.value[0], 1.0);
    }

    #[test]
    fn belief_maxn_aggregates_legal_hidden_worlds() {
        let mut first = GameState::standard(101, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(102));
        let mut second = first.clone();
        let total = first.players[1].resource_total();
        first.players[1].resources = [total, 0, 0, 0, 0];
        second.players[1].resources = [0, total, 0, 0, 0];

        let report = search_belief_maxn(&[first.clone(), second], 3, 12).unwrap();
        assert!(
            first
                .legal_actions()
                .contains(report.chosen.as_ref().unwrap())
        );
        assert_eq!(report.particles, 2);
        assert!(report.nodes > 0);
        assert!(!report.actions.is_empty());
    }

    #[test]
    fn belief_root_candidates_include_actions_legal_only_in_later_hidden_worlds() {
        let mut unavailable = GameState::standard(111, 4);
        advance_setup_and_roll(&mut unavailable, &mut SplitMix64::new(112));
        unavailable.phase = Phase::Main;
        unavailable.current_player = 0;
        unavailable.bank_is_public = false;
        unavailable.domestic_trade_used = true;
        for player in &mut unavailable.players {
            player.resources = [0; 5];
        }
        unavailable.players[0].resources[Resource::Lumber.index()] = 4;
        unavailable.players[1].resources[Resource::Ore.index()] = 19;
        unavailable.bank = [15, 19, 19, 19, 0];

        let mut available = unavailable.clone();
        available.players[1].resources[Resource::Ore.index()] -= 1;
        available.players[1].resources[Resource::Brick.index()] += 1;
        available.bank[Resource::Ore.index()] += 1;
        available.bank[Resource::Brick.index()] -= 1;

        unavailable.validate().unwrap();
        available.validate().unwrap();
        assert_eq!(
            unavailable.observation_hash(0),
            available.observation_hash(0)
        );

        let target = Action::MaritimeTrade {
            give: Resource::Lumber,
            receive: Resource::Ore,
            ratio: 4,
        };
        assert!(!unavailable.legal_actions().contains(&target));
        assert!(available.legal_actions().contains(&target));

        let particles = vec![
            BeliefParticle {
                state: unavailable,
                weight: 0.75,
            },
            BeliefParticle {
                state: available,
                weight: 0.25,
            },
        ];
        let report = search_weighted_belief_maxn_bounded(&particles, 2, 32, 4_000).unwrap();
        let candidate = report
            .actions
            .iter()
            .find(|candidate| candidate.action == target)
            .expect("belief root must union actions across hidden worlds");
        assert!((candidate.legal_weight - 0.25).abs() < 1e-6);
    }

    #[test]
    fn belief_root_priors_apply_the_whole_turn_planner_before_truncation() {
        let mut state = GameState::standard(107, 3);
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

        let legal = state.legal_actions();
        let mut expected = crate::policy::normalize_priors(&state, &legal, 0);
        crate::planner::plan_adjusted_priors(&state, &mut expected, 5_000);
        let expected = crate::policy::order_scored_with_state_quotas(&state, 0, expected);
        let particles = vec![BeliefParticle {
            state: state.clone(),
            weight: 1.0,
        }];
        let actual = normalize_belief_root_priors(&particles, 0, 5_000);

        assert_eq!(
            actual.iter().map(|(action, _)| action).collect::<Vec<_>>(),
            expected
                .iter()
                .map(|(action, _)| action)
                .collect::<Vec<_>>(),
        );
        for ((_, actual), (_, expected)) in actual.iter().zip(&expected) {
            assert!((actual - expected).abs() < 1e-6);
        }
    }

    #[test]
    fn belief_root_priors_and_candidates_do_not_depend_on_particle_order() {
        let mut first = GameState::standard(109, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(110));
        first.phase = Phase::Main;
        first.current_player = 0;
        first.players[0].resources = [2, 2, 1, 1, 1];
        let mut second = first.clone();
        let opponent_total = first.players[1].resource_total().max(4);
        first.players[1].resources = [opponent_total, 0, 0, 0, 0];
        second.players[1].resources = [0, 0, 0, opponent_total, 0];

        let forward = vec![
            BeliefParticle {
                state: first.clone(),
                weight: 0.78,
            },
            BeliefParticle {
                state: second.clone(),
                weight: 0.22,
            },
        ];
        let reversed = vec![
            BeliefParticle {
                state: second,
                weight: 0.22,
            },
            BeliefParticle {
                state: first.clone(),
                weight: 0.78,
            },
        ];
        let forward_priors = normalize_belief_root_priors(&forward, 0, 1_000);
        let reversed_priors = normalize_belief_root_priors(&reversed, 0, 1_000);
        assert_eq!(
            forward_priors
                .iter()
                .map(|(action, _)| action)
                .collect::<Vec<_>>(),
            reversed_priors
                .iter()
                .map(|(action, _)| action)
                .collect::<Vec<_>>(),
        );
        for ((_, forward), (_, reversed)) in forward_priors.iter().zip(&reversed_priors) {
            assert!((forward - reversed).abs() < 1e-6);
        }

        let forward_report = search_weighted_belief_maxn_bounded(&forward, 3, 12, 8_000).unwrap();
        let reversed_report = search_weighted_belief_maxn_bounded(&reversed, 3, 12, 8_000).unwrap();
        assert_eq!(forward_report.chosen, reversed_report.chosen);
        assert_eq!(
            forward_report
                .actions
                .iter()
                .map(|candidate| &candidate.action)
                .collect::<Vec<_>>(),
            reversed_report
                .actions
                .iter()
                .map(|candidate| &candidate.action)
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn depth_search_prices_domestic_negotiation_without_penalizing_builds() {
        let state = GameState::standard(227, 3);
        let offer = Action::OfferTrade {
            recipients: 0b110,
            give: [1, 0, 0, 0, 0],
            receive: [0, 1, 0, 0, 0],
        };
        let mut offered = [0.5, 0.2, 0.2, 0.1];
        apply_action_friction(&mut offered, &state, &offer, 0);
        assert!((offered[0] - 0.494).abs() < 1e-6);

        let mut built = [0.5, 0.2, 0.2, 0.1];
        apply_action_friction(&mut built, &state, &Action::BuildRoad { edge: 0 }, 0);
        assert_eq!(built, [0.5, 0.2, 0.2, 0.1]);
    }

    #[test]
    fn depth_search_preserves_low_friction_hand_safety_trades() {
        let mut state = GameState::standard(229, 3);
        state.players[0].resources = [3, 2, 1, 1, 1];
        let conversion = Action::OfferTrade {
            recipients: 0b110,
            give: [2, 0, 0, 0, 0],
            receive: [0, 1, 0, 0, 0],
        };
        let mut value = [0.5, 0.2, 0.2, 0.1];
        apply_action_friction(&mut value, &state, &conversion, 0);
        assert!((value[0] - 0.4985).abs() < 1e-6);
    }

    #[test]
    fn turn_54_fair_budget_does_not_prefer_maritime_before_information_gain() {
        let state = recovered_turn_54_control();
        let maritime = Action::MaritimeTrade {
            give: Resource::Brick,
            receive: Resource::Lumber,
            ratio: 4,
        };
        assert!(state.legal_actions().contains(&Action::BuyDevelopment));
        assert!(state.legal_actions().contains(&maritime));

        let report = search_weighted_belief_maxn_bounded(
            &[BeliefParticle { state, weight: 1.0 }],
            4,
            8,
            4_000,
        )
        .unwrap();
        assert!(report.nodes <= 4_000);
        assert_ne!(report.chosen, Some(maritime.clone()));
        let buy = report
            .actions
            .iter()
            .find(|candidate| candidate.action == Action::BuyDevelopment)
            .expect("BuyDevelopment must survive the production-width root");
        let trade = report
            .actions
            .iter()
            .find(|candidate| candidate.action == maritime)
            .expect("recovered maritime action must survive the production-width root");
        assert!(buy.value[0] + 1e-6 >= trade.value[0]);
    }

    #[test]
    fn node_budget_allocator_reserves_positive_work_for_positive_chance_outcomes() {
        let budgets = super::allocate_weighted_node_budgets(&[14.0, 5.0, 2.0, 2.0, 2.0], 11);
        assert_eq!(budgets.iter().sum::<u32>(), 11);
        assert!(budgets.iter().all(|budget| *budget > 0));
    }

    #[test]
    fn threat_f8_verified_blocker_beats_end_turn_at_production_and_wider_limits() {
        let (state, blocker) = crate::threats::forced_blocker_fixture();
        for (depth, branch_cap, maximum_nodes) in [(4, 8, 4_000), (6, 32, 16_000)] {
            let report = search_weighted_belief_maxn_bounded(
                &[BeliefParticle {
                    state: state.clone(),
                    weight: 1.0,
                }],
                depth,
                branch_cap,
                maximum_nodes,
            )
            .unwrap();
            assert_eq!(report.chosen, Some(blocker.clone()));
            assert!(report.nodes <= maximum_nodes);
            assert!(report.actions.iter().any(|candidate| {
                candidate.action == blocker && candidate.legal_weight >= 1.0 - 1e-6
            }));
        }
    }

    #[test]
    fn threat_immediate_winning_road_is_not_replaced_by_blocker() {
        let (state, winning_road, blocker) =
            crate::threats::winning_road_over_blocker_fixture();
        let report = search_weighted_belief_maxn_bounded(
            &[BeliefParticle {
                state: state.clone(),
                weight: 1.0,
            }],
            4,
            8,
            4_000,
        )
        .unwrap();

        let chosen = report
            .chosen
            .clone()
            .expect("production-width search must select a root action");
        assert_ne!(chosen, blocker);
        let mut after = state.clone();
        after.apply(&chosen).unwrap();
        assert_eq!(after.winner(), Some(0));
        assert!(after.is_terminal());
        assert!(report.nodes <= 4_000);
        assert!(report.actions.iter().any(|candidate| {
            candidate.action == winning_road && candidate.legal_weight >= 1.0 - 1e-6
        }));
    }

    #[test]
    fn observation_policy_ignores_third_party_hidden_resource_swap() {
        let (left, right) = observation_swap_control(3);
        assert_eq!(left.observation_hash(3), right.observation_hash(3));
        assert_eq!(left.legal_actions(), right.legal_actions());
        let left_policy = super::recursive_observation_policy(
            &left,
            &left.legal_actions(),
            3,
            8,
        );
        let right_policy = super::recursive_observation_policy(
            &right,
            &right.legal_actions(),
            3,
            8,
        );
        assert_eq!(left_policy, right_policy);
    }

    #[test]
    fn observation_policy_is_safe_for_root_actor_after_same_turn_development_draw() {
        let mut left = recovered_turn_54_control();
        let mut right = left.clone();
        right.players[1].resources = left.players[2].resources;
        right.players[2].resources = left.players[1].resources;
        assert_eq!(left.observation_hash(0), right.observation_hash(0));

        for state in [&mut left, &mut right] {
            state.apply(&Action::BuyDevelopment).unwrap();
            state
                .apply(&Action::ResolveDevelopment { card: DevCard::Knight })
                .unwrap();
        }
        assert_eq!(left.observation_hash(0), right.observation_hash(0));
        assert_eq!(left.legal_actions(), right.legal_actions());
        let left_policy = super::recursive_observation_policy(
            &left,
            &left.legal_actions(),
            0,
            8,
        );
        let right_policy = super::recursive_observation_policy(
            &right,
            &right.legal_actions(),
            0,
            8,
        );
        assert_eq!(left_policy, right_policy);
    }

    #[test]
    fn node_budget_turn_54_buy_and_maritime_orders_are_state_identical() {
        let state = recovered_turn_54_control();
        let maritime = Action::MaritimeTrade {
            give: Resource::Brick,
            receive: Resource::Lumber,
            ratio: 4,
        };
        for card in DevCard::ALL {
            if state.development_deck[card.index()] == 0 {
                continue;
            }
            let mut maritime_first = state.clone();
            maritime_first.apply(&maritime).unwrap();
            maritime_first.apply(&Action::BuyDevelopment).unwrap();
            maritime_first
                .apply(&Action::ResolveDevelopment { card })
                .unwrap();

            let mut buy_first = state.clone();
            buy_first.apply(&Action::BuyDevelopment).unwrap();
            buy_first
                .apply(&Action::ResolveDevelopment { card })
                .unwrap();
            buy_first.apply(&maritime).unwrap();

            assert_eq!(maritime_first.state_hash(), buy_first.state_hash());
            let left = crate::eval::evaluate(&maritime_first);
            let right = crate::eval::evaluate(&buy_first);
            for player in 0..4 {
                assert!((left[player] - right[player]).abs() <= 1e-6);
            }
        }
    }

    #[test]
    fn node_budget_equal_ranked_sibling_permutation_is_value_stable() {
        let state = recovered_turn_54_control();
        let maritime = Action::MaritimeTrade {
            give: Resource::Brick,
            receive: Resource::Lumber,
            ratio: 4,
        };
        let forward = vec![(Action::BuyDevelopment, 0.5), (maritime.clone(), 0.5)];
        let reverse = vec![(maritime, 0.5), (Action::BuyDevelopment, 0.5)];
        let make_searcher = || super::Searcher {
            algorithm: super::Algorithm::MaxN,
            maximum_depth: 4,
            maximum_nodes: 400,
            node_limit: 400,
            branch_cap: 8,
            nodes: 0,
            cutoffs: 0,
            deepest_depth: 0,
            deadline: crate::deadline::CooperativeDeadline::start(0),
            deadline_reached: false,
            observation_safe_recursive: false,
        };
        let mut left = make_searcher();
        let (left_value, left_chosen) =
            left.visit_ranked_decision(&state, 0, forward, 0, 0, 0.0, 1.0, 400);
        let mut right = make_searcher();
        let (right_value, right_chosen) =
            right.visit_ranked_decision(&state, 0, reverse, 0, 0, 0.0, 1.0, 400);

        assert_eq!(left_chosen, right_chosen);
        assert_eq!(left.nodes, right.nodes);
        for player in 0..4 {
            assert!((left_value[player] - right_value[player]).abs() <= 1e-6);
        }
    }

    #[test]
    fn bounded_belief_maxn_respects_one_global_node_budget() {
        let mut first = GameState::standard(201, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(202));
        let particles = vec![first; 8];

        let report = search_belief_maxn_bounded(&particles, 3, 12, 4_000).unwrap();

        assert!(report.nodes <= 4_000);
        assert_eq!(report.posterior_particles, particles.len());
        assert_eq!(report.particles, 1);
        assert!(!report.actions.is_empty());
    }

    #[test]
    fn timed_belief_maxn_honors_one_global_deadline_without_dropping_root_actions() {
        let mut first = GameState::standard(203, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(204));
        first.phase = Phase::Main;
        first.current_player = 0;
        for (resource, target) in [2_u8, 2, 2, 3, 3].into_iter().enumerate() {
            let transfer = target.saturating_sub(first.players[0].resources[resource]);
            first.players[0].resources[resource] += transfer;
            first.bank[resource] -= transfer;
        }
        let particles = (0..8)
            .map(|_| BeliefParticle {
                state: first.clone(),
                weight: 1.0,
            })
            .collect::<Vec<_>>();

        let fallback = search_weighted_belief_maxn_bounded(&particles, 6, 32, 1).unwrap();
        let report =
            search_weighted_belief_maxn_bounded_timed(&particles, 6, 32, 250_000, 1).unwrap();

        assert!(report.deadline_reached);
        assert!(report.nodes < 250_000);
        assert_eq!(report.posterior_particles, particles.len());
        assert_eq!(report.particles, 1);
        assert_eq!(report.actions.len(), fallback.actions.len());
        for candidate in &report.actions {
            let expected = fallback
                .actions
                .iter()
                .find(|other| other.action == candidate.action)
                .expect("timed result must retain every minimally evaluated root action");
            assert!((candidate.legal_weight - expected.legal_weight).abs() < 1e-6);
        }
        assert!(
            report
                .chosen
                .as_ref()
                .is_some_and(|action| first.legal_actions().contains(action))
        );
    }

    #[test]
    fn zero_time_budget_is_identical_to_the_node_bounded_api() {
        let mut state = GameState::standard(211, 3);
        advance_setup_and_roll(&mut state, &mut SplitMix64::new(212));
        let particles = vec![BeliefParticle { state, weight: 1.0 }];

        let untimed = search_weighted_belief_maxn_bounded(&particles, 3, 12, 2_000).unwrap();
        let zero_timed =
            search_weighted_belief_maxn_bounded_timed(&particles, 3, 12, 2_000, 0).unwrap();

        assert_eq!(untimed.chosen, zero_timed.chosen);
        assert_eq!(untimed.nodes, zero_timed.nodes);
        assert_eq!(untimed.deadline_reached, zero_timed.deadline_reached);
        assert_eq!(
            untimed
                .actions
                .iter()
                .map(|candidate| (&candidate.action, candidate.value, candidate.legal_weight))
                .collect::<Vec<_>>(),
            zero_timed
                .actions
                .iter()
                .map(|candidate| (&candidate.action, candidate.value, candidate.legal_weight))
                .collect::<Vec<_>>(),
        );
    }

    #[test]
    fn belief_maxn_uses_the_joint_snake_order_opening_solver() {
        let state = GameState::standard(205, 4);
        let report = search_belief_maxn_bounded(&[state], 4, 12, 20_000).unwrap();

        assert!(matches!(
            report.chosen,
            Some(Action::PlaceSettlement { .. })
        ));
        assert!(
            report.depth >= 8,
            "opening depth reports the complete remaining setup horizon"
        );
        assert!(report.nodes <= 20_000);
        assert!(
            report
                .actions
                .iter()
                .all(|candidate| candidate.legal_weight == 1.0)
        );
        assert!(!report.actions.is_empty());
    }

    #[test]
    fn belief_maxn_plays_monopoly_when_it_unlocks_an_immediate_city() {
        let mut state = GameState::standard(209, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        let board = Arc::make_mut(&mut state.board);
        for tile in &mut board.hexes {
            if tile.resource.is_some() {
                tile.resource = Some(Resource::Ore);
            }
        }
        state.players[0].resources = [0, 0, 0, 0, 3];
        state.players[0].development[4] = 1;
        for player in 1..4 {
            state.players[player].resources = [0, 0, 0, 4, 0];
        }

        let report = search_weighted_belief_maxn_bounded(
            &[BeliefParticle { state, weight: 1.0 }],
            3,
            12,
            8_000,
        )
        .unwrap();

        assert_eq!(
            report.chosen,
            Some(Action::PlayMonopoly {
                resource: Resource::Grain,
            })
        );
        let monopoly_roots = report
            .actions
            .iter()
            .filter(|candidate| matches!(candidate.action, Action::PlayMonopoly { .. }))
            .collect::<Vec<_>>();
        assert_eq!(monopoly_roots.len(), 1);
        assert_eq!(
            monopoly_roots[0].action,
            Action::PlayMonopoly {
                resource: Resource::Grain,
            }
        );
        assert!(monopoly_roots[0].legal_weight <= 1.0);
    }

    #[test]
    fn strategic_particle_f14_full_posterior_preserves_monopoly_family() {
        let mut base = GameState::standard(907, 4);
        while matches!(
            base.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = base.legal_actions()[0].clone();
            base.apply(&action).unwrap();
        }
        base.phase = Phase::Main;
        base.current_player = 0;
        base.bank_is_public = false;
        let board = Arc::make_mut(&mut base.board);
        for tile in &mut board.hexes {
            if tile.resource.is_some() {
                tile.resource = Some(Resource::Ore);
            }
        }
        for player in 0..4 {
            for resource in 0..5 {
                base.bank[resource] += base.players[player].resources[resource];
                base.players[player].resources[resource] = 0;
            }
        }
        base.players[0].resources[Resource::Ore.index()] = 3;
        base.bank[Resource::Ore.index()] -= 3;
        base.players[0].development[DevCard::Monopoly.index()] += 1;
        base.development_deck[DevCard::Monopoly.index()] -= 1;

        const F14_GRAIN_COUNTS: [[u8; 3]; 24] = [
            [1, 2, 1],
            [2, 2, 1],
            [0, 1, 4],
            [3, 1, 1],
            [3, 0, 4],
            [1, 1, 0],
            [0, 2, 4],
            [4, 2, 1],
            [0, 4, 2],
            [3, 3, 0],
            [0, 2, 1],
            [1, 0, 3],
            [3, 1, 2],
            [1, 4, 0],
            [1, 0, 0],
            [4, 3, 2],
            [0, 0, 0],
            [0, 0, 2],
            [0, 4, 1],
            [0, 2, 2],
            [4, 1, 3],
            [0, 1, 1],
            [4, 3, 3],
            [0, 4, 4],
        ];
        let particles = F14_GRAIN_COUNTS
            .iter()
            .map(|grains| {
                let mut world = base.clone();
                for (offset, grain) in grains.iter().copied().enumerate() {
                    let player = offset + 1;
                    let ore = 4 - grain;
                    world.players[player].resources[Resource::Grain.index()] = grain;
                    world.players[player].resources[Resource::Ore.index()] = ore;
                    world.bank[Resource::Grain.index()] -= grain;
                    world.bank[Resource::Ore.index()] -= ore;
                }
                world.validate().unwrap();
                BeliefParticle {
                    state: world,
                    weight: 1.0 / 24.0,
                }
            })
            .collect::<Vec<_>>();
        let observation = particles[0].state.observation_hash(0);
        assert!(
            particles
                .iter()
                .all(|particle| particle.state.observation_hash(0) == observation)
        );
        assert_eq!(
            crate::shared::coalesce_identical_particles(&particles).len(),
            24,
        );

        let compressed = crate::shared::select_experimental_strategic_particles(&particles, 12);
        assert_eq!(compressed.len(), 12);
        let full_exact = crate::exact::solve_exact_belief(
            &particles,
            crate::exact::ExactActionFamily::Monopoly,
        );
        let compressed_exact = crate::exact::solve_exact_belief(
            &compressed,
            crate::exact::ExactActionFamily::Monopoly,
        );
        assert_eq!(
            full_exact.chosen,
            Some(Action::PlayMonopoly {
                resource: Resource::Grain,
            })
        );
        assert_eq!(
            compressed_exact.chosen,
            Some(Action::PlayMonopoly {
                resource: Resource::Ore,
            })
        );

        let production = super::search_weighted_belief_maxn_bounded(&particles, 4, 8, 4_000)
            .unwrap();
        let experimental_compressed = super::search_weighted_belief_maxn_with_config(
            &particles,
            super::BeliefDepthConfig {
                maximum_depth: 4,
                branch_cap: 8,
                maximum_nodes: 4_000,
                time_budget_ms: 0,
                strategic_particle_limit: 12,
            },
        )
        .unwrap();
        assert_eq!(
            production.chosen,
            Some(Action::PlayMonopoly {
                resource: Resource::Grain,
            })
        );
        assert_eq!(experimental_compressed.chosen, Some(Action::EndTurn));
        assert_eq!(production.posterior_particles, 24);
        assert_eq!(production.particles, 24);
        assert_eq!(experimental_compressed.posterior_particles, 24);
        assert_eq!(experimental_compressed.particles, 12);
    }

    #[test]
    fn configured_strategic_particle_limit_is_applied() {
        let mut state = GameState::standard(151, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        for player in 0..3 {
            for resource in 0..5 {
                state.bank[resource] += state.players[player].resources[resource];
                state.players[player].resources[resource] = 0;
            }
        }
        state.bank[Resource::Lumber.index()] -= 4;
        state.bank[Resource::Brick.index()] -= 4;
        let particles = (0..16)
            .map(|index| {
                let mut world = state.clone();
                if index % 2 == 0 {
                    world.players[1].resources = [4, 0, 0, 0, 0];
                    world.players[2].resources = [0, 4, 0, 0, 0];
                } else {
                    world.players[1].resources = [0, 4, 0, 0, 0];
                    world.players[2].resources = [4, 0, 0, 0, 0];
                }
                BeliefParticle {
                    state: world,
                    weight: 1.0,
                }
            })
            .collect::<Vec<_>>();
        let report = super::search_weighted_belief_maxn_with_config(
            &particles,
            super::BeliefDepthConfig {
                maximum_depth: 1,
                branch_cap: 4,
                maximum_nodes: 500,
                time_budget_ms: 0,
                strategic_particle_limit: 4,
            },
        )
        .unwrap();
        assert_eq!(report.posterior_particles, 16);
        assert_eq!(report.particles, 2);
    }

    #[test]
    fn public_opening_is_identical_across_information_modes() {
        let state = GameState::standard(153, 3);
        let perfect = super::search_maxn_bounded(&state, 3, 12, 12_000);
        let belief = super::search_weighted_belief_maxn_bounded(
            &[BeliefParticle {
                state: state.clone(),
                weight: 1.0,
            }],
            3,
            12,
            12_000,
        )
        .unwrap();
        assert_eq!(perfect.chosen, belief.chosen);
    }
}
