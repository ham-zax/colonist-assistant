use colonist_catan_core::{Action, GameState, NodeKind};

use crate::eval::evaluate;
use crate::mcts::BeliefParticle;
use crate::opening::opening_adjusted_priors;
use crate::planner::plan_adjusted_priors;
use crate::policy::{normalize_priors, order_scored_with_state_quotas, rank_with_class_quotas};

// Convenience APIs must remain safe in UI/tests. Production callers that
// explicitly want a larger budget use the `_bounded` variants.
const DEFAULT_DEPTH_NODE_BUDGET: u32 = 8_000;

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
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepthBeliefError {
    Empty,
    PublicStateMismatch,
    RootObservationMismatch,
}

#[derive(Clone, Copy)]
enum Algorithm {
    MaxN,
    Paranoid { root: u8 },
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
                    for player in 0..4 {
                        expected[player] += child[player] * weight;
                    }
                }
                expected
            }
            NodeKind::Decision { actor } => {
                let ranked = rank_with_class_quotas(state, &actions, actor, self.branch_cap);
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
                    let child = self.visit(
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
        let ranked = order_scored_with_state_quotas(state, actor, ranked)
            .into_iter()
            .take(self.branch_cap)
            .collect::<Vec<_>>();
        let per_root_budget = (self.maximum_nodes / ranked.len().max(1) as u32)
            .max(32)
            .min(self.maximum_nodes);
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
        for (action, _) in ranked {
            if self.nodes >= self.maximum_nodes {
                break;
            }
            self.node_limit = self
                .nodes
                .saturating_add(per_root_budget)
                .min(self.maximum_nodes);
            let mut next = state.clone();
            next.apply(&action)
                .expect("ranked root action must transition");
            let completed_turn =
                next.turn != state.turn || next.current_player != state.current_player;
            let child = self.visit(
                &next,
                u8::from(completed_turn),
                if completed_turn { 0 } else { 1 },
                alpha,
                beta,
            );
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
        }
    }
}

