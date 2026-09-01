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
use crate::shared::{STRATEGIC_PARTICLE_TARGET, select_strategic_particles};
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
    pub particles: usize,
    pub posterior_particles: usize,
    pub deadline_reached: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct BeliefDepthConfig {
    pub maximum_depth: u8,
    pub branch_cap: usize,
    pub maximum_nodes: u32,
    pub time_budget_ms: u32,
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
    /// When true, non-root actors use a prior-weighted mixture of their top
    /// observation-ranked actions. Disabled by default: the mixture fixes one
    /// fusion source but dilutes strategic opponent MaxN under thin budgets.
    /// Enable only behind arena ablation flags until held-out evidence exists.
    observation_safe_root: Option<u8>,
}

fn normalize_belief_root_priors(
    particles: &[BeliefParticle],
    actions: &[Action],
    actor: u8,
) -> Vec<(Action, f32)> {
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut aggregate = actions
        .iter()
        .cloned()
        .map(|action| (action, 0.0_f32))
        .collect::<Vec<_>>();

    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        let legal = particle.state.legal_actions();
        for (action, prior) in normalize_priors(&particle.state, &legal, actor) {
            if let Some((_, score)) = aggregate
                .iter_mut()
                .find(|(candidate, _)| *candidate == action)
            {
                *score += prior * weight;
            }
        }
    }

    let prior_mass = aggregate
        .iter()
        .map(|(_, prior)| *prior)
        .sum::<f32>()
        .max(f32::EPSILON);
    for (_, prior) in &mut aggregate {
        *prior /= prior_mass;
    }
    aggregate.sort_by(|left, right| right.1.total_cmp(&left.1));
    aggregate
}

