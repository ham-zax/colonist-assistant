use colonist_catan_core::{Action, GameState, NodeKind, SplitMix64};

use crate::deadline::CooperativeDeadline;
use crate::eval::{evaluate, road_frontier_value};
use crate::exact::{
    ExactActionFamily, ExactDecisionResult, exact_family_for_action, solve_exact_belief,
};
use crate::opening::opening_adjusted_priors;
use crate::planner::plan_adjusted_priors;
use crate::policy::{
    choose_rollout_action, normalize_observed_priors, order_scored_with_state_quotas,
};
use crate::tactical::{TacticalResult, solve_belief_current_turn, solve_current_turn};
use crate::trade_safety::belief_domestic_trade_threat;

const NONE: u32 = u32::MAX;

fn empty_tactical_result() -> TacticalResult {
    TacticalResult {
        win_probability: 0.0,
        lower_bound: 0.0,
        principal_line: Vec::new(),
        nodes: 0,
        proven: false,
    }
}

fn information_identity(state: &GameState) -> u64 {
    match state.node_kind() {
        NodeKind::Decision { actor } => state.observation_hash(actor),
        NodeKind::Chance | NodeKind::Terminal => state.public_hash(),
    }
}

fn sample_weighted_index(weights: &[f32], rng: &mut SplitMix64) -> usize {
    let target = (rng.next_u64() as f64 / u64::MAX as f64) as f32 * weights.iter().sum::<f32>();
    let mut cursor = 0.0;
    for (index, weight) in weights.iter().enumerate() {
        cursor += weight.max(0.0);
        if cursor >= target {
            return index;
        }
    }
    weights.len().saturating_sub(1)
}

fn belief_fingerprint(particles: &[BeliefParticle]) -> u64 {
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut worlds = particles
        .iter()
        .map(|particle| {
            (
                particle.state.state_hash(),
                particle.weight.max(0.0) / total,
            )
        })
        .collect::<Vec<_>>();
    worlds.sort_by_key(|(state_hash, _)| *state_hash);
    let mut merged = Vec::<(u64, f32)>::with_capacity(worlds.len());
    for (state_hash, mass) in worlds {
        if let Some((_, existing_mass)) = merged
            .last_mut()
            .filter(|(existing_hash, _)| *existing_hash == state_hash)
        {
            *existing_mass += mass;
        } else {
            merged.push((state_hash, mass));
        }
    }
    merged.into_iter().fold(
        0xcbf2_9ce4_8422_2325_u64,
        |mut fingerprint, (state_hash, mass)| {
            for value in [state_hash, (mass.clamp(0.0, 1.0) * 65_535.0).round() as u64] {
                fingerprint ^= value;
                fingerprint = fingerprint.wrapping_mul(0x0000_0100_0000_01b3);
            }
            fingerprint
        },
    )
}

