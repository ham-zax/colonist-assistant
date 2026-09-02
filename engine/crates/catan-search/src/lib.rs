//! Clean-room tactical and multiplayer search for Colonist Assistant.
//!
//! The search uses one value component per player. Decision nodes optimize the
//! acting player's component, while dice, development draws, and steals are
//! sampled from explicit chance nodes in `colonist-catan-core`.

mod deadline;
mod depth;
mod eval;
mod exact;
mod features;
mod mcts;
mod model;
mod opening;
mod planner;
mod policy;
mod resilience;
mod root_impact;
mod shared;
mod tactical;
mod threats;
mod trade_model;
mod trade_safety;

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
mod cuda_exact;
#[cfg(all(feature = "cuda-sim", not(target_arch = "wasm32")))]
mod cuda_sim;

#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub use cuda_exact::*;
#[cfg(all(feature = "cuda-sim", not(target_arch = "wasm32")))]
pub use cuda_sim::*;

pub use deadline::CooperativeDeadline;
pub use eval::{
    ExpansionOption, TrophyOutlook, evaluate, expansion_option_value, expected_discard_loss,
    largest_army_outlook, longest_road_outlook, marginal_development_value, production_pips,
    strategic_utility,
};
#[cfg(feature = "benchmark-profile")]
pub use eval::{EvaluateProfile, evaluate_profiled};
pub use exact::{
    DEVELOPMENT_EXACT_FAMILIES, ExactActionFamily, ExactActionValue, ExactDecisionResult,
    exact_action_comparator_score, exact_family_for_action, solve_exact_belief,
    solve_exact_belief_excluding,
    solve_exact_belief_excluding_controlled,
};
pub use features::{
    ACTION_FEATURES, EDGE_FEATURES, GLOBAL_FEATURES, HEX_FEATURES, HeterogeneousGraphFeatures,
    PLAYER_FEATURES, STATE_FEATURES, VERTEX_FEATURES, encode_action, encode_heterogeneous_graph,
    pool_heterogeneous_graph,
};
pub use mcts::{
    ActionStats, BeliefError, BeliefParticle, Mcts, SearchConfig, SearchMode, SearchReport,
    SearchStatistics, safer_end_turn_alternative,
};
pub use model::{
    learned_action_logit, learned_action_logits, learned_model_ready, learned_model_version,
    learned_policy_promoted, learned_value,
};
pub use opening::{OpeningActionValue, OpeningConfig, OpeningReport, solve_opening};
pub use planner::{TurnPlan, TurnPlanConfig, plan_current_turn};
pub use policy::{
    ActionClass, action_prior, allocate_root_node_budgets, choose_rollout_action,
    trade_acceptance_probability,
};
pub use resilience::{
    CriticalEdge, CriticalVertex, RoadResilience, analyze_road_resilience,
    evaluate_edge_cut, evaluate_vertex_cut,
};
pub use root_impact::{
    RoadImpactDelta, RootImpactReport, RootPromotionReason, RootStrategicImpact,
    compute_spatial_root_impacts,
};
pub use shared::{
    EXPERIMENTAL_STRATEGIC_PARTICLE_TARGET, STRATEGIC_ROOT_WIDTH, admit_promoted_roots,
    coalesce_identical_particles, group_particles_by_observation,
    select_experimental_strategic_particles, shared_root_candidates,
};
pub use tactical::{
    TacticalResult, solve_belief_current_turn, solve_belief_current_turn_timed, solve_current_turn,
};
pub use threats::{
    OpponentThreat, OpponentThreatKind, action_blocks_threat, detect_opponent_threats,
    force_threat_blocking_actions, forced_loss_weight, posterior_expected_tactical_threat_weight,
    posterior_immediate_threat_weight,
};
pub use trade_model::{
    TRADE_ACCEPTANCE_FEATURES, learned_trade_acceptance_probability, learned_trade_model_version,
    trade_acceptance_features,
};
pub use trade_safety::{
    DomesticTradeThreat, HARD_VETO_POSTERIOR, belief_domestic_trade_threat, domestic_trade_threat,
};

pub const ENGINE_REVISION: &str = "deep-maxn-v10";
pub use depth::{
    BeliefDepthConfig, BeliefDepthResult, BeliefSearchProvenance, BeliefSearchStageTimings,
    DepthActionValue,
    DepthBeliefError, DepthSearchResult, PrunedRootDiagnostic, RankedRootDiagnostic,
    RetainedRootDiagnostic, RootPruneReason, search_belief_maxn, search_belief_maxn_bounded,
    search_belief_paranoid, search_belief_paranoid_bounded, search_maxn, search_maxn_bounded,
    search_maxn_bounded_timed, search_paranoid, search_paranoid_bounded,
    search_paranoid_bounded_timed, search_weighted_belief_maxn_bounded,
    search_weighted_belief_maxn_bounded_timed, search_weighted_belief_maxn_bounded_timed_excluding,
    search_weighted_belief_maxn_iterative_timed_excluding, search_weighted_belief_maxn_with_config,
    search_weighted_belief_paranoid_bounded, search_weighted_belief_paranoid_bounded_timed,
    search_weighted_belief_paranoid_bounded_timed_excluding,
    search_weighted_belief_paranoid_iterative_timed_excluding,
    search_weighted_belief_paranoid_with_config,
};
#[cfg(all(feature = "cuda-exact", not(target_arch = "wasm32")))]
pub use depth::{
    CudaExactSearchStats, cuda_exact_search_stats,
    search_weighted_belief_maxn_cuda_with_config,
    search_weighted_belief_maxn_cuda_with_config_excluding,
    search_weighted_belief_maxn_cuda_with_config_mutex,
    search_weighted_belief_maxn_cuda_with_config_mutex_excluding,
};
