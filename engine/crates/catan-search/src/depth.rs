use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use colonist_catan_core::{Action, GameState, NodeKind, Phase};

use crate::deadline::CooperativeDeadline;
use crate::eval::{RoadIntent, evaluate, road_intent, strategic_utility};
use crate::exact::{
    DEVELOPMENT_EXACT_FAMILIES, ExactActionFamily, ExactDecisionResult, exact_family_for_action,
    solve_exact_belief_excluding_controlled,
};
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
use crate::exact::solve_exact_belief;
use crate::mcts::BeliefParticle;
use crate::opening::opening_adjusted_priors;
use crate::opening::{OpeningConfig, solve_opening};
use crate::planner::{TurnPlan, plan_adjusted_priors, plan_adjusted_priors_with_plans};
use crate::policy::{
    actor_proposal_actions, allocate_root_node_budgets, normalize_observed_priors,
    normalize_priors, order_scored_with_state_quotas, rank_with_class_quotas,
    truncate_root_preserving_end_turn,
};
use crate::root_impact::{
    IntroducedRoadFragility, RootPromotionReason, RootStrategicImpact, apply_closeout_root_impacts,
    compute_spatial_root_impacts,
};
use crate::shared::{
    admit_promoted_roots, coalesce_identical_particles, immediate_winning_roots,
    select_experimental_strategic_particles,
};
use crate::threats::{
    RoadCutContinuationAssessment, belief_road_cut_continuation_assessment, forced_loss_weight,
    posterior_immediate_threat_weight,
};
use crate::trade_safety::{
    DomesticTradeThreat, HARD_VETO_POSTERIOR, belief_domestic_trade_assessment,
    belief_domestic_trade_threat,
};

// Convenience APIs must remain safe in UI/tests. Production callers that
// explicitly want a larger budget use the `_bounded` variants.
const DEFAULT_DEPTH_NODE_BUDGET: u32 = 8_000;
const DOMESTIC_OFFER_FRICTION: f32 = 0.006;
const COUNTEROFFER_FRICTION: f32 = 0.004;
const MAX_ROOT_PROVENANCE: usize = 256;

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RootPruneReason {
    RootExcluded,
    BranchTruncated,
    TradeSafety,
    ExactFamilyCollapsed,
}

#[derive(Clone, Debug)]
pub struct RankedRootDiagnostic {
    pub action: Action,
    pub rank: usize,
    pub prior: f32,
    pub planner_value: Option<f32>,
    pub planner_completion_mass: Option<f32>,
    pub planner_decisive_completion_mass: Option<f32>,
    pub planner_response_windows: Option<f32>,
    pub(crate) quota_score: f32,
}

