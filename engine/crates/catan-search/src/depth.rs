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
    CLOSEOUT_COMPARABLE_VALUE_DELTA, IntroducedRoadFragility, RootPromotionReason,
    RootStrategicImpact, apply_closeout_root_impacts, compute_spatial_root_impacts,
};
use crate::shared::{
    admit_promoted_roots, coalesce_identical_particles, immediate_winning_roots,
    select_experimental_strategic_particles,
};
use crate::strategy::{
    ShadowInputs, StrategyPolicy, StrategyShadowDiagnostics, admit_strategy_challengers,
    finalize_strategy_diagnostics, shadow_strategy_diagnostics, strategy_diagnostics_for_admission,
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
// Scale-free ambiguity gate for future-self policy alternatives. A challenger
// must retain at least 85% of the leading observation-safe prior, so widening
// is reserved for genuinely close policy choices rather than broad K=2.
const FUTURE_SELF_WIDENING_MIN_PRIOR_RATIO: f32 = 0.85;
// Each policy variant must be able to expand at least one recursive child.
// Smaller cells keep the full existing K=1 allowance.
const FUTURE_SELF_WIDENING_MIN_NODES_PER_VARIANT: u32 = 2;

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

#[derive(Clone, Debug, PartialEq)]
pub struct RootSearchWorkDiagnostic {
    pub action: Action,
    pub nodes: u32,
    pub completed_wave_depth: u8,
    pub cutoff_depth_counts: Vec<u32>,
    /// Posterior particle mass for which at least one expanded continuation
    /// reached the controlled player's next turn decision.
    pub posterior_mass_reaching_controlled_next_decision: f32,
    /// Posterior particle mass for which at least one expanded continuation
    /// reached a terminal state. Kept separate from depth/budget cutoffs.
    pub posterior_mass_reaching_terminal: f32,
}

#[derive(Clone, Debug)]
pub struct BeliefSearchProvenance {
    pub ranked_root_count: usize,
    pub ranked_roots: Vec<RankedRootDiagnostic>,
    pub retained_roots: Vec<RetainedRootDiagnostic>,
    pub pruned_root_count: usize,
    pub pruned_roots: Vec<PrunedRootDiagnostic>,
    pub root_evidence: Vec<RootCausalEvidence>,
    pub root_search_work: Vec<RootSearchWorkDiagnostic>,
    pub strategy_shadow: Option<StrategyShadowDiagnostics>,
    pub trade_hard_veto_threshold: f32,
    /// Ordinary backed-up search winner before any later decisive-plan or safety replacement.
    pub search_winner: Option<Action>,
    pub decisive_plan_replacement: Option<(Action, Action)>,
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
            root_search_work: Vec::new(),
            strategy_shadow: None,
            trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
            search_winner: None,
            decisive_plan_replacement: None,
            exact_family_replacement: None,
            exact_family_results: Vec::new(),
            safety_replacement: None,
        }
    }
}