fn belief_search(
    particles: &[BeliefParticle],
    maximum_depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    paranoid: bool,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let Some(first_particle) = particles.first() else {
        return Err(DepthBeliefError::Empty);
    };
    let first = &first_particle.state;
    let observer = first.actor();
    let public = first.public_hash();
    let observation = first.observation_hash(observer);
    if particles
        .iter()
        .any(|particle| particle.state.public_hash() != public)
    {
        return Err(DepthBeliefError::PublicStateMismatch);
    }
    if particles
        .iter()
        .any(|particle| particle.state.observation_hash(observer) != observation)
    {
        return Err(DepthBeliefError::RootObservationMismatch);
    }
    struct Aggregate {
        action: Action,
        value: [f32; 4],
        covered_weight: f32,
        legal_weight: f32,
        lower_bound: [f32; 4],
    }
    let mut aggregate = Vec::<Aggregate>::new();
    let mut nodes = 0;
    let mut cutoffs = 0;
    let mut depth = 0;
    let mut particles_searched = 0;
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let root_actions = rank_with_class_quotas(first, &first.legal_actions(), observer, branch_cap)
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    // Give every legal hidden world a fair slice. Previously each particle
    // received an unbounded depth-3 tree, which made a normal four-player
    // position grow into minutes of synchronous WASM work.
    let nodes_per_action =
        (maximum_nodes / particles.len().max(1) as u32 / root_actions.len().max(1) as u32).max(1);
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        particles_searched += 1;
        for action in &root_actions {
            let mut next = particle.state.clone();
            if next.apply(action).is_err() {
                // An unavailable action retains the no-action baseline rather
                // than disappearing from the denominator.
                let fallback = evaluate(&particle.state);
                if let Some(existing) = aggregate.iter_mut().find(|entry| entry.action == *action) {
                    for (sum, value) in existing.value.iter_mut().zip(fallback) {
                        *sum += value * weight;
                    }
                    existing.covered_weight += weight;
                    for (bound, value) in existing.lower_bound.iter_mut().zip(fallback) {
                        *bound = bound.min(value);
                    }
                } else {
                    aggregate.push(Aggregate {
                        action: action.clone(),
                        value: fallback.map(|value| value * weight),
                        covered_weight: weight,
                        legal_weight: 0.0,
                        lower_bound: fallback,
                    });
                }
                continue;
            }
            let completed_turn = next.turn != particle.state.turn
                || next.current_player != particle.state.current_player;
            // Each root action gets the same local budget. Otherwise an early
            // high-prior action can exhaust a particle's entire allocation and
            // leave every later settlement/road/trade with a one-ply proxy.
            let mut searcher = Searcher {
                algorithm: if paranoid {
                    Algorithm::Paranoid { root: observer }
                } else {
                    Algorithm::MaxN
                },
                maximum_depth,
                maximum_nodes: nodes_per_action,
                node_limit: nodes_per_action,
                branch_cap: branch_cap.max(1),
                nodes: 0,
                cutoffs: 0,
                deepest_depth: 0,
            };
            let candidate_value = if searcher.nodes < searcher.node_limit {
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
            nodes += searcher.nodes;
            cutoffs += searcher.cutoffs;
            depth = depth.max(searcher.deepest_depth);
            if let Some(existing) = aggregate.iter_mut().find(|entry| entry.action == *action) {
                for (sum, value) in existing.value.iter_mut().zip(candidate_value) {
                    *sum += value * weight;
                }
                existing.covered_weight += weight;
                existing.legal_weight += weight;
                for (bound, value) in existing.lower_bound.iter_mut().zip(candidate_value) {
                    *bound = bound.min(value);
                }
            } else {
                aggregate.push(Aggregate {
                    action: action.clone(),
                    value: candidate_value.map(|value| value * weight),
                    covered_weight: weight,
                    legal_weight: weight,
                    lower_bound: candidate_value,
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
    })
}

pub fn search_maxn(state: &GameState, depth: u8, branch_cap: usize) -> DepthSearchResult {
    search_maxn_bounded(state, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

pub fn search_maxn_bounded(
    state: &GameState,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> DepthSearchResult {
    Searcher {
        algorithm: Algorithm::MaxN,
        maximum_depth: depth,
        maximum_nodes: maximum_nodes.max(1),
        node_limit: maximum_nodes.max(1),
        branch_cap: branch_cap.max(1),
        nodes: 0,
        cutoffs: 0,
        deepest_depth: 0,
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
    Searcher {
        algorithm: Algorithm::Paranoid { root },
        maximum_depth: depth,
        maximum_nodes: maximum_nodes.max(1),
        node_limit: maximum_nodes.max(1),
        branch_cap: branch_cap.max(1),
        nodes: 0,
        cutoffs: 0,
        deepest_depth: 0,
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
    belief_search(&weighted, depth, branch_cap, maximum_nodes.max(1), false)
}

pub fn search_weighted_belief_maxn_bounded(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, depth, branch_cap, maximum_nodes.max(1), false)
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
    belief_search(&weighted, depth, branch_cap, maximum_nodes.max(1), true)
}

pub fn search_weighted_belief_paranoid_bounded(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(particles, depth, branch_cap, maximum_nodes.max(1), true)
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, NodeKind, Phase, Resource, SplitMix64};

    use super::{search_belief_maxn, search_belief_maxn_bounded, search_maxn, search_paranoid};

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
    fn bounded_belief_maxn_respects_one_global_node_budget() {
        let mut first = GameState::standard(201, 4);
        advance_setup_and_roll(&mut first, &mut SplitMix64::new(202));
        let particles = vec![first; 8];

        let report = search_belief_maxn_bounded(&particles, 3, 12, 4_000).unwrap();

        assert!(report.nodes <= 4_000);
        assert_eq!(report.particles, particles.len());
        assert!(!report.actions.is_empty());
    }
}