#[derive(Clone, Debug)]
pub struct RetainedRootDiagnostic {
    pub action: Action,
    pub pre_truncation_rank: Option<usize>,
    pub prior: f32,
    pub node_budget_per_particle: u32,
    pub allocated_nodes: u32,
    pub planner_value: Option<f32>,
    pub planner_completion_mass: Option<f32>,
    pub planner_decisive_completion_mass: Option<f32>,
    pub planner_response_windows: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct PrunedRootDiagnostic {
    pub action: Action,
    pub pre_truncation_rank: Option<usize>,
    pub reason: RootPruneReason,
}

#[derive(Clone, Debug)]
pub struct RootCausalEvidence {
    pub action: Action,
    pub promotion_reason: Option<RootPromotionReason>,
    pub road_intent: Option<RoadIntent>,
    /// Structural road vulnerability introduced or worsened by this root.
    pub introduced_road_fragility: IntroducedRoadFragility,
    /// Belief-weighted, legally proved opponent road -> settlement exploitation
    /// of the introduced cut vertices. Diagnostic/escalation evidence only.
    pub road_cut_continuation: RoadCutContinuationAssessment,
    /// True only when adding measured promotion reasons changed this root from
    /// outside to inside the shared admission result.
    pub admitted_by_promotion: bool,
    pub closeout_gain: f32,
    pub response_windows: Option<f32>,
    pub decisive_completion_mass: f32,
    pub trade_threat: Option<DomesticTradeThreat>,
    pub trade_risk_posterior: f32,
    pub dirty_monopoly_posterior: f32,
    pub trade_hard_veto_posterior: f32,
    pub trade_hard_veto: bool,
}

fn road_intent_for_root(state: &GameState, actor: u8, action: &Action) -> Option<RoadIntent> {
    match action {
        Action::BuildRoad { edge } | Action::PlaceRoad { edge } => {
            Some(road_intent(state, *edge, actor))
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
pub struct BeliefSearchProvenance {
    pub ranked_root_count: usize,
    pub ranked_roots: Vec<RankedRootDiagnostic>,
    pub retained_roots: Vec<RetainedRootDiagnostic>,
    pub pruned_root_count: usize,
    pub pruned_roots: Vec<PrunedRootDiagnostic>,
    pub root_evidence: Vec<RootCausalEvidence>,
    pub trade_hard_veto_threshold: f32,
    /// Ordinary backed-up search winner before any later safety replacement.
    pub search_winner: Option<Action>,
    pub exact_family_replacement: Option<(Action, Action)>,
    pub exact_family_results: Vec<(ExactActionFamily, ExactDecisionResult)>,
    pub safety_replacement: Option<(Action, Action)>,
}

impl Default for BeliefSearchProvenance {
    fn default() -> Self {
        Self {
            ranked_root_count: 0,
            ranked_roots: Vec::new(),
            retained_roots: Vec::new(),
            pruned_root_count: 0,
            pruned_roots: Vec::new(),
            root_evidence: Vec::new(),
            trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
            search_winner: None,
            exact_family_replacement: None,
            exact_family_results: Vec::new(),
            safety_replacement: None,
        }
    }
}

fn road_cut_continuation_for_root(
    particles: &[BeliefParticle],
    actor: u8,
    action: &Action,
    impact: Option<&RootStrategicImpact>,
) -> RoadCutContinuationAssessment {
    let exposed_vertices = impact
        .map(|impact| {
            impact
                .introduced_road_fragility
                .critical_vertices
                .iter()
                .map(|cut| cut.vertex)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if exposed_vertices.is_empty() {
        return RoadCutContinuationAssessment::default();
    }
    belief_road_cut_continuation_assessment(
        particles
            .iter()
            .map(|particle| (&particle.state, particle.weight)),
        actor,
        action,
        &exposed_vertices,
    )
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BeliefSearchStageTimings {
    pub particle_preparation_ms: u32,
    pub root_scoring_ms: u32,
    pub exact_families_ms: u32,
    pub threat_safety_ms: u32,
    pub one_ply_floor_ms: u32,
    pub deep_waves_ms: u32,
    pub floor_complete: bool,
    pub attempted_depth: u8,
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
    pub stage_timings: Option<BeliefSearchStageTimings>,
    pub provenance: BeliefSearchProvenance,
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
    HostilityStress { root: u8, hostility: f32 },
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
        ranked[start..end]
            .sort_by(|left, right| format!("{:?}", left.0).cmp(&format!("{:?}", right.0)));
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
    let observed_ranked = normalize_priors(&observed, actions, actor);
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
    evaluation_cache: Rc<RefCell<HashMap<u64, [f32; 4]>>>,
}

struct DecisionVisitContext {
    depth: u8,
    actions_in_turn: u8,
    alpha: f32,
    beta: f32,
    subtree_limit: u32,
}

fn normalize_belief_root_priors_with_diagnostics(
    particles: &[BeliefParticle],
    actor: u8,
    planner_nodes: u32,
) -> Vec<RankedRootDiagnostic> {
    struct Aggregate {
        action: Action,
        prior: f32,
        quota_score: f32,
        planner_value: f32,
        planner_completion_mass: f32,
        planner_decisive_completion_mass: f32,
        planner_weighted_response_windows: f32,
        planner_response_weight: f32,
        planner_weight: f32,
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
        let legal = actor_proposal_actions(&particle.state);
        let mut ranked = normalize_priors(&particle.state, &legal, actor);
        let plans = plan_adjusted_priors_with_plans(
            &particle.state,
            &mut ranked,
            per_particle_planner_nodes,
        );
        let ordered = order_scored_with_state_quotas(&particle.state, actor, ranked);
        let rank_scale = ordered.len().max(1) as f32;
        for (position, (action, prior)) in ordered.into_iter().enumerate() {
            let quota_score = (rank_scale - position as f32) / rank_scale;
            let planner = plans.iter().find(|plan| plan.first_action == action);
            let planner_weight = planner.map_or(0.0, |_| weight);
            let planner_value = planner.map_or(0.0, |plan| plan.value * weight);
            let planner_completion_mass = planner.map_or(0.0, |plan| plan.completion_mass * weight);
            let planner_decisive_completion_mass =
                planner.map_or(0.0, |plan| plan.decisive_completion_mass * weight);
            let planner_weighted_response_windows = planner
                .and_then(|plan| plan.response_windows)
                .map_or(0.0, |windows| windows * weight);
            let planner_response_weight = planner
                .and_then(|plan| plan.response_windows)
                .map_or(0.0, |_| weight);
            if let Some(existing) = aggregate
                .iter_mut()
                .find(|candidate| candidate.action == action)
            {
                existing.prior += prior * weight;
                existing.quota_score += quota_score * weight;
                existing.planner_value += planner_value;
                existing.planner_completion_mass += planner_completion_mass;
                existing.planner_decisive_completion_mass += planner_decisive_completion_mass;
                existing.planner_weighted_response_windows += planner_weighted_response_windows;
                existing.planner_response_weight += planner_response_weight;
                existing.planner_weight += planner_weight;
            } else {
                aggregate.push(Aggregate {
                    action,
                    prior: prior * weight,
                    quota_score: quota_score * weight,
                    planner_value,
                    planner_completion_mass,
                    planner_decisive_completion_mass,
                    planner_weighted_response_windows,
                    planner_response_weight,
                    planner_weight,
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
        .enumerate()
        .map(|(rank, candidate)| RankedRootDiagnostic {
            action: candidate.action,
            rank: rank + 1,
            prior: candidate.prior,
            planner_value: (candidate.planner_weight > f32::EPSILON)
                .then_some(candidate.planner_value / candidate.planner_weight),
            planner_completion_mass: (candidate.planner_weight > f32::EPSILON)
                .then_some(candidate.planner_completion_mass.clamp(0.0, 1.0)),
            planner_decisive_completion_mass: (candidate.planner_weight > f32::EPSILON)
                .then_some(candidate.planner_decisive_completion_mass.clamp(0.0, 1.0)),
            planner_response_windows: (candidate.planner_response_weight > f32::EPSILON).then_some(
                (candidate.planner_weighted_response_windows / candidate.planner_response_weight)
                    .max(0.0),
            ),
            quota_score: candidate.quota_score,
        })
        .collect()
}

fn closeout_plans_from_ranked_diagnostics(
    ranked_diagnostics: &[RankedRootDiagnostic],
) -> Vec<TurnPlan> {
    ranked_diagnostics
        .iter()
        .filter_map(|candidate| {
            Some(TurnPlan {
                first_action: candidate.action.clone(),
                actions: Vec::new(),
                value: candidate.planner_value?,
                nodes: 0,
                completion_mass: candidate.planner_completion_mass?,
                decisive_completion_mass: candidate.planner_decisive_completion_mass.unwrap_or(0.0),
                response_windows: candidate.planner_response_windows,
            })
        })
        .collect()
}

pub fn belief_root_closeout_plans(
    particles: &[BeliefParticle],
    actor: u8,
    planner_nodes: u32,
) -> Vec<TurnPlan> {
    let ranked_diagnostics =
        normalize_belief_root_priors_with_diagnostics(particles, actor, planner_nodes);
    closeout_plans_from_ranked_diagnostics(&ranked_diagnostics)
}

#[cfg(test)]
fn normalize_belief_root_priors(
    particles: &[BeliefParticle],
    actor: u8,
    planner_nodes: u32,
) -> Vec<(Action, f32)> {
    normalize_belief_root_priors_with_diagnostics(particles, actor, planner_nodes)
        .into_iter()
        .map(|candidate| (candidate.action, candidate.prior))
        .collect()
}

impl Searcher {
    fn evaluate_cached(&self, state: &GameState) -> [f32; 4] {
        let hash = state.state_hash();
        if let Some(value) = self.evaluation_cache.borrow().get(&hash) {
            return *value;
        }
        let value = evaluate(state);
        self.evaluation_cache.borrow_mut().insert(hash, value);
        value
    }

    fn decision_maximizes(&self, actor: u8) -> bool {
        match self.algorithm {
            Algorithm::MaxN | Algorithm::HostilityStress { .. } => true,
            Algorithm::Paranoid { root } => actor == root,
        }
    }

    fn decision_scalar(&self, actor: u8, value: &[f32; 4]) -> f32 {
        match self.algorithm {
            Algorithm::MaxN => value[actor as usize],
            Algorithm::HostilityStress { root, .. } if actor == root => value[root as usize],
            Algorithm::HostilityStress { root, hostility } => {
                (1.0 - hostility) * value[actor as usize] + hostility * (1.0 - value[root as usize])
            }
            Algorithm::Paranoid { root } => value[root as usize],
        }
    }

    fn visit_ranked_decision(
        &mut self,
        state: &GameState,
        actor: u8,
        mut ranked: Vec<(Action, f32)>,
        context: DecisionVisitContext,
    ) -> ([f32; 4], Option<Action>) {
        let DecisionVisitContext {
            depth,
            actions_in_turn,
            mut alpha,
            mut beta,
            subtree_limit,
        } = context;
        canonicalize_equal_prior_siblings(&mut ranked);
        let remaining = subtree_limit.saturating_sub(self.nodes);
        if remaining == 0 || ranked.is_empty() {
            return (self.evaluate_cached(state), None);
        }
        ranked.truncate(ranked.len().min(remaining as usize));
        let maximize = self.decision_maximizes(actor);
        let mut best = [0.0; 4];
        let mut chosen = None;
        let mut best_scalar = if maximize {
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
            let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
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
                self.evaluate_cached(&next)
            };
            let used = self.nodes.saturating_sub(before);
            carry = allowance.saturating_sub(used);
            if self.deadline_reached {
                return (self.evaluate_cached(state), None);
            }
            apply_action_friction(&mut child, state, &action, actor);
            let scalar = self.decision_scalar(actor, &child);
            let improves = if maximize {
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
                if maximize {
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
            (self.evaluate_cached(state), None)
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
            return self.evaluate_cached(state);
        }
        if self.deadline.expired_at_checkpoint(self.nodes, 8) {
            self.deadline_reached = true;
            return self.evaluate_cached(state);
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() || depth >= self.maximum_depth || actions_in_turn >= 18 {
            return self.evaluate_cached(state);
        }
        let exact_actions = state.legal_actions();
        if exact_actions.is_empty() {
            return self.evaluate_cached(state);
        }
        match state.node_kind() {
            NodeKind::Terminal => self.evaluate_cached(state),
            NodeKind::Chance => {
                let total = exact_actions
                    .iter()
                    .map(|action| state.chance_weight(action) as f32)
                    .sum::<f32>()
                    .max(f32::EPSILON);
                let weighted_actions = exact_actions
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
                        self.evaluate_cached(&next)
                    };
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                    if self.deadline_reached {
                        return self.evaluate_cached(state);
                    }
                    for player in 0..4 {
                        expected[player] += child[player] * weight;
                    }
                }
                expected
            }
            NodeKind::Decision { actor } => {
                // Belief recursion derives candidate availability and ordering
                // from the acting player's information set. Exact particles
                // remain authoritative for applying/evaluating the selected
                // proposal. Perfect-information diagnostic search intentionally
                // keeps the exact legal domain.
                let remaining = subtree_limit.saturating_sub(self.nodes);
                if remaining == 0 {
                    return self.evaluate_cached(state);
                }
                let observation_safe = self.observation_safe_recursive;
                let proposal_actions;
                let actions = if observation_safe {
                    proposal_actions = actor_proposal_actions(state);
                    proposal_actions.as_slice()
                } else {
                    exact_actions.as_slice()
                };
                if actions.is_empty() {
                    return self.evaluate_cached(state);
                }
                let mut ranked = if observation_safe {
                    recursive_observation_policy(state, actions, actor, self.branch_cap)
                } else {
                    let observed_ranked = normalize_observed_priors(state, actions, actor);
                    let mut ranked = order_scored_with_state_quotas(
                        &state.observed_state(actor),
                        actor,
                        observed_ranked,
                    );
                    ranked = truncate_root_preserving_end_turn(ranked, self.branch_cap);
                    if ranked.is_empty() {
                        ranked = rank_with_class_quotas(state, actions, actor, self.branch_cap);
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
                        let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
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
                            self.evaluate_cached(&next)
                        };
                        let used = self.nodes.saturating_sub(before);
                        carry = allowance.saturating_sub(used);
                        if self.deadline_reached {
                            return self.evaluate_cached(state);
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
                    DecisionVisitContext {
                        depth,
                        actions_in_turn,
                        alpha,
                        beta,
                        subtree_limit,
                    },
                )
                .0
            }
        }
    }

    fn root(&mut self, state: &GameState) -> DepthSearchResult {
        let NodeKind::Decision { actor } = state.node_kind() else {
            return DepthSearchResult {
                chosen: None,
                value: self.evaluate_cached(state),
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
                let mut next = state.clone();
                let ends_game = next.apply(action).is_ok() && next.is_terminal();
                ends_game
                    || belief_domestic_trade_threat(std::iter::once((state, 1.0)), action)
                        .is_none()
            })
            .cloned()
            .collect::<Vec<_>>();
        if !safe_ranked.is_empty() {
            ranked = safe_ranked;
        }
        let root_budgets = allocate_root_node_budgets(ranked.len(), self.maximum_nodes);
        let maximize = self.decision_maximizes(actor);
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
                self.evaluate_cached(&next)
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
            let scalar = self.decision_scalar(actor, &child);
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

#[derive(Clone, Copy)]
enum BeliefNodeBudgetMode {
    Global,
    PerDepthWave,
}

fn evaluate_after_forced_chance(state: &GameState, depth: u8) -> [f32; 4] {
    if depth >= 5 || state.node_kind() != NodeKind::Chance {
        return evaluate(state);
    }
    let actions = state.legal_actions();
    if actions.is_empty() {
        return evaluate(state);
    }
    let total = actions
        .iter()
        .map(|action| state.chance_weight(action) as f32)
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut result = [0.0; 4];
    let mut mass = 0.0_f32;
    for action in actions {
        let probability = state.chance_weight(&action) as f32 / total;
        let mut next = state.clone();
        if next.apply(&action).is_err() {
            continue;
        }
        let child = evaluate_after_forced_chance(&next, depth + 1);
        for player in 0..4 {
            result[player] += child[player] * probability;
        }
        mass += probability;
    }
    if mass > 0.0 {
        result.map(|value| value / mass)
    } else {
        evaluate(state)
    }
}

fn elapsed_stage_ms(deadline: &CooperativeDeadline, started_elapsed_ms: u32) -> u32 {
    deadline.elapsed_ms().saturating_sub(started_elapsed_ms)
}

fn belief_search(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    paranoid: bool,
    root_exclusions: &[Action],
    node_budget_mode: BeliefNodeBudgetMode,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let config = config.normalized();
    let maximum_depth = config.maximum_depth;
    let branch_cap = config.branch_cap;
    let maximum_nodes = config.maximum_nodes;
    let deadline = CooperativeDeadline::start(config.time_budget_ms);
    let particle_preparation_started = deadline.elapsed_ms();
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
            .filter(|candidate| candidate.authoritative)
            .map(|candidate| candidate.value)
            .reduce(f32::min)
            .unwrap_or(0.0);
        let maximum = report
            .actions
            .iter()
            .filter(|candidate| candidate.authoritative)
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
            .filter(|candidate| candidate.authoritative)
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
            stage_timings: None,
            provenance: BeliefSearchProvenance::default(),
        });
    }
    // Preserve every distinct WASM world in production. Exact-identical states
    // may be merged losslessly by summing their weights; finite configured
    // limits are reserved for explicit arena/benchmark experiments.
    let posterior_particles = particles.len();
    // Belief particles represent a weighted distribution, not an ordered
    // sequence. Canonicalize the exact worlds before timed planning,
    // coalescing, or strategic selection so caller permutation cannot decide
    // which world receives scarce wall-clock time first.
    let mut posterior_storage = particles.to_vec();
    posterior_storage.sort_by(|left, right| {
        left.state
            .state_hash()
            .cmp(&right.state.state_hash())
            .then_with(|| left.weight.total_cmp(&right.weight))
    });
    let posterior = posterior_storage.as_slice();
    let coalesced_storage = coalesce_identical_particles(posterior);
    let coalesced = coalesced_storage.as_slice();
    let strategic_storage;
    let particles = if coalesced.len() > config.strategic_particle_limit {
        strategic_storage =
            select_experimental_strategic_particles(coalesced, config.strategic_particle_limit);
        strategic_storage.as_slice()
    } else {
        coalesced
    };
    let particle_preparation_ms = elapsed_stage_ms(&deadline, particle_preparation_started);
    let root_scoring_started = deadline.elapsed_ms();
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
    let accumulate = |aggregate: &mut Vec<Aggregate>, entry: RowEntry, weight: f32| {
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
    };
    let mut nodes = 0;
    let mut cutoffs = 0;
    let mut depth = 0;
    let mut deadline_reached = false;
    let evaluation_cache = Rc::new(RefCell::new(HashMap::new()));
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let planner_nodes = (maximum_nodes / 12).clamp(300, 4_000);
    let mut ranked_diagnostics =
        normalize_belief_root_priors_with_diagnostics(particles, observer, planner_nodes);
    let root_scoring_ms = elapsed_stage_ms(&deadline, root_scoring_started);
    let exact_families_started = deadline.elapsed_ms();
    if deadline.has_elapsed() {
        deadline_reached = true;
    }
    let mut pruned_roots = Vec::<PrunedRootDiagnostic>::new();
    for candidate in &ranked_diagnostics {
        if root_exclusions.contains(&candidate.action) {
            pruned_roots.push(PrunedRootDiagnostic {
                action: candidate.action.clone(),
                pre_truncation_rank: Some(candidate.rank),
                reason: RootPruneReason::RootExcluded,
            });
        }
    }
    ranked_diagnostics.retain(|candidate| !root_exclusions.contains(&candidate.action));

    // Parameterized development cards are strategic families, not independent
    // root slots. Resolve each compact family over the full posterior before
    // branch competition, merge the family's prior mass into the exact
    // representative, then rank that one representative alongside ordinary
    // roots. The cached exact result is reused by final arbitration.
    let mut exact_family_results = Vec::<(ExactActionFamily, ExactDecisionResult)>::new();
    let mut exact_family_fallbacks = Vec::<(ExactActionFamily, Action)>::new();
    for family in DEVELOPMENT_EXACT_FAMILIES {
        if deadline.has_elapsed() {
            deadline_reached = true;
            break;
        }
        let family_members = ranked_diagnostics
            .iter()
            .filter(|candidate| exact_family_for_action(&candidate.action) == Some(family))
            .cloned()
            .collect::<Vec<_>>();
        let Some(fallback) = family_members
            .first()
            .map(|candidate| candidate.action.clone())
        else {
            continue;
        };
        let Some(exact) =
            solve_exact_belief_excluding_controlled(posterior, family, root_exclusions, || {
                deadline.has_elapsed()
            })
        else {
            deadline_reached = true;
            break;
        };
        let Some(representative) = exact.chosen.clone() else {
            continue;
        };
        let family_prior = family_members
            .iter()
            .map(|candidate| candidate.prior.max(0.0))
            .sum::<f32>();
        let quota_score = family_members
            .iter()
            .map(|candidate| candidate.quota_score)
            .fold(f32::NEG_INFINITY, f32::max);
        let representative_diagnostic = family_members
            .iter()
            .find(|candidate| candidate.action == representative)
            .or_else(|| family_members.first())
            .expect("non-empty exact family has a representative diagnostic");
        for candidate in &family_members {
            if candidate.action != representative {
                pruned_roots.push(PrunedRootDiagnostic {
                    action: candidate.action.clone(),
                    pre_truncation_rank: Some(candidate.rank),
                    reason: RootPruneReason::ExactFamilyCollapsed,
                });
            }
        }
        ranked_diagnostics
            .retain(|candidate| exact_family_for_action(&candidate.action) != Some(family));
        ranked_diagnostics.push(RankedRootDiagnostic {
            action: representative,
            rank: 0,
            prior: family_prior,
            planner_value: representative_diagnostic.planner_value,
            planner_completion_mass: representative_diagnostic.planner_completion_mass,
            planner_decisive_completion_mass: representative_diagnostic
                .planner_decisive_completion_mass,
            planner_response_windows: representative_diagnostic.planner_response_windows,
            quota_score,
        });
        exact_family_fallbacks.push((family, fallback));
        exact_family_results.push((family, exact));
    }
    if deadline_reached {
        // Exact-family preparation is atomic at the family boundary. If the
        // shared deadline expires, keep only representatives whose complete
        // posterior exact result was already cached; unresolved parameterized
        // families must not re-enter strategic branch competition.
        ranked_diagnostics.retain(|candidate| {
            let Some(family) = exact_family_for_action(&candidate.action) else {
                return true;
            };
            exact_family_results.iter().any(|(resolved_family, exact)| {
                *resolved_family == family && exact.chosen.as_ref() == Some(&candidate.action)
            })
        });
    }
    let exact_families_ms = elapsed_stage_ms(&deadline, exact_families_started);
    let threat_safety_started = deadline.elapsed_ms();
    ranked_diagnostics.sort_by(|left, right| {
        right
            .quota_score
            .total_cmp(&left.quota_score)
            .then_with(|| right.prior.total_cmp(&left.prior))
            .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
    });
    for (index, candidate) in ranked_diagnostics.iter_mut().enumerate() {
        candidate.rank = index + 1;
    }
    let ranked_root_count = ranked_diagnostics.len();
    let root_scored = ranked_diagnostics
        .iter()
        .map(|candidate| (candidate.action.clone(), candidate.prior))
        .collect::<Vec<_>>();
    let immediate_threat_weight = posterior_immediate_threat_weight(
        posterior
            .iter()
            .map(|particle| (&particle.state, particle.weight)),
        observer,
    );
    let mut verified_blockers = immediate_winning_roots(first, observer, &root_scored);
    if immediate_threat_weight > f32::EPSILON {
        for (action, _) in &root_scored {
            if verified_blockers
                .iter()
                .any(|(candidate, _)| candidate == action)
            {
                continue;
            }
            let residual_loss = forced_loss_weight(
                posterior
                    .iter()
                    .map(|particle| (&particle.state, particle.weight)),
                observer,
                action,
            );
            if residual_loss + 1e-6 < immediate_threat_weight {
                verified_blockers.push((action.clone(), residual_loss));
            }
        }
    };
    let root_actions_list: Vec<Action> = root_scored.iter().map(|(a, _)| a.clone()).collect();
    let closeout_plans = closeout_plans_from_ranked_diagnostics(&ranked_diagnostics);
    let spatial_impact_report = particles.first().map(|first| {
        let mut report = compute_spatial_root_impacts(&first.state, observer, &root_actions_list);
        apply_closeout_root_impacts(&mut report, &closeout_plans);
        report
    });
    let promoted_spatial_actions: Vec<Action> = spatial_impact_report
        .as_ref()
        .map(|report| {
            report
                .actions
                .iter()
                .filter(|impact| impact.promotion.is_some())
                .map(|impact| impact.action.clone())
                .collect()
        })
        .unwrap_or_default();
    let retained_without_promotions =
        admit_promoted_roots(&root_scored, &verified_blockers, &[], branch_cap);
    let retained = admit_promoted_roots(
        &root_scored,
        &verified_blockers,
        &promoted_spatial_actions,
        branch_cap,
    );
    let mut root_evidence = ranked_diagnostics
        .iter()
        .map(|candidate| {
            let impact = spatial_impact_report.as_ref().and_then(|report| {
                report
                    .actions
                    .iter()
                    .find(|impact| impact.action == candidate.action)
            });
            let trade = if retained
                .iter()
                .any(|(action, _)| action == &candidate.action)
            {
                belief_domestic_trade_assessment(
                    particles
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    &candidate.action,
                )
            } else {
                Default::default()
            };
            let road_cut_continuation =
                road_cut_continuation_for_root(posterior, observer, &candidate.action, impact);
            let road_intent = particles.first().and_then(|particle| {
                road_intent_for_root(&particle.state, observer, &candidate.action)
            });
            RootCausalEvidence {
                action: candidate.action.clone(),
                promotion_reason: impact.and_then(|impact| impact.promotion),
                road_intent,
                introduced_road_fragility: impact
                    .map(|impact| impact.introduced_road_fragility.clone())
                    .unwrap_or_default(),
                road_cut_continuation,
                admitted_by_promotion: impact.is_some_and(|impact| {
                    impact.promotion.is_some()
                        && retained
                            .iter()
                            .any(|(action, _)| action == &candidate.action)
                        && !retained_without_promotions
                            .iter()
                            .any(|(action, _)| action == &candidate.action)
                }),
                closeout_gain: impact.map_or(0.0, |impact| impact.closeout_gain),
                response_windows: impact.and_then(|impact| impact.response_windows),
                decisive_completion_mass: impact
                    .map_or(0.0, |impact| impact.decisive_completion_mass),
                trade_threat: trade.threat,
                trade_risk_posterior: trade.posterior,
                dirty_monopoly_posterior: trade.dirty_monopoly_posterior,
                trade_hard_veto_posterior: trade.hard_veto_posterior,
                trade_hard_veto: trade.hard_veto,
            }
        })
        .collect::<Vec<_>>();
    for (action, _) in &root_scored {
        if !retained
            .iter()
            .any(|(retained_action, _)| retained_action == action)
        {
            pruned_roots.push(PrunedRootDiagnostic {
                action: action.clone(),
                pre_truncation_rank: ranked_diagnostics
                    .iter()
                    .find(|candidate| candidate.action == *action)
                    .map(|candidate| candidate.rank),
                reason: RootPruneReason::BranchTruncated,
            });
        }
    }
    let root_actions = retained;
    let mut unique_root_actions = Vec::with_capacity(root_actions.len());
    for candidate in root_actions {
        if !unique_root_actions
            .iter()
            .any(|(action, _)| action == &candidate.0)
        {
            unique_root_actions.push(candidate);
        }
    }
    let mut root_actions = unique_root_actions;
    debug_assert!(root_actions.iter().enumerate().all(|(index, (action, _))| {
        root_actions
            .iter()
            .skip(index + 1)
            .all(|(other, _)| other != action)
    }));
    for family in DEVELOPMENT_EXACT_FAMILIES {
        debug_assert!(
            root_actions
                .iter()
                .filter(|(action, _)| exact_family_for_action(action) == Some(family))
                .count()
                <= 1
        );
    }
    let safe_root_actions = root_actions
        .iter()
        .filter(|(action, _)| {
            root_evidence
                .iter()
                .find(|evidence| evidence.action == *action)
                .is_none_or(|evidence| !evidence.trade_hard_veto)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !safe_root_actions.is_empty() {
        for (action, _) in &root_actions {
            if !safe_root_actions
                .iter()
                .any(|(safe_action, _)| safe_action == action)
            {
                pruned_roots.push(PrunedRootDiagnostic {
                    action: action.clone(),
                    pre_truncation_rank: ranked_diagnostics
                        .iter()
                        .find(|candidate| candidate.action == *action)
                        .map(|candidate| candidate.rank),
                    reason: RootPruneReason::TradeSafety,
                });
            }
        }
        root_actions = safe_root_actions;
    }
    // Concentrate nodes on the leading root actions instead of giving every
    // particle/action pair the same tiny equal slice. Uniform fairness left
    // live search with ~7 nodes/action at 32 particles × 16 actions.
    let action_budgets = allocate_root_node_budgets(
        root_actions.len(),
        maximum_nodes / particles.len().max(1) as u32,
    );
    let positive_particle_count = particles
        .iter()
        .filter(|particle| particle.weight > 0.0)
        .count()
        .max(1) as u32;
    let retained_roots = root_actions
        .iter()
        .zip(action_budgets.iter().copied())
        .map(|((action, prior), node_budget_per_particle)| {
            let diagnostic = ranked_diagnostics
                .iter()
                .find(|candidate| candidate.action == *action);
            RetainedRootDiagnostic {
                action: action.clone(),
                pre_truncation_rank: diagnostic.map(|candidate| candidate.rank),
                prior: *prior,
                node_budget_per_particle,
                allocated_nodes: node_budget_per_particle.saturating_mul(positive_particle_count),
                planner_value: diagnostic.and_then(|candidate| candidate.planner_value),
                planner_completion_mass: diagnostic
                    .and_then(|candidate| candidate.planner_completion_mass),
                planner_decisive_completion_mass: diagnostic
                    .and_then(|candidate| candidate.planner_decisive_completion_mass),
                planner_response_windows: diagnostic
                    .and_then(|candidate| candidate.planner_response_windows),
            }
        })
        .collect::<Vec<_>>();
    let pruned_root_count = pruned_roots.len();
    pruned_roots.truncate(MAX_ROOT_PROVENANCE);
    root_evidence.truncate(MAX_ROOT_PROVENANCE);
    let mut provenance = BeliefSearchProvenance {
        ranked_root_count,
        ranked_roots: ranked_diagnostics
            .iter()
            .take(MAX_ROOT_PROVENANCE)
            .cloned()
            .collect(),
        retained_roots,
        pruned_root_count,
        pruned_roots,
        root_evidence,
        trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
        search_winner: None,
        exact_family_replacement: None,
        exact_family_results,
        safety_replacement: None,
    };
    let root_actions = root_actions
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    let threat_safety_ms = elapsed_stage_ms(&deadline, threat_safety_started);
    let one_ply_floor_started = deadline.elapsed_ms();

    // Always retain one complete posterior-wide one-ply table. Deeper search
    // may replace it only after an entire depth wave completes across every
    // weighted hidden world and retained root. This preserves particle-order
    // invariance without throwing away the last completed strategic result
    // when the wall clock expires during a deeper wave.
    aggregate.clear();
    let mut particles_searched = 0;
    // The retained root/world table is deliberately small and is the minimum
    // action-specific evidence this search may return. Finish it even if
    // preparation consumed the nominal deadline; otherwise every root receives
    // the same fallback value and root ordering becomes the recommendation.
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= 0.0 {
            continue;
        }
        particles_searched += 1;
        for action in &root_actions {
            let mut next = particle.state.clone();
            let entry = if next.apply(action).is_ok() {
                let mut value = evaluate_after_forced_chance(&next, 0);
                apply_action_friction(&mut value, &particle.state, action, observer);
                RowEntry {
                    action: action.clone(),
                    value,
                    legal: true,
                }
            } else {
                RowEntry {
                    action: action.clone(),
                    value: evaluate(&particle.state),
                    legal: false,
                }
            };
            accumulate(&mut aggregate, entry, weight);
        }
    }
    let floor_complete = true;
    if deadline.has_elapsed() {
        deadline_reached = true;
    }
    let one_ply_floor_ms = elapsed_stage_ms(&deadline, one_ply_floor_started);
    let deep_waves_started = deadline.elapsed_ms();
    let mut attempted_depth = 0u8;

    for target_depth in 1..=maximum_depth {
        if deadline.has_elapsed() {
            deadline_reached = true;
            break;
        }
        let wave_node_budget = match node_budget_mode {
            BeliefNodeBudgetMode::Global => maximum_nodes.saturating_sub(nodes),
            BeliefNodeBudgetMode::PerDepthWave => maximum_nodes,
        };
        let minimum_complete_wave_nodes =
            positive_particle_count.saturating_mul(root_actions.len().max(1) as u32);
        if wave_node_budget < minimum_complete_wave_nodes {
            break;
        }
        attempted_depth = target_depth;
        let wave_action_budgets = allocate_root_node_budgets(
            root_actions.len(),
            wave_node_budget / positive_particle_count,
        );
        let mut wave = Vec::<Aggregate>::new();
        let mut wave_particles = 0usize;
        let mut wave_depth = 0u8;
        let mut wave_complete = true;
        let total_wave_cells = positive_particle_count as usize * root_actions.len().max(1);
        let mut completed_wave_cells = 0usize;

        'particles: for particle in particles {
            let weight = particle.weight.max(0.0) / total_weight;
            if weight <= 0.0 {
                continue;
            }
            wave_particles += 1;
            for (action_index, action) in root_actions.iter().enumerate() {
                if deadline.has_elapsed() {
                    deadline_reached = true;
                    wave_complete = false;
                    break 'particles;
                }
                let mut next = particle.state.clone();
                if next.apply(action).is_err() {
                    accumulate(
                        &mut wave,
                        RowEntry {
                            action: action.clone(),
                            value: evaluate(&particle.state),
                            legal: false,
                        },
                        weight,
                    );
                    completed_wave_cells += 1;
                    continue;
                }
                let completed_turn = next.turn != particle.state.turn
                    || next.current_player != particle.state.current_player;
                let nodes_for_action = wave_action_budgets
                    .get(action_index)
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let remaining_cells =
                    total_wave_cells.saturating_sub(completed_wave_cells).max(1) as u32;
                let remaining_ms = deadline.remaining_ms();
                if remaining_ms != u32::MAX && remaining_ms < remaining_cells {
                    wave_complete = false;
                    break 'particles;
                }
                let child_deadline = if remaining_ms == u32::MAX {
                    deadline.clone()
                } else {
                    CooperativeDeadline::start((remaining_ms / remaining_cells).max(1))
                };
                let mut searcher = Searcher {
                    algorithm: if paranoid {
                        Algorithm::Paranoid { root: observer }
                    } else {
                        Algorithm::MaxN
                    },
                    maximum_depth: target_depth,
                    maximum_nodes: nodes_for_action,
                    node_limit: nodes_for_action,
                    branch_cap: branch_cap.max(1),
                    nodes: 0,
                    cutoffs: 0,
                    deepest_depth: 0,
                    deadline: child_deadline,
                    deadline_reached: false,
                    observation_safe_recursive: true,
                    evaluation_cache: Rc::clone(&evaluation_cache),
                };
                let mut candidate_value = searcher.visit(
                    &next,
                    u8::from(completed_turn),
                    if completed_turn { 0 } else { 1 },
                    0.0,
                    1.0,
                    searcher.node_limit,
                );
                apply_action_friction(&mut candidate_value, &particle.state, action, observer);
                nodes += searcher.nodes;
                cutoffs += searcher.cutoffs;
                wave_depth = wave_depth.max(searcher.deepest_depth);
                if searcher.deadline_reached || deadline.has_elapsed() {
                    deadline_reached |= deadline.has_elapsed();
                    wave_complete = false;
                    break 'particles;
                }
                accumulate(
                    &mut wave,
                    RowEntry {
                        action: action.clone(),
                        value: candidate_value,
                        legal: true,
                    },
                    weight,
                );
                completed_wave_cells += 1;
            }
        }

        if !wave_complete {
            break;
        }
        aggregate = wave;
        particles_searched = wave_particles;
        depth = wave_depth;
    }
    let deep_waves_ms = elapsed_stage_ms(&deadline, deep_waves_started);
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
    provenance.search_winner = actions.first().map(|entry| entry.action.clone());
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
    if chosen_index > 0
        && let (Some(leading), Some(replacement)) = (actions.first(), actions.get(chosen_index))
    {
        provenance.safety_replacement = Some((leading.action.clone(), replacement.action.clone()));
    }
    let chosen = actions.get(chosen_index).map(|entry| entry.action.clone());
    if let Some(chosen_action) = chosen.as_ref()
        && let Some(family) = exact_family_for_action(chosen_action)
        && let Some((_, fallback)) = exact_family_fallbacks
            .iter()
            .find(|(candidate, _)| *candidate == family)
        && fallback != chosen_action
    {
        provenance.exact_family_replacement = Some((fallback.clone(), chosen_action.clone()));
    }
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
        stage_timings: Some(BeliefSearchStageTimings {
            particle_preparation_ms,
            root_scoring_ms,
            exact_families_ms,
            threat_safety_ms,
            one_ply_floor_ms,
            deep_waves_ms,
            floor_complete,
            attempted_depth,
        }),
        provenance,
    })
}

pub fn search_maxn(state: &GameState, depth: u8, branch_cap: usize) -> DepthSearchResult {
    search_maxn_bounded(state, depth, branch_cap, DEFAULT_DEPTH_NODE_BUDGET)
}

#[derive(Clone, Debug)]
pub struct DecisiveContinuationDiagnostic {
    pub root_action: Action,
    pub decisive_action: Action,
    pub response_windows: u8,
    pub endpoint_strategic_value: f32,
    pub same_turn: bool,
    pub transitions: u8,
}

fn materially_decisive_for_root(
    before: &GameState,
    after: &GameState,
    action: &Action,
    root: u8,
) -> bool {
    after.winner() == Some(root)
        || matches!(
            action,
            Action::BuildSettlement { .. } | Action::BuildCity { .. }
        )
        || (before.longest_road_holder != Some(root) && after.longest_road_holder == Some(root))
        || (before.largest_army_holder != Some(root) && after.largest_army_holder == Some(root))
}

/// Traces one bounded, diagnostic-only principal continuation after a supplied root action.
///
/// Decision actions come from the existing bounded MaxN search. Chance nodes use their
/// highest-probability legal outcome only to make the otherwise expected-value search path
/// nameable. The result is never consumed by root scoring, admission, promotion, or search.
pub fn diagnose_decisive_continuation(
    state: &GameState,
    root_action: &Action,
    maximum_transitions: u8,
    search_depth: u8,
    branch_cap: usize,
    maximum_nodes_per_decision: u32,
) -> Result<DecisiveContinuationDiagnostic, String> {
    let root = state.actor();
    let origin_turn = state.turn;
    if !state.legal_actions().contains(root_action) {
        return Err(format!("diagnostic root is not legal: {root_action:?}"));
    }

    let mut next = state.clone();
    next.apply(root_action)
        .map_err(|error| format!("diagnostic root failed: {root_action:?}: {error:?}"))?;
    if materially_decisive_for_root(state, &next, root_action, root) {
        return Ok(DecisiveContinuationDiagnostic {
            root_action: root_action.clone(),
            decisive_action: root_action.clone(),
            response_windows: 0,
            endpoint_strategic_value: strategic_utility(&next, root),
            same_turn: true,
            transitions: 1,
        });
    }

    let mut response_windows = 0u8;
    for transition in 1..maximum_transitions.max(1) {
        if next.is_terminal() {
            return Err("diagnostic continuation terminated before a root decisive action".into());
        }
        let action = match next.node_kind() {
            NodeKind::Terminal => {
                return Err(
                    "diagnostic continuation reached terminal node without root completion".into(),
                );
            }
            NodeKind::Chance => next
                .legal_actions()
                .into_iter()
                .max_by_key(|action| next.chance_weight(action))
                .ok_or_else(|| "diagnostic chance node has no legal outcome".to_string())?,
            NodeKind::Decision { .. } => {
                let search = search_maxn_bounded(
                    &next,
                    search_depth,
                    branch_cap,
                    maximum_nodes_per_decision,
                );
                search.chosen.ok_or_else(|| {
                    format!(
                        "diagnostic MaxN returned no action in phase {:?} for actor {}",
                        next.phase,
                        next.actor()
                    )
                })?
            }
        };
        let before = next.clone();
        let actor = before.actor();
        let previous_player = before.current_player;
        let opponent_trade_response = actor != root
            && previous_player == root
            && matches!(before.phase, Phase::TradeResponses);
        next.apply(&action)
            .map_err(|error| format!("diagnostic continuation failed: {action:?}: {error:?}"))?;
        if opponent_trade_response {
            response_windows = response_windows.saturating_add(1);
        }
        if previous_player != root && next.current_player != previous_player {
            response_windows = response_windows.saturating_add(1);
        }
        if actor == root && materially_decisive_for_root(&before, &next, &action, root) {
            return Ok(DecisiveContinuationDiagnostic {
                root_action: root_action.clone(),
                decisive_action: action,
                response_windows,
                endpoint_strategic_value: strategic_utility(&next, root),
                same_turn: before.turn == origin_turn && before.current_player == root,
                transitions: transition.saturating_add(1),
            });
        }
    }

    Err(format!(
        "diagnostic continuation found no root decisive action within {} transitions",
        maximum_transitions.max(1)
    ))
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
        BeliefNodeBudgetMode::Global,
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

/// Runs an explicit offline MaxN hostility stress search.
///
/// The root player keeps ordinary MaxN utility. Simulated opponents maximize
/// `(1 - h) * V_i + h * (1 - V_root)` while leaf evaluation, root ordering,
/// branch/node budgeting, and state transitions remain the ordinary bounded
/// MaxN implementation. This entry point is diagnostic-only; production
/// callers use `search_maxn_bounded[_timed]` and cannot enable hostility.
pub fn search_maxn_hostility_stress_bounded(
    state: &GameState,
    hostility: f32,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
) -> Result<DepthSearchResult, String> {
    if !hostility.is_finite() || !(0.0..=1.0).contains(&hostility) {
        return Err(format!(
            "hostility stress must be finite and within [0, 1], got {hostility}"
        ));
    }
    if hostility == 0.0 {
        return Ok(search_maxn_bounded(state, depth, branch_cap, maximum_nodes));
    }
    if matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        return Err("hostility stress diagnostic does not support setup decisions".into());
    }

    let root = state.actor();
    Ok(Searcher {
        algorithm: Algorithm::HostilityStress { root, hostility },
        maximum_depth: depth,
        maximum_nodes: maximum_nodes.max(1),
        node_limit: maximum_nodes.max(1),
        branch_cap: branch_cap.max(1),
        nodes: 0,
        cutoffs: 0,
        deepest_depth: 0,
        deadline: CooperativeDeadline::start(0),
        deadline_reached: false,
        observation_safe_recursive: false,
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
    }
    .root(state))
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
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
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
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
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
    belief_search(particles, config, false, &[], BeliefNodeBudgetMode::Global)
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
        BeliefNodeBudgetMode::Global,
    )
}

pub fn search_weighted_belief_maxn_iterative_timed_excluding(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    nodes_per_depth_wave: u32,
    time_budget_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes: nodes_per_depth_wave,
            time_budget_ms,
            strategic_particle_limit: usize::MAX,
        },
        false,
        root_exclusions,
        BeliefNodeBudgetMode::PerDepthWave,
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
    belief_search(particles, config, true, &[], BeliefNodeBudgetMode::Global)
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
        BeliefNodeBudgetMode::Global,
    )
}

pub fn search_weighted_belief_paranoid_iterative_timed_excluding(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    nodes_per_depth_wave: u32,
    time_budget_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes: nodes_per_depth_wave,
            time_budget_ms,
            strategic_particle_limit: usize::MAX,
        },
        true,
        root_exclusions,
        BeliefNodeBudgetMode::PerDepthWave,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default)]
pub struct CudaExactSearchStats {
    pub calls: u64,
    pub linear_calls: u64,
    pub deferred_calls: u64,
    pub streamed_leaves: u64,
    pub stream_flushes: u64,
    pub total_nanos: u64,
    pub root_preparation_nanos: u64,
    pub linear_traversal_nanos: u64,
    pub linear_legal_actions_nanos: u64,
    pub linear_policy_nanos: u64,
    pub linear_budget_nanos: u64,
    pub linear_apply_nanos: u64,
    pub tree_build_nanos: u64,
    pub host_packing_nanos: u64,
    pub queue_wait_nanos: u64,
    pub evaluation_nanos: u64,
    pub backup_nanos: u64,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_SEARCH_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_SEARCH_TOTAL_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_DEFERRED_CALLS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_STREAMED_LEAVES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_STREAM_FLUSHES: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_ROOT_PREPARATION_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_TRAVERSAL_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_LEGAL_ACTIONS_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_POLICY_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_BUDGET_NANOS: std::sync::atomic::AtomicU64 =
    std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_LINEAR_APPLY_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_TREE_BUILD_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_HOST_PACKING_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_QUEUE_WAIT_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_EVALUATION_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
static CUDA_BACKUP_NANOS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn record_cuda_duration(counter: &std::sync::atomic::AtomicU64, elapsed: std::time::Duration) {
    counter.fetch_add(
        elapsed.as_nanos().min(u64::MAX as u128) as u64,
        std::sync::atomic::Ordering::Relaxed,
    );
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn cuda_exact_search_stats() -> CudaExactSearchStats {
    use std::sync::atomic::Ordering::Relaxed;

    CudaExactSearchStats {
        calls: CUDA_SEARCH_CALLS.load(Relaxed),
        linear_calls: CUDA_LINEAR_CALLS.load(Relaxed),
        deferred_calls: CUDA_DEFERRED_CALLS.load(Relaxed),
        streamed_leaves: CUDA_STREAMED_LEAVES.load(Relaxed),
        stream_flushes: CUDA_STREAM_FLUSHES.load(Relaxed),
        total_nanos: CUDA_SEARCH_TOTAL_NANOS.load(Relaxed),
        root_preparation_nanos: CUDA_ROOT_PREPARATION_NANOS.load(Relaxed),
        linear_traversal_nanos: CUDA_LINEAR_TRAVERSAL_NANOS.load(Relaxed),
        linear_legal_actions_nanos: CUDA_LINEAR_LEGAL_ACTIONS_NANOS.load(Relaxed),
        linear_policy_nanos: CUDA_LINEAR_POLICY_NANOS.load(Relaxed),
        linear_budget_nanos: CUDA_LINEAR_BUDGET_NANOS.load(Relaxed),
        linear_apply_nanos: CUDA_LINEAR_APPLY_NANOS.load(Relaxed),
        tree_build_nanos: CUDA_TREE_BUILD_NANOS.load(Relaxed),
        host_packing_nanos: CUDA_HOST_PACKING_NANOS.load(Relaxed),
        queue_wait_nanos: CUDA_QUEUE_WAIT_NANOS.load(Relaxed),
        evaluation_nanos: CUDA_EVALUATION_NANOS.load(Relaxed),
        backup_nanos: CUDA_BACKUP_NANOS.load(Relaxed),
    }
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
    leaves: Vec<crate::CudaExactPackedState>,
    packing_nanos: u64,
    packing_failed: bool,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaDeferredTree {
    fn new() -> Self {
        Self {
            nodes: Vec::new(),
            leaves: Vec::new(),
            packing_nanos: 0,
            packing_failed: false,
        }
    }

    fn leaf(&mut self, state: &GameState) -> usize {
        let leaf = self.leaves.len();
        let packing_started = std::time::Instant::now();
        match crate::CudaExactPackedState::new(state) {
            Ok(packed) => self.leaves.push(packed),
            Err(_) => self.packing_failed = true,
        }
        self.packing_nanos = self
            .packing_nanos
            .saturating_add(packing_started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
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
const CUDA_LINEAR_BATCH_LEAVES: usize = 32_768;

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
#[derive(Clone, Copy, Debug, Default)]
struct CudaLinearLane {
    value: [f32; 4],
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
#[derive(Default)]
struct CudaLinearStaging {
    packed: Vec<crate::CudaExactPackedState>,
    lane_ids: Vec<usize>,
    path_weights: Vec<f32>,
    results: Vec<[f32; 4]>,
    states: Vec<GameState>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
thread_local! {
    static CUDA_LINEAR_STAGING_POOL: std::cell::RefCell<Vec<CudaLinearStaging>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaLinearStagingLease {
    staging: Option<CudaLinearStaging>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaLinearStagingLease {
    fn take() -> Self {
        let staging =
            CUDA_LINEAR_STAGING_POOL.with(|pool| pool.borrow_mut().pop().unwrap_or_default());
        Self {
            staging: Some(staging),
        }
    }

    fn get_mut(&mut self) -> &mut CudaLinearStaging {
        self.staging
            .as_mut()
            .expect("CUDA linear staging lease must remain live")
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl Drop for CudaLinearStagingLease {
    fn drop(&mut self) {
        let Some(mut staging) = self.staging.take() else {
            return;
        };
        staging.packed.clear();
        staging.lane_ids.clear();
        staging.path_weights.clear();
        staging.results.clear();
        CUDA_LINEAR_STAGING_POOL.with(|pool| pool.borrow_mut().push(staging));
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_checkout_state(pool: &mut Vec<GameState>, parent: &GameState) -> GameState {
    pool.pop().unwrap_or_else(|| parent.clone())
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_release_state(pool: &mut Vec<GameState>, state: GameState) {
    pool.push(state);
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaLinearBatch<'a> {
    packed: &'a mut Vec<crate::CudaExactPackedState>,
    lane_ids: &'a mut Vec<usize>,
    path_weights: &'a mut Vec<f32>,
    results: &'a mut Vec<[f32; 4]>,
    lanes: &'a mut Vec<CudaLinearLane>,
    evaluate_batch: &'a mut dyn FnMut(
        &[crate::CudaExactPackedState],
        &mut Vec<[f32; 4]>,
    ) -> Result<(), DepthBeliefError>,
    packing_nanos: u64,
    evaluation_nanos: u64,
    leaves_emitted: u64,
    flushes: u64,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaLinearBatch<'_> {
    fn emit(
        &mut self,
        state: &GameState,
        lane_id: usize,
        path_weight: f32,
    ) -> Result<(), DepthBeliefError> {
        if path_weight <= 0.0 {
            return Ok(());
        }
        let packing_started = std::time::Instant::now();
        let packed = crate::CudaExactPackedState::new(state)
            .map_err(|_| DepthBeliefError::CudaEvaluationFailed)?;
        self.packing_nanos = self
            .packing_nanos
            .saturating_add(packing_started.elapsed().as_nanos().min(u64::MAX as u128) as u64);
        self.packed.push(packed);
        self.lane_ids.push(lane_id);
        self.path_weights.push(path_weight);
        self.leaves_emitted = self.leaves_emitted.saturating_add(1);
        if self.packed.len() >= CUDA_LINEAR_BATCH_LEAVES {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<(), DepthBeliefError> {
        if self.packed.is_empty() {
            return Ok(());
        }
        let evaluation_started = std::time::Instant::now();
        self.results.clear();
        (self.evaluate_batch)(&self.packed, self.results)?;
        self.flushes = self.flushes.saturating_add(1);
        self.evaluation_nanos = self.evaluation_nanos.saturating_add(
            evaluation_started
                .elapsed()
                .as_nanos()
                .min(u64::MAX as u128) as u64,
        );
        if self.results.len() != self.packed.len() {
            return Err(DepthBeliefError::CudaBatchLengthMismatch);
        }
        for ((lane_id, path_weight), value) in self
            .lane_ids
            .iter()
            .copied()
            .zip(self.path_weights.iter().copied())
            .zip(self.results.iter().copied())
        {
            let lane = self
                .lanes
                .get_mut(lane_id)
                .expect("CUDA linear leaf must reference an existing lane");
            for player in 0..4 {
                lane.value[player] += value[player] * path_weight;
            }
        }
        self.packed.clear();
        self.lane_ids.clear();
        self.path_weights.clear();
        Ok(())
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaLinearSearcher<'a, 'b> {
    batch: &'a mut CudaLinearBatch<'b>,
    state_pool: &'a mut Vec<GameState>,
    lane_id: usize,
    maximum_depth: u8,
    maximum_nodes: u32,
    node_limit: u32,
    branch_cap: usize,
    nodes: u32,
    deepest_depth: u8,
    legal_actions_nanos: u64,
    policy_nanos: u64,
    budget_nanos: u64,
    apply_nanos: u64,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaLinearSearcher<'_, '_> {
    fn visit(
        &mut self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        subtree_limit: u32,
        path_weight: f32,
    ) -> Result<(), DepthBeliefError> {
        let subtree_limit = subtree_limit.min(self.node_limit).min(self.maximum_nodes);
        if self.nodes >= subtree_limit {
            return self.batch.emit(state, self.lane_id, path_weight);
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() || depth >= self.maximum_depth || actions_in_turn >= 18 {
            return self.batch.emit(state, self.lane_id, path_weight);
        }
        let legal_actions_started = std::time::Instant::now();
        let actions = state.legal_actions();
        self.legal_actions_nanos = self.legal_actions_nanos.saturating_add(
            legal_actions_started
                .elapsed()
                .as_nanos()
                .min(u64::MAX as u128) as u64,
        );
        if actions.is_empty() {
            return self.batch.emit(state, self.lane_id, path_weight);
        }
        match state.node_kind() {
            NodeKind::Terminal => self.batch.emit(state, self.lane_id, path_weight),
            NodeKind::Chance => {
                let budget_started = std::time::Instant::now();
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
                self.budget_nanos = self.budget_nanos.saturating_add(
                    budget_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                );
                let mut carry = 0_u32;
                for (index, (action, weight)) in weighted_actions.into_iter().enumerate() {
                    let allowance = budgets
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(carry);
                    let before = self.nodes;
                    let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
                    let mut next = cuda_checkout_state(self.state_pool, state);
                    let apply_started = std::time::Instant::now();
                    next.clone_from_and_apply(state, &action)
                        .expect("legal chance action must transition");
                    self.apply_nanos = self.apply_nanos.saturating_add(
                        apply_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                    );
                    let visit_result = if allowance > 0 && self.nodes < child_limit {
                        self.visit(
                            &next,
                            depth,
                            actions_in_turn.saturating_add(1),
                            child_limit,
                            path_weight * weight,
                        )
                    } else {
                        self.batch.emit(&next, self.lane_id, path_weight * weight)
                    };
                    cuda_release_state(self.state_pool, next);
                    visit_result?;
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                }
                Ok(())
            }
            NodeKind::Decision { actor } => {
                let remaining = subtree_limit.saturating_sub(self.nodes);
                if remaining == 0 {
                    return self.batch.emit(state, self.lane_id, path_weight);
                }
                let policy_started = std::time::Instant::now();
                let mut ranked =
                    recursive_observation_policy(state, &actions, actor, self.branch_cap);
                self.policy_nanos = self.policy_nanos.saturating_add(
                    policy_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                );
                ranked.truncate(ranked.len().min(remaining as usize));
                let budget_started = std::time::Instant::now();
                let budgets = allocate_root_node_budgets(ranked.len(), remaining);
                self.budget_nanos = self.budget_nanos.saturating_add(
                    budget_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                );
                let mut carry = 0_u32;
                for (index, (action, weight)) in ranked.into_iter().enumerate() {
                    if weight <= 0.0 {
                        continue;
                    }
                    debug_assert_eq!(cuda_action_friction(state, &action, actor), 0.0);
                    let allowance = budgets
                        .get(index)
                        .copied()
                        .unwrap_or(0)
                        .saturating_add(carry);
                    let before = self.nodes;
                    let child_limit = self.nodes.saturating_add(allowance).min(subtree_limit);
                    let mut next = cuda_checkout_state(self.state_pool, state);
                    let apply_started = std::time::Instant::now();
                    next.clone_from_and_apply(state, &action)
                        .expect("observation-policy action must transition");
                    self.apply_nanos = self.apply_nanos.saturating_add(
                        apply_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                    );
                    let completed_turn =
                        next.turn != state.turn || next.current_player != state.current_player;
                    let visit_result = if allowance > 0 && self.nodes < child_limit {
                        self.visit(
                            &next,
                            depth + u8::from(completed_turn),
                            if completed_turn {
                                0
                            } else {
                                actions_in_turn.saturating_add(1)
                            },
                            child_limit,
                            path_weight * weight,
                        )
                    } else {
                        self.batch.emit(&next, self.lane_id, path_weight * weight)
                    };
                    cuda_release_state(self.state_pool, next);
                    visit_result?;
                    let used = self.nodes.saturating_sub(before);
                    carry = allowance.saturating_sub(used);
                }
                Ok(())
            }
        }
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaLinearRootEntry {
    action: Action,
    lane_id: usize,
    legal: bool,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaLinearRootRow {
    weight: f32,
    entries: Vec<CudaLinearRootEntry>,
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
    evaluate_batch: &mut dyn FnMut(
        &[crate::CudaExactPackedState],
        &mut Vec<[f32; 4]>,
    ) -> Result<(), DepthBeliefError>,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let search_started = std::time::Instant::now();
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

    let root_preparation_started = std::time::Instant::now();
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
    let ranked_diagnostics =
        normalize_belief_root_priors_with_diagnostics(particles, observer, planner_nodes);
    let ranked_root_count = ranked_diagnostics.len();
    let mut pruned_roots = Vec::<PrunedRootDiagnostic>::new();
    for candidate in &ranked_diagnostics {
        if root_exclusions.contains(&candidate.action) {
            pruned_roots.push(PrunedRootDiagnostic {
                action: candidate.action.clone(),
                pre_truncation_rank: Some(candidate.rank),
                reason: RootPruneReason::RootExcluded,
            });
        }
    }
    let root_scored = ranked_diagnostics
        .iter()
        .filter(|candidate| !root_exclusions.contains(&candidate.action))
        .map(|candidate| (candidate.action.clone(), candidate.prior))
        .collect::<Vec<_>>();
    let immediate_threat_weight = posterior_immediate_threat_weight(
        posterior
            .iter()
            .map(|particle| (&particle.state, particle.weight)),
        observer,
    );
    let mut verified_blockers = immediate_winning_roots(first, observer, &root_scored);
    if immediate_threat_weight > f32::EPSILON {
        for (action, _) in &root_scored {
            if verified_blockers
                .iter()
                .any(|(candidate, _)| candidate == action)
            {
                continue;
            }
            let residual_loss = forced_loss_weight(
                posterior
                    .iter()
                    .map(|particle| (&particle.state, particle.weight)),
                observer,
                action,
            );
            if residual_loss + 1e-6 < immediate_threat_weight {
                verified_blockers.push((action.clone(), residual_loss));
            }
        }
    };
    let root_actions_list: Vec<Action> = root_scored.iter().map(|(a, _)| a.clone()).collect();
    let closeout_plans = closeout_plans_from_ranked_diagnostics(&ranked_diagnostics);
    let spatial_impact_report = particles.first().map(|first| {
        let mut report = compute_spatial_root_impacts(&first.state, observer, &root_actions_list);
        apply_closeout_root_impacts(&mut report, &closeout_plans);
        report
    });
    let promoted_spatial_actions: Vec<Action> = spatial_impact_report
        .as_ref()
        .map(|report| {
            report
                .actions
                .iter()
                .filter(|impact| impact.promotion.is_some())
                .map(|impact| impact.action.clone())
                .collect()
        })
        .unwrap_or_default();
    let retained_without_promotions =
        admit_promoted_roots(&root_scored, &verified_blockers, &[], branch_cap);
    let retained = admit_promoted_roots(
        &root_scored,
        &verified_blockers,
        &promoted_spatial_actions,
        branch_cap,
    );
    let mut root_evidence = ranked_diagnostics
        .iter()
        .map(|candidate| {
            let impact = spatial_impact_report.as_ref().and_then(|report| {
                report
                    .actions
                    .iter()
                    .find(|impact| impact.action == candidate.action)
            });
            let trade = if retained
                .iter()
                .any(|(action, _)| action == &candidate.action)
            {
                belief_domestic_trade_assessment(
                    particles
                        .iter()
                        .map(|particle| (&particle.state, particle.weight)),
                    &candidate.action,
                )
            } else {
                Default::default()
            };
            let road_cut_continuation =
                road_cut_continuation_for_root(posterior, observer, &candidate.action, impact);
            let road_intent = particles.first().and_then(|particle| {
                road_intent_for_root(&particle.state, observer, &candidate.action)
            });
            RootCausalEvidence {
                action: candidate.action.clone(),
                promotion_reason: impact.and_then(|impact| impact.promotion),
                road_intent,
                introduced_road_fragility: impact
                    .map(|impact| impact.introduced_road_fragility.clone())
                    .unwrap_or_default(),
                road_cut_continuation,
                admitted_by_promotion: impact.is_some_and(|impact| {
                    impact.promotion.is_some()
                        && retained
                            .iter()
                            .any(|(action, _)| action == &candidate.action)
                        && !retained_without_promotions
                            .iter()
                            .any(|(action, _)| action == &candidate.action)
                }),
                closeout_gain: impact.map_or(0.0, |impact| impact.closeout_gain),
                response_windows: impact.and_then(|impact| impact.response_windows),
                decisive_completion_mass: impact
                    .map_or(0.0, |impact| impact.decisive_completion_mass),
                trade_threat: trade.threat,
                trade_risk_posterior: trade.posterior,
                dirty_monopoly_posterior: trade.dirty_monopoly_posterior,
                trade_hard_veto_posterior: trade.hard_veto_posterior,
                trade_hard_veto: trade.hard_veto,
            }
        })
        .collect::<Vec<_>>();
    for (action, _) in &root_scored {
        if !retained
            .iter()
            .any(|(retained_action, _)| retained_action == action)
        {
            pruned_roots.push(PrunedRootDiagnostic {
                action: action.clone(),
                pre_truncation_rank: ranked_diagnostics
                    .iter()
                    .find(|candidate| candidate.action == *action)
                    .map(|candidate| candidate.rank),
                reason: RootPruneReason::BranchTruncated,
            });
        }
    }
    let mut root_actions = retained;
    let mut exact_family_replacement = None;
    if let Some(monopoly_slot) = root_actions
        .iter()
        .position(|(action, _)| matches!(action, Action::PlayMonopoly { .. }))
    {
        let (fallback, fallback_prior) = root_actions[monopoly_slot].clone();
        let replacement = solve_exact_belief(particles, ExactActionFamily::Monopoly)
            .chosen
            .unwrap_or_else(|| fallback.clone());
        for (action, _) in root_actions
            .iter()
            .filter(|(action, _)| matches!(action, Action::PlayMonopoly { .. }))
        {
            if action != &replacement {
                pruned_roots.push(PrunedRootDiagnostic {
                    action: action.clone(),
                    pre_truncation_rank: ranked_diagnostics
                        .iter()
                        .find(|candidate| candidate.action == *action)
                        .map(|candidate| candidate.rank),
                    reason: RootPruneReason::ExactFamilyCollapsed,
                });
            }
        }
        if replacement != fallback {
            exact_family_replacement = Some((fallback, replacement.clone()));
        }
        let replacement_prior = ranked_diagnostics
            .iter()
            .find(|candidate| candidate.action == replacement)
            .map_or(fallback_prior, |candidate| candidate.prior);
        root_actions.retain(|(action, _)| !matches!(action, Action::PlayMonopoly { .. }));
        root_actions.insert(
            monopoly_slot.min(root_actions.len()),
            (replacement, replacement_prior),
        );
    }
    let mut unique_root_actions = Vec::with_capacity(root_actions.len());
    for candidate in root_actions {
        if !unique_root_actions
            .iter()
            .any(|(action, _)| action == &candidate.0)
        {
            unique_root_actions.push(candidate);
        }
    }
    let mut root_actions = unique_root_actions;
    let safe_root_actions = root_actions
        .iter()
        .filter(|(action, _)| {
            root_evidence
                .iter()
                .find(|evidence| evidence.action == *action)
                .is_none_or(|evidence| !evidence.trade_hard_veto)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !safe_root_actions.is_empty() {
        for (action, _) in &root_actions {
            if !safe_root_actions
                .iter()
                .any(|(safe_action, _)| safe_action == action)
            {
                pruned_roots.push(PrunedRootDiagnostic {
                    action: action.clone(),
                    pre_truncation_rank: ranked_diagnostics
                        .iter()
                        .find(|candidate| candidate.action == *action)
                        .map(|candidate| candidate.rank),
                    reason: RootPruneReason::TradeSafety,
                });
            }
        }
        root_actions = safe_root_actions;
    }

    let action_budgets = allocate_root_node_budgets(
        root_actions.len(),
        maximum_nodes / particles.len().max(1) as u32,
    );
    let positive_particle_count = particles
        .iter()
        .filter(|particle| particle.weight > 0.0)
        .count()
        .max(1) as u32;
    let retained_roots = root_actions
        .iter()
        .zip(action_budgets.iter().copied())
        .map(|((action, prior), node_budget_per_particle)| {
            let diagnostic = ranked_diagnostics
                .iter()
                .find(|candidate| candidate.action == *action);
            RetainedRootDiagnostic {
                action: action.clone(),
                pre_truncation_rank: diagnostic.map(|candidate| candidate.rank),
                prior: *prior,
                node_budget_per_particle,
                allocated_nodes: node_budget_per_particle.saturating_mul(positive_particle_count),
                planner_value: diagnostic.and_then(|candidate| candidate.planner_value),
                planner_completion_mass: diagnostic
                    .and_then(|candidate| candidate.planner_completion_mass),
                planner_decisive_completion_mass: diagnostic
                    .and_then(|candidate| candidate.planner_decisive_completion_mass),
                planner_response_windows: diagnostic
                    .and_then(|candidate| candidate.planner_response_windows),
            }
        })
        .collect::<Vec<_>>();
    let pruned_root_count = pruned_roots.len();
    pruned_roots.truncate(MAX_ROOT_PROVENANCE);
    root_evidence.truncate(MAX_ROOT_PROVENANCE);
    let mut provenance = BeliefSearchProvenance {
        ranked_root_count,
        ranked_roots: ranked_diagnostics
            .iter()
            .take(MAX_ROOT_PROVENANCE)
            .cloned()
            .collect(),
        retained_roots,
        pruned_root_count,
        pruned_roots,
        root_evidence,
        trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
        search_winner: None,
        exact_family_replacement,
        exact_family_results: Vec::new(),
        safety_replacement: None,
    };
    let root_actions = root_actions
        .into_iter()
        .map(|(action, _)| action)
        .collect::<Vec<_>>();
    record_cuda_duration(
        &CUDA_ROOT_PREPARATION_NANOS,
        root_preparation_started.elapsed(),
    );

    if particles
        .iter()
        .all(|particle| !particle.state.player_trades_enabled)
    {
        let traversal_started = std::time::Instant::now();
        let mut lanes = Vec::<CudaLinearLane>::with_capacity(
            particles
                .len()
                .saturating_mul(root_actions.len())
                .saturating_add(1),
        );
        let mut root_rows = Vec::<CudaLinearRootRow>::with_capacity(particles.len());
        let mut nodes = 0_u32;
        let mut depth = 0_u8;
        let mut particles_searched = 0_usize;
        let mut legal_actions_nanos = 0_u64;
        let mut policy_nanos = 0_u64;
        let mut budget_nanos = 0_u64;
        let mut apply_nanos = 0_u64;
        let batch_capacity = CUDA_LINEAR_BATCH_LEAVES
            .min(maximum_nodes.max(1) as usize)
            .max(1);
        let mut staging_lease = CudaLinearStagingLease::take();
        let staging = staging_lease.get_mut();
        staging.packed.clear();
        staging.lane_ids.clear();
        staging.path_weights.clear();
        staging.results.clear();
        if staging.packed.capacity() < batch_capacity {
            staging.packed.reserve(batch_capacity);
        }
        if staging.lane_ids.capacity() < batch_capacity {
            staging.lane_ids.reserve(batch_capacity);
        }
        if staging.path_weights.capacity() < batch_capacity {
            staging.path_weights.reserve(batch_capacity);
        }
        if staging.results.capacity() < batch_capacity {
            staging.results.reserve(batch_capacity);
        }
        let CudaLinearStaging {
            packed,
            lane_ids,
            path_weights,
            results,
            states,
        } = staging;
        let mut batch = CudaLinearBatch {
            packed,
            lane_ids,
            path_weights,
            results,
            lanes: &mut lanes,
            evaluate_batch,
            packing_nanos: 0,
            evaluation_nanos: 0,
            leaves_emitted: 0,
            flushes: 0,
        };

        for particle in particles {
            let weight = particle.weight.max(0.0) / total_weight;
            if weight <= 0.0 {
                continue;
            }
            particles_searched += 1;
            let mut entries = Vec::with_capacity(root_actions.len());
            for (action_index, action) in root_actions.iter().enumerate() {
                let lane_id = batch.lanes.len();
                batch.lanes.push(CudaLinearLane::default());
                let mut next = cuda_checkout_state(states, &particle.state);
                let apply_started = std::time::Instant::now();
                let root_apply = next.clone_from_and_apply(&particle.state, action);
                apply_nanos =
                    apply_nanos.saturating_add(
                        apply_started.elapsed().as_nanos().min(u64::MAX as u128) as u64,
                    );
                if root_apply.is_err() {
                    cuda_release_state(states, next);
                    batch.emit(&particle.state, lane_id, 1.0)?;
                    entries.push(CudaLinearRootEntry {
                        action: action.clone(),
                        lane_id,
                        legal: false,
                    });
                    continue;
                }
                debug_assert_eq!(cuda_action_friction(&particle.state, action, observer), 0.0);
                let completed_turn = next.turn != particle.state.turn
                    || next.current_player != particle.state.current_player;
                let nodes_for_action = action_budgets
                    .get(action_index)
                    .copied()
                    .unwrap_or(1)
                    .max(1);
                let mut searcher = CudaLinearSearcher {
                    batch: &mut batch,
                    state_pool: states,
                    lane_id,
                    maximum_depth,
                    maximum_nodes: nodes_for_action,
                    node_limit: nodes_for_action,
                    branch_cap: branch_cap.max(1),
                    nodes: 0,
                    deepest_depth: 0,
                    legal_actions_nanos: 0,
                    policy_nanos: 0,
                    budget_nanos: 0,
                    apply_nanos: 0,
                };
                let node_limit = searcher.node_limit;
                let visit_result = searcher.visit(
                    &next,
                    u8::from(completed_turn),
                    if completed_turn { 0 } else { 1 },
                    node_limit,
                    1.0,
                );
                let searched_nodes = searcher.nodes;
                let searched_depth = searcher.deepest_depth;
                legal_actions_nanos =
                    legal_actions_nanos.saturating_add(searcher.legal_actions_nanos);
                policy_nanos = policy_nanos.saturating_add(searcher.policy_nanos);
                budget_nanos = budget_nanos.saturating_add(searcher.budget_nanos);
                apply_nanos = apply_nanos.saturating_add(searcher.apply_nanos);
                drop(searcher);
                cuda_release_state(states, next);
                visit_result?;
                nodes += searched_nodes;
                depth = depth.max(searched_depth);
                entries.push(CudaLinearRootEntry {
                    action: action.clone(),
                    lane_id,
                    legal: true,
                });
            }
            root_rows.push(CudaLinearRootRow { weight, entries });
        }

        let fallback_lane =
            if root_rows.is_empty() || root_rows.iter().all(|row| row.entries.is_empty()) {
                let lane_id = batch.lanes.len();
                batch.lanes.push(CudaLinearLane::default());
                batch.emit(first, lane_id, 1.0)?;
                Some(lane_id)
            } else {
                None
            };
        batch.flush()?;
        let packing_nanos = batch.packing_nanos;
        let evaluation_nanos = batch.evaluation_nanos;
        let leaves_emitted = batch.leaves_emitted;
        let flushes = batch.flushes;
        drop(batch);
        record_cuda_duration(
            &CUDA_HOST_PACKING_NANOS,
            std::time::Duration::from_nanos(packing_nanos),
        );
        record_cuda_duration(
            &CUDA_EVALUATION_NANOS,
            std::time::Duration::from_nanos(evaluation_nanos),
        );
        let traversal_nanos = traversal_started
            .elapsed()
            .as_nanos()
            .saturating_sub(u128::from(packing_nanos))
            .saturating_sub(u128::from(evaluation_nanos))
            .min(u64::MAX as u128) as u64;
        record_cuda_duration(
            &CUDA_LINEAR_TRAVERSAL_NANOS,
            std::time::Duration::from_nanos(traversal_nanos),
        );
        record_cuda_duration(
            &CUDA_LINEAR_LEGAL_ACTIONS_NANOS,
            std::time::Duration::from_nanos(legal_actions_nanos),
        );
        record_cuda_duration(
            &CUDA_LINEAR_POLICY_NANOS,
            std::time::Duration::from_nanos(policy_nanos),
        );
        record_cuda_duration(
            &CUDA_LINEAR_BUDGET_NANOS,
            std::time::Duration::from_nanos(budget_nanos),
        );
        record_cuda_duration(
            &CUDA_LINEAR_APPLY_NANOS,
            std::time::Duration::from_nanos(apply_nanos),
        );
        CUDA_LINEAR_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        CUDA_STREAMED_LEAVES.fetch_add(leaves_emitted, std::sync::atomic::Ordering::Relaxed);
        CUDA_STREAM_FLUSHES.fetch_add(flushes, std::sync::atomic::Ordering::Relaxed);

        let backup_started = std::time::Instant::now();
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
                let value = lanes[entry.lane_id].value;
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
        if chosen_index > 0
            && let (Some(leading), Some(replacement)) = (actions.first(), actions.get(chosen_index))
        {
            provenance.safety_replacement =
                Some((leading.action.clone(), replacement.action.clone()));
        }
        provenance.search_winner = actions.first().map(|entry| entry.action.clone());
        let chosen = actions.get(chosen_index).map(|entry| entry.action.clone());
        let value = actions
            .get(chosen_index)
            .map(|entry| entry.value)
            .or_else(|| fallback_lane.map(|lane_id| lanes[lane_id].value))
            .expect("CUDA linear belief search must have a root value");
        record_cuda_duration(&CUDA_BACKUP_NANOS, backup_started.elapsed());
        record_cuda_duration(&CUDA_SEARCH_TOTAL_NANOS, search_started.elapsed());
        CUDA_SEARCH_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        return Ok(BeliefDepthResult {
            chosen,
            value,
            actions,
            nodes,
            cutoffs: 0,
            depth,
            particles: particles_searched,
            posterior_particles,
            deadline_reached: false,
            stage_timings: None,
            provenance,
        });
    }

    let tree_build_started = std::time::Instant::now();
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
    record_cuda_duration(
        &CUDA_HOST_PACKING_NANOS,
        std::time::Duration::from_nanos(tree.packing_nanos),
    );
    record_cuda_duration(&CUDA_TREE_BUILD_NANOS, tree_build_started.elapsed());
    if tree.packing_failed {
        return Err(DepthBeliefError::CudaEvaluationFailed);
    }

    let evaluation_started = std::time::Instant::now();
    let mut leaf_values = Vec::with_capacity(tree.leaves.len());
    evaluate_batch(&tree.leaves, &mut leaf_values)?;
    record_cuda_duration(&CUDA_EVALUATION_NANOS, evaluation_started.elapsed());
    if leaf_values.len() != tree.leaves.len() {
        return Err(DepthBeliefError::CudaBatchLengthMismatch);
    }

    let backup_started = std::time::Instant::now();
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
    if chosen_index > 0
        && let (Some(leading), Some(replacement)) = (actions.first(), actions.get(chosen_index))
    {
        provenance.safety_replacement = Some((leading.action.clone(), replacement.action.clone()));
    }
    provenance.search_winner = actions.first().map(|entry| entry.action.clone());
    let chosen = actions.get(chosen_index).map(|entry| entry.action.clone());
    let value = actions
        .get(chosen_index)
        .map(|entry| entry.value)
        .or_else(|| fallback_node.map(|node| tree.backup(node, &leaf_values)))
        .expect("CUDA belief search must have a root value");
    record_cuda_duration(&CUDA_BACKUP_NANOS, backup_started.elapsed());
    record_cuda_duration(&CUDA_SEARCH_TOTAL_NANOS, search_started.elapsed());
    CUDA_DEFERRED_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    CUDA_SEARCH_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
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
        stage_timings: None,
        provenance,
    })
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn cuda_belief_search(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let mut evaluate_batch = |states: &[crate::CudaExactPackedState],
                              result: &mut Vec<[f32; 4]>| {
        evaluator
            .evaluate_packed_batch_into(states, result)
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
    let mut evaluate_batch = |states: &[crate::CudaExactPackedState],
                              result: &mut Vec<[f32; 4]>| {
        let wait_started = std::time::Instant::now();
        let evaluation = {
            let mut evaluator = evaluator
                .lock()
                .map_err(|_| DepthBeliefError::CudaEvaluatorLockPoisoned)?;
            record_cuda_duration(&CUDA_QUEUE_WAIT_NANOS, wait_started.elapsed());
            evaluator.evaluate_packed_batch_into(states, result)
        };
        evaluation.map_err(|_| DepthBeliefError::CudaEvaluationFailed)
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

    use colonist_catan_core::{Action, DevCard, GameState, NodeKind, Phase, Resource, SplitMix64};

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
        while matches!(left.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
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
    fn belief_root_rankings_exclude_hidden_bank_only_maritime_actions() {
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
        let ranked = normalize_belief_root_priors(&particles, 0, 1_000);
        assert!(
            !ranked.iter().any(|(candidate, _)| candidate == &target),
            "hidden-bank-only maritime actions must never enter the actor-facing root domain",
        );

        let report = search_weighted_belief_maxn_bounded(&particles, 2, 32, 4_000).unwrap();
        assert!(
            !report
                .provenance
                .ranked_roots
                .iter()
                .any(|candidate| candidate.action == target),
        );
        assert!(
            !report.actions.iter().any(|candidate| candidate.action == target),
        );
    }

    fn hidden_bank_domestic_offer_pair_for_root() -> (GameState, GameState) {
        let mut left = GameState::standard(241, 3);
        while matches!(left.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
            let action = left.legal_actions()[0].clone();
            left.apply(&action).unwrap();
        }
        left.phase = Phase::Main;
        left.current_player = 0;
        left.bank_is_public = false;
        for player in &mut left.players {
            player.resources = [0; 5];
        }
        left.bank = [19; 5];
        left.players[0].resources[Resource::Lumber.index()] = 4;
        left.bank[Resource::Lumber.index()] = 15;
        left.players[1].resources[Resource::Brick.index()] = 19;
        left.bank[Resource::Brick.index()] = 0;

        let mut right = left.clone();
        right.players[1].resources[Resource::Brick.index()] -= 1;
        right.players[1].resources[Resource::Ore.index()] += 1;
        right.bank[Resource::Brick.index()] += 1;
        right.bank[Resource::Ore.index()] -= 1;

        left.validate().unwrap();
        right.validate().unwrap();
        assert_eq!(left.observation_hash(0), right.observation_hash(0));
        (left, right)
    }

    #[test]
    fn belief_root_domain_is_observation_safe_for_hidden_bank_domestic_offer() {
        let (left, right) = hidden_bank_domestic_offer_pair_for_root();
        let recipients = ((1u8 << left.board.num_players) - 1) & !1u8;
        let target = Action::OfferTrade {
            recipients,
            give: [4, 0, 0, 0, 0],
            receive: [0, 1, 0, 0, 0],
        };
        assert!(left.legal_actions().contains(&target));
        assert!(!right.legal_actions().contains(&target));

        let left_ranked = normalize_belief_root_priors(
            &[BeliefParticle {
                state: left,
                weight: 1.0,
            }],
            0,
            1_000,
        );
        let right_ranked = normalize_belief_root_priors(
            &[BeliefParticle {
                state: right,
                weight: 1.0,
            }],
            0,
            1_000,
        );
        let canonical = |ranked: Vec<(Action, f32)>| {
            let mut actions = ranked
                .into_iter()
                .map(|(action, _)| action)
                .collect::<Vec<_>>();
            actions.sort_by(|left, right| format!("{left:?}").cmp(&format!("{right:?}")));
            actions
        };
        let left_actions = canonical(left_ranked);
        let right_actions = canonical(right_ranked);
        assert_eq!(left_actions, right_actions);
        assert!(left_actions.contains(&target));
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
        let (state, winning_road, blocker) = crate::threats::winning_road_over_blocker_fixture();
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
        let left_policy = super::recursive_observation_policy(&left, &left.legal_actions(), 3, 8);
        let right_policy =
            super::recursive_observation_policy(&right, &right.legal_actions(), 3, 8);
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
                .apply(&Action::ResolveDevelopment {
                    card: DevCard::Knight,
                })
                .unwrap();
        }
        assert_eq!(left.observation_hash(0), right.observation_hash(0));
        assert_eq!(left.legal_actions(), right.legal_actions());
        let left_policy = super::recursive_observation_policy(&left, &left.legal_actions(), 0, 8);
        let right_policy =
            super::recursive_observation_policy(&right, &right.legal_actions(), 0, 8);
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
            evaluation_cache: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::HashMap::new(),
            )),
        };
        let context = || super::DecisionVisitContext {
            depth: 0,
            actions_in_turn: 0,
            alpha: 0.0,
            beta: 1.0,
            subtree_limit: 400,
        };
        let mut left = make_searcher();
        let (left_value, left_chosen) = left.visit_ranked_decision(&state, 0, forward, context());
        let mut right = make_searcher();
        let (right_value, right_chosen) =
            right.visit_ranked_decision(&state, 0, reverse, context());

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
        while matches!(base.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
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
        let full_exact =
            crate::exact::solve_exact_belief(&particles, crate::exact::ExactActionFamily::Monopoly);
        let compressed_exact = crate::exact::solve_exact_belief(
            &compressed,
            crate::exact::ExactActionFamily::Monopoly,
        );
        let full_choice = full_exact
            .chosen
            .clone()
            .expect("the full posterior has an exact Monopoly choice");
        assert!(matches!(full_choice, Action::PlayMonopoly { .. }));
        assert_ne!(
            compressed_exact.chosen.as_ref(),
            Some(&full_choice),
            "this fixture must retain a parameter choice that changes under the intentionally compressed strategic subset"
        );

        let production =
            super::search_weighted_belief_maxn_bounded(&particles, 4, 8, 4_000).unwrap();
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
        assert_eq!(production.chosen.as_ref(), Some(&full_choice));
        assert_eq!(
            experimental_compressed.chosen.as_ref(),
            Some(&full_choice),
            "exact family arbitration must keep the full posterior parameter choice even when the strategic tree uses an experimental compressed subset"
        );
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
