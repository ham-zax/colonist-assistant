//! Narrow structured-clone WebAssembly boundary.
//!
//! JavaScript validates the Colonist DOM and sends one compact snapshot.
//! Search performs no callback into JavaScript and returns one small report.

use std::cell::RefCell;

use colonist_catan_core::{
    Action, Board, Building, DiceMode, Edge, GameState, Phase, PlayerState, Port, Resource,
    TradeOffer, Vertex,
};
use colonist_catan_search::{
    ActionStats, BeliefParticle, BeliefSearchProvenance, BeliefSearchStageTimings,
    CooperativeDeadline, DomesticTradeThreat, ENGINE_REVISION, ExactActionFamily, ExactActionValue,
    ExactDecisionResult, HARD_VETO_POSTERIOR, IntroducedRoadFragility, Mcts,
    RoadCutContinuationAssessment, RootPromotionReason, RootPruneReason, SearchConfig, SearchMode,
    SearchReport, SearchStatistics, TacticalResult, action_prior, evaluate,
    exact_action_comparator_score, exact_family_for_action, learned_model_version,
    learned_trade_model_version, safer_end_turn_alternative,
    search_weighted_belief_maxn_iterative_timed_excluding,
    search_weighted_belief_paranoid_iterative_timed_excluding, solve_belief_current_turn,
    solve_belief_current_turn_timed, solve_exact_belief_excluding,
    solve_exact_belief_excluding_controlled,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

#[cfg(all(feature = "native-gpu", not(target_arch = "wasm32")))]
mod native_gpu;
#[cfg(all(feature = "native-gpu", not(target_arch = "wasm32")))]
pub use native_gpu::{
    NATIVE_GPU_PROTOCOL_VERSION, NATIVE_GPU_STATE_SCHEMA_VERSION, NativeGpuDeviceIdentity,
    NativeGpuSearchEngine,
};

thread_local! {
    static PERSISTENT_SEARCH: RefCell<Vec<Mcts>> = const { RefCell::new(Vec::new()) };
}

type DecisionClock = CooperativeDeadline;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HexInput {
    resource: i8,
    number: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexInput {
    adjacent_hexes: Vec<u8>,
    adjacent_vertices: Vec<u8>,
    adjacent_edges: Vec<u8>,
    port: i8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeInput {
    vertices: [u8; 2],
    adjacent_hexes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoardInput {
    hexes: Vec<HexInput>,
    vertices: Vec<VertexInput>,
    edges: Vec<EdgeInput>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerInput {
    resources: [u8; 5],
    development: [u8; 5],
    bought_development: [u8; 5],
    public_victory_points: u8,
    played_knights: u8,
    roads_left: u8,
    settlements_left: u8,
    cities_left: u8,
    has_longest_road: bool,
    has_largest_army: bool,
    played_development_this_turn: bool,
    policy_profile: Option<[u8; 5]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorldInput {
    weight: Option<f32>,
    hands: Vec<[u8; 5]>,
    development: Option<Vec<[u8; 5]>>,
    bought_development: Option<Vec<[u8; 5]>>,
    development_deck: Option<[u8; 5]>,
    bank: Option<[u8; 5]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeInput {
    creator: u8,
    recipients: u8,
    give: [u8; 5],
    receive: [u8; 5],
    accepted: u8,
    rejected: u8,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum DiceModeInput {
    #[default]
    Unknown,
    Random,
    Balanced,
    Unsupported,
}

impl From<DiceModeInput> for DiceMode {
    fn from(value: DiceModeInput) -> Self {
        match value {
            DiceModeInput::Unknown => Self::Unknown,
            DiceModeInput::Random => Self::Random,
            DiceModeInput::Balanced => Self::Balanced,
            DiceModeInput::Unsupported => Self::Unsupported,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateInput {
    board: BoardInput,
    players: Vec<PlayerInput>,
    worlds: Vec<WorldInput>,
    buildings: Vec<i8>,
    roads: Vec<i8>,
    bank: [u8; 5],
    bank_visible: Option<bool>,
    development_deck: [u8; 5],
    played_development: [u8; 5],
    robber_hex: u8,
    current_player: u8,
    phase: String,
    phase_parameter: Option<u8>,
    turn: u16,
    last_roll: u8,
    #[serde(default)]
    dice_mode: DiceModeInput,
    victory_target: u8,
    card_discard_limit: Option<u8>,
    friendly_robber: Option<bool>,
    setup_step: u8,
    discard_remaining: [u8; 4],
    discard_cursor: u8,
    robber_return_phase: String,
    trade: Option<TradeInput>,
    trade_cursor: u8,
    domestic_trade_used: bool,
    player_trades_enabled: Option<bool>,
    domestic_trade_disabled: Option<u8>,
    domestic_trade_embargoes: Option<u16>,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectedTradeInput {
    give: [u8; 5],
    receive: [u8; 5],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootExclusionInput {
    kind: String,
    give: [u8; 5],
    receive: [u8; 5],
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct TacticalEffortInput {
    max_depth: u8,
    node_budget: u32,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct CpuEffortInput {
    max_depth: u8,
    root_cap: usize,
    nodes_per_depth_wave: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    evidence_escalation_ms: Option<u32>,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct GpuEffortInput {
    root_cap: usize,
    rollout_budget: u32,
    rollout_steps: u16,
}

#[derive(Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchEffortInput {
    decision_time_ms: u32,
    tactical: TacticalEffortInput,
    cpu: CpuEffortInput,
    gpu: GpuEffortInput,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    state: StateInput,
    effort: Option<SearchEffortInput>,
    last_rejected_trade: Option<RejectedTradeInput>,
    #[serde(default)]
    root_exclusions: Vec<RootExclusionInput>,
    iterations: Option<u32>,
    max_nodes: Option<usize>,
    rollout_actions: Option<u16>,
    tactical_depth: Option<u8>,
    tactical_nodes: Option<u32>,
    time_budget_ms: Option<u32>,
    seed: Option<u64>,
    mode: Option<String>,
    depth: Option<u8>,
    branch_cap: Option<usize>,
    ponder: Option<bool>,
}

impl Request {
    fn resolved_effort(&self) -> SearchEffortInput {
        let effort = self.effort.unwrap_or(SearchEffortInput {
            decision_time_ms: self.time_budget_ms.unwrap_or(2_800),
            tactical: TacticalEffortInput {
                max_depth: self.tactical_depth.unwrap_or(18),
                node_budget: self.tactical_nodes.unwrap_or(12_000),
            },
            cpu: CpuEffortInput {
                max_depth: self.depth.unwrap_or(3),
                root_cap: self.branch_cap.unwrap_or(12),
                nodes_per_depth_wave: self.max_nodes.unwrap_or(48_000) as u32,
                evidence_escalation_ms: None,
            },
            gpu: GpuEffortInput {
                root_cap: self.branch_cap.unwrap_or(12),
                rollout_budget: self.iterations.unwrap_or(384),
                rollout_steps: self.rollout_actions.unwrap_or(96),
            },
        });
        SearchEffortInput {
            decision_time_ms: effort.decision_time_ms.clamp(50, 10_000),
            tactical: TacticalEffortInput {
                max_depth: effort.tactical.max_depth.clamp(4, 32),
                node_budget: effort.tactical.node_budget.clamp(100, 100_000),
            },
            cpu: CpuEffortInput {
                max_depth: effort.cpu.max_depth.clamp(1, 6),
                root_cap: effort.cpu.root_cap.clamp(2, 32),
                nodes_per_depth_wave: effort.cpu.nodes_per_depth_wave.clamp(1_000, 250_000),
                evidence_escalation_ms: effort
                    .cpu
                    .evidence_escalation_ms
                    .and_then(|milliseconds| {
                        (milliseconds > 0).then(|| milliseconds.clamp(1, 3_000))
                    }),
            },
            gpu: GpuEffortInput {
                root_cap: effort.gpu.root_cap.clamp(2, 24),
                rollout_budget: effort.gpu.rollout_budget.clamp(16, 50_000),
                rollout_steps: effort.gpu.rollout_steps.clamp(24, 160),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestedMode {
    Maxn,
    Weighted,
    Puct,
    Uct,
    AlphaBeta,
}

impl RequestedMode {
    fn parse(value: Option<&str>) -> Result<Self, JsValue> {
        match value.unwrap_or("maxn") {
            "maxn" | "deep" => Ok(Self::Maxn),
            "weighted" => Ok(Self::Weighted),
            // Retain old request names only as an explicit experimental PUCT
            // compatibility path. The packaged live worker never selects it.
            "puct" | "strategist" => Ok(Self::Puct),
            "uct" => Ok(Self::Uct),
            "alpha-beta" => Ok(Self::AlphaBeta),
            other => Err(JsValue::from_str(&format!("unknown search mode: {other}"))),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Maxn => "maxn",
            Self::Weighted => "weighted",
            Self::Puct => "puct",
            Self::Uct => "uct",
            Self::AlphaBeta => "alpha-beta",
        }
    }

    fn mcts_mode(self) -> SearchMode {
        if self == Self::Uct {
            SearchMode::Uct
        } else {
            SearchMode::Puct
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionOutput {
    kind: &'static str,
    first: Option<u8>,
    second: Option<u8>,
    player: Option<u8>,
    resource: Option<u8>,
    other_resource: Option<u8>,
    cards: Option<[u8; 5]>,
    receive_cards: Option<[u8; 5]>,
    accept: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionStatisticsOutput {
    action: ActionOutput,
    visits: u32,
    availability: u32,
    availability_weight: f32,
    legal_weight: f32,
    prior: f32,
    value: [f32; 4],
    lower_confidence_value: [f32; 4],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ExactActionDiagnosticOutput {
    action: ActionOutput,
    value: [f32; 4],
    lower_bound: [f32; 4],
    legal_weight: f32,
    decision_score: f32,
    lower_score: f32,
    comparator_score: f32,
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum DecisionAuthority {
    ExactMandatory,
    TacticalProven,
    DeepMaxn,
    #[cfg(all(feature = "native-gpu", not(target_arch = "wasm32")))]
    GpuRootRollout,
    WeightedPolicy,
    ExactFamily,
    SafetyOverride,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionReplacementOutput {
    from: ActionOutput,
    to: ActionOutput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RankedRootOutput {
    action: ActionOutput,
    rank: usize,
    prior: f32,
    planner_value: Option<f32>,
    planner_completion_mass: Option<f32>,
    planner_decisive_completion_mass: Option<f32>,
    planner_response_windows: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetainedRootOutput {
    action: ActionOutput,
    pre_truncation_rank: Option<usize>,
    prior: f32,
    node_budget_per_particle: u32,
    allocated_nodes: u32,
    planner_value: Option<f32>,
    planner_completion_mass: Option<f32>,
    planner_decisive_completion_mass: Option<f32>,
    planner_response_windows: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_rank: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    final_evaluation_horizon: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_terminal_outcome: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_terminal_rate: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_victory_margin: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    initial_strategic_margin: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_outcome: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_rate: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_lower_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    terminal_upper_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    victory_margin: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    victory_margin_lower_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    victory_margin_upper_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strategic_margin: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strategic_margin_lower_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    strategic_margin_upper_bound: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mean_turn: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrunedRootOutput {
    action: ActionOutput,
    pre_truncation_rank: Option<usize>,
    reason: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IntroducedCriticalVertexOutput {
    vertex: u8,
    road_loss: u8,
    additional_road_loss: u8,
    award_loss: bool,
    award_loss_introduced: bool,
    award_vp_exposure: u8,
    expansion_loss: f32,
    additional_expansion_loss: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct IntroducedRoadFragilityOutput {
    critical_vertices: Vec<IntroducedCriticalVertexOutput>,
    maximum_additional_road_loss: u8,
    award_vp_exposure: u8,
    maximum_additional_expansion_loss: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoadCutContinuationEvidenceOutput {
    vertex: u8,
    opponent: u8,
    posterior: f32,
    maritime_trade_required_posterior: f32,
    award_loss_posterior: f32,
    maximum_road_loss: u8,
    approach_edges: Vec<u8>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoadCutContinuationAssessmentOutput {
    posterior: f32,
    award_loss_posterior: f32,
    continuations: Vec<RoadCutContinuationEvidenceOutput>,
}

fn introduced_road_fragility_output(
    value: &IntroducedRoadFragility,
) -> IntroducedRoadFragilityOutput {
    IntroducedRoadFragilityOutput {
        critical_vertices: value
            .critical_vertices
            .iter()
            .map(|cut| IntroducedCriticalVertexOutput {
                vertex: cut.vertex,
                road_loss: cut.road_loss,
                additional_road_loss: cut.additional_road_loss,
                award_loss: cut.award_loss,
                award_loss_introduced: cut.award_loss_introduced,
                award_vp_exposure: cut.award_vp_exposure,
                expansion_loss: cut.expansion_loss,
                additional_expansion_loss: cut.additional_expansion_loss,
            })
            .collect(),
        maximum_additional_road_loss: value.maximum_additional_road_loss,
        award_vp_exposure: value.award_vp_exposure,
        maximum_additional_expansion_loss: value.maximum_additional_expansion_loss,
    }
}

fn road_cut_continuation_output(
    value: &RoadCutContinuationAssessment,
) -> RoadCutContinuationAssessmentOutput {
    RoadCutContinuationAssessmentOutput {
        posterior: value.posterior,
        award_loss_posterior: value.award_loss_posterior,
        continuations: value
            .continuations
            .iter()
            .map(|continuation| RoadCutContinuationEvidenceOutput {
                vertex: continuation.vertex,
                opponent: continuation.opponent,
                posterior: continuation.posterior,
                maritime_trade_required_posterior: continuation.maritime_trade_required_posterior,
                award_loss_posterior: continuation.award_loss_posterior,
                maximum_road_loss: continuation.maximum_road_loss,
                approach_edges: continuation.approach_edges.clone(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RoadIntentOutput {
    #[serde(skip_serializing_if = "Option::is_none")]
    target_vertex: Option<u8>,
    roads_remaining: u8,
    #[serde(skip_serializing_if = "Option::is_none")]
    expected_rolls: Option<f32>,
    survival_probability: f32,
    target_value: f32,
    portfolio_value: f32,
    frontier_gain: f32,
    ordering_score: f32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RootCausalEvidenceOutput {
    action: ActionOutput,
    promotion_reason: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    road_intent: Option<RoadIntentOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    introduced_road_fragility: Option<IntroducedRoadFragilityOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    road_cut_continuation: Option<RoadCutContinuationAssessmentOutput>,
    admitted_by_promotion: bool,
    closeout_gain: f32,
    response_windows: Option<f32>,
    decisive_completion_mass: f32,
    trade_threat: Option<&'static str>,
    trade_risk_posterior: f32,
    dirty_monopoly_posterior: f32,
    trade_hard_veto_posterior: f32,
    trade_hard_veto: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct HorizonEscalationOutput {
    reason: &'static str,
    provisional_winner: ActionOutput,
    initial_horizon: u16,
    unresolved_cut_mass: f32,
    roots: Vec<ActionOutput>,
    attempted_horizons: Vec<u16>,
    completed_horizon: Option<u16>,
    final_winner: Option<ActionOutput>,
    deadline_limited: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RootProvenanceOutput {
    ranked_root_count: usize,
    ranked_roots: Vec<RankedRootOutput>,
    retained_roots: Vec<RetainedRootOutput>,
    pruned_root_count: usize,
    pruned_roots: Vec<PrunedRootOutput>,
    root_evidence: Vec<RootCausalEvidenceOutput>,
    #[serde(skip_serializing_if = "Option::is_none")]
    horizon_escalation: Option<HorizonEscalationOutput>,
    trade_hard_veto_threshold: f32,
    search_winner: Option<ActionOutput>,
    exact_family_replacement: Option<ActionReplacementOutput>,
    safety_replacement: Option<ActionReplacementOutput>,
}

impl Default for RootProvenanceOutput {
    fn default() -> Self {
        Self {
            ranked_root_count: 0,
            ranked_roots: Vec::new(),
            retained_roots: Vec::new(),
            pruned_root_count: 0,
            pruned_roots: Vec::new(),
            root_evidence: Vec::new(),
            horizon_escalation: None,
            trade_hard_veto_threshold: HARD_VETO_POSTERIOR,
            search_winner: None,
            exact_family_replacement: None,
            safety_replacement: None,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchStagesOutput {
    particle_preparation_ms: u32,
    root_scoring_ms: u32,
    exact_families_ms: u32,
    threat_safety_ms: u32,
    one_ply_floor_ms: u32,
    deep_waves_ms: u32,
    floor_complete: bool,
    attempted_depth: u8,
    evidence_escalation_triggered: bool,
    evidence_escalation_completed: bool,
    evidence_escalation_nodes: u32,
    evidence_escalation_ms: u32,
}

impl From<BeliefSearchStageTimings> for SearchStagesOutput {
    fn from(value: BeliefSearchStageTimings) -> Self {
        Self {
            particle_preparation_ms: value.particle_preparation_ms,
            root_scoring_ms: value.root_scoring_ms,
            exact_families_ms: value.exact_families_ms,
            threat_safety_ms: value.threat_safety_ms,
            one_ply_floor_ms: value.one_ply_floor_ms,
            deep_waves_ms: value.deep_waves_ms,
            floor_complete: value.floor_complete,
            attempted_depth: value.attempted_depth,
            evidence_escalation_triggered: value.evidence_escalation_triggered,
            evidence_escalation_completed: value.evidence_escalation_completed,
            evidence_escalation_nodes: value.evidence_escalation_nodes,
            evidence_escalation_ms: value.evidence_escalation_ms,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityTraceOutput {
    initial_authority: DecisionAuthority,
    exact_family: Option<&'static str>,
    exact_family_replacement: Option<ActionReplacementOutput>,
    safety_replacement: Option<ActionReplacementOutput>,
}

struct ResponseDiagnostics {
    rust_posterior_particles: usize,
    rust_search_particles: usize,
    effective_effort: SearchEffortInput,
    search_stages: Option<SearchStagesOutput>,
    root_provenance: RootProvenanceOutput,
    authority_trace: AuthorityTraceOutput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    engine_revision: &'static str,
    authority: DecisionAuthority,
    learned_model_version: &'static str,
    trade_model_version: &'static str,
    algorithm: &'static str,
    chosen: Option<ActionOutput>,
    root_value: [f32; 4],
    tactical_win_probability: f32,
    tactical_lower_bound: f32,
    tactical_proven: bool,
    tactical_line: Vec<ActionOutput>,
    exact_decision: bool,
    exact_worlds: usize,
    exact_actions: Vec<ExactActionDiagnosticOutput>,
    actions: Vec<ActionStatisticsOutput>,
    iterations: u32,
    nodes: usize,
    deepest_decision_depth: u16,
    rollouts: u32,
    particles: usize,
    wasm_particles: usize,
    rust_posterior_particles: usize,
    rust_search_particles: usize,
    effective_effort: SearchEffortInput,
    search_stages: Option<SearchStagesOutput>,
    root_provenance: RootProvenanceOutput,
    authority_trace: AuthorityTraceOutput,
    effective_particle_count: f32,
    deadline_reached: bool,
}

fn resource(value: i8) -> Result<Option<Resource>, String> {
    Ok(match value {
        -1 => None,
        0 => Some(Resource::Lumber),
        1 => Some(Resource::Brick),
        2 => Some(Resource::Wool),
        3 => Some(Resource::Grain),
        4 => Some(Resource::Ore),
        _ => return Err("invalid resource code".into()),
    })
}

fn port(value: i8) -> Result<Option<Port>, String> {
    Ok(match value {
        -1 => None,
        5 => Some(Port::Generic),
        0..=4 => Some(Port::Resource(
            resource(value)?.expect("resource port code is exact"),
        )),
        _ => return Err("invalid port code".into()),
    })
}

fn phase(value: &str, parameter: Option<u8>) -> Result<Phase, String> {
    Ok(match value {
        "setup-settlement" => Phase::SetupSettlement,
        "setup-road" => Phase::SetupRoad {
            settlement: parameter.ok_or_else(|| "setup road needs anchor".to_string())?,
        },
        "pre-roll" => Phase::PreRoll,
        "roll-chance" => Phase::RollChance,
        "discard" => Phase::Discard,
        "move-robber" => Phase::MoveRobber,
        "resolve-steal" => Phase::ResolveSteal {
            victim: parameter.ok_or_else(|| "steal needs victim".to_string())?,
        },
        "main" => Phase::Main,
        "development-chance" => Phase::DevelopmentChance,
        "trade-responses" => Phase::TradeResponses,
        "finished" => Phase::Finished,
        _ => return Err("invalid phase".into()),
    })
}

fn player(input: PlayerInput) -> PlayerState {
    PlayerState {
        resources: input.resources,
        development: input.development,
        bought_development: input.bought_development,
        public_victory_points: input.public_victory_points,
        played_knights: input.played_knights,
        roads_left: input.roads_left,
        settlements_left: input.settlements_left,
        cities_left: input.cities_left,
        has_longest_road: input.has_longest_road,
        has_largest_army: input.has_largest_army,
        played_development_this_turn: input.played_development_this_turn,
        policy_profile: input.policy_profile.unwrap_or([51; 5]),
    }
}

fn game_states(
    input: StateInput,
    last_rejected_trade: Option<RejectedTradeInput>,
) -> Result<Vec<BeliefParticle>, String> {
    let num_players = input.players.len() as u8;
    let board = Board {
        num_players,
        hexes: input
            .board
            .hexes
            .into_iter()
            .enumerate()
            .map(|(index, hex)| {
                Ok(colonist_catan_core::Hex {
                    resource: resource(hex.resource)?,
                    number: hex.number,
                    coord: (index as i8, 0),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        vertices: input
            .board
            .vertices
            .into_iter()
            .map(|vertex| {
                Ok(Vertex {
                    adjacent_hexes: vertex.adjacent_hexes,
                    adjacent_vertices: vertex.adjacent_vertices,
                    adjacent_edges: vertex.adjacent_edges,
                    port: port(vertex.port)?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
        edges: input
            .board
            .edges
            .into_iter()
            .map(|edge| Edge {
                vertices: edge.vertices,
                adjacent_hexes: edge.adjacent_hexes,
            })
            .collect(),
    };
    let buildings = input
        .buildings
        .into_iter()
        .map(|piece| {
            if piece < 0 {
                None
            } else {
                let owner = piece as u8 / 2;
                Some(if (piece as u8).is_multiple_of(2) {
                    Building::Settlement(owner)
                } else {
                    Building::City(owner)
                })
            }
        })
        .collect::<Vec<_>>();
    let roads = input
        .roads
        .into_iter()
        .map(|owner| (owner >= 0).then_some(owner as u8))
        .collect::<Vec<_>>();
    let players = input.players.into_iter().map(player).collect::<Vec<_>>();
    let game_phase = phase(&input.phase, input.phase_parameter)?;
    let robber_return_phase = phase(&input.robber_return_phase, None)?;
    let trade = input.trade.map(|trade| TradeOffer {
        creator: trade.creator,
        recipients: trade.recipients,
        give: trade.give,
        receive: trade.receive,
        accepted: trade.accepted,
        rejected: trade.rejected,
    });
    let worlds = if input.worlds.is_empty() {
        vec![WorldInput {
            weight: Some(1.0),
            hands: players.iter().map(|state| state.resources).collect(),
            development: None,
            bought_development: None,
            development_deck: None,
            bank: None,
        }]
    } else {
        input.worlds
    };
    let mut result = Vec::with_capacity(worlds.len());
    for world in worlds {
        if world.hands.len() != players.len()
            || world
                .development
                .as_ref()
                .is_some_and(|cards| cards.len() != players.len())
            || world
                .bought_development
                .as_ref()
                .is_some_and(|cards| cards.len() != players.len())
        {
            return Err("world player count mismatch".into());
        }
        let mut world_players = players.clone();
        for (state, hand) in world_players.iter_mut().zip(world.hands) {
            state.resources = hand;
        }
        if let Some(development) = world.development {
            for (state, cards) in world_players.iter_mut().zip(development) {
                state.development = cards;
            }
        }
        if let Some(bought_development) = world.bought_development {
            for (state, cards) in world_players.iter_mut().zip(bought_development) {
                state.bought_development = cards;
            }
        }
        let state = GameState {
            board: board.clone().into(),
            players: world_players,
            buildings: buildings.clone(),
            roads: roads.clone(),
            bank: world.bank.unwrap_or(input.bank),
            bank_is_public: input.bank_visible.unwrap_or(true),
            development_deck: world.development_deck.unwrap_or(input.development_deck),
            played_development: input.played_development,
            robber_hex: input.robber_hex,
            current_player: input.current_player,
            phase: game_phase,
            turn: input.turn,
            last_roll: input.last_roll,
            dice_mode: input.dice_mode.into(),
            victory_target: input.victory_target,
            card_discard_limit: input.card_discard_limit.unwrap_or(7),
            friendly_robber: input.friendly_robber.unwrap_or(false),
            setup_step: input.setup_step,
            discard_remaining: input.discard_remaining,
            discard_cursor: input.discard_cursor,
            robber_return_phase,
            free_roads: 0,
            domestic_trade_used: input.domestic_trade_used,
            // The live adapter's boolean means this turn's offer budget is
            // exhausted. Mapping it to one left the simulator's second
            // negotiation round open and made trivial end-turn states
            // expand a full tree of redundant offers.
            domestic_trade_count: if input.domestic_trade_used { 2 } else { 0 },
            player_trades_enabled: input.player_trades_enabled.unwrap_or(true),
            domestic_trade_disabled: input.domestic_trade_disabled.unwrap_or(0),
            domestic_trade_embargoes: input.domestic_trade_embargoes.unwrap_or(0),
            last_rejected_trade: last_rejected_trade.as_ref().map(|rejected| TradeOffer {
                creator: input.current_player,
                recipients: ((1u8 << num_players) - 1) & !(1u8 << input.current_player),
                give: rejected.give,
                receive: rejected.receive,
                accepted: 0,
                rejected: ((1u8 << num_players) - 1) & !(1u8 << input.current_player),
            }),
            trade,
            trade_cursor: input.trade_cursor,
            trade_negotiation_round: 0,
            longest_road_holder: input.longest_road_holder,
            largest_army_holder: input.largest_army_holder,
        };
        state
            .validate()
            .map_err(|error| format!("invalid search state: {error}"))?;
        result.push(BeliefParticle {
            weight: world.weight.unwrap_or(1.0).max(0.0),
            state,
        });
    }
    Ok(result)
}

fn action(action: Action) -> ActionOutput {
    let mut output = ActionOutput {
        kind: "unknown",
        first: None,
        second: None,
        player: None,
        resource: None,
        other_resource: None,
        cards: None,
        receive_cards: None,
        accept: None,
    };
    match action {
        Action::PlaceSettlement { vertex } => {
            output.kind = "place-settlement";
            output.first = Some(vertex);
        }
        Action::PlaceRoad { edge } => {
            output.kind = "place-road";
            output.first = Some(edge);
        }
        Action::Roll => output.kind = "roll",
        Action::ResolveRoll { value } => {
            output.kind = "resolve-roll";
            output.first = Some(value);
        }
        Action::Discard { cards } => {
            output.kind = "discard";
            output.cards = Some(cards);
        }
        Action::MoveRobber { hex, victim } => {
            output.kind = "move-robber";
            output.first = Some(hex);
            output.player = victim;
        }
        Action::ResolveSteal { victim, resource } => {
            output.kind = "resolve-steal";
            output.player = Some(victim);
            output.resource = Some(resource as u8);
        }
        Action::BuildRoad { edge } => {
            output.kind = "build-road";
            output.first = Some(edge);
        }
        Action::BuildSettlement { vertex } => {
            output.kind = "build-settlement";
            output.first = Some(vertex);
        }
        Action::BuildCity { vertex } => {
            output.kind = "build-city";
            output.first = Some(vertex);
        }
        Action::BuyDevelopment => output.kind = "buy-development",
        Action::ResolveDevelopment { card } => {
            output.kind = "resolve-development";
            output.resource = Some(card as u8);
        }
        Action::PlayKnight { hex, victim } => {
            output.kind = "play-knight";
            output.first = Some(hex);
            output.player = victim;
        }
        Action::PlayRoadBuilding { first, second } => {
            output.kind = "play-road-building";
            output.first = Some(first);
            output.second = second;
        }
        Action::PlayYearOfPlenty { first, second } => {
            output.kind = "play-year-of-plenty";
            output.resource = Some(first as u8);
            output.other_resource = Some(second as u8);
        }
        Action::PlayMonopoly { resource } => {
            output.kind = "play-monopoly";
            output.resource = Some(resource as u8);
        }
        Action::MaritimeTrade {
            give,
            receive,
            ratio,
        } => {
            output.kind = "maritime-trade";
            output.first = Some(ratio);
            output.resource = Some(give as u8);
            output.other_resource = Some(receive as u8);
        }
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } => {
            output.kind = "offer-trade";
            output.first = Some(recipients);
            output.cards = Some(give);
            output.receive_cards = Some(receive);
        }
        Action::RespondTrade { accept } => {
            output.kind = "respond-trade";
            output.accept = Some(accept);
        }
        Action::CounterTrade { give, receive } => {
            output.kind = "counter-trade";
            output.cards = Some(give);
            output.receive_cards = Some(receive);
        }
        Action::ConfirmTrade { partner } => {
            output.kind = "confirm-trade";
            output.player = Some(partner);
        }
        Action::CancelTrade => output.kind = "cancel-trade",
        Action::EndTurn => output.kind = "end-turn",
    }
    output
}

fn root_prune_reason(reason: RootPruneReason) -> &'static str {
    match reason {
        RootPruneReason::RootExcluded => "root-excluded",
        RootPruneReason::BranchTruncated => "branch-truncated",
        RootPruneReason::TradeSafety => "trade-safety",
        RootPruneReason::ExactFamilyCollapsed => "exact-family-collapsed",
    }
}

fn exact_family_label(family: ExactActionFamily) -> &'static str {
    match family {
        ExactActionFamily::Mandatory => "mandatory",
        ExactActionFamily::Knight => "knight",
        ExactActionFamily::Monopoly => "monopoly",
        ExactActionFamily::YearOfPlenty => "year-of-plenty",
        ExactActionFamily::RoadBuilding => "road-building",
    }
}

fn root_promotion_reason(reason: RootPromotionReason) -> &'static str {
    match reason {
        RootPromotionReason::RoadAwardProtection => "road-award-protection",
        RootPromotionReason::CriticalExpansionProtection => "critical-expansion-protection",
        RootPromotionReason::OpponentRouteCut => "opponent-route-cut",
        RootPromotionReason::CloseoutCompression => "closeout-compression",
    }
}

fn domestic_trade_threat_label(threat: DomesticTradeThreat) -> &'static str {
    match threat {
        DomesticTradeThreat::DirtyMonopoly => "dirty-monopoly",
        DomesticTradeThreat::ImmediateWin => "immediate-win",
        DomesticTradeThreat::AwardSwing => "award-swing",
        DomesticTradeThreat::ContestedSettlement => "contested-settlement",
        DomesticTradeThreat::MaterialBuild => "material-build",
    }
}

fn replacement_output(replacement: (Action, Action)) -> ActionReplacementOutput {
    ActionReplacementOutput {
        from: action(replacement.0),
        to: action(replacement.1),
    }
}

fn road_intent_output(intent: colonist_catan_search::RoadIntent) -> RoadIntentOutput {
    RoadIntentOutput {
        target_vertex: intent.target_vertex,
        roads_remaining: intent.roads_remaining,
        expected_rolls: intent.expected_rolls.is_finite().then_some(intent.expected_rolls),
        survival_probability: intent.survival_probability,
        target_value: intent.target_value,
        portfolio_value: intent.portfolio_value,
        frontier_gain: intent.frontier_gain,
        ordering_score: intent.ordering_score(),
    }
}

fn root_provenance_output(provenance: BeliefSearchProvenance) -> RootProvenanceOutput {
    RootProvenanceOutput {
        ranked_root_count: provenance.ranked_root_count,
        ranked_roots: provenance
            .ranked_roots
            .into_iter()
            .map(|candidate| RankedRootOutput {
                action: action(candidate.action),
                rank: candidate.rank,
                prior: candidate.prior,
                planner_value: candidate.planner_value,
                planner_completion_mass: candidate.planner_completion_mass,
                planner_decisive_completion_mass: candidate.planner_decisive_completion_mass,
                planner_response_windows: candidate.planner_response_windows,
            })
            .collect(),
        retained_roots: provenance
            .retained_roots
            .into_iter()
            .map(|candidate| RetainedRootOutput {
                action: action(candidate.action),
                pre_truncation_rank: candidate.pre_truncation_rank,
                prior: candidate.prior,
                node_budget_per_particle: candidate.node_budget_per_particle,
                allocated_nodes: candidate.allocated_nodes,
                planner_value: candidate.planner_value,
                planner_completion_mass: candidate.planner_completion_mass,
                planner_decisive_completion_mass: candidate.planner_decisive_completion_mass,
                planner_response_windows: candidate.planner_response_windows,
                final_rank: None,
                final_evaluation_horizon: None,
                initial_terminal_outcome: None,
                initial_terminal_rate: None,
                initial_victory_margin: None,
                initial_strategic_margin: None,
                terminal_outcome: None,
                terminal_rate: None,
                terminal_lower_bound: None,
                terminal_upper_bound: None,
                victory_margin: None,
                victory_margin_lower_bound: None,
                victory_margin_upper_bound: None,
                strategic_margin: None,
                strategic_margin_lower_bound: None,
                strategic_margin_upper_bound: None,
                mean_turn: None,
            })
            .collect(),
        pruned_root_count: provenance.pruned_root_count,
        pruned_roots: provenance
            .pruned_roots
            .into_iter()
            .map(|candidate| PrunedRootOutput {
                action: action(candidate.action),
                pre_truncation_rank: candidate.pre_truncation_rank,
                reason: root_prune_reason(candidate.reason),
            })
            .collect(),
        root_evidence: provenance
            .root_evidence
            .into_iter()
            .map(|evidence| {
                let has_introduced_fragility = !evidence
                    .introduced_road_fragility
                    .critical_vertices
                    .is_empty();
                RootCausalEvidenceOutput {
                    action: action(evidence.action),
                    promotion_reason: evidence.promotion_reason.map(root_promotion_reason),
                    road_intent: evidence.road_intent.map(road_intent_output),
                    introduced_road_fragility: has_introduced_fragility.then(|| {
                        introduced_road_fragility_output(&evidence.introduced_road_fragility)
                    }),
                    road_cut_continuation: has_introduced_fragility
                        .then(|| road_cut_continuation_output(&evidence.road_cut_continuation)),
                    admitted_by_promotion: evidence.admitted_by_promotion,
                    closeout_gain: evidence.closeout_gain,
                    response_windows: evidence.response_windows,
                    decisive_completion_mass: evidence.decisive_completion_mass,
                    trade_threat: evidence.trade_threat.map(domestic_trade_threat_label),
                    trade_risk_posterior: evidence.trade_risk_posterior,
                    dirty_monopoly_posterior: evidence.dirty_monopoly_posterior,
                    trade_hard_veto_posterior: evidence.trade_hard_veto_posterior,
                    trade_hard_veto: evidence.trade_hard_veto,
                }
            })
            .collect(),
        horizon_escalation: None,
        trade_hard_veto_threshold: provenance.trade_hard_veto_threshold,
        search_winner: provenance.search_winner.map(action),
        exact_family_replacement: provenance.exact_family_replacement.map(replacement_output),
        safety_replacement: provenance.safety_replacement.map(replacement_output),
    }
}

fn weighted_policy_report(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
) -> SearchReport {
    weighted_policy_report_for_actions_controlled(particles, root_exclusions, None, || false)
        .expect("uncontrolled weighted policy report cannot stop")
}

fn weighted_policy_report_for_actions_controlled<F>(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
    candidate_actions: Option<&[Action]>,
    mut should_stop: F,
) -> Result<SearchReport, ()>
where
    F: FnMut() -> bool,
{
    if should_stop() {
        return Err(());
    }
    let state = &particles[0].state;
    let actor = state.actor();
    let observed = state.observed_state(actor);
    let actor_domain = colonist_catan_search::actor_proposal_actions(state);
    let actions = if let Some(candidate_actions) = candidate_actions {
        candidate_actions
            .iter()
            .filter(|action| actor_domain.contains(action) && !root_exclusions.contains(action))
            .cloned()
            .collect::<Vec<_>>()
    } else {
        actor_domain
            .into_iter()
            .filter(|action| !root_exclusions.contains(action))
            .collect::<Vec<_>>()
    };
    let action_count = actions.len();
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut statistics = Vec::with_capacity(action_count);
    for selected in actions {
        if should_stop() {
            return Err(());
        }
        let prior = action_prior(&observed, &selected, actor);
        let mut value = [0.0; 4];
        let mut lower_confidence_value = [f32::INFINITY; 4];
        let mut legal_weight = 0.0_f32;
        for particle in particles {
            if should_stop() {
                return Err(());
            }
            let weight = particle.weight.max(0.0) / total_weight;
            if weight <= f32::EPSILON {
                continue;
            }
            let mut next = particle.state.clone();
            let legal = next.apply(&selected).is_ok();
            let evaluated = if legal {
                legal_weight += weight;
                evaluate(&next)
            } else {
                evaluate(&particle.state)
            };
            for player in 0..4 {
                value[player] += evaluated[player] * weight;
                lower_confidence_value[player] =
                    lower_confidence_value[player].min(evaluated[player]);
            }
        }
        statistics.push(ActionStats {
            action: selected,
            visits: particles.len() as u32,
            availability: (legal_weight * particles.len() as f32).round() as u32,
            availability_weight: legal_weight,
            legal_weight,
            prior,
            value,
            lower_confidence_value,
        });
    }
    statistics.sort_by(|left, right| {
        right.value[actor as usize]
            .total_cmp(&left.value[actor as usize])
            .then_with(|| right.prior.total_cmp(&left.prior))
            .then_with(|| format!("{:?}", left.action).cmp(&format!("{:?}", right.action)))
    });
    let chosen = statistics.first().map(|candidate| candidate.action.clone());
    let mut root_value = [0.0; 4];
    for particle in particles {
        if should_stop() {
            return Err(());
        }
        let weight = particle.weight.max(0.0) / total_weight;
        let evaluated = evaluate(&particle.state);
        for player in 0..4 {
            root_value[player] += evaluated[player] * weight;
        }
    }
    Ok(SearchReport {
        chosen,
        root_value,
        actions: statistics,
        tactical: TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        },
        exact: ExactDecisionResult::default(),
        statistics: SearchStatistics {
            iterations: particles.len() as u32,
            nodes: particles.len().saturating_mul(action_count),
            deepest_decision_depth: 1,
            rollouts: 0,
            effective_particle_count: effective_particle_count(particles),
            deadline_reached: false,
        },
    })
}

fn basic_response_diagnostics(
    particles: usize,
    authority: DecisionAuthority,
    effective_effort: SearchEffortInput,
) -> ResponseDiagnostics {
    ResponseDiagnostics {
        rust_posterior_particles: particles,
        rust_search_particles: particles,
        effective_effort,
        search_stages: None,
        root_provenance: RootProvenanceOutput::default(),
        authority_trace: AuthorityTraceOutput {
            initial_authority: authority,
            exact_family: None,
            exact_family_replacement: None,
            safety_replacement: None,
        },
    }
}

fn response(
    report: SearchReport,
    particles: usize,
    algorithm: &'static str,
    authority: DecisionAuthority,
    diagnostics: ResponseDiagnostics,
) -> Response {
    Response {
        engine_revision: ENGINE_REVISION,
        authority,
        learned_model_version: learned_model_version(),
        trade_model_version: learned_trade_model_version(),
        algorithm,
        chosen: report.chosen.map(action),
        root_value: report.root_value,
        tactical_win_probability: report.tactical.win_probability,
        tactical_lower_bound: report.tactical.lower_bound,
        tactical_proven: report.tactical.proven,
        tactical_line: report
            .tactical
            .principal_line
            .into_iter()
            .map(action)
            .collect(),
        exact_decision: report.exact.applicable,
        exact_worlds: report.exact.worlds,
        exact_actions: report
            .exact
            .actions
            .into_iter()
            .map(|candidate| ExactActionDiagnosticOutput {
                action: action(candidate.action),
                value: candidate.value,
                lower_bound: candidate.lower_bound,
                legal_weight: candidate.legal_weight,
                decision_score: candidate.decision_score,
                lower_score: candidate.lower_score,
                comparator_score: exact_action_comparator_score(
                    candidate.decision_score,
                    candidate.lower_score,
                ),
            })
            .collect(),
        actions: report
            .actions
            .into_iter()
            .map(|statistics| ActionStatisticsOutput {
                action: action(statistics.action),
                visits: statistics.visits,
                availability: statistics.availability,
                availability_weight: statistics.availability_weight,
                legal_weight: statistics.legal_weight,
                prior: statistics.prior,
                value: statistics.value,
                lower_confidence_value: statistics.lower_confidence_value,
            })
            .collect(),
        iterations: report.statistics.iterations,
        nodes: report.statistics.nodes,
        deepest_decision_depth: report.statistics.deepest_decision_depth,
        rollouts: report.statistics.rollouts,
        particles,
        wasm_particles: particles,
        rust_posterior_particles: diagnostics.rust_posterior_particles,
        rust_search_particles: diagnostics.rust_search_particles,
        effective_effort: diagnostics.effective_effort,
        search_stages: diagnostics.search_stages,
        root_provenance: diagnostics.root_provenance,
        authority_trace: diagnostics.authority_trace,
        effective_particle_count: report.statistics.effective_particle_count,
        deadline_reached: report.statistics.deadline_reached,
    }
}

fn effective_particle_count(particles: &[BeliefParticle]) -> f32 {
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let squared = particles
        .iter()
        .map(|particle| {
            let weight = particle.weight.max(0.0) / total;
            weight * weight
        })
        .sum::<f32>();
    1.0 / squared.max(f32::EPSILON)
}

fn exact_single_action(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
) -> Option<ExactDecisionResult> {
    let first = particles.first()?;
    let legal = first
        .state
        .legal_actions()
        .into_iter()
        .filter(|action| !root_exclusions.contains(action))
        .collect::<Vec<_>>();
    if legal.len() != 1 {
        return None;
    }
    let chosen = legal[0].clone();
    if particles.iter().any(|particle| {
        particle
            .state
            .legal_actions()
            .into_iter()
            .filter(|action| !root_exclusions.contains(action))
            .collect::<Vec<_>>()
            != [chosen.clone()]
    }) {
        return None;
    }
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut value = [0.0; 4];
    let mut lower_bound = [f32::INFINITY; 4];
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= f32::EPSILON {
            continue;
        }
        let mut next = particle.state.clone();
        next.apply(&chosen)
            .expect("the sole legal action must transition");
        let evaluated = evaluate(&next);
        for player in 0..4 {
            value[player] += evaluated[player] * weight;
            lower_bound[player] = lower_bound[player].min(evaluated[player]);
        }
    }
    let actor = first.state.actor() as usize;
    Some(ExactDecisionResult {
        applicable: true,
        chosen: Some(chosen.clone()),
        actions: vec![ExactActionValue {
            action: chosen,
            value,
            lower_bound,
            legal_weight: 1.0,
            decision_score: value[actor],
            lower_score: lower_bound[actor],
        }],
        worlds: particles.len(),
    })
}

/// Mandatory protocol decisions and positions with one universally legal
/// action are exact. Returning them before any long-range search keeps roll,
/// end-turn, discard, and robber/victim latency independent of the strategic
/// simulation budget. Trade responses remain strategic because counteroffers
/// are a deliberately bounded candidate family rather than an exhaustive one.
fn exact_mandatory_report_controlled<F>(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
    mut should_stop: F,
) -> Result<Option<SearchReport>, ()>
where
    F: FnMut() -> bool,
{
    let Some(mandatory) = solve_exact_belief_excluding_controlled(
        particles,
        ExactActionFamily::Mandatory,
        root_exclusions,
        &mut should_stop,
    ) else {
        return Err(());
    };
    let exact = if mandatory.applicable {
        mandatory
    } else {
        // `legal_actions()` deliberately exposes only a bounded counteroffer
        // family during trade negotiation. A single generated response is not
        // proof that the rules domain contains only one legal response, so the
        // generic single-action exact shortcut must not certify this phase.
        if particles
            .first()
            .is_some_and(|particle| particle.state.phase == Phase::TradeResponses)
        {
            return Ok(None);
        }
        if should_stop() {
            return Err(());
        }
        let Some(exact) = exact_single_action(particles, root_exclusions) else {
            return Ok(None);
        };
        exact
    };
    if !exact.applicable {
        return Ok(None);
    }
    let chosen = exact.chosen.clone();
    let root_value = exact
        .actions
        .iter()
        .find(|candidate| Some(&candidate.action) == chosen.as_ref())
        .map(|candidate| candidate.value)
        .unwrap_or([0.0; 4]);
    let actions = exact
        .actions
        .iter()
        .map(|candidate| ActionStats {
            action: candidate.action.clone(),
            visits: particles.len() as u32,
            availability: (candidate.legal_weight * particles.len() as f32).round() as u32,
            availability_weight: candidate.legal_weight,
            legal_weight: candidate.legal_weight,
            prior: 0.0,
            value: candidate.value,
            lower_confidence_value: candidate.lower_bound,
        })
        .collect();
    Ok(Some(SearchReport {
        chosen,
        root_value,
        actions,
        tactical: TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        },
        exact,
        statistics: SearchStatistics {
            iterations: 0,
            nodes: 0,
            deepest_decision_depth: 0,
            rollouts: 0,
            effective_particle_count: effective_particle_count(particles),
            deadline_reached: false,
        },
    }))
}

fn root_exclusion_actions(
    inputs: &[RootExclusionInput],
    state: &GameState,
) -> Result<Vec<Action>, String> {
    let actor = state.actor();
    let recipients = (0..state.board.num_players)
        .filter(|recipient| state.domestic_trade_pair_allowed(actor, *recipient))
        .fold(0u8, |mask, recipient| mask | (1 << recipient));
    inputs
        .iter()
        .map(|input| match input.kind.as_str() {
            "offer-trade" => Ok(Action::OfferTrade {
                recipients,
                give: input.give,
                receive: input.receive,
            }),
            "counter-trade" => Ok(Action::CounterTrade {
                give: input.give,
                receive: input.receive,
            }),
            other => Err(format!("unsupported root exclusion kind: {other}")),
        })
        .collect()
}

#[wasm_bindgen]
pub fn analyze(request: JsValue) -> Result<JsValue, JsValue> {
    let request: Request = serde_wasm_bindgen::from_value(request)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let mode = RequestedMode::parse(request.mode.as_deref())?;
    let ponder = request.ponder.unwrap_or(false);
    let effort = request.resolved_effort();
    let decision_time_ms = effort.decision_time_ms;
    let tactical_depth = effort.tactical.max_depth;
    let tactical_nodes = effort.tactical.node_budget;
    let cpu_depth = effort.cpu.max_depth;
    let cpu_root_cap = effort.cpu.root_cap;
    let cpu_nodes_per_depth_wave = effort.cpu.nodes_per_depth_wave;
    let cpu_evidence_escalation_ms = effort.cpu.evidence_escalation_ms.unwrap_or(0);
    let decision_clock = DecisionClock::start(decision_time_ms);
    let particles = game_states(request.state, request.last_rejected_trade)
        .map_err(|error| JsValue::from_str(&error))?;
    let root_exclusions = root_exclusion_actions(&request.root_exclusions, &particles[0].state)
        .map_err(|error| JsValue::from_str(&error))?;
    let algorithm = mode.label();
    if !ponder {
        let mandatory = exact_mandatory_report_controlled(&particles, &root_exclusions, || {
            decision_clock.remaining_ms() == 0
        })
        .map_err(|_| JsValue::from_str("decision deadline expired during exact arbitration"))?;
        if let Some(report) = mandatory {
            return serde_wasm_bindgen::to_value(&response(
                report,
                particles.len(),
                algorithm,
                DecisionAuthority::ExactMandatory,
                basic_response_diagnostics(
                    particles.len(),
                    DecisionAuthority::ExactMandatory,
                    effort,
                ),
            ))
            .map_err(|error| JsValue::from_str(&error.to_string()));
        }
    }
    let config = SearchConfig {
        iterations: request.iterations.unwrap_or(2_400).clamp(16, 50_000),
        max_nodes: request.max_nodes.unwrap_or(60_000).clamp(1_000, 250_000),
        rollout_actions: request.rollout_actions.unwrap_or(260).clamp(24, 800),
        tactical_depth: request.tactical_depth.unwrap_or(18).clamp(4, 32),
        tactical_nodes: request.tactical_nodes.unwrap_or(12_000).clamp(100, 100_000),
        time_budget_ms: decision_time_ms,
        seed: request.seed.unwrap_or(0x0043_4154_414e),
        mode: mode.mcts_mode(),
        ..SearchConfig::default()
    };
    let opening = matches!(
        particles[0].state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    );
    let (report, authority, diagnostics) =
        if matches!(mode, RequestedMode::Maxn | RequestedMode::AlphaBeta) || opening {
            let tactical_particles = particles
                .iter()
                .map(|particle| (&particle.state, particle.weight))
                .collect::<Vec<_>>();
            let tactical = solve_belief_current_turn_timed(
                &tactical_particles,
                tactical_depth,
                tactical_nodes,
                decision_clock.remaining_ms(),
            );
            if tactical.proven {
                let total_weight = particles
                    .iter()
                    .map(|particle| particle.weight.max(0.0))
                    .sum::<f32>()
                    .max(f32::EPSILON);
                let root_value = particles.iter().fold([0.0; 4], |mut total, particle| {
                    let weight = particle.weight.max(0.0) / total_weight;
                    let evaluated = evaluate(&particle.state);
                    for player in 0..4 {
                        total[player] += evaluated[player] * weight;
                    }
                    total
                });
                let report = SearchReport {
                    chosen: tactical.principal_line.first().cloned(),
                    root_value,
                    actions: Vec::new(),
                    tactical: tactical.clone(),
                    exact: ExactDecisionResult::default(),
                    statistics: SearchStatistics {
                        iterations: 0,
                        nodes: tactical.nodes as usize,
                        deepest_decision_depth: 0,
                        rollouts: 0,
                        effective_particle_count: effective_particle_count(&particles),
                        deadline_reached: decision_clock.remaining_ms() == 0,
                    },
                };
                return serde_wasm_bindgen::to_value(&response(
                    report,
                    particles.len(),
                    algorithm,
                    DecisionAuthority::TacticalProven,
                    basic_response_diagnostics(
                        particles.len(),
                        DecisionAuthority::TacticalProven,
                        effort,
                    ),
                ))
                .map_err(|error| JsValue::from_str(&error.to_string()));
            }
            let depth = cpu_depth;
            let branch_cap = cpu_root_cap;
            let maximum_nodes = cpu_nodes_per_depth_wave;
            let remaining_time_ms = decision_clock.remaining_ms().max(1);
            let depth_report = if mode == RequestedMode::AlphaBeta {
                search_weighted_belief_paranoid_iterative_timed_excluding(
                    &particles,
                    depth,
                    branch_cap,
                    maximum_nodes,
                    remaining_time_ms,
                    &root_exclusions,
                )
            } else {
                search_weighted_belief_maxn_iterative_timed_excluding(
                    &particles,
                    depth,
                    branch_cap,
                    maximum_nodes,
                    remaining_time_ms,
                    cpu_evidence_escalation_ms,
                    &root_exclusions,
                )
            }
            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?;
            let rust_posterior_particles = depth_report.posterior_particles;
            let rust_search_particles = depth_report.particles;
            let search_stages = depth_report.stage_timings.map(SearchStagesOutput::from);
            let depth_safety_replacement = depth_report.provenance.safety_replacement.clone();
            let depth_exact_family_replacement =
                depth_report.provenance.exact_family_replacement.clone();
            let exact_family_results = depth_report.provenance.exact_family_results.clone();
            let retained_root_priors = depth_report.provenance.retained_roots.clone();
            let root_provenance = root_provenance_output(depth_report.provenance.clone());
            let actions = depth_report
                .actions
                .into_iter()
                .map(|candidate| {
                    let prior = retained_root_priors
                        .iter()
                        .find(|root| root.action == candidate.action)
                        .map_or(0.0, |root| root.prior);
                    ActionStats {
                        action: candidate.action,
                        visits: particles.len() as u32,
                        availability: (candidate.legal_weight * particles.len() as f32).round()
                            as u32,
                        availability_weight: candidate.legal_weight,
                        legal_weight: candidate.legal_weight,
                        prior,
                        value: candidate.value,
                        lower_confidence_value: candidate.lower_confidence_value,
                    }
                })
                .collect::<Vec<_>>();
            // Mandatory exact authority was already checked before any
            // expensive work. Development-family exact results were resolved
            // before branch competition inside belief MaxN and are reused here.
            let mut exact = ExactDecisionResult::default();
            let mut authority = if depth_safety_replacement.is_some() {
                DecisionAuthority::SafetyOverride
            } else {
                DecisionAuthority::DeepMaxn
            };
            let initial_authority = authority;
            let mut exact_family = None;
            let exact_family_replacement = depth_exact_family_replacement.map(replacement_output);
            let mut safety_replacement = depth_safety_replacement.map(replacement_output);
            let mut chosen = depth_report.chosen;
            if let Some(family) = chosen.as_ref().and_then(exact_family_for_action)
                && let Some((_, cached)) = exact_family_results
                    .iter()
                    .find(|(candidate, _)| *candidate == family)
            {
                exact_family = Some(exact_family_label(family));
                exact = cached.clone();
                if let Some(exact_chosen) = exact.chosen.clone() {
                    chosen = Some(exact_chosen);
                    authority = DecisionAuthority::ExactFamily;
                }
            }
            // This is the final arbitration gate: when it changes the selected
            // action, downstream telemetry/execution must retain safety-override
            // authority rather than being relabeled by an earlier family solver.
            if chosen == Some(Action::EndTurn)
                && let Some(safer) = safer_end_turn_alternative(
                    &particles[0].state,
                    particles[0].state.actor() as usize,
                    &actions,
                    Some(&particles),
                )
            {
                if safer != Action::EndTurn {
                    safety_replacement = Some(ActionReplacementOutput {
                        from: action(Action::EndTurn),
                        to: action(safer.clone()),
                    });
                }
                chosen = Some(safer);
                authority = DecisionAuthority::SafetyOverride;
            }
            let diagnostics = ResponseDiagnostics {
                rust_posterior_particles,
                rust_search_particles,
                effective_effort: effort,
                search_stages,
                root_provenance,
                authority_trace: AuthorityTraceOutput {
                    initial_authority,
                    exact_family,
                    exact_family_replacement,
                    safety_replacement,
                },
            };
            (
                SearchReport {
                    chosen,
                    root_value: depth_report.value,
                    actions,
                    tactical,
                    exact,
                    statistics: SearchStatistics {
                        iterations: particles.len() as u32,
                        nodes: depth_report.nodes as usize,
                        deepest_decision_depth: depth_report.depth as u16,
                        rollouts: 0,
                        effective_particle_count: effective_particle_count(&particles),
                        deadline_reached: depth_report.deadline_reached,
                    },
                },
                authority,
                diagnostics,
            )
        } else if mode == RequestedMode::Weighted {
            let mut report = weighted_policy_report(&particles, &root_exclusions);
            let tactical_particles = particles
                .iter()
                .map(|particle| (&particle.state, particle.weight))
                .collect::<Vec<_>>();
            report.tactical =
                solve_belief_current_turn(&tactical_particles, tactical_depth, tactical_nodes);
            let mut exact = solve_exact_belief_excluding(
                &particles,
                ExactActionFamily::Mandatory,
                &root_exclusions,
            );
            let mut authority = if exact.applicable {
                DecisionAuthority::ExactMandatory
            } else if report.tactical.proven {
                DecisionAuthority::TacticalProven
            } else {
                DecisionAuthority::WeightedPolicy
            };
            let initial_authority = authority;
            let mut exact_family = None;
            let mut exact_family_replacement = None;
            let mut safety_replacement = None;
            let mut chosen = if exact.applicable {
                exact.chosen.clone()
            } else if report.tactical.proven {
                report.tactical.principal_line.first().cloned()
            } else {
                report.chosen.clone()
            };
            if !exact.applicable
                && !report.tactical.proven
                && let Some(family) = chosen.as_ref().and_then(exact_family_for_action)
            {
                exact_family = Some(exact_family_label(family));
                let before = chosen.clone();
                exact = solve_exact_belief_excluding(&particles, family, &root_exclusions);
                if let Some(exact_chosen) = exact.chosen.clone() {
                    if before.as_ref() != Some(&exact_chosen)
                        && let Some(previous) = before
                    {
                        exact_family_replacement = Some(ActionReplacementOutput {
                            from: action(previous),
                            to: action(exact_chosen.clone()),
                        });
                    }
                    chosen = Some(exact_chosen);
                    authority = DecisionAuthority::ExactFamily;
                }
            }
            if chosen == Some(Action::EndTurn)
                && let Some(safer) = safer_end_turn_alternative(
                    &particles[0].state,
                    particles[0].state.actor() as usize,
                    &report.actions,
                    Some(&particles),
                )
            {
                if safer != Action::EndTurn {
                    safety_replacement = Some(ActionReplacementOutput {
                        from: action(Action::EndTurn),
                        to: action(safer.clone()),
                    });
                }
                chosen = Some(safer);
                authority = DecisionAuthority::SafetyOverride;
            }
            report.chosen = chosen;
            report.exact = exact;
            let diagnostics = ResponseDiagnostics {
                rust_posterior_particles: particles.len(),
                rust_search_particles: particles.len(),
                effective_effort: effort,
                search_stages: None,
                root_provenance: RootProvenanceOutput::default(),
                authority_trace: AuthorityTraceOutput {
                    initial_authority,
                    exact_family,
                    exact_family_replacement,
                    safety_replacement,
                },
            };
            (report, authority, diagnostics)
        } else {
            let mut groups = Vec::<(u64, Vec<BeliefParticle>)>::new();
            for particle in &particles {
                let identity = particle.state.observation_hash(particle.state.actor());
                if let Some((_, members)) = groups
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == identity)
                {
                    members.push(particle.clone());
                } else {
                    groups.push((identity, vec![particle.clone()]));
                }
            }
            if !ponder && groups.len() > 1 {
                return Err(JsValue::from_str(
                    "root observation mismatch outside pondering mode",
                ));
            }
            let report = PERSISTENT_SEARCH.with(|slot| {
                let mut forest = slot.borrow_mut();
                let group_count = groups.len().max(1) as u32;
                let mut selected: Option<(f32, SearchReport)> = None;
                for (identity, group) in groups {
                    let group_weight = group
                        .iter()
                        .map(|particle| particle.weight.max(0.0))
                        .sum::<f32>();
                    let mut group_config = config.clone();
                    if ponder {
                        group_config.iterations = (group_config.iterations / group_count).max(16);
                        group_config.max_nodes =
                            (group_config.max_nodes / group_count as usize).max(1_000);
                        group_config.tactical_nodes =
                            (group_config.tactical_nodes / group_count).max(100);
                    }
                    let search_index = forest
                        .iter()
                        .position(|search| search.contains_identity(identity))
                        .unwrap_or_else(|| {
                            if forest.len() >= 12 {
                                forest.remove(0);
                            }
                            forest.push(Mcts::new(group_config.clone(), &group[0].state));
                            forest.len() - 1
                        });
                    let search = &mut forest[search_index];
                    search.reconfigure(group_config);
                    let report = if group.len() == 1 {
                        search.search_excluding(&group[0].state, &root_exclusions)
                    } else {
                        search
                            .search_weighted_belief_excluding(&group, &root_exclusions)
                            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?
                    };
                    if selected
                        .as_ref()
                        .is_none_or(|(weight, _)| group_weight > *weight)
                    {
                        selected = Some((group_weight, report));
                    }
                }
                selected
                    .map(|(_, report)| report)
                    .ok_or_else(|| JsValue::from_str("pondering produced no search group"))
            })?;
            let authority = DecisionAuthority::DeepMaxn;
            let diagnostics = basic_response_diagnostics(particles.len(), authority, effort);
            (report, authority, diagnostics)
        };
    serde_wasm_bindgen::to_value(&response(
        report,
        particles.len(),
        algorithm,
        authority,
        diagnostics,
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn engine_version() -> String {
    ENGINE_REVISION.to_string()
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::DiceMode;
    use serde_json::{Value, json};

    use super::{DiceModeInput, StateInput};

    fn legacy_state_json() -> Value {
        json!({
            "board": { "hexes": [], "vertices": [], "edges": [] },
            "players": [],
            "worlds": [],
            "buildings": [],
            "roads": [],
            "bank": [19, 19, 19, 19, 19],
            "developmentDeck": [14, 5, 2, 2, 2],
            "playedDevelopment": [0, 0, 0, 0, 0],
            "robberHex": 0,
            "currentPlayer": 0,
            "phase": "main",
            "turn": 0,
            "lastRoll": 0,
            "victoryTarget": 10,
            "setupStep": 0,
            "discardRemaining": [0, 0, 0, 0],
            "discardCursor": 0,
            "robberReturnPhase": "main",
            "tradeCursor": 0,
            "domesticTradeUsed": false
        })
    }

    #[test]
    fn missing_legacy_dice_mode_deserializes_to_unknown() {
        let input: StateInput = serde_json::from_value(legacy_state_json()).unwrap();
        assert_eq!(input.dice_mode, DiceModeInput::Unknown);
    }

    #[test]
    fn all_dice_modes_deserialize_and_map_to_core() {
        for (raw, expected) in [
            ("unknown", DiceMode::Unknown),
            ("random", DiceMode::Random),
            ("balanced", DiceMode::Balanced),
            ("unsupported", DiceMode::Unsupported),
        ] {
            let mut value = legacy_state_json();
            value["diceMode"] = Value::String(raw.to_owned());
            let input: StateInput = serde_json::from_value(value).unwrap();
            assert_eq!(DiceMode::from(input.dice_mode), expected);
        }
    }
}
