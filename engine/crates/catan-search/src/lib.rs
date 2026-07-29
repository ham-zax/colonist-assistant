//! Clean-room tactical and multiplayer search for Colonist Assistant.
//!
//! The search uses one value component per player. Decision nodes optimize the
//! acting player's component, while dice, development draws, and steals are
//! sampled from explicit chance nodes in `colonist-catan-core`.

mod depth;
mod eval;
mod exact;
mod features;
mod mcts;
mod model;
mod opening;
mod planner;
mod policy;
mod tactical;
mod trade_model;

pub use eval::{
    ExpansionOption, TrophyOutlook, evaluate, expansion_option_value, expected_discard_loss,
    largest_army_outlook, longest_road_outlook, marginal_development_value, production_pips,
    strategic_utility,
};
pub use exact::{
    ExactActionFamily, ExactActionValue, ExactDecisionResult, exact_family_for_action,
    solve_exact_belief,
};
pub use features::{
    ACTION_FEATURES, EDGE_FEATURES, GLOBAL_FEATURES, HEX_FEATURES, HeterogeneousGraphFeatures,
    PLAYER_FEATURES, STATE_FEATURES, VERTEX_FEATURES, encode_action, encode_heterogeneous_graph,
    pool_heterogeneous_graph,
};
pub use mcts::{
    ActionStats, BeliefError, BeliefParticle, Mcts, SearchConfig, SearchMode, SearchReport,
    SearchStatistics,
};
pub use model::{
    learned_action_logit, learned_action_logits, learned_model_ready, learned_model_version,
    learned_value,
};
pub use opening::{OpeningActionValue, OpeningConfig, OpeningReport, solve_opening};
pub use planner::{TurnPlan, TurnPlanConfig, plan_current_turn};
pub use policy::{ActionClass, action_prior, choose_rollout_action, trade_acceptance_probability};
pub use tactical::{TacticalResult, solve_belief_current_turn, solve_current_turn};
pub use trade_model::{
    TRADE_ACCEPTANCE_FEATURES, learned_trade_acceptance_probability, learned_trade_model_version,
    trade_acceptance_features,
};

pub const ENGINE_REVISION: &str = "belief-puct-v3";
pub use depth::{
    BeliefDepthResult, DepthActionValue, DepthBeliefError, DepthSearchResult, search_belief_maxn,
    search_belief_maxn_bounded, search_belief_paranoid, search_belief_paranoid_bounded,
    search_maxn, search_maxn_bounded, search_paranoid, search_paranoid_bounded,
    search_weighted_belief_maxn_bounded, search_weighted_belief_paranoid_bounded,
};