fn attach_strategy_shadow(
    provenance: &mut BeliefSearchProvenance,
    particles: &[BeliefParticle],
    ranked_actions: &[Action],
    searched_actions: &[Action],
    requested_depth: u8,
    completed_depth: u8,
    deadline_reached: bool,
) {
    if provenance.strategy_shadow.is_none() {
        let actor = particles
            .first()
            .map_or(0, |particle| particle.state.actor());
        let retained_actions = provenance
            .retained_roots
            .iter()
            .map(|candidate| candidate.action.clone())
            .collect::<Vec<_>>();
        let promoted_actions = provenance
            .root_evidence
            .iter()
            .filter(|evidence| evidence.promotion_reason.is_some())
            .map(|evidence| evidence.action.clone())
            .collect::<Vec<_>>();
        provenance.strategy_shadow = shadow_strategy_diagnostics(
            particles,
            actor,
            &promoted_actions,
            ShadowInputs {
                ranked_actions,
                retained_actions: &retained_actions,
                search_winner: provenance.search_winner.as_ref(),
                requested_depth,
                completed_depth,
                deadline_reached,
            },
        );
    }
    if let Some(diagnostics) = provenance.strategy_shadow.as_mut() {
        finalize_strategy_diagnostics(
            diagnostics,
            searched_actions,
            provenance.search_winner.as_ref(),
            requested_depth,
            completed_depth,
            deadline_reached,
        );
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
    pub evidence_escalation_triggered: bool,
    pub evidence_escalation_completed: bool,
    pub evidence_escalation_strengthened: bool,
    pub evidence_escalation_baseline_nodes: u32,
    pub evidence_escalation_nodes: u32,
    pub evidence_escalation_ms: u32,
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
    /// Missing/legacy callers remain baseline-authoritative. Action-changing
    /// strategy admission is available only through an explicit policy value.
    pub strategy_policy: StrategyPolicy,
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
            strategy_policy: self.strategy_policy,
            strategic_particle_limit: self.strategic_particle_limit.max(1),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DepthBeliefError {
    Empty,
    PublicStateMismatch,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaSearchCancelled,
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    CudaDeadlineExceeded,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TranspositionAlgorithmKey {
    MaxN,
    HostilityStress { root: u8, hostility_bits: u32 },
    Paranoid { root: u8 },
}

impl From<Algorithm> for TranspositionAlgorithmKey {
    fn from(value: Algorithm) -> Self {
        match value {
            Algorithm::MaxN => Self::MaxN,
            Algorithm::HostilityStress { root, hostility } => Self::HostilityStress {
                root,
                hostility_bits: hostility.to_bits(),
            },
            Algorithm::Paranoid { root } => Self::Paranoid { root },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum TranspositionNodeKindKey {
    Decision,
    Chance,
    Terminal,
}

impl From<NodeKind> for TranspositionNodeKindKey {
    fn from(value: NodeKind) -> Self {
        match value {
            NodeKind::Decision { .. } => Self::Decision,
            NodeKind::Chance => Self::Chance,
            NodeKind::Terminal => Self::Terminal,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TranspositionIdentity {
    state_hash: u64,
    depth: u8,
    actions_in_turn: u8,
    actor: u8,
    node_kind: TranspositionNodeKindKey,
    algorithm: TranspositionAlgorithmKey,
    observation_safe_recursive: bool,
    controlled_player: Option<u8>,
    controlled_policy_override: Option<ControlledWideningTarget>,
    remaining_subtree_allowance: u32,
    alpha_bits: u32,
    beta_bits: u32,
    maximum_depth: u8,
    branch_cap: usize,
}

fn transposition_identity(
    state: &GameState,
    algorithm: Algorithm,
    depth: u8,
    actions_in_turn: u8,
    alpha: f32,
    beta: f32,
    remaining_subtree_allowance: u32,
    maximum_depth: u8,
    branch_cap: usize,
    observation_safe_recursive: bool,
    controlled_player: Option<u8>,
    controlled_policy_override: Option<ControlledWideningTarget>,
) -> TranspositionIdentity {
    TranspositionIdentity {
        state_hash: state.state_hash(),
        depth,
        actions_in_turn,
        actor: state.actor(),
        node_kind: state.node_kind().into(),
        algorithm: algorithm.into(),
        observation_safe_recursive,
        controlled_player,
        controlled_policy_override,
        remaining_subtree_allowance,
        alpha_bits: alpha.to_bits(),
        beta_bits: beta.to_bits(),
        maximum_depth,
        branch_cap,
    }
}

struct TranspositionEntry {
    state: GameState,
    value: [f32; 4],
}

const TRANSPOSITION_MAX_WAVE_ENTRIES: usize = 32_768;

#[derive(Default)]
struct TranspositionTable {
    entries: HashMap<TranspositionIdentity, TranspositionEntry>,
}

impl TranspositionTable {
    fn lookup(&self, key: &TranspositionIdentity, state: &GameState) -> Option<[f32; 4]> {
        let entry = self.entries.get(key)?;
        (entry.state == *state).then_some(entry.value)
    }

    fn insert(&mut self, key: TranspositionIdentity, state: &GameState, value: [f32; 4]) {
        if self.entries.len() >= TRANSPOSITION_MAX_WAVE_ENTRIES
            || self.entries.contains_key(&key)
        {
            return;
        }
        self.entries.insert(
            key,
            TranspositionEntry {
                state: state.clone(),
                value,
            },
        );
    }
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

fn recursive_observation_ranked_policy_actions(
    state: &GameState,
    actions: &[Action],
    actor: u8,
) -> Vec<(Action, f32)> {
    let observed = state.observed_state(actor);
    let mut ranked = normalize_priors(&observed, actions, actor);
    if ranked.is_empty() {
        ranked = actions
            .iter()
            .cloned()
            .map(|action| (action, 1.0))
            .collect();
    }
    canonicalize_equal_prior_siblings(&mut ranked);
    ranked
}

#[cfg(test)]
fn recursive_observation_best_policy_action(
    state: &GameState,
    actions: &[Action],
    actor: u8,
) -> Vec<(Action, f32)> {
    let mut ranked = recursive_observation_ranked_policy_actions(state, actions, actor);
    ranked.truncate(1);
    if let Some((_, prior)) = ranked.first_mut() {
        *prior = 1.0;
    }
    ranked
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
struct ControlledInformationSetKey {
    actor: u8,
    observation_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ControlledWideningTarget {
    key: ControlledInformationSetKey,
    baseline_action: Action,
    challenger_action: Action,
}

#[derive(Clone, Debug)]
struct ControlledAmbiguityHit {
    target: ControlledWideningTarget,
    ambiguity_ratio: f32,
}

#[derive(Clone, Debug)]
struct ControlledAmbiguityEvidence {
    target: ControlledWideningTarget,
    ambiguity_ratio: f32,
    action_consistent: bool,
}

fn accumulate_controlled_ambiguity_evidence(
    evidence: &mut Vec<ControlledAmbiguityEvidence>,
    hits: &[ControlledAmbiguityHit],
) {
    for hit in hits {
        if let Some(existing) = evidence
            .iter_mut()
            .find(|candidate| candidate.target.key == hit.target.key)
        {
            existing.action_consistent &= existing.target.baseline_action == hit.target.baseline_action
                && existing.target.challenger_action == hit.target.challenger_action;
            existing.ambiguity_ratio = existing.ambiguity_ratio.max(hit.ambiguity_ratio);
        } else {
            evidence.push(ControlledAmbiguityEvidence {
                target: hit.target.clone(),
                ambiguity_ratio: hit.ambiguity_ratio,
                action_consistent: true,
            });
        }
    }
}

fn controlled_widening_budgets(total_nodes: u32) -> Option<[u32; 2]> {
    let budgets = allocate_root_node_budgets(2, total_nodes);
    (budgets.len() == 2
        && budgets
            .iter()
            .all(|budget| *budget >= FUTURE_SELF_WIDENING_MIN_NODES_PER_VARIANT))
    .then(|| [budgets[0], budgets[1]])
}

fn select_controlled_widening_target(
    evidence: &[ControlledAmbiguityEvidence],
) -> Option<ControlledWideningTarget> {
    evidence
        .iter()
        .filter(|candidate| candidate.action_consistent)
        .max_by(|left, right| {
            left.ambiguity_ratio
                .total_cmp(&right.ambiguity_ratio)
                .then_with(|| right.target.key.cmp(&left.target.key))
        })
        .map(|candidate| candidate.target.clone())
}

fn controlled_widening_ambiguity_ratio(
    baseline_prior: f32,
    challenger_prior: f32,
) -> Option<f32> {
    let baseline_prior = baseline_prior.max(0.0);
    if baseline_prior <= f32::EPSILON {
        return None;
    }
    let ambiguity_ratio = (challenger_prior.max(0.0) / baseline_prior).clamp(0.0, 1.0);
    (ambiguity_ratio + f32::EPSILON >= FUTURE_SELF_WIDENING_MIN_PRIOR_RATIO)
        .then_some(ambiguity_ratio)
}

fn controlled_ambiguity_hit_from_observation(
    state: &GameState,
    actions: &[Action],
    actor: u8,
) -> Option<ControlledAmbiguityHit> {
    let ranked = recursive_observation_ranked_policy_actions(state, actions, actor);
    let (baseline_action, baseline_prior) = ranked.first()?;
    let (challenger_action, challenger_prior) = ranked.get(1)?;
    let ambiguity_ratio =
        controlled_widening_ambiguity_ratio(*baseline_prior, *challenger_prior)?;
    Some(ControlledAmbiguityHit {
        target: ControlledWideningTarget {
            key: ControlledInformationSetKey {
                actor,
                observation_hash: state.observation_hash(actor),
            },
            baseline_action: baseline_action.clone(),
            challenger_action: challenger_action.clone(),
        },
        ambiguity_ratio,
    })
}

fn controlled_policy_action(
    state: &GameState,
    actions: &[Action],
    actor: u8,
    policy_override: Option<&ControlledWideningTarget>,
) -> Vec<(Action, f32)> {
    let mut ranked = recursive_observation_ranked_policy_actions(state, actions, actor);
    if ranked.is_empty() {
        return ranked;
    }
    let key = ControlledInformationSetKey {
        actor,
        observation_hash: state.observation_hash(actor),
    };
    let challenger = policy_override.is_some_and(|target| {
        target.key == key
            && ranked.first().is_some_and(|entry| entry.0 == target.baseline_action)
            && ranked.get(1).is_some_and(|entry| entry.0 == target.challenger_action)
    });
    let index = usize::from(challenger && ranked.len() > 1);
    let mut selected = ranked.swap_remove(index);
    selected.1 = 1.0;
    vec![selected]
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
    /// The player whose root action this search is evaluating. Opponents keep
    /// the observation-safe stochastic policy; this player uses one deliberate
    /// observation-safe continuation action instead of an opponent-style mix.
    controlled_player: Option<u8>,
    /// Optional information-set-level policy variant. When present, only this
    /// observation uses the stored challenger; every other controlled decision
    /// remains the ordinary rank-0 K=1 policy.
    controlled_policy_override: Option<ControlledWideningTarget>,
    controlled_ambiguity_hits: Vec<ControlledAmbiguityHit>,
    controlled_next_decision_reached: bool,
    terminal_reached: bool,
    cutoff_depth_counts: Vec<u32>,
    evaluation_cache: Rc<RefCell<HashMap<u64, [f32; 4]>>>,
    transposition_table: Option<Rc<RefCell<TranspositionTable>>>,
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

const DECISIVE_CURRENT_TURN_MASS: f32 = 0.999;

fn decisive_current_turn_plan_replacement_index(
    actions: &[DepthActionValue],
    actor: usize,
    ranked_diagnostics: &[RankedRootDiagnostic],
    current_index: usize,
) -> usize {
    let Some(current) = actions.get(current_index) else {
        return current_index;
    };
    let diagnostic = |action: &Action| {
        ranked_diagnostics
            .iter()
            .find(|candidate| candidate.action == *action)
    };
    let Some(current_diagnostic) = diagnostic(&current.action) else {
        return current_index;
    };
    let Some(current_planner_value) = current_diagnostic.planner_value else {
        return current_index;
    };
    if current_diagnostic.planner_completion_mass.unwrap_or(0.0) < DECISIVE_CURRENT_TURN_MASS {
        return current_index;
    }

    let decisive = actions
        .iter()
        .enumerate()
        .filter_map(|(index, action)| {
            let candidate = diagnostic(&action.action)?;
            let planner_value = candidate.planner_value?;
            (candidate.planner_completion_mass.unwrap_or(0.0) >= DECISIVE_CURRENT_TURN_MASS
                && candidate
                    .planner_decisive_completion_mass
                    .unwrap_or(0.0)
                    >= DECISIVE_CURRENT_TURN_MASS
                && candidate.planner_response_windows.unwrap_or(f32::INFINITY)
                    <= f32::EPSILON
                && planner_value.is_finite())
            .then_some((index, planner_value))
        })
        .collect::<Vec<_>>();
    let Some(best_planner_value) = decisive
        .iter()
        .map(|(_, value)| *value)
        .reduce(f32::max)
    else {
        return current_index;
    };

    if best_planner_value <= current_planner_value + CLOSEOUT_COMPARABLE_VALUE_DELTA {
        return current_index;
    }

    decisive
        .into_iter()
        .filter(|(_, planner_value)| {
            *planner_value + CLOSEOUT_COMPARABLE_VALUE_DELTA >= best_planner_value
        })
        .max_by(|(left, _), (right, _)| {
            actions[*left].value[actor].total_cmp(&actions[*right].value[actor])
        })
        .map_or(current_index, |(index, _)| index)
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
    fn transposition_key(
        &self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        alpha: f32,
        beta: f32,
        remaining_subtree_allowance: u32,
    ) -> Option<TranspositionIdentity> {
        self.transposition_table.as_ref()?;
        Some(transposition_identity(
            state,
            self.algorithm,
            depth,
            actions_in_turn,
            alpha,
            beta,
            remaining_subtree_allowance,
            self.maximum_depth,
            self.branch_cap,
            self.observation_safe_recursive,
            self.controlled_player,
            self.controlled_policy_override.clone(),
        ))
    }

    fn lookup_transposition(
        &self,
        key: &TranspositionIdentity,
        state: &GameState,
    ) -> Option<[f32; 4]> {
        self.transposition_table
            .as_ref()?
            .borrow()
            .lookup(key, state)
    }

    fn insert_transposition(
        &self,
        key: TranspositionIdentity,
        state: &GameState,
        value: [f32; 4],
    ) {
        if let Some(table) = self.transposition_table.as_ref() {
            table.borrow_mut().insert(key, state, value);
        }
    }

    fn mark_cutoff(&mut self, depth: u8) {
        let index = depth as usize;
        if self.cutoff_depth_counts.len() <= index {
            self.cutoff_depth_counts.resize(index + 1, 0);
        }
        self.cutoff_depth_counts[index] = self.cutoff_depth_counts[index].saturating_add(1);
    }

    fn record_controlled_ambiguity(&mut self, state: &GameState, actor: u8, depth: u8) {
        if !self.observation_safe_recursive
            || self.controlled_player != Some(actor)
            || depth == 0
        {
            return;
        }
        let actions = actor_proposal_actions(state);
        if let Some(hit) = controlled_ambiguity_hit_from_observation(state, &actions, actor) {
            self.controlled_ambiguity_hits.push(hit);
        }
    }

    fn controlled_policy_action(
        &self,
        state: &GameState,
        actions: &[Action],
        actor: u8,
    ) -> Vec<(Action, f32)> {
        controlled_policy_action(
            state,
            actions,
            actor,
            self.controlled_policy_override.as_ref(),
        )
    }

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
        let remaining_subtree_allowance = subtree_limit.saturating_sub(self.nodes);
        if self.nodes >= subtree_limit {
            self.mark_cutoff(depth);
            return self.evaluate_cached(state);
        }
        if self.deadline.expired_at_checkpoint(self.nodes, 8) {
            self.deadline_reached = true;
            self.mark_cutoff(depth);
            return self.evaluate_cached(state);
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() {
            self.terminal_reached = true;
            return self.evaluate_cached(state);
        }
        let node_kind = state.node_kind();
        if let NodeKind::Decision { actor } = node_kind
            && self.controlled_player == Some(actor)
            && depth > 0
        {
            self.controlled_next_decision_reached = true;
            self.record_controlled_ambiguity(state, actor, depth);
        }
        if depth >= self.maximum_depth || actions_in_turn >= 18 {
            self.mark_cutoff(depth);
            return self.evaluate_cached(state);
        }
        let exact_actions = state.legal_actions();
        if exact_actions.is_empty() {
            return self.evaluate_cached(state);
        }
        let transposition_key = self.transposition_key(
            state,
            depth,
            actions_in_turn,
            alpha,
            beta,
            remaining_subtree_allowance,
        );
        if let Some(key) = transposition_key.as_ref()
            && let Some(value) = self.lookup_transposition(key, state)
        {
            return value;
        }

        let value = match node_kind {
            NodeKind::Terminal => {
                self.terminal_reached = true;
                self.evaluate_cached(state)
            }
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
                    self.mark_cutoff(depth);
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
                let mut ranked = if observation_safe && self.controlled_player == Some(actor) {
                    self.controlled_policy_action(state, actions, actor)
                } else if observation_safe {
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
                // The controlled player is not an opponent policy. Each search
                // profile therefore uses one deliberate observation-valid action:
                // rank 0 normally, or rank 1 only at one previously selected
                // ambiguous information set. The profile winner is chosen outside
                // this exact-particle Searcher after belief aggregation, so hidden
                // worlds cannot independently choose their future-self action.
                if observation_safe && self.controlled_player == Some(actor) {
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
                } else if observation_safe {
                    // Observation-safe opponents evaluate a prior-weighted mixture
                    // over the top observation-ranked actions. The mixture depends
                    // only on the actor's observation, so indistinguishable worlds
                    // share one strategy while still covering more than a single
                    // greedy prior line.
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
                    expected
                } else {
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
        };
        if let Some(key) = transposition_key
            && !self.deadline_reached
        {
            self.insert_transposition(key, state, value);
        }
        value
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
                    || belief_domestic_trade_threat(std::iter::once((state, 1.0)), action).is_none()
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum BeliefNodeBudgetMode {
    Global,
    PerDepthWave,
}

const EVIDENCE_ESCALATION_NODE_MULTIPLIER: u32 = 3;

fn should_escalate_binary_root_evidence(
    node_budget_mode: BeliefNodeBudgetMode,
    root_count: usize,
    evidence_escalation_ms: u32,
    floor_winner: Option<&Action>,
    wave_winner: Option<&Action>,
) -> bool {
    node_budget_mode == BeliefNodeBudgetMode::PerDepthWave
        && root_count == 2
        && evidence_escalation_ms > 0
        && floor_winner.is_some()
        && wave_winner.is_some()
        && floor_winner != wave_winner
}

fn evidence_escalation_node_budget(base_wave_nodes: u32) -> u32 {
    base_wave_nodes.saturating_mul(EVIDENCE_ESCALATION_NODE_MULTIPLIER)
}

fn evidence_escalation_target_depth(next_target_depth: u8) -> u8 {
    next_target_depth.saturating_sub(1).max(1)
}

fn realized_root_evidence_strengthened(
    baseline_root_nodes: &[u32],
    rerun_root_nodes: &[u32],
) -> bool {
    baseline_root_nodes.len() == rerun_root_nodes.len()
        && !baseline_root_nodes.is_empty()
        && baseline_root_nodes
            .iter()
            .zip(rerun_root_nodes)
            .all(|(baseline, rerun)| rerun >= baseline)
        && baseline_root_nodes
            .iter()
            .zip(rerun_root_nodes)
            .any(|(baseline, rerun)| rerun > baseline)
}

fn allocate_particle_node_budgets(
    particles: &[BeliefParticle],
    total_nodes: u32,
    minimum_nodes_per_positive_particle: u32,
) -> Vec<u32> {
    let mut budgets = vec![0u32; particles.len()];
    let positive = particles
        .iter()
        .enumerate()
        .filter(|(_, particle)| particle.weight > 0.0)
        .collect::<Vec<_>>();
    if positive.is_empty() {
        return budgets;
    }

    let minimum_total =
        minimum_nodes_per_positive_particle.saturating_mul(positive.len() as u32);
    if total_nodes < minimum_total {
        return budgets;
    }
    for (index, _) in &positive {
        budgets[*index] = minimum_nodes_per_positive_particle;
    }

    let distributable = total_nodes - minimum_total;
    if distributable == 0 {
        return budgets;
    }
    let total_weight = positive
        .iter()
        .map(|(_, particle)| particle.weight as f64)
        .sum::<f64>();
    let mut assigned = 0u32;
    let mut remainders = Vec::with_capacity(positive.len());
    for (index, particle) in positive {
        let exact = distributable as f64 * particle.weight as f64 / total_weight;
        let whole = exact.floor() as u32;
        budgets[index] = budgets[index].saturating_add(whole);
        assigned = assigned.saturating_add(whole);
        remainders.push((index, exact - whole as f64));
    }
    debug_assert!(assigned <= distributable);

    remainders.sort_by(|(left_index, left_remainder), (right_index, right_remainder)| {
        right_remainder
            .total_cmp(left_remainder)
            .then_with(|| {
                particles[*left_index]
                    .state
                    .state_hash()
                    .cmp(&particles[*right_index].state.state_hash())
            })
            .then_with(|| {
                format!("{:?}", particles[*left_index].state)
                    .cmp(&format!("{:?}", particles[*right_index].state))
            })
    });
    let leftover = distributable.saturating_sub(assigned) as usize;
    for (index, _) in remainders.into_iter().take(leftover) {
        budgets[index] = budgets[index].saturating_add(1);
    }

    budgets
}

fn allocate_root_node_budgets_by_priority(
    action_count: usize,
    total_nodes: u32,
    priority: &[usize],
) -> Vec<u32> {
    if action_count == 0 {
        return Vec::new();
    }
    debug_assert_eq!(priority.len(), action_count);
    let ranked_budgets = allocate_root_node_budgets(action_count, total_nodes);
    let mut canonical_budgets = vec![0u32; action_count];
    for (priority_rank, canonical_index) in priority.iter().copied().enumerate() {
        debug_assert!(canonical_index < action_count);
        canonical_budgets[canonical_index] = ranked_budgets[priority_rank];
    }
    canonical_budgets
}

fn root_allocation_priority_from_evidence(evidence: &[(f32, f32)]) -> Vec<usize> {
    let mut priority = (0..evidence.len()).collect::<Vec<_>>();
    priority.sort_by(|left, right| {
        let (left_lower, left_value) = evidence[*left];
        let (right_lower, right_value) = evidence[*right];
        right_lower
            .total_cmp(&left_lower)
            .then_with(|| right_value.total_cmp(&left_value))
            .then_with(|| left.cmp(right))
    });
    priority
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
    evidence_escalation_ms: u32,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search_backend(
        particles,
        config,
        paranoid,
        root_exclusions,
        node_budget_mode,
        evidence_escalation_ms,
        &mut BeliefBackend::Cpu,
    )
}

// Root preparation, admission, wave budgets, escalation and final arbitration
// have one owner. Only evaluation of a continuation cell changes backend.
enum BeliefBackend<'a> {
    Cpu,
    #[cfg(test)]
    BeforeVisit(&'a mut dyn FnMut(&mut Searcher)),
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    Cuda(&'a mut crate::CudaExactEvaluator, &'a dyn Fn() -> bool),
    #[cfg(not(all(feature = "cuda-exact", not(target_arch = "wasm32"))))]
    _Lifetime(std::marker::PhantomData<&'a ()>),
}

impl BeliefBackend<'_> {
    fn check_cancelled(&self) -> Result<(), DepthBeliefError> {
        #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
        if let Self::Cuda(_, cancel) = self
            && cancel()
        {
            return Err(DepthBeliefError::CudaSearchCancelled);
        }
        Ok(())
    }

    fn visit(
        &mut self,
        searcher: &mut Searcher,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
    ) -> Result<[f32; 4], DepthBeliefError> {
        self.check_cancelled()?;
        #[cfg(test)]
        if let Self::BeforeVisit(before_visit) = self {
            before_visit(searcher);
        }
        #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
        if let Self::Cuda(evaluator, cancel) = self {
            let tree_build_started = std::time::Instant::now();
            let mut tree = CudaDeferredTree::new();
            let deadline = searcher.deadline.clone();
            let stop = || {
                if cancel() {
                    Some(DepthBeliefError::CudaSearchCancelled)
                } else if deadline.has_elapsed() {
                    Some(DepthBeliefError::CudaDeadlineExceeded)
                } else {
                    None
                }
            };
            let mut deferred = CudaDeferredSearcher {
                tree: &mut tree,
                algorithm: searcher.algorithm,
                maximum_depth: searcher.maximum_depth,
                maximum_nodes: searcher.maximum_nodes,
                node_limit: searcher.node_limit,
                branch_cap: searcher.branch_cap,
                controlled_player: searcher
                    .controlled_player
                    .expect("belief controlled player"),
                controlled_policy_override: searcher.controlled_policy_override.clone(),
                controlled_ambiguity_hits: Vec::new(),
                nodes: 0,
                deepest_depth: 0,
                controlled_next_decision_reached: false,
                terminal_reached: false,
                cutoff_depth_counts: Vec::new(),
                transposition_table: searcher
                    .transposition_table
                    .as_ref()
                    .map(Rc::clone),
                local_transpositions: HashMap::new(),
                pending_transpositions: Vec::new(),
            };
            let node = deferred.visit(state, depth, actions_in_turn, searcher.node_limit, &stop);
            searcher.nodes = deferred.nodes;
            searcher.deepest_depth = deferred.deepest_depth;
            searcher.controlled_next_decision_reached = deferred.controlled_next_decision_reached;
            searcher.controlled_ambiguity_hits =
                std::mem::take(&mut deferred.controlled_ambiguity_hits);
            searcher.terminal_reached = deferred.terminal_reached;
            searcher.cutoff_depth_counts = std::mem::take(&mut deferred.cutoff_depth_counts);
            let pending_transpositions = std::mem::take(&mut deferred.pending_transpositions);
            drop(deferred);
            record_cuda_duration(&CUDA_TREE_BUILD_NANOS, tree_build_started.elapsed());
            CUDA_HOST_PACKING_NANOS.fetch_add(
                tree.packing_nanos,
                std::sync::atomic::Ordering::Relaxed,
            );
            CUDA_STREAMED_LEAVES.fetch_add(
                tree.leaves.len() as u64,
                std::sync::atomic::Ordering::Relaxed,
            );
            if !tree.leaves.is_empty() {
                CUDA_STREAM_FLUSHES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            }
            let node = match node {
                Ok(node) => node,
                Err(DepthBeliefError::CudaDeadlineExceeded) => {
                    searcher.deadline_reached = true;
                    return Ok([0.0; 4]); // Incomplete cells never enter a completed wave.
                }
                Err(error) => return Err(error),
            };
            if tree.packing_failed {
                return Err(DepthBeliefError::CudaEvaluationFailed);
            }
            let mut values = Vec::new();
            let evaluation_started = std::time::Instant::now();
            evaluator
                .evaluate_packed_batch_into(&tree.leaves, &mut values)
                .map_err(|_| DepthBeliefError::CudaEvaluationFailed)?;
            record_cuda_duration(&CUDA_EVALUATION_NANOS, evaluation_started.elapsed());
            if cancel() {
                return Err(DepthBeliefError::CudaSearchCancelled);
            }
            searcher.deadline_reached = deadline.has_elapsed();
            let backup_started = std::time::Instant::now();
            let node_values = tree.backup_all(&values);
            record_cuda_duration(&CUDA_BACKUP_NANOS, backup_started.elapsed());
            if !searcher.deadline_reached
                && let Some(table) = searcher.transposition_table.as_ref()
            {
                let mut table = table.borrow_mut();
                for pending in pending_transpositions {
                    table.insert(
                        pending.key,
                        &pending.state,
                        node_values[pending.node],
                    );
                }
            }
            return Ok(node_values[node]);
        }
        Ok(searcher.visit(state, depth, actions_in_turn, 0.0, 1.0, searcher.node_limit))
    }
}

fn belief_search_backend(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    paranoid: bool,
    root_exclusions: &[Action],
    node_budget_mode: BeliefNodeBudgetMode,
    evidence_escalation_ms: u32,
    backend: &mut BeliefBackend<'_>,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    backend.check_cancelled()?;
    let config = config.normalized();
    let maximum_depth = config.maximum_depth;
    let branch_cap = config.branch_cap;
    let maximum_nodes = config.maximum_nodes;
    let deadline = CooperativeDeadline::start(config.time_budget_ms);
    let hard_deadline =
        deadline.with_budget_ms(config.time_budget_ms.saturating_add(evidence_escalation_ms));
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
    #[derive(Clone)]
    struct Aggregate {
        action: Action,
        value: [f32; 4],
        covered_weight: f32,
        legal_weight: f32,
        lower_bound: [f32; 4],
    }
    #[derive(Clone)]
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
    let baseline_retained = admit_promoted_roots(
        &root_scored,
        &verified_blockers,
        &promoted_spatial_actions,
        branch_cap,
    );
    let mut strategy_admission = None;
    let retained = if config.strategy_policy == StrategyPolicy::AdaptiveCandidateAdmissionV1 {
        let ranked_actions = root_scored
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let baseline_actions = baseline_retained
            .iter()
            .map(|(action, _)| action.clone())
            .collect::<Vec<_>>();
        let mut protected_actions = Vec::<Action>::new();
        for (action, _) in &verified_blockers {
            if baseline_actions.contains(action) && !protected_actions.contains(action) {
                protected_actions.push(action.clone());
            }
        }
        if baseline_actions.contains(&Action::EndTurn) {
            protected_actions.push(Action::EndTurn);
        }
        if let Some((leader, _)) = root_scored.first()
            && baseline_actions.contains(leader)
            && !protected_actions.contains(leader)
        {
            protected_actions.push(leader.clone());
        }
        for action in &promoted_spatial_actions {
            if baseline_actions.contains(action) && !protected_actions.contains(action) {
                protected_actions.push(action.clone());
            }
        }
        if let Some(mut diagnostics) = strategy_diagnostics_for_admission(
            posterior,
            observer,
            &ranked_actions,
            &baseline_actions,
            &promoted_spatial_actions,
            branch_cap,
        ) {
            let admitted = admit_strategy_challengers(
                &mut diagnostics,
                &root_scored,
                &baseline_retained,
                &protected_actions,
                branch_cap,
            );
            strategy_admission = Some(diagnostics);
            admitted
        } else {
            baseline_retained.clone()
        }
    } else {
        baseline_retained.clone()
    };
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
    // Concentrate nodes on the leading root actions, but preserve a complete
    // root table in every positive posterior world before spending
    // discretionary work according to posterior mass.
    let positive_particle_count = particles
        .iter()
        .filter(|particle| particle.weight > 0.0)
        .count()
        .max(1) as u32;
    let canonical_root_priority = (0..root_actions.len()).collect::<Vec<_>>();
    let initial_particle_budgets =
        allocate_particle_node_budgets(particles, maximum_nodes, root_actions.len() as u32);
    let mut initial_root_allocated_nodes = vec![0u32; root_actions.len()];
    for (particle, particle_budget) in particles.iter().zip(&initial_particle_budgets) {
        if particle.weight <= 0.0 || *particle_budget == 0 {
            continue;
        }
        let root_budgets = allocate_root_node_budgets_by_priority(
            root_actions.len(),
            *particle_budget,
            &canonical_root_priority,
        );
        for (allocated, budget) in initial_root_allocated_nodes.iter_mut().zip(root_budgets) {
            *allocated = allocated.saturating_add(budget);
        }
    }
    let retained_roots = root_actions
        .iter()
        .enumerate()
        .map(|(action_index, (action, prior))| {
            let diagnostic = ranked_diagnostics
                .iter()
                .find(|candidate| candidate.action == *action);
            let allocated_nodes = initial_root_allocated_nodes[action_index];
            RetainedRootDiagnostic {
                action: action.clone(),
                pre_truncation_rank: diagnostic.map(|candidate| candidate.rank),
                prior: *prior,
                // Particle budgets are no longer uniform. Keep the existing
                // scalar diagnostic as the floor average while allocated_nodes
                // records the exact first-wave root envelope.
                node_budget_per_particle: allocated_nodes / positive_particle_count,
                allocated_nodes,
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
        root_search_work: Vec::new(),
        strategy_shadow: strategy_admission,
        trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
        search_winner: None,
        decisive_plan_replacement: None,
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
    let mut completed_root_work = root_actions
        .iter()
        .cloned()
        .map(|action| RootSearchWorkDiagnostic {
            action,
            nodes: 0,
            completed_wave_depth: 0,
            cutoff_depth_counts: Vec::new(),
            posterior_mass_reaching_controlled_next_decision: 0.0,
            posterior_mass_reaching_terminal: 0.0,
        })
        .collect::<Vec<_>>();

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
        for (action_index, action) in root_actions.iter().enumerate() {
            let mut next = particle.state.clone();
            let entry = if next.apply(action).is_ok() {
                if next.is_terminal() {
                    completed_root_work[action_index].posterior_mass_reaching_terminal += weight;
                }
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
    let actor = observer as usize;
    let aggregate_winner = |entries: &[Aggregate]| {
        entries
            .iter()
            .max_by(|left, right| {
                let left_value = left.value[actor] / left.covered_weight.max(f32::EPSILON);
                let right_value = right.value[actor] / right.covered_weight.max(f32::EPSILON);
                left_value.total_cmp(&right_value)
            })
            .map(|entry| entry.action.clone())
    };
    let floor_winner = aggregate_winner(&aggregate);
    let mut attempted_depth = 0u8;
    let mut evidence_escalation_triggered = false;
    let mut evidence_escalation_pending = false;
    let mut evidence_escalation_completed = false;
    let mut evidence_escalation_strengthened = false;
    let mut evidence_escalation_baseline_nodes = 0u32;
    let mut evidence_escalation_baseline_root_nodes = Vec::<u32>::new();
    let mut evidence_escalation_start_nodes = 0u32;
    let mut evidence_escalation_start_ms = 0u32;
    let mut evidence_escalation_nodes = 0u32;
    let mut evidence_escalation_elapsed_ms = 0u32;
    let mut target_depth = 1u8;
    // Root actions stay in their canonical production order for values,
    // provenance and diagnostics. Only this permutation changes between
    // completed waves to steer the existing root budget profile.
    let mut root_allocation_priority = canonical_root_priority;
    // Targets come only from the last authoritative completed wave. Each root
    // may widen at most one future controlled information set in the next wave.
    let mut future_self_widening_targets = vec![None; root_actions.len()];

    while target_depth <= maximum_depth {
        backend.check_cancelled()?;
        let active_deadline = if evidence_escalation_pending {
            &hard_deadline
        } else {
            &deadline
        };
        if active_deadline.has_elapsed() {
            deadline_reached = true;
            break;
        }
        let wave_node_budget = match node_budget_mode {
            BeliefNodeBudgetMode::Global => maximum_nodes.saturating_sub(nodes),
            BeliefNodeBudgetMode::PerDepthWave if evidence_escalation_pending => {
                evidence_escalation_node_budget(maximum_nodes)
            }
            BeliefNodeBudgetMode::PerDepthWave => maximum_nodes,
        };
        let minimum_complete_wave_nodes =
            positive_particle_count.saturating_mul(root_actions.len().max(1) as u32);
        if wave_node_budget < minimum_complete_wave_nodes {
            break;
        }
        let wave_target_depth = if evidence_escalation_pending {
            evidence_escalation_target_depth(target_depth)
        } else {
            target_depth
        };
        attempted_depth = attempted_depth.max(wave_target_depth);
        let wave_particle_budgets = allocate_particle_node_budgets(
            particles,
            wave_node_budget,
            root_actions.len() as u32,
        );
        debug_assert!(
            particles.iter().all(|particle| particle.weight <= 0.0)
                || wave_particle_budgets.iter().sum::<u32>() == wave_node_budget
        );
        let mut baseline_wave = Vec::<Aggregate>::new();
        let mut challenger_wave = Vec::<Aggregate>::new();
        let mut wave_particles = 0usize;
        let mut wave_depth = 0u8;
        let mut wave_complete = true;
        let wave_start_nodes = nodes;
        let mut wave_root_nodes = vec![0u32; root_actions.len()];
        let mut baseline_root_future_self_mass = vec![0.0f32; root_actions.len()];
        let mut challenger_root_future_self_mass = vec![0.0f32; root_actions.len()];
        let mut baseline_root_terminal_mass = vec![0.0f32; root_actions.len()];
        let mut challenger_root_terminal_mass = vec![0.0f32; root_actions.len()];
        let mut baseline_ambiguity_evidence =
            vec![Vec::<ControlledAmbiguityEvidence>::new(); root_actions.len()];
        let mut challenger_ambiguity_evidence =
            vec![Vec::<ControlledAmbiguityEvidence>::new(); root_actions.len()];
        let mut wave_root_cutoff_depth_counts = vec![Vec::<u32>::new(); root_actions.len()];
        let wave_transposition_table = Rc::new(RefCell::new(TranspositionTable::default()));
        let mut wave_root_target_depths = vec![wave_target_depth; root_actions.len()];
        if wave_target_depth > 1 && wave_target_depth < maximum_depth {
            for canonical_index in root_allocation_priority
                .iter()
                .copied()
                .take(root_actions.len().min(4))
            {
                wave_root_target_depths[canonical_index] = wave_target_depth.saturating_add(1);
            }
            attempted_depth = attempted_depth.max(
                wave_root_target_depths
                    .iter()
                    .copied()
                    .max()
                    .unwrap_or(wave_target_depth),
            );
        }

        let wave_action_budgets_by_particle = particles
            .iter()
            .enumerate()
            .map(|(particle_index, particle)| {
                if particle.weight <= 0.0 {
                    return vec![0; root_actions.len()];
                }
                let budgets = allocate_root_node_budgets_by_priority(
                    root_actions.len(),
                    wave_particle_budgets[particle_index],
                    &root_allocation_priority,
                );
                debug_assert_eq!(
                    budgets.iter().sum::<u32>(),
                    wave_particle_budgets[particle_index]
                );
                budgets
            })
            .collect::<Vec<_>>();
        let root_widening_enabled = root_actions
            .iter()
            .enumerate()
            .map(|(action_index, _)| {
                !evidence_escalation_pending
                    && future_self_widening_targets[action_index].is_some()
                    && particles.iter().enumerate().all(|(particle_index, particle)| {
                        if particle.weight <= 0.0 {
                            return true;
                        }
                        controlled_widening_budgets(
                            wave_action_budgets_by_particle[particle_index][action_index],
                        )
                        .is_some()
                    })
            })
            .collect::<Vec<_>>();

        'particles: for (particle_index, particle) in particles.iter().enumerate() {
            let weight = particle.weight.max(0.0) / total_weight;
            if weight <= 0.0 {
                continue;
            }
            wave_particles += 1;
            let wave_action_budgets = &wave_action_budgets_by_particle[particle_index];
            for (action_index, action) in root_actions.iter().enumerate() {
                if active_deadline.has_elapsed() {
                    deadline_reached = true;
                    wave_complete = false;
                    break 'particles;
                }
                let nodes_for_action = wave_action_budgets[action_index].max(1);
                let variant_budgets = if root_widening_enabled[action_index] {
                    let budgets = controlled_widening_budgets(nodes_for_action)
                        .expect("widening-enabled root must fund both policy variants");
                    vec![budgets[0], budgets[1]]
                } else {
                    vec![nodes_for_action]
                };
                debug_assert_eq!(variant_budgets.iter().sum::<u32>(), nodes_for_action);

                let mut next = particle.state.clone();
                if next.apply(action).is_err() {
                    let entry = RowEntry {
                        action: action.clone(),
                        value: evaluate(&particle.state),
                        legal: false,
                    };
                    accumulate(&mut baseline_wave, entry.clone(), weight);
                    if root_widening_enabled[action_index] {
                        accumulate(&mut challenger_wave, entry, weight);
                    }
                    continue;
                }
                let completed_turn = next.turn != particle.state.turn
                    || next.current_player != particle.state.current_player;

                for (variant_index, allowance) in variant_budgets.into_iter().enumerate() {
                    if active_deadline.has_elapsed() {
                        deadline_reached = true;
                        wave_complete = false;
                        break 'particles;
                    }
                    let policy_override = (variant_index == 1)
                        .then(|| future_self_widening_targets[action_index].clone())
                        .flatten();
                    let mut searcher = Searcher {
                        algorithm: if paranoid {
                            Algorithm::Paranoid { root: observer }
                        } else {
                            Algorithm::MaxN
                        },
                        maximum_depth: wave_root_target_depths[action_index],
                        maximum_nodes: allowance,
                        node_limit: allowance,
                        branch_cap: branch_cap.max(1),
                        nodes: 0,
                        cutoffs: 0,
                        deepest_depth: 0,
                        // Variant allowances partition the original cell quota.
                        // Only the existing shared deadline can abort the wave.
                        deadline: active_deadline.clone(),
                        deadline_reached: false,
                        observation_safe_recursive: true,
                        controlled_player: Some(observer),
                        controlled_policy_override: policy_override,
                        controlled_ambiguity_hits: Vec::new(),
                        controlled_next_decision_reached: false,
                        terminal_reached: false,
                        cutoff_depth_counts: Vec::new(),
                        evaluation_cache: Rc::clone(&evaluation_cache),
                        transposition_table: Some(Rc::clone(&wave_transposition_table)),
                    };
                    let mut candidate_value = backend.visit(
                        &mut searcher,
                        &next,
                        u8::from(completed_turn),
                        if completed_turn { 0 } else { 1 },
                    )?;
                    apply_action_friction(
                        &mut candidate_value,
                        &particle.state,
                        action,
                        observer,
                    );

                    wave_root_nodes[action_index] =
                        wave_root_nodes[action_index].saturating_add(searcher.nodes);
                    let is_challenger = variant_index == 1;
                    if searcher.controlled_next_decision_reached {
                        if is_challenger {
                            challenger_root_future_self_mass[action_index] += weight;
                        } else {
                            baseline_root_future_self_mass[action_index] += weight;
                        }
                    }
                    if searcher.terminal_reached || next.is_terminal() {
                        if is_challenger {
                            challenger_root_terminal_mass[action_index] += weight;
                        } else {
                            baseline_root_terminal_mass[action_index] += weight;
                        }
                    }
                    let ambiguity_evidence = if is_challenger {
                        &mut challenger_ambiguity_evidence[action_index]
                    } else {
                        &mut baseline_ambiguity_evidence[action_index]
                    };
                    accumulate_controlled_ambiguity_evidence(
                        ambiguity_evidence,
                        &searcher.controlled_ambiguity_hits,
                    );
                    if wave_root_cutoff_depth_counts[action_index].len()
                        < searcher.cutoff_depth_counts.len()
                    {
                        wave_root_cutoff_depth_counts[action_index]
                            .resize(searcher.cutoff_depth_counts.len(), 0);
                    }
                    for (cutoff_depth, count) in searcher.cutoff_depth_counts.iter().enumerate() {
                        wave_root_cutoff_depth_counts[action_index][cutoff_depth] =
                            wave_root_cutoff_depth_counts[action_index][cutoff_depth]
                                .saturating_add(*count);
                    }
                    nodes += searcher.nodes;
                    cutoffs += searcher.cutoffs;
                    wave_depth = wave_depth.max(searcher.deepest_depth);
                    if searcher.deadline_reached || active_deadline.has_elapsed() {
                        deadline_reached |=
                            searcher.deadline_reached || active_deadline.has_elapsed();
                        wave_complete = false;
                        break 'particles;
                    }
                    let target_wave = if is_challenger {
                        &mut challenger_wave
                    } else {
                        &mut baseline_wave
                    };
                    accumulate(
                        target_wave,
                        RowEntry {
                            action: action.clone(),
                            value: candidate_value,
                            legal: true,
                        },
                        weight,
                    );
                }
            }
        }

        let mut wave = Vec::<Aggregate>::with_capacity(root_actions.len());
        let mut selected_challenger_by_root = vec![false; root_actions.len()];
        for (action_index, action) in root_actions.iter().enumerate() {
            let Some(baseline) = baseline_wave
                .iter()
                .find(|candidate| candidate.action == *action)
            else {
                continue;
            };
            if root_widening_enabled[action_index]
                && let Some(challenger) = challenger_wave
                    .iter()
                    .find(|candidate| candidate.action == *action)
            {
                let baseline_scalar =
                    baseline.value[actor] / baseline.covered_weight.max(f32::EPSILON);
                let challenger_scalar =
                    challenger.value[actor] / challenger.covered_weight.max(f32::EPSILON);
                if challenger_scalar > baseline_scalar {
                    selected_challenger_by_root[action_index] = true;
                    wave.push(challenger.clone());
                    continue;
                }
            }
            wave.push(baseline.clone());
        }
        let wave_root_future_self_mass = root_actions
            .iter()
            .enumerate()
            .map(|(action_index, _)| {
                if selected_challenger_by_root[action_index] {
                    challenger_root_future_self_mass[action_index]
                } else {
                    baseline_root_future_self_mass[action_index]
                }
            })
            .collect::<Vec<_>>();
        let wave_root_terminal_mass = root_actions
            .iter()
            .enumerate()
            .map(|(action_index, _)| {
                if selected_challenger_by_root[action_index] {
                    challenger_root_terminal_mass[action_index]
                } else {
                    baseline_root_terminal_mass[action_index]
                }
            })
            .collect::<Vec<_>>();
        let wave_next_widening_targets = root_actions
            .iter()
            .enumerate()
            .map(|(action_index, _)| {
                let evidence = if selected_challenger_by_root[action_index] {
                    &challenger_ambiguity_evidence[action_index]
                } else {
                    &baseline_ambiguity_evidence[action_index]
                };
                select_controlled_widening_target(evidence)
            })
            .collect::<Vec<_>>();

        wave_complete &= wave_particles == positive_particle_count as usize
            && wave.len() == root_actions.len();

        if !wave_complete {
            if evidence_escalation_pending {
                evidence_escalation_nodes = nodes.saturating_sub(evidence_escalation_start_nodes);
                evidence_escalation_elapsed_ms = hard_deadline
                    .elapsed_ms()
                    .saturating_sub(evidence_escalation_start_ms);
            }
            break;
        }
        let wave_winner = aggregate_winner(&wave);
        let wave_realized_nodes = nodes.saturating_sub(wave_start_nodes);
        let wave_root_work = root_actions
            .iter()
            .enumerate()
            .map(|(action_index, action)| RootSearchWorkDiagnostic {
                action: action.clone(),
                nodes: wave_root_nodes[action_index],
                completed_wave_depth: wave_root_target_depths[action_index],
                cutoff_depth_counts: wave_root_cutoff_depth_counts[action_index].clone(),
                posterior_mass_reaching_controlled_next_decision: wave_root_future_self_mass
                    [action_index]
                    .clamp(0.0, 1.0),
                posterior_mass_reaching_terminal: wave_root_terminal_mass[action_index]
                    .clamp(0.0, 1.0),
            })
            .collect::<Vec<_>>();
        let allocation_evidence = root_actions
            .iter()
            .map(|action| {
                wave.iter()
                    .find(|candidate| candidate.action == *action)
                    .map(|candidate| {
                        (
                            candidate.lower_bound[actor],
                            candidate.value[actor]
                                / candidate.covered_weight.max(f32::EPSILON),
                        )
                    })
            })
            .collect::<Option<Vec<_>>>()
            .expect("completed wave must cover every retained root");
        root_allocation_priority = root_allocation_priority_from_evidence(&allocation_evidence);

        if evidence_escalation_pending {
            evidence_escalation_completed = true;
            evidence_escalation_nodes = nodes.saturating_sub(evidence_escalation_start_nodes);
            evidence_escalation_elapsed_ms = hard_deadline
                .elapsed_ms()
                .saturating_sub(evidence_escalation_start_ms);
            evidence_escalation_strengthened = realized_root_evidence_strengthened(
                &evidence_escalation_baseline_root_nodes,
                &wave_root_nodes,
            );
            evidence_escalation_pending = false;
            if evidence_escalation_strengthened {
                aggregate = wave;
                particles_searched = wave_particles;
                depth = wave_depth;
                completed_root_work = wave_root_work;
                break;
            }
            continue;
        }

        aggregate = wave;
        particles_searched = wave_particles;
        depth = wave_depth;
        completed_root_work = wave_root_work;
        future_self_widening_targets = wave_next_widening_targets;
        if target_depth == 1
            && should_escalate_binary_root_evidence(
                node_budget_mode,
                root_actions.len(),
                evidence_escalation_ms,
                floor_winner.as_ref(),
                wave_winner.as_ref(),
            )
        {
            evidence_escalation_triggered = true;
            evidence_escalation_pending = true;
            evidence_escalation_baseline_nodes = wave_realized_nodes;
            evidence_escalation_baseline_root_nodes = wave_root_nodes;
            evidence_escalation_start_nodes = nodes;
            evidence_escalation_start_ms = hard_deadline.elapsed_ms();
        }
        target_depth = target_depth.saturating_add(1);
    }
    let deep_waves_ms = elapsed_stage_ms(&hard_deadline, deep_waves_started);
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
    let mut chosen_index =
        decisive_current_turn_plan_replacement_index(&actions, actor, &ranked_diagnostics, 0);
    if chosen_index > 0
        && let (Some(leading), Some(replacement)) = (actions.first(), actions.get(chosen_index))
    {
        provenance.decisive_plan_replacement =
            Some((leading.action.clone(), replacement.action.clone()));
    }
    if let Some(provisional) = actions.get(chosen_index) {
        let provisional_loss = forced_loss_weight(
            posterior
                .iter()
                .map(|particle| (&particle.state, particle.weight)),
            observer,
            &provisional.action,
        );
        if provisional_loss >= 1.0 - 1e-6
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
            provenance.safety_replacement =
                Some((provisional.action.clone(), actions[escape_index].action.clone()));
            chosen_index = escape_index;
        }
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
    let strategy_ranked_actions = ranked_diagnostics
        .iter()
        .map(|candidate| candidate.action.clone())
        .collect::<Vec<_>>();
    let strategy_search_actions = actions
        .iter()
        .map(|candidate| candidate.action.clone())
        .collect::<Vec<_>>();
    provenance.root_search_work = completed_root_work;
    attach_strategy_shadow(
        &mut provenance,
        posterior,
        &strategy_ranked_actions,
        &strategy_search_actions,
        maximum_depth,
        depth,
        deadline_reached,
    );
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
            evidence_escalation_triggered,
            evidence_escalation_completed,
            evidence_escalation_strengthened,
            evidence_escalation_baseline_nodes,
            evidence_escalation_nodes,
            evidence_escalation_ms: evidence_escalation_elapsed_ms,
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
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: 1,
        },
        paranoid,
        &[],
        BeliefNodeBudgetMode::Global,
        0,
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
        controlled_player: None,
        controlled_policy_override: None,
        controlled_ambiguity_hits: Vec::new(),
        controlled_next_decision_reached: false,
        terminal_reached: false,
        cutoff_depth_counts: Vec::new(),
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
        transposition_table: None,
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
        controlled_player: None,
        controlled_policy_override: None,
        controlled_ambiguity_hits: Vec::new(),
        controlled_next_decision_reached: false,
        terminal_reached: false,
        cutoff_depth_counts: Vec::new(),
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
        transposition_table: None,
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
        controlled_player: None,
        controlled_policy_override: None,
        controlled_ambiguity_hits: Vec::new(),
        controlled_next_decision_reached: false,
        terminal_reached: false,
        cutoff_depth_counts: Vec::new(),
        evaluation_cache: Rc::new(RefCell::new(HashMap::new())),
        transposition_table: None,
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
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_maxn_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        config,
        false,
        &[],
        BeliefNodeBudgetMode::Global,
        0,
    )
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
            strategy_policy: StrategyPolicy::Baseline,
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
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: usize::MAX,
        },
        false,
        root_exclusions,
        BeliefNodeBudgetMode::Global,
        0,
    )
}

pub fn search_weighted_belief_maxn_iterative_timed_excluding(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    nodes_per_depth_wave: u32,
    time_budget_ms: u32,
    evidence_escalation_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    search_weighted_belief_maxn_iterative_timed_excluding_with_strategy_policy(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes: nodes_per_depth_wave,
            time_budget_ms,
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: usize::MAX,
        },
        evidence_escalation_ms,
        root_exclusions,
    )
}

pub fn search_weighted_belief_maxn_iterative_timed_excluding_with_strategy_policy(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    evidence_escalation_ms: u32,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        config,
        false,
        root_exclusions,
        BeliefNodeBudgetMode::PerDepthWave,
        evidence_escalation_ms,
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
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: usize::MAX,
        },
    )
}

pub fn search_weighted_belief_paranoid_with_config(
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        config,
        true,
        &[],
        BeliefNodeBudgetMode::Global,
        0,
    )
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
            strategy_policy: StrategyPolicy::Baseline,
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
            strategy_policy: StrategyPolicy::Baseline,
            strategic_particle_limit: usize::MAX,
        },
        true,
        root_exclusions,
        BeliefNodeBudgetMode::Global,
        0,
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
    search_weighted_belief_paranoid_iterative_timed_excluding_with_strategy_policy(
        particles,
        depth,
        branch_cap,
        nodes_per_depth_wave,
        time_budget_ms,
        StrategyPolicy::Baseline,
        root_exclusions,
    )
}

pub fn search_weighted_belief_paranoid_iterative_timed_excluding_with_strategy_policy(
    particles: &[BeliefParticle],
    depth: u8,
    branch_cap: usize,
    nodes_per_depth_wave: u32,
    time_budget_ms: u32,
    strategy_policy: StrategyPolicy,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    belief_search(
        particles,
        BeliefDepthConfig {
            maximum_depth: depth,
            branch_cap,
            maximum_nodes: nodes_per_depth_wave,
            time_budget_ms,
            strategy_policy,
            strategic_particle_limit: usize::MAX,
        },
        true,
        root_exclusions,
        BeliefNodeBudgetMode::PerDepthWave,
        0,
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
    Constant([f32; 4]),
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

    fn constant(&mut self, value: [f32; 4]) -> usize {
        let node = self.nodes.len();
        self.nodes.push(CudaDeferredNode::Constant(value));
        node
    }

    fn weighted(&mut self, children: Vec<CudaDeferredChild>) -> usize {
        let node = self.nodes.len();
        self.nodes.push(CudaDeferredNode::Weighted(children));
        node
    }

    fn backup_all(&self, leaf_values: &[[f32; 4]]) -> Vec<[f32; 4]> {
        let mut values: Vec<[f32; 4]> = Vec::with_capacity(self.nodes.len());
        for node in &self.nodes {
            let value = match node {
                CudaDeferredNode::Leaf(leaf) => leaf_values[*leaf],
                CudaDeferredNode::Constant(value) => *value,
                CudaDeferredNode::Weighted(children) => {
                    let mut expected = [0.0; 4];
                    for child in children {
                        let mut value = values[child.node];
                        if let Some((actor, friction)) = child.friction {
                            value[actor as usize] =
                                (value[actor as usize] - friction).max(0.0);
                        }
                        for player in 0..4 {
                            expected[player] += value[player] * child.weight;
                        }
                    }
                    expected
                }
            };
            values.push(value);
        }
        values
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
struct CudaDeferredPendingTransposition {
    key: TranspositionIdentity,
    state: GameState,
    node: usize,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
struct CudaDeferredSearcher<'a> {
    tree: &'a mut CudaDeferredTree,
    algorithm: Algorithm,
    maximum_depth: u8,
    maximum_nodes: u32,
    node_limit: u32,
    branch_cap: usize,
    controlled_player: u8,
    controlled_policy_override: Option<ControlledWideningTarget>,
    controlled_ambiguity_hits: Vec<ControlledAmbiguityHit>,
    nodes: u32,
    deepest_depth: u8,
    controlled_next_decision_reached: bool,
    terminal_reached: bool,
    cutoff_depth_counts: Vec<u32>,
    transposition_table: Option<Rc<RefCell<TranspositionTable>>>,
    local_transpositions: HashMap<TranspositionIdentity, (GameState, usize)>,
    pending_transpositions: Vec<CudaDeferredPendingTransposition>,
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
impl CudaDeferredSearcher<'_> {
    fn transposition_key(
        &self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        remaining_subtree_allowance: u32,
    ) -> Option<TranspositionIdentity> {
        self.transposition_table.as_ref()?;
        Some(transposition_identity(
            state,
            self.algorithm,
            depth,
            actions_in_turn,
            0.0,
            1.0,
            remaining_subtree_allowance,
            self.maximum_depth,
            self.branch_cap,
            true,
            Some(self.controlled_player),
            self.controlled_policy_override.clone(),
        ))
    }

    fn lookup_transposition(
        &mut self,
        key: &TranspositionIdentity,
        state: &GameState,
    ) -> Option<usize> {
        if let Some(value) = self
            .transposition_table
            .as_ref()
            .and_then(|table| table.borrow().lookup(key, state))
        {
            return Some(self.tree.constant(value));
        }
        let node = self
            .local_transpositions
            .get(&key)
            .and_then(|(cached_state, node)| (cached_state == state).then_some(*node));
        node
    }

    fn record_transposition(
        &mut self,
        key: TranspositionIdentity,
        state: &GameState,
        node: usize,
    ) {
        let Some(table) = self.transposition_table.as_ref() else {
            return;
        };
        {
            let table = table.borrow();
            if table.entries.contains_key(&key)
                || table
                    .entries
                    .len()
                    .saturating_add(self.local_transpositions.len())
                    >= TRANSPOSITION_MAX_WAVE_ENTRIES
            {
                return;
            }
        }
        if self.local_transpositions.contains_key(&key) {
            return;
        }
        self.local_transpositions
            .insert(key.clone(), (state.clone(), node));
        self.pending_transpositions.push(CudaDeferredPendingTransposition {
            key,
            state: state.clone(),
            node,
        });
    }

    fn mark_cutoff(&mut self, depth: u8) {
        self.cutoff_depth_counts
            .resize(self.cutoff_depth_counts.len().max(depth as usize + 1), 0);
        self.cutoff_depth_counts[depth as usize] += 1;
    }

    fn record_controlled_ambiguity(&mut self, state: &GameState, actor: u8, depth: u8) {
        if actor != self.controlled_player || depth == 0 {
            return;
        }
        let actions = actor_proposal_actions(state);
        if let Some(hit) = controlled_ambiguity_hit_from_observation(state, &actions, actor) {
            self.controlled_ambiguity_hits.push(hit);
        }
    }

    fn controlled_policy_action(
        &self,
        state: &GameState,
        actions: &[Action],
        actor: u8,
    ) -> Vec<(Action, f32)> {
        controlled_policy_action(
            state,
            actions,
            actor,
            self.controlled_policy_override.as_ref(),
        )
    }

    fn visit(
        &mut self,
        state: &GameState,
        depth: u8,
        actions_in_turn: u8,
        subtree_limit: u32,
        should_stop: &dyn Fn() -> Option<DepthBeliefError>,
    ) -> Result<usize, DepthBeliefError> {
        if let Some(reason) = should_stop() {
            return Err(reason);
        }
        let subtree_limit = subtree_limit.min(self.node_limit).min(self.maximum_nodes);
        let remaining_subtree_allowance = subtree_limit.saturating_sub(self.nodes);
        if self.nodes >= subtree_limit {
            self.mark_cutoff(depth);
            return Ok(self.tree.leaf(state));
        }
        self.nodes += 1;
        self.deepest_depth = self.deepest_depth.max(depth);
        if state.is_terminal() {
            self.terminal_reached = true;
            return Ok(self.tree.leaf(state));
        }
        let node_kind = state.node_kind();
        if let NodeKind::Decision { actor } = node_kind
            && actor == self.controlled_player
            && depth > 0
        {
            self.controlled_next_decision_reached = true;
            self.record_controlled_ambiguity(state, actor, depth);
        }
        if depth >= self.maximum_depth || actions_in_turn >= 18 {
            self.mark_cutoff(depth);
            return Ok(self.tree.leaf(state));
        }
        let actions = state.legal_actions();
        if actions.is_empty() {
            return Ok(self.tree.leaf(state));
        }
        let transposition_key =
            self.transposition_key(state, depth, actions_in_turn, remaining_subtree_allowance);
        if let Some(key) = transposition_key.as_ref()
            && let Some(node) = self.lookup_transposition(key, state)
        {
            return Ok(node);
        }

        let node = match node_kind {
            NodeKind::Terminal => Ok(self.tree.leaf(state)),
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
                        self.visit(
                            &next,
                            depth,
                            actions_in_turn.saturating_add(1),
                            child_limit,
                            should_stop,
                        )?
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
                Ok(self.tree.weighted(children))
            }
            NodeKind::Decision { actor } => {
                let remaining = subtree_limit.saturating_sub(self.nodes);
                if remaining == 0 {
                    self.mark_cutoff(depth);
                    return Ok(self.tree.leaf(state));
                }
                let proposal_actions = actor_proposal_actions(state);
                if proposal_actions.is_empty() {
                    return Ok(self.tree.leaf(state));
                }
                let mut ranked = if actor == self.controlled_player {
                    self.controlled_policy_action(state, &proposal_actions, actor)
                } else {
                    recursive_observation_policy(state, &proposal_actions, actor, self.branch_cap)
                };
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
                            should_stop,
                        )?
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
                Ok(self.tree.weighted(children))
            }
        }?;
        if let Some(key) = transposition_key {
            self.record_transposition(key, state, node);
        }
        Ok(node)
    }
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn prepare_cuda_exact_evaluator(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
) -> Result<(), DepthBeliefError> {
    let started = std::time::Instant::now();
    let first = particles.first().ok_or(DepthBeliefError::Empty)?;
    evaluator
        .prepare_topology(first.state.board.as_ref())
        .map_err(|_| DepthBeliefError::CudaEvaluationFailed)?;
    for particle in particles.iter().skip(1) {
        if !evaluator
            .topology_matches(particle.state.board.as_ref())
            .map_err(|_| DepthBeliefError::CudaEvaluationFailed)?
        {
            return Err(DepthBeliefError::CudaEvaluationFailed);
        }
    }
    record_cuda_duration(&CUDA_ROOT_PREPARATION_NANOS, started.elapsed());
    Ok(())
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn shared_cuda_belief_search(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
    node_budget_mode: BeliefNodeBudgetMode,
    evidence_escalation_ms: u32,
    should_cancel: &dyn Fn() -> bool,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let started = std::time::Instant::now();
    prepare_cuda_exact_evaluator(evaluator, particles)?;
    let result = belief_search_backend(
        particles,
        config,
        false,
        root_exclusions,
        node_budget_mode,
        evidence_escalation_ms,
        &mut BeliefBackend::Cuda(evaluator, should_cancel),
    );
    CUDA_SEARCH_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    CUDA_DEFERRED_CALLS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    record_cuda_duration(&CUDA_SEARCH_TOTAL_NANOS, started.elapsed());
    result
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
fn shared_cuda_belief_search_mutex(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    let wait_started = std::time::Instant::now();
    let mut evaluator = evaluator
        .lock()
        .map_err(|_| DepthBeliefError::CudaEvaluatorLockPoisoned)?;
    record_cuda_duration(&CUDA_QUEUE_WAIT_NANOS, wait_started.elapsed());
    shared_cuda_belief_search(
        &mut evaluator,
        particles,
        config,
        root_exclusions,
        BeliefNodeBudgetMode::Global,
        0,
        &|| false,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    shared_cuda_belief_search(
        evaluator,
        particles,
        config,
        &[],
        BeliefNodeBudgetMode::Global,
        0,
        &|| false,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_mutex(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    shared_cuda_belief_search_mutex(evaluator, particles, config, &[])
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_excluding(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    shared_cuda_belief_search(
        evaluator,
        particles,
        config,
        root_exclusions,
        BeliefNodeBudgetMode::Global,
        0,
        &|| false,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_excluding_controlled<F>(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
    should_cancel: F,
) -> Result<BeliefDepthResult, DepthBeliefError>
where
    F: Fn() -> bool,
{
    shared_cuda_belief_search(
        evaluator,
        particles,
        config,
        root_exclusions,
        BeliefNodeBudgetMode::Global,
        0,
        &should_cancel,
    )
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_with_config_mutex_excluding(
    evaluator: &std::sync::Mutex<crate::CudaExactEvaluator>,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    root_exclusions: &[Action],
) -> Result<BeliefDepthResult, DepthBeliefError> {
    shared_cuda_belief_search_mutex(evaluator, particles, config, root_exclusions)
}

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub fn search_weighted_belief_maxn_cuda_iterative_controlled(
    evaluator: &mut crate::CudaExactEvaluator,
    particles: &[BeliefParticle],
    config: BeliefDepthConfig,
    evidence_escalation_ms: u32,
    root_exclusions: &[Action],
    should_cancel: &dyn Fn() -> bool,
) -> Result<BeliefDepthResult, DepthBeliefError> {
    shared_cuda_belief_search(
        evaluator,
        particles,
        config,
        root_exclusions,
        BeliefNodeBudgetMode::PerDepthWave,
        evidence_escalation_ms,
        should_cancel,
    )
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use colonist_catan_core::{Action, DevCard, GameState, NodeKind, Phase, Resource, SplitMix64};

    use super::{
        BeliefNodeBudgetMode, DepthActionValue, RankedRootDiagnostic, apply_action_friction,
        decisive_current_turn_plan_replacement_index, evidence_escalation_node_budget,
        evidence_escalation_target_depth, normalize_belief_root_priors,
        realized_root_evidence_strengthened, search_belief_maxn, search_belief_maxn_bounded,
        search_maxn, search_paranoid, search_weighted_belief_maxn_bounded,
        search_weighted_belief_maxn_bounded_timed,
        search_weighted_belief_maxn_iterative_timed_excluding,
        should_escalate_binary_root_evidence,
    };
    use crate::mcts::BeliefParticle;

    #[test]
    fn task9783_decisive_current_turn_plan_beats_shallow_end_turn() {
        let end_turn = Action::EndTurn;
        let road_a = Action::BuildRoad { edge: 10 };
        let road_b = Action::BuildRoad { edge: 11 };
        let actions = vec![
            DepthActionValue {
                action: end_turn.clone(),
                value: [0.3627, 0.0, 0.0, 0.0],
                legal_weight: 1.0,
                lower_confidence_value: [0.3169, 0.0, 0.0, 0.0],
            },
            DepthActionValue {
                action: road_a.clone(),
                value: [0.3474, 0.0, 0.0, 0.0],
                legal_weight: 1.0,
                lower_confidence_value: [0.2490, 0.0, 0.0, 0.0],
            },
            DepthActionValue {
                action: road_b.clone(),
                value: [0.3444, 0.0, 0.0, 0.0],
                legal_weight: 1.0,
                lower_confidence_value: [0.3085, 0.0, 0.0, 0.0],
            },
        ];
        let diagnostic = |action, rank, planner_value, decisive_completion_mass, response_windows| {
            RankedRootDiagnostic {
                action,
                rank,
                prior: 0.0,
                planner_value: Some(planner_value),
                planner_completion_mass: Some(1.0),
                planner_decisive_completion_mass: Some(decisive_completion_mass),
                planner_response_windows: Some(response_windows),
                quota_score: 0.0,
            }
        };
        let ranked = vec![
            diagnostic(end_turn, 4, 17.4961, 0.0, 3.0),
            diagnostic(road_a, 3, 34.6522, 1.0, 0.0),
            diagnostic(road_b, 1, 34.6189, 1.0, 0.0),
        ];

        assert_eq!(
            decisive_current_turn_plan_replacement_index(&actions, 0, &ranked, 0),
            1
        );
    }

    #[test]
    fn comparable_decisive_plan_does_not_override_search_winner() {
        let end_turn = Action::EndTurn;
        let road = Action::BuildRoad { edge: 10 };
        let actions = vec![
            DepthActionValue {
                action: end_turn.clone(),
                value: [0.51, 0.0, 0.0, 0.0],
                legal_weight: 1.0,
                lower_confidence_value: [0.51, 0.0, 0.0, 0.0],
            },
            DepthActionValue {
                action: road.clone(),
                value: [0.49, 0.0, 0.0, 0.0],
                legal_weight: 1.0,
                lower_confidence_value: [0.49, 0.0, 0.0, 0.0],
            },
        ];
        let ranked = vec![
            RankedRootDiagnostic {
                action: end_turn,
                rank: 1,
                prior: 0.5,
                planner_value: Some(10.0),
                planner_completion_mass: Some(1.0),
                planner_decisive_completion_mass: Some(0.0),
                planner_response_windows: Some(3.0),
                quota_score: 1.0,
            },
            RankedRootDiagnostic {
                action: road,
                rank: 2,
                prior: 0.5,
                planner_value: Some(10.1),
                planner_completion_mass: Some(1.0),
                planner_decisive_completion_mass: Some(1.0),
                planner_response_windows: Some(0.0),
                quota_score: 0.5,
            },
        ];

        assert_eq!(
            decisive_current_turn_plan_replacement_index(&actions, 0, &ranked, 0),
            0
        );
    }

    #[test]
    fn evidence_escalation_requires_binary_completed_wave_disagreement() {
        let roll = Action::Roll;
        let knight = Action::PlayKnight {
            hex: 0,
            victim: None,
        };

        assert!(should_escalate_binary_root_evidence(
            BeliefNodeBudgetMode::PerDepthWave,
            2,
            1_500,
            Some(&roll),
            Some(&knight),
        ));
        assert!(!should_escalate_binary_root_evidence(
            BeliefNodeBudgetMode::PerDepthWave,
            2,
            1_500,
            Some(&roll),
            Some(&roll),
        ));
        assert!(!should_escalate_binary_root_evidence(
            BeliefNodeBudgetMode::PerDepthWave,
            3,
            1_500,
            Some(&roll),
            Some(&knight),
        ));
        assert!(!should_escalate_binary_root_evidence(
            BeliefNodeBudgetMode::PerDepthWave,
            2,
            0,
            Some(&roll),
            Some(&knight),
        ));
        assert!(!should_escalate_binary_root_evidence(
            BeliefNodeBudgetMode::Global,
            2,
            1_500,
            Some(&roll),
            Some(&knight),
        ));
        assert_eq!(evidence_escalation_node_budget(8_000), 24_000);
        assert_eq!(evidence_escalation_target_depth(2), 1);
    }

    #[test]
    fn evidence_escalation_requires_realized_per_root_dominance() {
        assert!(!realized_root_evidence_strengthened(&[60, 63], &[60, 63]));
        assert!(realized_root_evidence_strengthened(&[60, 63], &[61, 63]));
        assert!(!realized_root_evidence_strengthened(&[60, 63], &[59, 100]));
        assert!(!realized_root_evidence_strengthened(&[60, 63], &[120]));
    }

    fn binary_preroll_fixture(seed: u64) -> GameState {
        let mut state = GameState::standard(seed, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::PreRoll;
        state.current_player = 0;
        state.turn = 5;
        state.player_trades_enabled = false;
        for player in 0..state.players.len() {
            for resource in 0..5 {
                let count = state.players[player].resources[resource];
                state.players[player].resources[resource] = 0;
                state.bank[resource] = state.bank[resource].saturating_add(count);
            }
        }
        let knight = DevCard::Knight.index();
        state.development_deck[knight] -= 1;
        state.players[0].development[knight] += 1;
        state.players[0].bought_development[knight] = 0;
        state.players[0].played_development_this_turn = false;
        state.validate().unwrap();
        state
    }

    fn controlled_city_settlement_fixture() -> GameState {
        let mut state = binary_preroll_fixture(733);
        state.phase = Phase::Main;
        for resource in 0..5 {
            let amount = 5.min(state.bank[resource]);
            state.bank[resource] -= amount;
            state.players[0].resources[resource] += amount;
        }
        if state
            .legal_actions()
            .iter()
            .any(|action| matches!(action, Action::BuildSettlement { .. }))
        {
            return state;
        }
        for edge in 0..state.roads.len() {
            if state.roads[edge].is_some() {
                continue;
            }
            let mut candidate = state.clone();
            candidate.roads[edge] = Some(0);
            candidate.players[0].roads_left = candidate.players[0].roads_left.saturating_sub(1);
            if candidate
                .legal_actions()
                .iter()
                .any(|action| matches!(action, Action::BuildSettlement { .. }))
            {
                return candidate;
            }
        }
        panic!("fixture must expose a legal settlement alongside a city");
    }

    fn empty_main_cycle_fixture(seed: u64) -> GameState {
        let mut state = GameState::standard(seed, 3);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.turn = 5;
        state.player_trades_enabled = false;
        for player in 0..state.players.len() {
            for resource in 0..5 {
                let count = state.players[player].resources[resource];
                state.players[player].resources[resource] = 0;
                state.bank[resource] = state.bank[resource].saturating_add(count);
            }
        }
        state.validate().unwrap();
        assert_eq!(state.legal_actions(), vec![Action::EndTurn]);
        state
    }

    fn run_binary_preroll(
        state: &GameState,
        maximum_depth: u8,
        nodes_per_depth_wave: u32,
        time_budget_ms: u32,
        evidence_escalation_ms: u32,
    ) -> super::BeliefDepthResult {
        search_weighted_belief_maxn_iterative_timed_excluding(
            &[BeliefParticle {
                state: state.clone(),
                weight: 1.0,
            }],
            maximum_depth,
            10,
            nodes_per_depth_wave,
            time_budget_ms,
            evidence_escalation_ms,
            &[],
        )
        .unwrap()
    }

    #[test]
    fn continuation_cells_share_the_parent_deadline_budget() {
        let mut state = empty_main_cycle_fixture(739);
        state.players[0].resources[0] = 1;
        state.players[0].resources[1] = 1;
        state.bank[0] -= 1;
        state.bank[1] -= 1;
        state.validate().unwrap();
        assert!(state.legal_actions().len() > 1);

        let mut child_budgets = Vec::new();
        let mut capture_deadline = |searcher: &mut super::Searcher| {
            if searcher.maximum_depth >= 2 {
                child_budgets.push(searcher.deadline.budget_ms_for_test());
            }
        };
        let report = super::belief_search_backend(
            &[BeliefParticle { state, weight: 1.0 }],
            super::BeliefDepthConfig {
                maximum_depth: 2,
                branch_cap: 10,
                maximum_nodes: 100,
                time_budget_ms: 10_000,
                strategy_policy: Default::default(),
                strategic_particle_limit: usize::MAX,
            },
            false,
            &[],
            BeliefNodeBudgetMode::PerDepthWave,
            0,
            &mut super::BeliefBackend::BeforeVisit(&mut capture_deadline),
        ).unwrap();

        assert!(!report.deadline_reached);
        assert_eq!(report.depth, 2);
        assert!(child_budgets.len() > 1);
        assert!(child_budgets.iter().all(|budget| *budget == 10_000));
    }

    #[test]
    fn root_work_reports_future_self_reach_without_changing_fixed_work_choice() {
        let state = empty_main_cycle_fixture(739);
        let shallow = run_binary_preroll(&state, 2, 20_000, 10_000, 0);
        let deep = run_binary_preroll(&state, 3, 20_000, 10_000, 0);
        let repeated = run_binary_preroll(&state, 3, 20_000, 10_000, 0);

        assert_eq!(deep.chosen, repeated.chosen);
        assert_eq!(
            deep.provenance.root_search_work,
            repeated.provenance.root_search_work
        );
        assert_eq!(shallow.provenance.root_search_work.len(), 1);
        assert_eq!(deep.provenance.root_search_work.len(), 1);
        let shallow_work = &shallow.provenance.root_search_work[0];
        let deep_work = &deep.provenance.root_search_work[0];
        assert_eq!(shallow_work.action, Action::EndTurn);
        assert_eq!(deep_work.action, Action::EndTurn);
        assert!(shallow_work.nodes > 0);
        assert!(deep_work.nodes > 0);
        assert_eq!(
            shallow_work.posterior_mass_reaching_controlled_next_decision,
            0.0
        );
        assert_eq!(
            deep_work.posterior_mass_reaching_controlled_next_decision,
            1.0
        );
        assert!(deep_work.cutoff_depth_counts.get(3).copied().unwrap_or(0) > 0);
    }

    fn binary_preroll_floor_winner(state: &GameState, report: &super::BeliefDepthResult) -> Action {
        assert_eq!(report.actions.len(), 2);
        report
            .actions
            .iter()
            .map(|candidate| {
                let mut next = state.clone();
                next.apply(&candidate.action).unwrap();
                let mut value = super::evaluate_after_forced_chance(&next, 0);
                apply_action_friction(&mut value, state, &candidate.action, state.actor());
                (candidate.action.clone(), value[state.actor() as usize])
            })
            .max_by(|left, right| left.1.total_cmp(&right.1))
            .unwrap()
            .0
    }

    fn assert_saturated_escalation_resumes_iterative_depth(seed: u64) {
        let state = binary_preroll_fixture(seed);
        let shallow = run_binary_preroll(&state, 1, 8_000, 2_000, 0);
        assert_ne!(
            Some(binary_preroll_floor_winner(&state, &shallow)),
            shallow.chosen,
            "positive escalation fixture requires disagreement with the one-ply floor"
        );
        let wider_shallow = run_binary_preroll(&state, 1, 24_000, 4_500, 0);
        assert_eq!(shallow.nodes, wider_shallow.nodes);
        assert_eq!(shallow.actions.len(), wider_shallow.actions.len());
        for (left, right) in shallow.actions.iter().zip(&wider_shallow.actions) {
            assert_eq!(left.action, right.action);
            assert_eq!(left.value, right.value);
            assert_eq!(left.lower_confidence_value, right.lower_confidence_value);
            assert_eq!(left.legal_weight, right.legal_weight);
        }

        let ordinary = run_binary_preroll(&state, 2, 8_000, 10_000, 0);
        let escalated = run_binary_preroll(&state, 2, 8_000, 10_000, 2_500);
        assert_eq!(
            escalated.chosen, ordinary.chosen,
            "a saturated same-depth rerun must not replace ordinary depth-2 authority"
        );
        assert!(
            escalated.depth >= 2,
            "the same-depth retry must not consume and skip the ordinary depth-2 iteration"
        );
        let timings = escalated.stage_timings.as_ref().unwrap();
        assert!(timings.attempted_depth >= 2);
        assert!(timings.evidence_escalation_triggered);
        assert!(timings.evidence_escalation_completed);
        assert!(!timings.evidence_escalation_strengthened);
        assert_eq!(timings.evidence_escalation_baseline_nodes, shallow.nodes);
        assert_eq!(timings.evidence_escalation_nodes, shallow.nodes);
    }

    #[test]
    fn controlled_preroll_chooses_knight_when_policy_score_exceeds_roll() {
        let mut state = binary_preroll_fixture(727);
        state.victory_target = 30;
        let lumber = Resource::Lumber.index();
        let amount = state.bank[lumber].min(19);
        state.bank[lumber] -= amount;
        state.players[1].resources[lumber] += amount;
        state.players[1].public_victory_points = 25;
        let actions = state.legal_actions();
        let roll = Action::Roll;
        let observed = state.observed_state(0);
        let knight = actions
            .iter()
            .filter(|action| {
                matches!(
                    action,
                    Action::PlayKnight {
                        victim: Some(1),
                        ..
                    }
                )
            })
            .max_by(|left, right| {
                crate::policy::action_prior(&observed, left, 0)
                    .total_cmp(&crate::policy::action_prior(&observed, right, 0))
            })
            .cloned()
            .expect("fixture must expose a playable Knight against player 1");
        let pair = vec![roll.clone(), knight.clone()];
        let policy = crate::policy::normalize_priors(&observed, &pair, 0);
        assert_eq!(policy.first().map(|entry| &entry.0), Some(&knight));
        let quota_ordered = super::recursive_observation_policy(&state, &pair, 0, 8);
        assert_eq!(
            quota_ordered.first().map(|entry| &entry.0),
            Some(&Action::Roll)
        );
        let selected = super::recursive_observation_best_policy_action(&state, &pair, 0);
        assert_eq!(selected, vec![(knight, 1.0)]);
    }

    #[test]
    fn controlled_preroll_chooses_roll_when_policy_score_exceeds_knight() {
        let state = binary_preroll_fixture(729);
        let actions = state.legal_actions();
        let observed = state.observed_state(0);
        let knight = actions
            .iter()
            .filter(|action| matches!(action, Action::PlayKnight { .. }))
            .min_by(|left, right| {
                crate::policy::action_prior(&observed, left, 0)
                    .total_cmp(&crate::policy::action_prior(&observed, right, 0))
            })
            .cloned()
            .expect("fixture must expose a playable Knight");
        let pair = vec![Action::Roll, knight];
        let policy = crate::policy::normalize_priors(&observed, &pair, 0);
        assert_eq!(policy.first().map(|entry| &entry.0), Some(&Action::Roll));
        let selected = super::recursive_observation_best_policy_action(&state, &pair, 0);
        assert_eq!(selected, vec![(Action::Roll, 1.0)]);
    }

    #[test]
    fn controlled_main_phase_chooses_higher_scored_city_despite_settlement_quota_order() {
        let state = controlled_city_settlement_fixture();
        let observed = state.observed_state(0);
        let legal = state.legal_actions();
        let city = legal
            .iter()
            .filter(|action| matches!(action, Action::BuildCity { .. }))
            .max_by(|left, right| {
                crate::policy::action_prior(&observed, left, 0)
                    .total_cmp(&crate::policy::action_prior(&observed, right, 0))
            })
            .cloned()
            .expect("fixture must expose a city");
        let settlement = legal
            .iter()
            .filter(|action| matches!(action, Action::BuildSettlement { .. }))
            .min_by(|left, right| {
                crate::policy::action_prior(&observed, left, 0)
                    .total_cmp(&crate::policy::action_prior(&observed, right, 0))
            })
            .cloned()
            .expect("fixture must expose a settlement");
        let actions = vec![settlement.clone(), city.clone()];
        assert!(
            crate::policy::action_prior(&observed, &city, 0)
                > crate::policy::action_prior(&observed, &settlement, 0)
        );
        let opponent_style = super::recursive_observation_policy(&state, &actions, 0, 8);
        assert_eq!(
            opponent_style.first().map(|entry| &entry.0),
            Some(&settlement)
        );
        let controlled = super::recursive_observation_best_policy_action(&state, &actions, 0);
        assert_eq!(controlled, vec![(city, 1.0)]);
    }

    #[test]
    fn controlled_policy_ties_use_stable_canonical_action_order() {
        let mut tied = vec![
            (Action::BuildRoad { edge: 9 }, 0.5),
            (Action::BuildRoad { edge: 2 }, 0.5),
            (Action::BuildRoad { edge: 7 }, 0.4),
        ];
        super::canonicalize_equal_prior_siblings(&mut tied);
        let once = tied.clone();
        super::canonicalize_equal_prior_siblings(&mut tied);
        assert_eq!(tied, once);
        assert_eq!(tied[0].0, Action::BuildRoad { edge: 2 });
    }

    #[test]
    fn selective_future_self_ambiguity_gate_defaults_to_k1() {
        assert_eq!(
            super::controlled_widening_ambiguity_ratio(1.0, 0.84),
            None
        );
        let threshold = super::controlled_widening_ambiguity_ratio(1.0, 0.85)
            .expect("a challenger at the private threshold is eligible");
        assert!((threshold - 0.85).abs() <= f32::EPSILON);
        assert_eq!(
            super::select_controlled_widening_target(&[]),
            None,
            "without completed-wave ambiguity evidence production stays K=1"
        );
    }

    #[test]
    fn selective_future_self_widening_budget_stays_inside_one_cell() {
        assert_eq!(super::controlled_widening_budgets(0), None);
        assert_eq!(super::controlled_widening_budgets(3), None);
        assert_eq!(super::controlled_widening_budgets(4), None);
        for total in [5_u32, 17, 400] {
            let budgets = super::controlled_widening_budgets(total)
                .expect("sufficient allowance must fund both policy variants");
            assert_eq!(budgets[0] + budgets[1], total);
            assert!(budgets.iter().all(|budget| {
                *budget >= super::FUTURE_SELF_WIDENING_MIN_NODES_PER_VARIANT
            }));
        }
    }

    #[test]
    fn selective_future_self_policy_is_identical_across_hidden_swaps() {
        let (left, right) = observation_swap_control(0);
        assert_eq!(left.observation_hash(0), right.observation_hash(0));
        let left_actions = crate::policy::actor_proposal_actions(&left);
        let right_actions = crate::policy::actor_proposal_actions(&right);
        let left_ranked =
            super::recursive_observation_ranked_policy_actions(&left, &left_actions, 0);
        let right_ranked =
            super::recursive_observation_ranked_policy_actions(&right, &right_actions, 0);
        assert_eq!(left_ranked, right_ranked);
        assert!(left_ranked.len() >= 2, "fixture must expose a challenger");

        let target = super::ControlledWideningTarget {
            key: super::ControlledInformationSetKey {
                actor: 0,
                observation_hash: left.observation_hash(0),
            },
            baseline_action: left_ranked[0].0.clone(),
            challenger_action: left_ranked[1].0.clone(),
        };
        let make_searcher = |controlled_policy_override| super::Searcher {
            algorithm: super::Algorithm::MaxN,
            maximum_depth: 3,
            maximum_nodes: 64,
            node_limit: 64,
            branch_cap: 8,
            nodes: 0,
            cutoffs: 0,
            deepest_depth: 0,
            deadline: crate::deadline::CooperativeDeadline::start(0),
            deadline_reached: false,
            observation_safe_recursive: true,
            controlled_player: Some(0),
            controlled_policy_override,
            controlled_ambiguity_hits: Vec::new(),
            controlled_next_decision_reached: false,
            terminal_reached: false,
            cutoff_depth_counts: Vec::new(),
            evaluation_cache: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::HashMap::new(),
            )),
            transposition_table: None,
        };

        let baseline_left =
            make_searcher(None).controlled_policy_action(&left, &left_actions, 0);
        let baseline_right =
            make_searcher(None).controlled_policy_action(&right, &right_actions, 0);
        assert_eq!(baseline_left, baseline_right);
        assert_eq!(baseline_left, vec![(target.baseline_action.clone(), 1.0)]);

        let challenger_left = make_searcher(Some(target.clone()))
            .controlled_policy_action(&left, &left_actions, 0);
        let challenger_right = make_searcher(Some(target.clone()))
            .controlled_policy_action(&right, &right_actions, 0);
        assert_eq!(challenger_left, challenger_right);
        assert_eq!(
            challenger_left,
            vec![(target.challenger_action.clone(), 1.0)]
        );
    }

    #[test]
    fn selective_future_self_target_selection_ignores_evidence_order() {
        let evidence = |observation_hash| super::ControlledAmbiguityEvidence {
            target: super::ControlledWideningTarget {
                key: super::ControlledInformationSetKey {
                    actor: 0,
                    observation_hash,
                },
                baseline_action: Action::Roll,
                challenger_action: Action::EndTurn,
            },
            ambiguity_ratio: 0.9,
            action_consistent: true,
        };
        let forward = vec![evidence(9), evidence(2)];
        let reverse = vec![evidence(2), evidence(9)];
        assert_eq!(
            super::select_controlled_widening_target(&forward),
            super::select_controlled_widening_target(&reverse)
        );
    }

    #[test]
    fn selective_future_self_transposition_identity_separates_policy_variants() {
        let state = binary_preroll_fixture(739);
        let actions = crate::policy::actor_proposal_actions(&state);
        let ranked = super::recursive_observation_ranked_policy_actions(&state, &actions, 0);
        assert!(ranked.len() >= 2, "fixture must expose a challenger");
        let target = super::ControlledWideningTarget {
            key: super::ControlledInformationSetKey {
                actor: 0,
                observation_hash: state.observation_hash(0),
            },
            baseline_action: ranked[0].0.clone(),
            challenger_action: ranked[1].0.clone(),
        };
        let base = super::transposition_identity(
            &state,
            super::Algorithm::MaxN,
            1,
            0,
            0.0,
            1.0,
            32,
            3,
            8,
            true,
            Some(0),
            None,
        );
        let widened = super::transposition_identity(
            &state,
            super::Algorithm::MaxN,
            1,
            0,
            0.0,
            1.0,
            32,
            3,
            8,
            true,
            Some(0),
            Some(target),
        );
        assert_ne!(base, widened);
    }

    #[test]
    #[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
    fn selective_future_self_cuda_deferred_profile_matches_cpu_across_hidden_swaps() {
        let (left, right) = observation_swap_control(0);
        assert_eq!(left.observation_hash(0), right.observation_hash(0));
        let left_actions = crate::policy::actor_proposal_actions(&left);
        let right_actions = crate::policy::actor_proposal_actions(&right);
        let left_ranked =
            super::recursive_observation_ranked_policy_actions(&left, &left_actions, 0);
        let right_ranked =
            super::recursive_observation_ranked_policy_actions(&right, &right_actions, 0);
        assert_eq!(left_ranked, right_ranked);
        assert!(left_ranked.len() >= 2, "fixture must expose a challenger");

        let target = super::ControlledWideningTarget {
            key: super::ControlledInformationSetKey {
                actor: 0,
                observation_hash: left.observation_hash(0),
            },
            baseline_action: left_ranked[0].0.clone(),
            challenger_action: left_ranked[1].0.clone(),
        };
        assert_eq!(
            target.key.observation_hash,
            right.observation_hash(0),
            "hidden swap must preserve the controlled information-set identity"
        );
        assert_eq!(target.baseline_action, right_ranked[0].0);
        assert_eq!(target.challenger_action, right_ranked[1].0);

        let make_cpu = |controlled_policy_override| super::Searcher {
            algorithm: super::Algorithm::MaxN,
            maximum_depth: 3,
            maximum_nodes: 64,
            node_limit: 64,
            branch_cap: 8,
            nodes: 0,
            cutoffs: 0,
            deepest_depth: 0,
            deadline: crate::deadline::CooperativeDeadline::start(0),
            deadline_reached: false,
            observation_safe_recursive: true,
            controlled_player: Some(0),
            controlled_policy_override,
            controlled_ambiguity_hits: Vec::new(),
            controlled_next_decision_reached: false,
            terminal_reached: false,
            cutoff_depth_counts: Vec::new(),
            evaluation_cache: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::HashMap::new(),
            )),
            transposition_table: None,
        };

        let cpu_baseline_left =
            make_cpu(None).controlled_policy_action(&left, &left_actions, 0);
        let cpu_baseline_right =
            make_cpu(None).controlled_policy_action(&right, &right_actions, 0);
        let cpu_challenger_left = make_cpu(Some(target.clone()))
            .controlled_policy_action(&left, &left_actions, 0);
        let cpu_challenger_right = make_cpu(Some(target.clone()))
            .controlled_policy_action(&right, &right_actions, 0);

        let mut baseline_tree = super::CudaDeferredTree::new();
        let cuda_baseline = super::CudaDeferredSearcher {
            tree: &mut baseline_tree,
            algorithm: super::Algorithm::MaxN,
            maximum_depth: 3,
            maximum_nodes: 64,
            node_limit: 64,
            branch_cap: 8,
            controlled_player: 0,
            controlled_policy_override: None,
            controlled_ambiguity_hits: Vec::new(),
            nodes: 0,
            deepest_depth: 0,
            controlled_next_decision_reached: false,
            terminal_reached: false,
            cutoff_depth_counts: Vec::new(),
            transposition_table: None,
            local_transpositions: std::collections::HashMap::new(),
            pending_transpositions: Vec::new(),
        };
        assert_eq!(
            cuda_baseline.controlled_policy_action(&left, &left_actions, 0),
            cpu_baseline_left
        );
        assert_eq!(
            cuda_baseline.controlled_policy_action(&right, &right_actions, 0),
            cpu_baseline_right
        );

        let mut challenger_tree = super::CudaDeferredTree::new();
        let cuda_challenger = super::CudaDeferredSearcher {
            tree: &mut challenger_tree,
            algorithm: super::Algorithm::MaxN,
            maximum_depth: 3,
            maximum_nodes: 64,
            node_limit: 64,
            branch_cap: 8,
            controlled_player: 0,
            controlled_policy_override: Some(target.clone()),
            controlled_ambiguity_hits: Vec::new(),
            nodes: 0,
            deepest_depth: 0,
            controlled_next_decision_reached: false,
            terminal_reached: false,
            cutoff_depth_counts: Vec::new(),
            transposition_table: None,
            local_transpositions: std::collections::HashMap::new(),
            pending_transpositions: Vec::new(),
        };
        assert_eq!(
            cuda_challenger.controlled_policy_action(&left, &left_actions, 0),
            cpu_challenger_left
        );
        assert_eq!(
            cuda_challenger.controlled_policy_action(&right, &right_actions, 0),
            cpu_challenger_right
        );
        assert_eq!(
            cpu_baseline_left,
            vec![(target.baseline_action.clone(), 1.0)]
        );
        assert_eq!(
            cpu_challenger_left,
            vec![(target.challenger_action.clone(), 1.0)]
        );
    }

    #[test]
    fn opponent_observation_policy_keeps_weighted_quota_mixture() {
        let state = controlled_city_settlement_fixture();
        let actions = state.legal_actions();
        let ranked = super::recursive_observation_policy(&state, &actions, 0, 8);
        assert!(ranked.len() > 1);
        assert!((ranked.iter().map(|(_, weight)| *weight).sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(ranked.iter().all(|(_, weight)| *weight > 0.0));
    }

    #[test]
    fn saturated_seed_22_agreement_avoids_unnecessary_escalation() {
        let state = binary_preroll_fixture(22);
        let shallow = run_binary_preroll(&state, 1, 8_000, 2_000, 0);
        assert_eq!(binary_preroll_floor_winner(&state, &shallow), Action::Roll);
        assert_eq!(shallow.chosen, Some(Action::Roll));

        let ordinary = run_binary_preroll(&state, 2, 8_000, 10_000, 0);
        let with_reserve = run_binary_preroll(&state, 2, 8_000, 10_000, 2_500);
        assert_eq!(with_reserve.chosen, ordinary.chosen);
        assert_eq!(with_reserve.nodes, ordinary.nodes);
        assert_eq!(with_reserve.depth, ordinary.depth);
        assert!(with_reserve.depth >= 2);
        let timings = with_reserve.stage_timings.as_ref().unwrap();
        assert!(!timings.evidence_escalation_triggered);
        assert!(!timings.evidence_escalation_completed);
        assert_eq!(timings.evidence_escalation_nodes, 0);
    }

    #[test]
    fn saturated_seed_25_escalation_resumes_iterative_depth() {
        assert_saturated_escalation_resumes_iterative_depth(25);
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
            !report
                .actions
                .iter()
                .any(|candidate| candidate.action == target),
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
            controlled_player: None,
            controlled_policy_override: None,
            controlled_ambiguity_hits: Vec::new(),
            controlled_next_decision_reached: false,
            terminal_reached: false,
            cutoff_depth_counts: Vec::new(),
            evaluation_cache: std::rc::Rc::new(std::cell::RefCell::new(
                std::collections::HashMap::new(),
            )),
            transposition_table: None,
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
                strategy_policy: crate::StrategyPolicy::Baseline,
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
                strategy_policy: crate::StrategyPolicy::Baseline,
                strategic_particle_limit: 4,
            },
        )
        .unwrap();
        assert_eq!(report.posterior_particles, 16);
        assert_eq!(report.particles, 2);
    }

    #[test]
    fn adaptive_candidate_admission_preserves_baseline_roster_contract_and_protected_roots() {
        let state = recovered_turn_54_control();
        let particles = [BeliefParticle { state, weight: 1.0 }];
        let config = |strategy_policy| super::BeliefDepthConfig {
            maximum_depth: 2,
            branch_cap: 4,
            maximum_nodes: 1_500,
            time_budget_ms: 0,
            strategy_policy,
            strategic_particle_limit: usize::MAX,
        };
        let baseline = super::search_weighted_belief_maxn_with_config(
            &particles,
            config(crate::StrategyPolicy::Baseline),
        )
        .unwrap();
        let legacy = search_weighted_belief_maxn_bounded(&particles, 2, 4, 1_500).unwrap();
        let experimental = super::search_weighted_belief_maxn_with_config(
            &particles,
            config(crate::StrategyPolicy::AdaptiveCandidateAdmissionV1),
        )
        .unwrap();

        assert_eq!(baseline.chosen, legacy.chosen);
        assert_eq!(
            baseline
                .provenance
                .retained_roots
                .iter()
                .map(|root| &root.action)
                .collect::<Vec<_>>(),
            legacy
                .provenance
                .retained_roots
                .iter()
                .map(|root| &root.action)
                .collect::<Vec<_>>(),
        );
        assert!(baseline.provenance.retained_roots.len() <= 4);
        assert_eq!(
            experimental.provenance.retained_roots.len(),
            baseline.provenance.retained_roots.len(),
        );
        assert!(experimental.provenance.retained_roots.len() <= 4);

        let baseline_actions = baseline
            .provenance
            .retained_roots
            .iter()
            .map(|root| root.action.clone())
            .collect::<Vec<_>>();
        let experimental_actions = experimental
            .provenance
            .retained_roots
            .iter()
            .map(|root| root.action.clone())
            .collect::<Vec<_>>();
        if baseline_actions.contains(&Action::EndTurn) {
            assert!(experimental_actions.contains(&Action::EndTurn));
        }
        if let Some(leader) = baseline.provenance.ranked_roots.first()
            && baseline_actions.contains(&leader.action)
        {
            assert!(experimental_actions.contains(&leader.action));
        }
        for evidence in &baseline.provenance.root_evidence {
            if evidence.promotion_reason.is_some() && baseline_actions.contains(&evidence.action) {
                assert!(experimental_actions.contains(&evidence.action));
            }
        }

        let diagnostics = experimental
            .provenance
            .strategy_shadow
            .as_ref()
            .expect("explicit strategy policy must expose admission provenance");
        assert_eq!(
            diagnostics.strategy_policy,
            Some(crate::ADAPTIVE_CANDIDATE_ADMISSION_V1),
        );
        assert!(diagnostics.admission.challengers_admitted <= 3);
        assert!(diagnostics.admission.evaluated_challenger_count <= 3);
        assert!(
            diagnostics
                .proposals
                .iter()
                .all(|proposal| !proposal.admitted || proposal.entered_common_search)
        );
    }

    #[test]
    fn adaptive_candidate_admission_keeps_verified_forced_blocker_protected() {
        let (state, blocker) = crate::threats::forced_blocker_fixture();
        let particles = [BeliefParticle { state, weight: 1.0 }];
        let report = super::search_weighted_belief_maxn_with_config(
            &particles,
            super::BeliefDepthConfig {
                maximum_depth: 3,
                branch_cap: 4,
                maximum_nodes: 2_000,
                time_budget_ms: 0,
                strategy_policy: crate::StrategyPolicy::AdaptiveCandidateAdmissionV1,
                strategic_particle_limit: usize::MAX,
            },
        )
        .unwrap();
        assert!(
            report
                .provenance
                .retained_roots
                .iter()
                .any(|root| root.action == blocker)
        );
        assert_eq!(report.chosen, Some(blocker));
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