fn progressive_width(config: &SearchConfig, visits: u32) -> usize {
    (config.progressive_width_k * (visits.max(1) as f32).powf(config.progressive_width_alpha))
        .ceil()
        .max(1.0) as usize
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchMode {
    Uct,
    Puct,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SearchConfig {
    pub iterations: u32,
    pub max_nodes: usize,
    pub max_tree_depth: u16,
    pub rollout_actions: u16,
    pub exploration: f32,
    pub opponent_temperature: f32,
    pub progressive_width_k: f32,
    pub progressive_width_alpha: f32,
    pub tactical_depth: u8,
    pub tactical_nodes: u32,
    /// Cooperative wall-clock limit for strategic simulations. Zero keeps the
    /// deterministic iteration/node-only behavior used by native benchmarks.
    pub time_budget_ms: u32,
    pub seed: u64,
    pub mode: SearchMode,
}

impl Default for SearchConfig {
    fn default() -> Self {
        Self {
            iterations: 4_000,
            max_nodes: 80_000,
            max_tree_depth: 8,
            rollout_actions: 420,
            exploration: 1.35,
            opponent_temperature: 0.34,
            // At live budgets this expands roughly 13 root candidates at 112
            // visits and 21 at 320, leaving enough evidence per action while
            // class quotas still preserve every strategically distinct family.
            progressive_width_k: 1.45,
            progressive_width_alpha: 0.46,
            tactical_depth: 18,
            tactical_nodes: 20_000,
            time_budget_ms: 0,
            seed: 0x0043_4154_414e,
            mode: SearchMode::Puct,
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActionStats {
    pub action: Action,
    pub visits: u32,
    pub availability: u32,
    pub availability_weight: f32,
    pub legal_weight: f32,
    pub prior: f32,
    pub value: [f32; 4],
    pub lower_confidence_value: [f32; 4],
}

#[derive(Clone, Debug, Default)]
pub struct SearchStatistics {
    pub iterations: u32,
    pub nodes: usize,
    pub deepest_decision_depth: u16,
    pub rollouts: u32,
    pub effective_particle_count: f32,
    pub deadline_reached: bool,
}

#[derive(Clone, Debug)]
pub struct SearchReport {
    pub chosen: Option<Action>,
    pub root_value: [f32; 4],
    pub actions: Vec<ActionStats>,
    pub tactical: TacticalResult,
    pub exact: ExactDecisionResult,
    pub statistics: SearchStatistics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeliefError {
    Empty,
    InvalidWeight,
    PublicStateMismatch,
    RootObservationMismatch,
}

#[derive(Clone, Debug)]
pub struct BeliefParticle {
    pub state: GameState,
    pub weight: f32,
}

fn robust_root_score(candidate: &ActionStats, actor: usize) -> f32 {
    candidate.value[actor] * 0.72 + candidate.lower_confidence_value[actor] * 0.28
        - (1.0 - candidate.legal_weight).max(0.0) * 0.12
}

fn rank_root_actions(actions: &mut [ActionStats], actor: usize) {
    // PUCT's supported policy target is visit count. Sorting primarily by a
    // noisy sample mean created a winner's curse across dozens of roads and
    // trade parameters; one lucky rollout could beat a well-supported action.
    actions.sort_by(|a, b| {
        b.visits
            .cmp(&a.visits)
            .then_with(|| robust_root_score(b, actor).total_cmp(&robust_root_score(a, actor)))
            .then_with(|| b.prior.total_cmp(&a.prior))
    });
}

pub fn safer_end_turn_alternative(
    state: &GameState,
    actor: usize,
    actions: &[ActionStats],
) -> Option<Action> {
    let held = state.players.get(actor)?.resource_total();
    if held <= state.card_discard_limit {
        return None;
    }
    let end_score = actions
        .iter()
        .find(|candidate| candidate.action == Action::EndTurn)
        .map(|candidate| robust_root_score(candidate, actor))?;
    let alternative = actions
        .iter()
        .filter(|candidate| {
            if candidate.action == Action::EndTurn {
                return false;
            }
            if let Action::BuildRoad { edge } = &candidate.action
                && road_frontier_value(state, *edge, actor as u8) <= 0.04
            {
                return false;
            }
            let mut next = state.clone();
            next.apply(&candidate.action).is_ok() && next.players[actor].resource_total() < held
        })
        .max_by(|left, right| {
            robust_root_score(left, actor).total_cmp(&robust_root_score(right, actor))
        })?;
    // A tiny noisy search edge is not enough to justify exposing an at-risk
    // hand to the next orbit. Preserve EndTurn only when its modeled advantage
    // is material; the tolerance grows with overflow.
    let safety_tolerance = 0.015 + held.saturating_sub(state.card_discard_limit) as f32 * 0.01;
    (robust_root_score(alternative, actor) + safety_tolerance >= end_score)
        .then(|| alternative.action.clone())
}

#[derive(Default)]
struct Arena {
    parent: Vec<u32>,
    first_child: Vec<u32>,
    next_sibling: Vec<u32>,
    action: Vec<Option<Action>>,
    visits: Vec<u32>,
    availability: Vec<u32>,
    value_sum: Vec<[f32; 4]>,
    prior: Vec<f32>,
    state_hash: Vec<u64>,
    depth: Vec<u16>,
}

impl Arena {
    fn with_capacity(capacity: usize, root_hash: u64) -> Self {
        let mut arena = Self {
            parent: Vec::with_capacity(capacity),
            first_child: Vec::with_capacity(capacity),
            next_sibling: Vec::with_capacity(capacity),
            action: Vec::with_capacity(capacity),
            visits: Vec::with_capacity(capacity),
            availability: Vec::with_capacity(capacity),
            value_sum: Vec::with_capacity(capacity),
            prior: Vec::with_capacity(capacity),
            state_hash: Vec::with_capacity(capacity),
            depth: Vec::with_capacity(capacity),
        };
        arena.push(NONE, None, 1.0, root_hash, 0);
        arena
    }

    fn push(
        &mut self,
        parent: u32,
        action: Option<Action>,
        prior: f32,
        state_hash: u64,
        depth: u16,
    ) -> u32 {
        let index = self.parent.len() as u32;
        self.parent.push(parent);
        self.first_child.push(NONE);
        self.next_sibling.push(NONE);
        self.action.push(action);
        self.visits.push(0);
        self.availability.push(0);
        self.value_sum.push([0.0; 4]);
        self.prior.push(prior);
        self.state_hash.push(state_hash);
        self.depth.push(depth);
        if parent != NONE {
            self.next_sibling[index as usize] = self.first_child[parent as usize];
            self.first_child[parent as usize] = index;
        }
        index
    }

    fn children(&self, node: u32) -> Vec<u32> {
        let mut result = Vec::new();
        let mut child = self.first_child[node as usize];
        while child != NONE {
            result.push(child);
            child = self.next_sibling[child as usize];
        }
        result
    }

    fn child_for(&self, node: u32, action: &Action, identity: u64) -> Option<u32> {
        self.children(node).into_iter().find(|child| {
            self.action[*child as usize].as_ref() == Some(action)
                && self.state_hash[*child as usize] == identity
        })
    }

    fn mean_value(&self, node: u32) -> [f32; 4] {
        let visits = self.visits[node as usize];
        if visits == 0 {
            return [0.0; 4];
        }
        self.value_sum[node as usize].map(|value| value / visits as f32)
    }

    fn retained_subtree(&self, old_root: u32, capacity: usize) -> Self {
        let mut retained = Self::with_capacity(capacity.max(1), self.state_hash[old_root as usize]);
        retained.visits[0] = self.visits[old_root as usize];
        retained.availability[0] = self.availability[old_root as usize];
        retained.value_sum[0] = self.value_sum[old_root as usize];
        retained.prior[0] = self.prior[old_root as usize];
        retained.depth[0] = 0;
        let mut frontier = vec![(old_root, 0u32, self.depth[old_root as usize])];
        while let Some((old_parent, new_parent, root_depth)) = frontier.pop() {
            if retained.parent.len() >= capacity {
                break;
            }
            let mut children = self.children(old_parent);
            // Keep the best explored continuations first when compaction must
            // discard unreachable/low-visit history.
            children.sort_by_key(|child| std::cmp::Reverse(self.visits[*child as usize]));
            for old_child in children.into_iter().rev() {
                if retained.parent.len() >= capacity {
                    break;
                }
                let old_index = old_child as usize;
                let new_child = retained.push(
                    new_parent,
                    self.action[old_index].clone(),
                    self.prior[old_index],
                    self.state_hash[old_index],
                    self.depth[old_index].saturating_sub(root_depth),
                );
                let new_index = new_child as usize;
                retained.visits[new_index] = self.visits[old_index];
                retained.availability[new_index] = self.availability[old_index];
                retained.value_sum[new_index] = self.value_sum[old_index];
                frontier.push((old_child, new_child, root_depth));
            }
        }
        retained
    }
}

pub struct Mcts {
    config: SearchConfig,
    root: u32,
    root_hash: u64,
    arena: Arena,
    rng: SplitMix64,
    statistics: SearchStatistics,
    information_set_mode: bool,
    root_priors: Vec<(Action, f32)>,
    turn_plan_priors: Vec<(u64, Vec<(Action, f32)>)>,
    belief_fingerprint: Option<u64>,
}

impl Mcts {
    pub fn new(config: SearchConfig, state: &GameState) -> Self {
        let root_hash = state.state_hash();
        Self {
            arena: Arena::with_capacity(config.max_nodes, root_hash),
            root: 0,
            root_hash,
            rng: SplitMix64::new(config.seed),
            statistics: SearchStatistics::default(),
            config,
            information_set_mode: false,
            root_priors: Vec::new(),
            turn_plan_priors: Vec::new(),
            belief_fingerprint: None,
        }
    }

    pub fn reset(&mut self, state: &GameState) {
        self.reset_identity(if self.information_set_mode {
            information_identity(state)
        } else {
            state.state_hash()
        });
    }

    pub fn reconfigure(&mut self, mut config: SearchConfig) {
        config.max_nodes = config.max_nodes.max(2);
        let mode_changed = config.mode != self.config.mode;
        self.config = config;
        if mode_changed {
            let identity = self.root_hash;
            self.reset_identity(identity);
        } else if self.arena.parent.len() >= self.config.max_nodes.saturating_mul(3) / 4 {
            self.arena = self
                .arena
                .retained_subtree(self.root, (self.config.max_nodes / 2).max(2));
            self.root = 0;
        }
    }

    pub fn contains_identity(&self, identity: u64) -> bool {
        self.arena.state_hash.contains(&identity)
    }

    fn reset_identity(&mut self, identity: u64) {
        self.root_hash = identity;
        self.root = 0;
        self.arena = Arena::with_capacity(self.config.max_nodes, self.root_hash);
        self.statistics = SearchStatistics::default();
        self.root_priors.clear();
        self.turn_plan_priors.clear();
        self.belief_fingerprint = None;
    }

    fn reuse_identity(&mut self, identity: u64) -> bool {
        let candidate = self
            .arena
            .state_hash
            .iter()
            .enumerate()
            .filter(|(_, hash)| **hash == identity)
            .max_by_key(|(index, _)| self.arena.visits[*index])
            .map(|(index, _)| index as u32);
        let Some(candidate) = candidate else {
            return false;
        };
        self.arena = self
            .arena
            .retained_subtree(candidate, (self.config.max_nodes / 2).max(2));
        self.root = 0;
        self.root_hash = identity;
        self.root_priors.clear();
        true
    }

    pub fn reroot(&mut self, action: &Action, state: &GameState) -> bool {
        let expected = if self.information_set_mode {
            information_identity(state)
        } else {
            state.state_hash()
        };
        let Some(child) = self.arena.child_for(self.root, action, expected) else {
            self.reset(state);
            return false;
        };
        self.root = child;
        self.root_hash = expected;
        true
    }

    pub fn search(&mut self, state: &GameState) -> SearchReport {
        let timer = CooperativeDeadline::start(self.config.time_budget_ms);
        self.information_set_mode = false;
        self.belief_fingerprint = None;
        if state.state_hash() != self.root_hash && !self.reuse_identity(state.state_hash()) {
            self.reset(state);
        }
        self.statistics = SearchStatistics::default();
        let exact = solve_exact_belief(
            &[BeliefParticle {
                state: state.clone(),
                weight: 1.0,
            }],
            ExactActionFamily::Mandatory,
        );
        if exact.applicable {
            // Discard, robber, and trade-response prompts have one compact,
            // authoritative exact family. Do not spend the interactive budget
            // on an unrelated tactical tree and MCTS after that decision is
            // already known.
            self.reset(state);
            self.statistics = SearchStatistics::default();
            return self.finish_report(state, empty_tactical_result(), exact, None);
        }
        let tactical = solve_current_turn(
            state,
            self.config.tactical_depth,
            self.config.tactical_nodes,
        );
        self.prepare_root_priors(state);
        for _ in 0..self.config.iterations {
            if self.arena.parent.len() >= self.config.max_nodes {
                break;
            }
            if timer.expired_at_checkpoint(self.statistics.iterations, 1) {
                self.statistics.deadline_reached = true;
                break;
            }
            self.iteration(state);
            self.statistics.iterations += 1;
        }
        self.finish_report(state, tactical, exact, None)
    }

    /// Root-determinization baseline: every iteration samples one legal hidden
    /// world while one action tree aggregates availability and value.
    ///
    /// This is intentionally exposed as a separate stage from observation-keyed
    /// ISMCTS so benchmarks can measure strategy-fusion cost rather than
    /// silently labeling determinization as solved imperfect-information play.
    pub fn search_belief(&mut self, particles: &[GameState]) -> Result<SearchReport, BeliefError> {
        let weighted = particles
            .iter()
            .cloned()
            .map(|state| BeliefParticle { state, weight: 1.0 })
            .collect::<Vec<_>>();
        self.search_weighted_belief(&weighted)
    }

    pub fn search_weighted_belief(
        &mut self,
        particles: &[BeliefParticle],
    ) -> Result<SearchReport, BeliefError> {
        let timer = CooperativeDeadline::start(self.config.time_budget_ms);
        let Some(first_particle) = particles.first() else {
            return Err(BeliefError::Empty);
        };
        let first = &first_particle.state;
        if particles
            .iter()
            .any(|particle| !particle.weight.is_finite() || particle.weight < 0.0)
            || particles
                .iter()
                .map(|particle| particle.weight)
                .sum::<f32>()
                <= f32::EPSILON
        {
            return Err(BeliefError::InvalidWeight);
        }
        let observer = first.actor();
        let public = first.public_hash();
        let observation = first.observation_hash(observer);
        let posterior_fingerprint = belief_fingerprint(particles);
        if particles
            .iter()
            .any(|particle| particle.state.public_hash() != public)
        {
            return Err(BeliefError::PublicStateMismatch);
        }
        if particles
            .iter()
            .any(|particle| particle.state.observation_hash(observer) != observation)
        {
            return Err(BeliefError::RootObservationMismatch);
        }
        self.statistics = SearchStatistics::default();
        self.information_set_mode = true;
        if observation != self.root_hash {
            if !self.reuse_identity(observation) {
                self.reset_identity(observation);
            }
        } else if self
            .belief_fingerprint
            .is_some_and(|previous| previous != posterior_fingerprint)
        {
            self.reset_identity(observation);
        }
        self.belief_fingerprint = Some(posterior_fingerprint);
        let exact = solve_exact_belief(particles, ExactActionFamily::Mandatory);
        if exact.applicable {
            // The response family already evaluates every legal action over
            // the full weighted posterior. Resetting here also prevents stale
            // strategic children from a reused tree leaking into diagnostics.
            self.reset_identity(observation);
            self.statistics = SearchStatistics::default();
            return Ok(self.finish_report(first, empty_tactical_result(), exact, Some(particles)));
        }
        let tactical_particles = particles
            .iter()
            .map(|particle| (&particle.state, particle.weight))
            .collect::<Vec<_>>();
        let tactical = solve_belief_current_turn(
            &tactical_particles,
            self.config.tactical_depth,
            self.config.tactical_nodes,
        );
        self.prepare_root_priors(first);
        let total_weight = particles
            .iter()
            .map(|particle| particle.weight.max(0.0))
            .sum::<f32>()
            .max(f32::EPSILON);
        let weights = particles
            .iter()
            .map(|particle| particle.weight.max(0.0) / total_weight)
            .collect::<Vec<_>>();
        self.statistics.effective_particle_count = 1.0
            / weights
                .iter()
                .map(|weight| weight * weight)
                .sum::<f32>()
                .max(f32::EPSILON);
        for _ in 0..self.config.iterations {
            if self.arena.parent.len() >= self.config.max_nodes {
                break;
            }
            if timer.expired_at_checkpoint(self.statistics.iterations, 1) {
                self.statistics.deadline_reached = true;
                break;
            }
            let particle = &particles[sample_weighted_index(&weights, &mut self.rng)].state;
            self.iteration(particle);
            self.statistics.iterations += 1;
        }
        Ok(self.finish_report(first, tactical, exact, Some(particles)))
    }

    fn prepare_root_priors(&mut self, state: &GameState) {
        let legal = state.legal_actions();
        let actor = state.actor();
        let observed = state.observed_state(actor);
        let mut ranked = normalize_observed_priors(state, &legal, actor);
        plan_adjusted_priors(
            &observed,
            &mut ranked,
            // The root plan generator supplies an informed prior; it is not a
            // second exhaustive search. Reserve most of the node budget for
            // the shared information tree and refine plans through reuse.
            (self.config.max_nodes as u32 / 80).clamp(160, 2_000),
        );
        opening_adjusted_priors(
            &observed,
            &mut ranked,
            (self.config.max_nodes as u32 / 10).clamp(1_000, 18_000),
        );
        self.root_priors = order_scored_with_state_quotas(&observed, actor, ranked);
        let identity = if self.information_set_mode {
            information_identity(state)
        } else {
            state.state_hash()
        };
        self.remember_turn_priors(identity, self.root_priors.clone());
    }

    fn remember_turn_priors(&mut self, identity: u64, priors: Vec<(Action, f32)>) {
        if let Some((_, existing)) = self
            .turn_plan_priors
            .iter_mut()
            .find(|(candidate, _)| *candidate == identity)
        {
            *existing = priors;
            return;
        }
        if self.turn_plan_priors.len() >= 384 {
            self.turn_plan_priors.remove(0);
        }
        self.turn_plan_priors.push((identity, priors));
    }

    fn ranked_for_state(
        &mut self,
        state: &GameState,
        legal: &[Action],
        actor: u8,
    ) -> Vec<(Action, f32)> {
        let identity = if self.information_set_mode {
            information_identity(state)
        } else {
            state.state_hash()
        };
        if let Some((_, cached)) = self
            .turn_plan_priors
            .iter()
            .find(|(candidate, _)| *candidate == identity)
        {
            return cached
                .iter()
                .filter(|(action, _)| legal.contains(action))
                .cloned()
                .collect();
        }
        let observed = state.observed_state(actor);
        let mut ranked = normalize_observed_priors(state, legal, actor);
        if matches!(state.phase, colonist_catan_core::Phase::Main) {
            // The tree still returns one state-validated click at a time, but
            // every newly reached turn node is expanded from coherent endpoint
            // plans (trade → road → settlement, YOP → city, and so on).
            // Caching makes this a turn-level abstraction instead of paying for
            // the planner on every simulation through the same information set.
            plan_adjusted_priors(&observed, &mut ranked, 320);
        }
        if matches!(
            state.phase,
            colonist_catan_core::Phase::SetupSettlement
                | colonist_catan_core::Phase::SetupRoad { .. }
        ) {
            opening_adjusted_priors(&observed, &mut ranked, 900);
        }
        let ranked = order_scored_with_state_quotas(&observed, actor, ranked);
        self.remember_turn_priors(identity, ranked.clone());
        ranked
    }

    fn finish_report(
        &mut self,
        state: &GameState,
        tactical: TacticalResult,
        exact: ExactDecisionResult,
        particles: Option<&[BeliefParticle]>,
    ) -> SearchReport {
        self.statistics.nodes = self.arena.parent.len();
        let actor = state.actor() as usize;
        struct Aggregate {
            action: Action,
            visits: u32,
            availability: u32,
            prior: f32,
            value_sum: [f32; 4],
        }
        let mut aggregate = Vec::<Aggregate>::new();
        for child in self.arena.children(self.root) {
            let Some(action) = self.arena.action[child as usize].clone() else {
                continue;
            };
            let visits = self.arena.visits[child as usize];
            if let Some(existing) = aggregate.iter_mut().find(|entry| entry.action == action) {
                existing.visits += visits;
                existing.availability += self.arena.availability[child as usize];
                existing.prior = existing.prior.max(self.arena.prior[child as usize]);
                for player in 0..4 {
                    existing.value_sum[player] += self.arena.value_sum[child as usize][player];
                }
            } else {
                aggregate.push(Aggregate {
                    action,
                    visits,
                    availability: self.arena.availability[child as usize],
                    prior: self.arena.prior[child as usize],
                    value_sum: self.arena.value_sum[child as usize],
                });
            }
        }
        let mut actions = aggregate
            .into_iter()
            .map(|entry| {
                let conditional_value = entry
                    .value_sum
                    .map(|component| component / entry.visits.max(1) as f32);
                let (legal_weight, unavailable_value, unavailable_lower) =
                    particles.map_or((1.0, [0.0; 4], [1.0; 4]), |worlds| {
                        let total = worlds
                            .iter()
                            .map(|particle| particle.weight.max(0.0))
                            .sum::<f32>()
                            .max(f32::EPSILON);
                        let mut legal = 0.0;
                        let mut unavailable = [0.0_f32; 4];
                        let mut lower = [1.0_f32; 4];
                        for particle in worlds {
                            let weight = particle.weight.max(0.0) / total;
                            if particle.state.legal_actions().contains(&entry.action) {
                                legal += weight;
                                continue;
                            }
                            let baseline = evaluate(&particle.state);
                            for player in 0..4 {
                                unavailable[player] += baseline[player] * weight;
                                lower[player] = lower[player].min(baseline[player]);
                            }
                        }
                        (legal, unavailable, lower)
                    });
                // Root statistics are expectations over the full posterior.
                // A hidden-world-dependent action cannot inflate its score by
                // disappearing from worlds where it is illegal or unavailable.
                let value = conditional_value.map(|component| component * legal_weight);
                let value = std::array::from_fn(|player| value[player] + unavailable_value[player]);
                ActionStats {
                    action: entry.action,
                    visits: entry.visits,
                    availability: entry.availability,
                    availability_weight: entry.availability as f32
                        / self.arena.visits[self.root as usize].max(1) as f32,
                    legal_weight,
                    prior: entry.prior,
                    value,
                    lower_confidence_value: std::array::from_fn(|player| {
                        let component = conditional_value[player];
                        let standard_error = (component * (1.0 - component).max(0.0)
                            / entry.visits.max(1) as f32)
                            .sqrt();
                        let conditional_lower = (component - 1.96 * standard_error).clamp(0.0, 1.0);
                        let expected_lower =
                            conditional_lower * legal_weight + unavailable_value[player];
                        if legal_weight < 1.0 {
                            expected_lower.min(unavailable_lower[player])
                        } else {
                            expected_lower
                        }
                    }),
                }
            })
            .collect::<Vec<_>>();
        let safe_actions = actions
            .iter()
            .filter(|stats| {
                let threat = if let Some(worlds) = particles {
                    belief_domestic_trade_threat(
                        worlds
                            .iter()
                            .map(|particle| (&particle.state, particle.weight)),
                        &stats.action,
                    )
                } else {
                    belief_domestic_trade_threat(std::iter::once((state, 1.0)), &stats.action)
                };
                threat.is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        // Main-phase roots always retain EndTurn, and trade-response roots
        // retain reject/cancel. The fallback protects malformed diagnostic
        // roots without ever manufacturing an illegal action.
        if !safe_actions.is_empty() {
            actions = safe_actions;
        }
        rank_root_actions(&mut actions, actor);
        let mut exact = exact;
        let mut chosen = if exact.applicable {
            exact.chosen.clone()
        } else if tactical.proven {
            tactical.principal_line.first().cloned()
        } else {
            actions.first().map(|stats| stats.action.clone())
        };
        if chosen == Some(Action::EndTurn)
            && let Some(safer) = safer_end_turn_alternative(state, actor, &actions)
        {
            chosen = Some(safer);
        }
        if !exact.applicable
            && !tactical.proven
            && let Some(family) = chosen.as_ref().and_then(exact_family_for_action)
        {
            exact = if let Some(worlds) = particles {
                solve_exact_belief(worlds, family)
            } else {
                solve_exact_belief(
                    &[BeliefParticle {
                        state: state.clone(),
                        weight: 1.0,
                    }],
                    family,
                )
            };
            chosen = exact.chosen.clone().or(chosen);
        }
        SearchReport {
            chosen,
            root_value: self.arena.mean_value(self.root),
            actions,
            tactical,
            exact,
            statistics: self.statistics.clone(),
        }
    }

    fn iteration(&mut self, root_state: &GameState) {
        let mut state = root_state.clone();
        let mut node = self.root;
        let mut path = vec![node];
        let root_turn = state.turn;
        let root_actor = root_state.actor();
        let mut strategic_depth = 0u16;
        let mut atomic_depth = 0u16;

        while !state.is_terminal()
            && strategic_depth < self.config.max_tree_depth
            && atomic_depth < 120
            && self.arena.parent.len() < self.config.max_nodes
        {
            atomic_depth += 1;
            match state.node_kind() {
                NodeKind::Terminal => break,
                NodeKind::Chance => {
                    let Some(action) = state.sample_chance(&mut self.rng) else {
                        break;
                    };
                    state
                        .apply(&action)
                        .expect("sampled chance outcome must be legal");
                    strategic_depth = state.turn.saturating_sub(root_turn);
                    let identity = if self.information_set_mode {
                        information_identity(&state)
                    } else {
                        state.state_hash()
                    };
                    node = if let Some(child) = self.arena.child_for(node, &action, identity) {
                        child
                    } else {
                        self.arena
                            .push(node, Some(action), 1.0, identity, strategic_depth)
                    };
                    path.push(node);
                }
                NodeKind::Decision { actor } => {
                    self.statistics.deepest_decision_depth =
                        self.statistics.deepest_decision_depth.max(strategic_depth);
                    let legal = state.legal_actions();
                    if legal.is_empty() {
                        break;
                    }
                    let ranked = if node == self.root {
                        self.root_priors
                            .iter()
                            .filter(|(action, _)| legal.contains(action))
                            .cloned()
                            .collect::<Vec<_>>()
                    } else {
                        self.ranked_for_state(&state, &legal, actor)
                    };
                    let children = self.arena.children(node);
                    let available_children = children
                        .iter()
                        .copied()
                        .filter(|child| {
                            let Some(action) = self.arena.action[*child as usize].as_ref() else {
                                return false;
                            };
                            if !legal.contains(action) {
                                return false;
                            }
                            let mut next = state.clone();
                            if next.apply(action).is_err() {
                                return false;
                            }
                            let identity = if self.information_set_mode {
                                information_identity(&next)
                            } else {
                                next.state_hash()
                            };
                            self.arena.state_hash[*child as usize] == identity
                        })
                        .collect::<Vec<_>>();
                    for child in &available_children {
                        if legal.contains(
                            self.arena.action[*child as usize]
                                .as_ref()
                                .expect("child has action"),
                        ) {
                            self.arena.availability[*child as usize] += 1;
                        }
                    }
                    let widening =
                        progressive_width(&self.config, self.arena.visits[node as usize]);
                    let unexpanded = ranked.iter().find(|(action, _)| {
                        let mut next = state.clone();
                        if next.apply(action).is_err() {
                            return false;
                        }
                        let identity = if self.information_set_mode {
                            information_identity(&next)
                        } else {
                            next.state_hash()
                        };
                        self.arena.child_for(node, action, identity).is_none()
                    });
                    let force_expand = unexpanded.is_some_and(|(action, _)| {
                        let mut next = state.clone();
                        next.apply(action).is_ok() && next.winner() == Some(actor)
                    });
                    if (available_children.len() < widening || force_expand)
                        && let Some((action, prior)) = unexpanded
                    {
                        state
                            .apply(action)
                            .expect("ranked legal action must transition");
                        strategic_depth = state.turn.saturating_sub(root_turn);
                        let identity = if self.information_set_mode {
                            information_identity(&state)
                        } else {
                            state.state_hash()
                        };
                        node = self.arena.push(
                            node,
                            Some(action.clone()),
                            *prior,
                            identity,
                            strategic_depth,
                        );
                        self.arena.availability[node as usize] = 1;
                        path.push(node);
                        break;
                    }
                    let parent_visits = self.arena.visits[node as usize].max(1) as f32;
                    let selected = if actor == root_actor {
                        available_children.into_iter().max_by(|a, b| {
                            self.selection_score(*a, actor as usize, parent_visits)
                                .total_cmp(&self.selection_score(*b, actor as usize, parent_visits))
                        })
                    } else {
                        // An observation-only bounded-rational opponent policy
                        // avoids strategy fusion. Its own sampled hand decides
                        // legality; public state, learned priors, and the
                        // inferred policy profile decide the distribution.
                        // Rollout Q is deliberately excluded because it embeds
                        // downstream facts from the full determinization.
                        let temperature = self.config.opponent_temperature.max(0.05);
                        let weights = available_children
                            .iter()
                            .map(|child| {
                                self.arena.prior[*child as usize]
                                    .max(0.0001)
                                    .powf(1.0 / temperature)
                            })
                            .collect::<Vec<_>>();
                        (!available_children.is_empty()).then(|| {
                            available_children[sample_weighted_index(&weights, &mut self.rng)]
                        })
                    };
                    let Some(selected) = selected else {
                        break;
                    };
                    let action = self.arena.action[selected as usize]
                        .as_ref()
                        .expect("child has incoming action")
                        .clone();
                    state
                        .apply(&action)
                        .expect("available child action must transition");
                    strategic_depth = state.turn.saturating_sub(root_turn);
                    node = selected;
                    path.push(node);
                }
            }
        }

        let value = if state.is_terminal() {
            evaluate(&state)
        } else {
            self.rollout(&mut state)
        };
        for visited in path {
            self.arena.visits[visited as usize] += 1;
            for (player, component) in value.iter().enumerate() {
                self.arena.value_sum[visited as usize][player] += component;
            }
        }
    }

    fn selection_score(&self, child: u32, actor: usize, parent_visits: f32) -> f32 {
        let visits = self.arena.visits[child as usize] as f32;
        let exploitation = if visits > 0.0 {
            self.arena.value_sum[child as usize][actor] / visits
        } else {
            0.0
        };
        let exploration = match self.config.mode {
            SearchMode::Uct => {
                self.config.exploration * ((parent_visits + 1.0).ln() / (visits + 1.0)).sqrt()
            }
            SearchMode::Puct => {
                self.config.exploration * self.arena.prior[child as usize] * parent_visits.sqrt()
                    / (1.0 + visits)
            }
        };
        exploitation + exploration
    }

    fn rollout(&mut self, state: &mut GameState) -> [f32; 4] {
        self.statistics.rollouts += 1;
        // Compare branches at a common strategic horizon. Counting only
        // primitive actions let trade-heavy or road-first lines terminate
        // before the same opponent turns that an immediate EndTurn exposed.
        let starting_turn = state.turn;
        let target_turns = (self.config.rollout_actions / 16).clamp(4, 12);
        let atomic_safety_cap = self.config.rollout_actions.saturating_mul(3);
        for _ in 0..atomic_safety_cap {
            if state.is_terminal() {
                break;
            }
            if state.turn.saturating_sub(starting_turn) >= target_turns {
                break;
            }
            let actions = state.legal_actions();
            if actions.is_empty() {
                break;
            }
            let action = choose_rollout_action(state, &actions, &mut self.rng);
            state.apply(&action).unwrap_or_else(|error| {
                panic!(
                    "rollout policy selected {action:?} from phase {:?} actor {} hash {:016x}, trade {:?}, hands {:?}, but transition failed: {error:?}",
                    state.phase,
                    state.actor(),
                    state.state_hash(),
                    state.trade,
                    state.players.iter().map(|player| player.resources).collect::<Vec<_>>(),
                )
            });
        }
        evaluate(state)
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, NodeKind, Phase, Resource, SplitMix64};

    use super::{
        ActionStats, BeliefError, BeliefParticle, Mcts, SearchConfig, progressive_width,
        rank_root_actions, safer_end_turn_alternative,
    };

    #[test]
    fn progressive_widening_follows_k_times_visits_to_alpha() {
        let config = SearchConfig {
            progressive_width_k: 2.0,
            progressive_width_alpha: 0.5,
            ..SearchConfig::default()
        };
        assert_eq!(progressive_width(&config, 1), 2);
        assert_eq!(progressive_width(&config, 25), 10);
        assert_eq!(progressive_width(&config, 100), 20);
        assert_eq!(progressive_width(&config, 10_000), 200);
    }

    fn root_stats(action: Action, value: f32) -> ActionStats {
        ActionStats {
            action,
            visits: 20,
            availability: 20,
            availability_weight: 1.0,
            legal_weight: 1.0,
            prior: 0.5,
            value: [value; 4],
            lower_confidence_value: [value; 4],
        }
    }

    #[test]
    fn one_visit_outlier_does_not_beat_a_supported_root_action() {
        let mut supported = root_stats(Action::EndTurn, 0.54);
        supported.visits = 24;
        let mut outlier = root_stats(Action::BuyDevelopment, 0.99);
        outlier.visits = 1;
        let mut actions = vec![outlier, supported];

        rank_root_actions(&mut actions, 0);

        assert_eq!(actions[0].action, Action::EndTurn);
    }

    fn advance_setup_and_roll(state: &mut GameState, rng: &mut SplitMix64) {
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.apply(&Action::Roll).unwrap();
        assert_eq!(state.node_kind(), NodeKind::Chance);
        let roll = state.sample_chance(rng).unwrap();
        state.apply(&roll).unwrap();
        if state.phase == Phase::Discard {
            while state.phase == Phase::Discard {
                let discard = state.legal_actions()[0].clone();
                state.apply(&discard).unwrap();
            }
            let robber = state.legal_actions()[0].clone();
            state.apply(&robber).unwrap();
            if state.node_kind() == NodeKind::Chance {
                let steal = state.sample_chance(rng).unwrap();
                state.apply(&steal).unwrap();
            }
        } else if state.phase == Phase::MoveRobber {
            let robber = state.legal_actions()[0].clone();
            state.apply(&robber).unwrap();
            if state.node_kind() == NodeKind::Chance {
                let steal = state.sample_chance(rng).unwrap();
                state.apply(&steal).unwrap();
            }
        }
    }

    #[test]
    fn search_is_seeded_and_returns_legal_action() {
        let mut state = GameState::standard(7, 4);
        let mut rng = SplitMix64::new(9);
        advance_setup_and_roll(&mut state, &mut rng);
        let config = SearchConfig {
            iterations: 120,
            max_nodes: 5_000,
            rollout_actions: 80,
            tactical_nodes: 200,
            ..SearchConfig::default()
        };
        let mut first = Mcts::new(config.clone(), &state);
        let mut second = Mcts::new(config, &state);
        let first_report = first.search(&state);
        let second_report = second.search(&state);
        assert_eq!(first_report.chosen, second_report.chosen);
        assert!(
            state
                .legal_actions()
                .contains(first_report.chosen.as_ref().unwrap())
        );
        assert_eq!(first_report.statistics.iterations, 120);
        assert!(first_report.statistics.nodes > 1);
    }

    #[test]
    fn strategic_simulations_honor_the_cooperative_deadline() {
        let mut state = GameState::standard(8, 4);
        let mut rng = SplitMix64::new(10);
        advance_setup_and_roll(&mut state, &mut rng);
        let config = SearchConfig {
            iterations: 50_000,
            max_nodes: 100_000,
            rollout_actions: 80,
            tactical_nodes: 100,
            time_budget_ms: 1,
            ..SearchConfig::default()
        };
        let mut search = Mcts::new(config, &state);
        let report = search.search(&state);

        assert!(report.statistics.deadline_reached);
        assert!(
            state
                .legal_actions()
                .contains(report.chosen.as_ref().unwrap())
        );
    }

    #[test]
    fn close_end_turn_ties_prefer_reducing_an_unsafe_hand() {
        let mut state = GameState::standard(71, 4);
        let mut rng = SplitMix64::new(72);
        advance_setup_and_roll(&mut state, &mut rng);
        let actor = state.actor() as usize;
        state.players[actor].resources = [0, 0, 4, 3, 2];
        let actions = vec![
            root_stats(Action::EndTurn, 0.5325),
            root_stats(Action::BuyDevelopment, 0.5306),
        ];

        assert_eq!(
            safer_end_turn_alternative(&state, actor, &actions),
            Some(Action::BuyDevelopment),
        );
    }

    #[test]
    fn end_turn_safety_uses_the_best_robust_hand_reducing_alternative() {
        let mut state = GameState::standard(75, 4);
        let mut rng = SplitMix64::new(76);
        advance_setup_and_roll(&mut state, &mut rng);
        let actor = state.actor() as usize;
        state.players[actor].resources = [0, 0, 4, 3, 2];

        let end_turn = root_stats(Action::EndTurn, 0.54);
        let mut brittle_high_mean = root_stats(Action::BuyDevelopment, 0.55);
        brittle_high_mean.lower_confidence_value = [0.30; 4];
        brittle_high_mean.legal_weight = 0.55;
        let mut robust_lower_mean = root_stats(
            Action::MaritimeTrade {
                give: Resource::Wool,
                receive: Resource::Ore,
                ratio: 4,
            },
            0.535,
        );
        robust_lower_mean.lower_confidence_value = [0.535; 4];

        let actions = vec![end_turn, brittle_high_mean, robust_lower_mean.clone()];
        assert_eq!(
            safer_end_turn_alternative(&state, actor, &actions),
            Some(robust_lower_mean.action),
        );
    }

    #[test]
    fn materially_better_end_turn_is_not_replaced_by_hand_cleanup() {
        let mut state = GameState::standard(73, 4);
        let mut rng = SplitMix64::new(74);
        advance_setup_and_roll(&mut state, &mut rng);
        let actor = state.actor() as usize;
        state.players[actor].resources = [0, 0, 4, 3, 2];
        let actions = vec![
            root_stats(Action::EndTurn, 0.60),
            root_stats(Action::BuyDevelopment, 0.50),
        ];

        assert_eq!(safer_end_turn_alternative(&state, actor, &actions), None);
    }

    #[test]
    fn tree_can_retain_a_matching_child() {
        let state = GameState::standard(2, 3);
        let config = SearchConfig {
            iterations: 32,
            rollout_actions: 20,
            ..SearchConfig::default()
        };
        let mut search = Mcts::new(config, &state);
        let report = search.search(&state);
        let action = report.chosen.unwrap();
        let mut next = state.clone();
        next.apply(&action).unwrap();
        assert!(search.reroot(&action, &next));
    }

    #[test]
    fn repeated_search_refines_the_same_information_tree() {
        let mut state = GameState::standard(12, 3);
        let mut rng = SplitMix64::new(13);
        advance_setup_and_roll(&mut state, &mut rng);
        let config = SearchConfig {
            iterations: 48,
            max_nodes: 8_000,
            rollout_actions: 40,
            tactical_nodes: 200,
            ..SearchConfig::default()
        };
        let mut search = Mcts::new(config, &state);
        let first = search.search(&state);
        let first_visits = first
            .actions
            .iter()
            .map(|action| action.visits)
            .sum::<u32>();
        let second = search.search(&state);
        let second_visits = second
            .actions
            .iter()
            .map(|action| action.visits)
            .sum::<u32>();
        assert!(second_visits > first_visits);
        assert!(second.chosen.is_some());
    }

    #[test]
    fn belief_search_accepts_hidden_worlds_with_one_root_observation() {
        let mut first = GameState::standard(27, 4);
        let mut rng = SplitMix64::new(31);
        advance_setup_and_roll(&mut first, &mut rng);
        let mut second = first.clone();
        let total = first.players[1].resource_total();
        first.players[1].resources = [total, 0, 0, 0, 0];
        second.players[1].resources = [0, total, 0, 0, 0];
        assert_eq!(first.public_hash(), second.public_hash());
        assert_eq!(
            first.observation_hash(first.actor()),
            second.observation_hash(second.actor())
        );

        let config = SearchConfig {
            iterations: 80,
            rollout_actions: 50,
            tactical_nodes: 200,
            ..SearchConfig::default()
        };
        let mut search = Mcts::new(config, &first);
        let report = search.search_belief(&[first.clone(), second]).unwrap();
        assert!(
            first
                .legal_actions()
                .contains(report.chosen.as_ref().unwrap())
        );
        assert!(report.statistics.nodes > 1);
    }

    #[test]
    fn belief_search_rejects_a_zero_mass_posterior() {
        let state = GameState::standard(91, 3);
        let mut search = Mcts::new(SearchConfig::default(), &state);
        let error = search
            .search_weighted_belief(&[BeliefParticle { state, weight: 0.0 }])
            .unwrap_err();
        assert_eq!(error, BeliefError::InvalidWeight);
    }

    #[test]
    fn belief_tree_reuses_equivalent_ordering_and_resets_on_new_posterior() {
        let mut first = GameState::standard(93, 3);
        let mut rng = SplitMix64::new(94);
        advance_setup_and_roll(&mut first, &mut rng);
        let hidden_player = (first.actor() as usize + 1) % 3;
        first.players[hidden_player].resources = [2, 1, 0, 0, 0];
        let mut second = first.clone();
        second.players[hidden_player].resources.rotate_left(1);
        assert_eq!(
            first.observation_hash(first.actor()),
            second.observation_hash(second.actor())
        );
        let config = SearchConfig {
            iterations: 32,
            max_nodes: 6_000,
            rollout_actions: 32,
            tactical_nodes: 100,
            ..SearchConfig::default()
        };
        let mut search = Mcts::new(config, &first);
        let first_report = search
            .search_weighted_belief(&[
                BeliefParticle {
                    state: first.clone(),
                    weight: 0.5,
                },
                BeliefParticle {
                    state: second.clone(),
                    weight: 0.5,
                },
            ])
            .unwrap();
        let first_visits = first_report
            .actions
            .iter()
            .map(|action| action.visits)
            .sum::<u32>();
        let reordered = search
            .search_weighted_belief(&[
                BeliefParticle {
                    state: second.clone(),
                    weight: 0.5,
                },
                BeliefParticle {
                    state: first.clone(),
                    weight: 0.5,
                },
            ])
            .unwrap();
        let reordered_visits = reordered
            .actions
            .iter()
            .map(|action| action.visits)
            .sum::<u32>();
        assert!(reordered_visits > first_visits);

        let changed = search
            .search_weighted_belief(&[
                BeliefParticle {
                    state: first,
                    weight: 0.9,
                },
                BeliefParticle {
                    state: second,
                    weight: 0.1,
                },
            ])
            .unwrap();
        let changed_visits = changed
            .actions
            .iter()
            .map(|action| action.visits)
            .sum::<u32>();
        assert!(changed_visits < reordered_visits);
    }
}