impl Searcher {
    fn visit(
        &mut self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        mut alpha: f32,
        mut beta: f32,
    ) -> [f32; 4] {
        if self.nodes >= self.node_limit {
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
                    .sum::<f32>();
                let mut expected = [0.0; 4];
                for action in actions {
                    let weight = state.chance_weight(&action) as f32 / total;
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("legal chance action must transition");
                    let child = if self.nodes < self.node_limit {
                        self.visit(&next, depth, actions_in_turn.saturating_add(1), alpha, beta)
                    } else {
                        evaluate(&next)
                    };
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
                let observation_safe = self.observation_safe_root.is_some_and(|root| root != actor);
                // Observation-safe opponents evaluate a prior-weighted mixture
                // over the top observation-ranked actions. The mixture depends
                // only on the actor's observation, so indistinguishable worlds
                // share one strategy while still covering more than a single
                // greedy prior line.
                if observation_safe {
                    let mixture = ranked.iter().take(3.min(ranked.len())).collect::<Vec<_>>();
                    let mass = mixture
                        .iter()
                        .map(|(_, prior)| prior.max(0.0))
                        .sum::<f32>()
                        .max(f32::EPSILON);
                    let mut expected = [0.0_f32; 4];
                    for (action, prior) in mixture {
                        let weight = prior.max(0.0) / mass;
                        if weight <= 0.0 {
                            continue;
                        }
                        let mut next = state.clone();
                        next.apply(action)
                            .expect("ranked depth-search action must transition");
                        let completed_turn =
                            next.turn != state.turn || next.current_player != state.current_player;
                        let mut child = self.visit(
                            &next,
                            depth + u8::from(completed_turn),
                            if completed_turn {
                                0
                            } else {
                                actions_in_turn.saturating_add(1)
                            },
                            alpha,
                            beta,
                        );
                        if self.deadline_reached {
                            return evaluate(state);
                        }
                        apply_action_friction(&mut child, state, action, actor);
                        for player in 0..4 {
                            expected[player] += child[player] * weight;
                        }
                    }
                    return expected;
                }
                let mut best = [0.0; 4];
                let maximize_root = match self.algorithm {
                    Algorithm::MaxN => true,
                    Algorithm::Paranoid { root } => actor == root,
                };
                let component = match self.algorithm {
                    Algorithm::MaxN => actor as usize,
                    Algorithm::Paranoid { root } => root as usize,
                };
                let mut best_scalar = if maximize_root {
                    f32::NEG_INFINITY
                } else {
                    f32::INFINITY
                };
                for (action, _) in ranked {
                    if self.nodes >= self.node_limit {
                        break;
                    }
                    let mut next = state.clone();
                    next.apply(&action)
                        .expect("ranked depth-search action must transition");
                    let completed_turn =
                        next.turn != state.turn || next.current_player != state.current_player;
                    let mut child = self.visit(
                        &next,
                        depth + u8::from(completed_turn),
                        if completed_turn {
                            0
                        } else {
                            actions_in_turn.saturating_add(1)
                        },
                        alpha,
                        beta,
                    );
                    if self.deadline_reached {
                        return evaluate(state);
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
                    best
                } else {
                    evaluate(state)
                }
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
    // Exact mandatory/tactical solvers in the WASM adapter see the full
    // posterior. Strategic MaxN deliberately searches a compact representative
    // subset so node budget is not diluted across near-duplicate worlds.
    let posterior_particles = particles.len();
    let strategic_storage;
    let particles = if particles.len() > config.strategic_particle_limit {
        strategic_storage = select_strategic_particles(particles, config.strategic_particle_limit);
        strategic_storage.as_slice()
    } else {
        particles
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
    let first_legal = first.legal_actions();
    let root_priors = normalize_belief_root_priors(particles, &first_legal, observer);
    let root_scored = order_scored_with_state_quotas(first, observer, root_priors);
    // Threat forcing disabled until posterior-aggregated and post-apply verified.
    let mut root_actions = truncate_root_preserving_end_turn(root_scored, branch_cap)
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    // Monopoly's five resource parameters share one strategic family slot.
    // Pick that representative over the complete posterior before MaxN spends
    // its root budget; a production-based public prior cannot know which
    // accumulated resource the opponents are actually holding.
    if first_legal
        .iter()
        .any(|action| matches!(action, Action::PlayMonopoly { .. }))
    {
        let exact_monopoly = solve_exact_belief(particles, ExactActionFamily::Monopoly);
        if let Some(best_monopoly) = exact_monopoly.chosen {
            if let Some(candidate) = root_actions
                .iter_mut()
                .find(|action| matches!(action, Action::PlayMonopoly { .. }))
            {
                *candidate = best_monopoly;
            } else if root_actions.len() < branch_cap {
                root_actions.push(best_monopoly);
            }
        }
    }
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
                // Observation-safe opponent mixtures stay off until ablated.
                observation_safe_root: None,
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
            legal_weight: entry.legal_weight,
            lower_confidence_value: entry.lower_bound,
        })
        .collect::<Vec<_>>();
    actions.sort_by(|left, right| right.value[actor].total_cmp(&left.value[actor]));
    let chosen = actions.first().map(|entry| entry.action.clone());
    let value = actions
        .first()
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
        observation_safe_root: None,
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
        observation_safe_root: None,
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
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
        },
    )
}

pub fn search_weighted_belief_maxn_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, config, false)
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
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
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
    search_weighted_belief_maxn_with_config(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
        },
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
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
        },
    )
}

pub fn search_weighted_belief_paranoid_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, config, true)
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
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
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
    search_weighted_belief_paranoid_with_config(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes,
            time_budget_ms,
            strategic_particle_limit: STRATEGIC_PARTICLE_TARGET,
        },
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use colonist_catan_core::{Action, GameState, NodeKind, Phase, Resource, SplitMix64};

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
        let legal = first.legal_actions();
        let forward_priors = normalize_belief_root_priors(&forward, &legal, 0);
        let reversed_priors = normalize_belief_root_priors(&reversed, &legal, 0);
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
    fn bounded_belief_maxn_respects_one_global_node_budget() {
        let mut first = GameState::standard(201, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(202));
        let particles = vec![first; 8];

        let report = search_belief_maxn_bounded(&particles, 3, 12, 4_000).unwrap();

        assert!(report.nodes <= 4_000);
        assert_eq!(report.particles, particles.len());
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
        assert_eq!(report.particles, particles.len());
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
