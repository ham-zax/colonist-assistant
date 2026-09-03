use std::collections::HashSet;
use std::env;
use std::fs::File;
use std::io::{BufRead, BufReader, BufWriter, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
#[cfg(feature = "cuda-exact")]
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use colonist_catan_arena::belief_particles;
use colonist_catan_core::{
    Action, Building, DevCard, GameState, NodeKind, Phase, PlayerState, Port, Resource, SplitMix64,
    TradeOffer,
};
use colonist_catan_search::{
    BeliefDepthConfig, BeliefParticle, BeliefSearchProvenance, DepthActionValue, ENGINE_REVISION,
    Mcts, RootPromotionReason, RootPruneReason, SearchConfig, SearchMode, SearchReport,
    action_prior, choose_rollout_action, compute_spatial_root_impacts, encode_action,
    encode_heterogeneous_graph, evaluate, expansion_option_value, pool_heterogeneous_graph,
    production_pips, search_maxn_bounded_timed, search_paranoid_bounded_timed,
    search_weighted_belief_maxn_with_config, search_weighted_belief_paranoid_with_config,
    strategic_utility, trade_acceptance_features,
};
#[cfg(feature = "cuda-exact")]
use colonist_catan_search::{
    CudaExactEvaluator, cuda_exact_search_stats, search_weighted_belief_maxn_cuda_with_config_mutex,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    Random,
    Weighted,
    MaxN,
    AlphaBeta,
    Uct,
    Puct,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EvaluatorBackend {
    Cpu,
    Cuda,
}

impl EvaluatorBackend {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "cpu" => Some(Self::Cpu),
            "cuda" => Some(Self::Cuda),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Cpu => "cpu-exact",
            Self::Cuda => "cuda-exact",
        }
    }
}

#[cfg(feature = "cuda-exact")]
static CUDA_EXACT_EVALUATOR: OnceLock<Mutex<CudaExactEvaluator>> = OnceLock::new();

#[cfg(feature = "cuda-exact")]
fn evaluator_benchmark_metrics(
    backend: EvaluatorBackend,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    if backend != EvaluatorBackend::Cuda {
        return (
            serde_json::Value::Null,
            serde_json::Value::Null,
            serde_json::Value::Null,
        );
    }
    let evaluator = CUDA_EXACT_EVALUATOR
        .get()
        .expect("CUDA evaluator initialized before metrics")
        .lock()
        .expect("CUDA evaluator lock poisoned before metrics");
    let identity = evaluator.device_identity();
    let stats = evaluator.stats();
    let search_stats = cuda_exact_search_stats();
    (
        serde_json::json!({
            "name": identity.name,
            "ordinal": identity.ordinal,
            "computeCapability": [identity.compute_capability.0, identity.compute_capability.1],
            "evaluatorStatesPerSecond": stats.states_per_second(),
            "searchCalls": search_stats.calls,
            "linearSearchCalls": search_stats.linear_calls,
            "deferredSearchCalls": search_stats.deferred_calls,
            "streamedLeaves": search_stats.streamed_leaves,
            "streamFlushes": search_stats.stream_flushes,
            "evaluatorTimingMs": {
                "total": stats.total_nanos as f64 / 1_000_000.0,
                "pack": stats.total_pack_nanos as f64 / 1_000_000.0,
                "hostToDevice": stats.total_upload_nanos as f64 / 1_000_000.0,
                "kernel": stats.total_kernel_nanos as f64 / 1_000_000.0,
                "deviceToHost": stats.total_download_nanos as f64 / 1_000_000.0,
            },
            "searchTimingMs": {
                "total": search_stats.total_nanos as f64 / 1_000_000.0,
                "rootPreparation": search_stats.root_preparation_nanos as f64 / 1_000_000.0,
                "linearTraversal": search_stats.linear_traversal_nanos as f64 / 1_000_000.0,
                "linearLegalActions": search_stats.linear_legal_actions_nanos as f64 / 1_000_000.0,
                "linearPolicy": search_stats.linear_policy_nanos as f64 / 1_000_000.0,
                "linearBudget": search_stats.linear_budget_nanos as f64 / 1_000_000.0,
                "linearApply": search_stats.linear_apply_nanos as f64 / 1_000_000.0,
                "treeBuild": search_stats.tree_build_nanos as f64 / 1_000_000.0,
                "hostPacking": search_stats.host_packing_nanos as f64 / 1_000_000.0,
                "queueWait": search_stats.queue_wait_nanos as f64 / 1_000_000.0,
                "evaluation": search_stats.evaluation_nanos as f64 / 1_000_000.0,
                "backup": search_stats.backup_nanos as f64 / 1_000_000.0,
            },
        }),
        serde_json::json!({
            "batches": stats.batches,
            "states": stats.states,
            "averageBatchSize": stats.average_batch_size(),
            "lastBatchSize": stats.last_batch_size,
        }),
        serde_json::json!(stats.average_batch_size()),
    )
}

#[cfg(not(feature = "cuda-exact"))]
fn evaluator_benchmark_metrics(
    _backend: EvaluatorBackend,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    (
        serde_json::Value::Null,
        serde_json::Value::Null,
        serde_json::Value::Null,
    )
}

impl Engine {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "random" => Some(Self::Random),
            "weighted" => Some(Self::Weighted),
            "maxn" | "deep" => Some(Self::MaxN),
            "alphabeta" | "alpha-beta" => Some(Self::AlphaBeta),
            "uct" => Some(Self::Uct),
            "puct" | "strategist" => Some(Self::Puct),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Weighted => "weighted",
            Self::MaxN => "maxn",
            Self::AlphaBeta => "alphabeta",
            Self::Uct => "uct",
            Self::Puct => "puct",
        }
    }

    fn uses_search_information(self) -> bool {
        matches!(self, Self::MaxN | Self::AlphaBeta | Self::Uct | Self::Puct)
    }
}

#[derive(Clone, Debug)]
struct Config {
    players: u8,
    blocks: u32,
    seed: u64,
    iterations: u32,
    rollout_actions: u16,
    max_turns: u16,
    candidate: Engine,
    baseline: Engine,
    lineup: Option<Vec<Engine>>,
    validate: bool,
    threads: usize,
    quiet: bool,
    json: bool,
    checkpoint_output: Option<String>,
    challenge_output: Option<String>,
    takeover_input: Option<String>,
    takeover_output: Option<String>,
    takeover_engine: Option<Engine>,
    takeover_native_gpu_host: Option<String>,
    takeover_native_gpu_revision: Option<String>,
    takeover_root_only: bool,
    takeover_random_followup: bool,
    takeover_target_seat: Option<u8>,
    expert_output: Option<String>,
    trade_output: Option<String>,
    trajectory_output: Option<String>,
    expert_stride: u32,
    expert_iterations: u32,
    expert_rollout_actions: u16,
    belief_particles: usize,
    strategic_particle_limit: usize,
    maxn_depth: u8,
    maxn_branch: usize,
    maxn_nodes: Option<u32>,
    maxn_time_ms: u32,
    opening_nodes: u32,
    opening_time_ms: u32,
    trade_response_nodes: u32,
    trade_response_time_ms: u32,
    player_trades_enabled: bool,
    evaluator_backend: EvaluatorBackend,
    perfect_information_search: bool,
    build_git_sha: &'static str,
    build_dirty: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: 4,
            blocks: 8,
            seed: 20260728,
            iterations: 300,
            rollout_actions: 180,
            max_turns: 600,
            candidate: Engine::MaxN,
            baseline: Engine::Weighted,
            lineup: None,
            validate: false,
            threads: thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(4),
            quiet: false,
            json: false,
            checkpoint_output: None,
            challenge_output: None,
            takeover_input: None,
            takeover_output: None,
            takeover_engine: None,
            takeover_native_gpu_host: None,
            takeover_native_gpu_revision: None,
            takeover_root_only: false,
            takeover_random_followup: false,
            takeover_target_seat: None,
            expert_output: None,
            trade_output: None,
            trajectory_output: None,
            expert_stride: 1,
            expert_iterations: 0,
            expert_rollout_actions: 0,
            belief_particles: 24,
            strategic_particle_limit: 12,
            maxn_depth: 3,
            maxn_branch: 12,
            maxn_nodes: None,
            maxn_time_ms: 0,
            opening_nodes: 12_000,
            opening_time_ms: 1_200,
            trade_response_nodes: 2_000,
            trade_response_time_ms: 350,
            player_trades_enabled: true,
            evaluator_backend: EvaluatorBackend::Cpu,
            perfect_information_search: false,
            build_git_sha: option_env!("COLONIST_BUILD_GIT_SHA").unwrap_or("unknown"),
            build_dirty: option_env!("COLONIST_BUILD_DIRTY") == Some("1"),
        }
    }
}

fn parse_config() -> Config {
    let mut config = Config::default();
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < arguments.len() {
        let value = arguments.get(index + 1).map(String::as_str);
        match arguments[index].as_str() {
            "--players" => config.players = value.and_then(|v| v.parse().ok()).unwrap_or(4),
            "--blocks" => config.blocks = value.and_then(|v| v.parse().ok()).unwrap_or(8),
            "--seed" => config.seed = value.and_then(|v| v.parse().ok()).unwrap_or(20260728),
            "--iterations" => config.iterations = value.and_then(|v| v.parse().ok()).unwrap_or(300),
            "--rollout-actions" => {
                config.rollout_actions = value.and_then(|v| v.parse().ok()).unwrap_or(180)
            }
            "--max-turns" => config.max_turns = value.and_then(|v| v.parse().ok()).unwrap_or(600),
            "--checkpoint-output" => config.checkpoint_output = value.map(str::to_string),
            "--challenge-output" => config.challenge_output = value.map(str::to_string),
            "--takeover-input" => config.takeover_input = value.map(str::to_string),
            "--takeover-output" => config.takeover_output = value.map(str::to_string),
            "--takeover-engine" => {
                config.takeover_engine = match value {
                    Some("control") => None,
                    Some(engine) => Engine::parse(engine).or_else(|| {
                        eprintln!("unknown --takeover-engine: {engine}");
                        std::process::exit(2);
                    }),
                    None => {
                        eprintln!("--takeover-engine requires a value");
                        std::process::exit(2);
                    }
                }
            }
            "--takeover-native-gpu-host" => {
                config.takeover_native_gpu_host = value.map(str::to_string);
            }
            "--takeover-native-gpu-revision" => {
                config.takeover_native_gpu_revision = value.map(str::to_string);
            }
            "--takeover-root-only" => {
                config.takeover_root_only = true;
                index += 1;
                continue;
            }
            "--takeover-random-followup" => {
                config.takeover_random_followup = true;
                index += 1;
                continue;
            }
            "--expert-output" => config.expert_output = value.map(str::to_string),
            "--trade-output" => config.trade_output = value.map(str::to_string),
            "--trajectory-output" => config.trajectory_output = value.map(str::to_string),
            "--expert-stride" => {
                config.expert_stride = value.and_then(|v| v.parse().ok()).unwrap_or(1).max(1)
            }
            "--expert-iterations" => {
                config.expert_iterations = value.and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            "--expert-rollout-actions" => {
                config.expert_rollout_actions = value.and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            "--belief-particles" => {
                config.belief_particles = value
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(24)
                    .clamp(1, 256)
            }
            "--strategic-particles" => {
                config.strategic_particle_limit = value
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(12)
                    .clamp(1, 256)
            }
            "--maxn-depth" => {
                config.maxn_depth = value.and_then(|v| v.parse().ok()).unwrap_or(3).clamp(1, 8)
            }
            "--maxn-branch" => {
                config.maxn_branch = value
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(12)
                    .clamp(1, 64)
            }
            "--maxn-nodes" => config.maxn_nodes = value.and_then(|v| v.parse().ok()),
            "--maxn-time-ms" => {
                config.maxn_time_ms = value.and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            "--opening-nodes" => {
                config.opening_nodes = value.and_then(|v| v.parse().ok()).unwrap_or(12_000).max(1)
            }
            "--opening-time-ms" => {
                config.opening_time_ms = value.and_then(|v| v.parse().ok()).unwrap_or(1_200)
            }
            "--trade-response-nodes" => {
                config.trade_response_nodes =
                    value.and_then(|v| v.parse().ok()).unwrap_or(2_000).max(1)
            }
            "--trade-response-time-ms" => {
                config.trade_response_time_ms = value.and_then(|v| v.parse().ok()).unwrap_or(350)
            }
            "--threads" => config.threads = value.and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            "--candidate" => {
                config.candidate = value.and_then(Engine::parse).unwrap_or(Engine::MaxN)
            }
            "--baseline" => {
                config.baseline = value.and_then(Engine::parse).unwrap_or(Engine::Weighted)
            }
            "--lineup" => {
                config.lineup = value.map(|lineup| {
                    lineup
                        .split(',')
                        .map(|engine| {
                            Engine::parse(engine.trim()).unwrap_or_else(|| {
                                eprintln!("unknown engine in --lineup: {engine}");
                                std::process::exit(2);
                            })
                        })
                        .collect()
                });
            }
            "--validate" => {
                config.validate = true;
                index += 1;
                continue;
            }
            "--quiet" => {
                config.quiet = true;
                index += 1;
                continue;
            }
            "--json" => {
                config.json = true;
                config.quiet = true;
                index += 1;
                continue;
            }
            "--perfect-information" => {
                config.perfect_information_search = true;
                index += 1;
                continue;
            }
            "--no-player-trades" => {
                config.player_trades_enabled = false;
                index += 1;
                continue;
            }
            "--evaluator" => {
                config.evaluator_backend =
                    value.and_then(EvaluatorBackend::parse).unwrap_or_else(|| {
                        eprintln!("--evaluator must be cpu or cuda");
                        std::process::exit(2);
                    });
            }
            "--help" | "-h" => {
                println!(
                    "colonist-arena [--players 2|3|4] [--blocks N] [--seed N] \\
                     [--candidate random|weighted|maxn|alphabeta|uct|puct] [--baseline ...] \\
                     [--lineup puct,puct,maxn,maxn] \\
                     [--iterations N] [--rollout-actions N] [--max-turns N] \\
                     [--belief-particles N] [--strategic-particles N] \\
                     [--maxn-depth N] [--maxn-branch N] [--maxn-nodes N] [--maxn-time-ms N] \\
                     [--perfect-information] [--no-player-trades] [--evaluator cpu|cuda] \\
                     [--checkpoint-output progress.jsonl] [--challenge-output challenges.jsonl] \\
                     [--takeover-input challenges.jsonl] [--takeover-output outcomes.jsonl] \\
                     [--takeover-engine control|random|weighted|maxn|alphabeta|uct|puct] \\
                     [--takeover-native-gpu-host PATH] [--takeover-native-gpu-revision SHA] \\
                     [--takeover-root-only] [--takeover-random-followup] \\
                     [--expert-output samples.jsonl] [--trade-output trades.jsonl] \\
                     [--trajectory-output trajectory.jsonl] \\
                     [--expert-stride N] [--expert-iterations N] \\
                     [--expert-rollout-actions N] \\
                     [--threads N] [--validate] [--quiet] [--json]\n\
                     maxn (also deep) is the validated default; puct is experimental.\n\
                     strategist remains a compatibility alias for puct.\n\
                     Search engines use identical weighted beliefs unless --perfect-information\n\
                     explicitly enables oracle access to hidden state.\n\
                     --no-player-trades forbids player offers/accepts/counters/confirms while\n\
                     preserving maritime bank and port trades.\n\
                     CUDA is opt-in and accelerates exact fixed-node MaxN leaf evaluation;\n\
                     setup continues to use the existing exact CPU opening solver.\n\
                     Checkpoints record git SHA and ENGINE_REVISION for reproducibility."
                );
                std::process::exit(0);
            }
            unknown => {
                eprintln!("unknown argument: {unknown}");
                std::process::exit(2);
            }
        }
        index += 2;
    }
    if !(2..=4).contains(&config.players) {
        eprintln!("--players must be 2, 3, or 4");
        std::process::exit(2);
    }
    if config.takeover_input.is_some() != config.takeover_output.is_some() {
        eprintln!("--takeover-input and --takeover-output must be provided together");
        std::process::exit(2);
    }
    if config.takeover_root_only && config.takeover_input.is_none() {
        eprintln!("--takeover-root-only requires --takeover-input/--takeover-output");
        std::process::exit(2);
    }
    if config.takeover_random_followup && config.takeover_input.is_none() {
        eprintln!("--takeover-random-followup requires --takeover-input/--takeover-output");
        std::process::exit(2);
    }
    if config.takeover_native_gpu_host.is_some() && config.takeover_input.is_none() {
        eprintln!("--takeover-native-gpu-host requires --takeover-input/--takeover-output");
        std::process::exit(2);
    }
    if config.takeover_native_gpu_host.is_some() && config.takeover_engine.is_some() {
        eprintln!("--takeover-native-gpu-host and --takeover-engine are mutually exclusive");
        std::process::exit(2);
    }
    if config.takeover_native_gpu_revision.is_some() != config.takeover_native_gpu_host.is_some() {
        eprintln!(
            "--takeover-native-gpu-revision must be provided with --takeover-native-gpu-host"
        );
        std::process::exit(2);
    }
    if config.blocks == 0 {
        eprintln!("--blocks must be at least 1");
        std::process::exit(2);
    }
    if config
        .lineup
        .as_ref()
        .is_some_and(|lineup| lineup.len() != config.players as usize)
    {
        eprintln!("--lineup must contain exactly --players engines");
        std::process::exit(2);
    }
    if config.lineup.is_none() && config.candidate == config.baseline {
        eprintln!("--candidate and --baseline must be different engines");
        std::process::exit(2);
    }
    if config.evaluator_backend == EvaluatorBackend::Cuda {
        #[cfg(not(feature = "cuda-exact"))]
        {
            eprintln!(
                "--evaluator cuda requires rebuilding colonist-arena with --features cuda-exact"
            );
            std::process::exit(2);
        }
        #[cfg(feature = "cuda-exact")]
        {
            if config
                .lineup
                .as_ref()
                .is_none_or(|lineup| lineup.iter().any(|engine| *engine != Engine::MaxN))
            {
                eprintln!("--evaluator cuda currently requires an all-maxn --lineup");
                std::process::exit(2);
            }
            if config.maxn_time_ms != 0 || config.trade_response_time_ms != 0 {
                eprintln!(
                    "--evaluator cuda requires --maxn-time-ms 0 and --trade-response-time-ms 0"
                );
                std::process::exit(2);
            }
            if config.perfect_information_search {
                eprintln!("--evaluator cuda currently requires weighted-belief search");
                std::process::exit(2);
            }
        }
    }
    config
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootReplacementTrace {
    from: String,
    to: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PrunedRootTrace {
    action: String,
    reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootEvidenceTrace {
    action: String,
    promotion_reason: Option<String>,
    admitted_by_promotion: bool,
    trade_risk_posterior: f32,
    dirty_monopoly_posterior: f32,
    trade_hard_veto_posterior: f32,
    trade_hard_veto: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootProvenanceTrace {
    retained_roots: Vec<String>,
    pruned_roots: Vec<PrunedRootTrace>,
    root_evidence: Vec<RootEvidenceTrace>,
    search_winner: Option<String>,
    exact_family_replacement: Option<RootReplacementTrace>,
    safety_replacement: Option<RootReplacementTrace>,
}

fn root_prune_reason(reason: RootPruneReason) -> &'static str {
    match reason {
        RootPruneReason::RootExcluded => "root-excluded",
        RootPruneReason::BranchTruncated => "branch-truncated",
        RootPruneReason::TradeSafety => "trade-safety",
        RootPruneReason::ExactFamilyCollapsed => "exact-family-collapsed",
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

fn root_provenance_trace(provenance: &BeliefSearchProvenance) -> RootProvenanceTrace {
    RootProvenanceTrace {
        retained_roots: provenance
            .retained_roots
            .iter()
            .map(|entry| format!("{:?}", entry.action))
            .collect(),
        pruned_roots: provenance
            .pruned_roots
            .iter()
            .map(|entry| PrunedRootTrace {
                action: format!("{:?}", entry.action),
                reason: root_prune_reason(entry.reason).to_string(),
            })
            .collect(),
        root_evidence: provenance
            .root_evidence
            .iter()
            .map(|entry| RootEvidenceTrace {
                action: format!("{:?}", entry.action),
                promotion_reason: entry
                    .promotion_reason
                    .map(root_promotion_reason)
                    .map(str::to_string),
                admitted_by_promotion: entry.admitted_by_promotion,
                trade_risk_posterior: entry.trade_risk_posterior,
                dirty_monopoly_posterior: entry.dirty_monopoly_posterior,
                trade_hard_veto_posterior: entry.trade_hard_veto_posterior,
                trade_hard_veto: entry.trade_hard_veto,
            })
            .collect(),
        search_winner: provenance
            .search_winner
            .as_ref()
            .map(|action| format!("{action:?}")),
        exact_family_replacement: provenance
            .exact_family_replacement
            .as_ref()
            .map(|(from, to)| RootReplacementTrace {
                from: format!("{from:?}"),
                to: format!("{to:?}"),
            }),
        safety_replacement: provenance.safety_replacement.as_ref().map(|(from, to)| {
            RootReplacementTrace {
                from: format!("{from:?}"),
                to: format!("{to:?}"),
            }
        }),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootDecisionTrace {
    action: String,
    selected_value: Option<f32>,
    best_value: Option<f32>,
    regret: Option<f32>,
    provenance: Option<RootProvenanceTrace>,
}

struct EngineChoice {
    action: Action,
    root_value: Option<[f32; 4]>,
    nodes: u32,
    depth: u8,
    posterior_particles: usize,
    strategic_particles: usize,
    deadline_reached: bool,
    action_values: Vec<DepthActionValue>,
    provenance: Option<RootProvenanceTrace>,
    search: Option<SearchReport>,
}

impl EngineChoice {
    fn simple(action: Action) -> Self {
        Self {
            action,
            root_value: None,
            nodes: 0,
            depth: 0,
            posterior_particles: 0,
            strategic_particles: 0,
            deadline_reached: false,
            action_values: Vec::new(),
            provenance: None,
            search: None,
        }
    }

    fn root_trace(&self, actor: usize) -> RootDecisionTrace {
        let selected_value = self
            .action_values
            .iter()
            .find(|entry| entry.action == self.action)
            .map(|entry| entry.value[actor]);
        let best_value = (!self.action_values.is_empty()).then(|| {
            self.action_values
                .iter()
                .map(|entry| entry.value[actor])
                .max_by(f32::total_cmp)
                .expect("non-empty action values have a maximum")
        });
        let regret = selected_value
            .zip(best_value)
            .map(|(selected, best)| (best - selected).max(0.0));
        RootDecisionTrace {
            action: format!("{:?}", self.action),
            selected_value,
            best_value,
            regret,
            provenance: self.provenance.clone(),
        }
    }
}

const NATIVE_GPU_PROTOCOL_VERSION: u32 = 6;
const NATIVE_GPU_STATE_SCHEMA_VERSION: u32 = 2;

struct NativeGpuClient {
    child: Child,
    stdin: ChildStdin,
    stdout: ChildStdout,
    next_id: u64,
    identity: Value,
}

struct NativeGpuDecision {
    action: Action,
    response: Value,
    root_trace: RootDecisionTrace,
    action_count: usize,
    posterior_particles: usize,
    rollouts: u64,
    rollout_steps: u64,
    deadline_reached: bool,
}

fn native_gpu_exchange(
    stdin: &mut ChildStdin,
    stdout: &mut ChildStdout,
    message: &Value,
) -> Result<Value, String> {
    let payload = serde_json::to_vec(message)
        .map_err(|error| format!("failed to encode native GPU request: {error}"))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| "native GPU request exceeded protocol size".to_string())?;
    stdin
        .write_all(&length.to_ne_bytes())
        .and_then(|_| stdin.write_all(&payload))
        .and_then(|_| stdin.flush())
        .map_err(|error| format!("failed to write native GPU request: {error}"))?;

    let mut length_bytes = [0u8; 4];
    stdout
        .read_exact(&mut length_bytes)
        .map_err(|error| format!("failed to read native GPU response header: {error}"))?;
    let response_length = u32::from_ne_bytes(length_bytes) as usize;
    let mut response_bytes = vec![0u8; response_length];
    stdout
        .read_exact(&mut response_bytes)
        .map_err(|error| format!("failed to read native GPU response: {error}"))?;
    serde_json::from_slice(&response_bytes)
        .map_err(|error| format!("invalid native GPU response JSON: {error}"))
}

impl NativeGpuClient {
    fn spawn(path: &str) -> Result<Self, String> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|error| format!("failed to start native GPU host {path}: {error}"))?;
        let mut stdin = child
            .stdin
            .take()
            .ok_or_else(|| "native GPU host stdin was unavailable".to_string())?;
        let mut stdout = child
            .stdout
            .take()
            .ok_or_else(|| "native GPU host stdout was unavailable".to_string())?;
        let hello = native_gpu_exchange(
            &mut stdin,
            &mut stdout,
            &json!({
                "type": "hello",
                "id": 0,
                "protocolVersion": NATIVE_GPU_PROTOCOL_VERSION,
                "stateSchemaVersion": NATIVE_GPU_STATE_SCHEMA_VERSION,
            }),
        )?;
        if let Some(error) = hello.get("error").and_then(Value::as_str) {
            return Err(format!("native GPU hello failed: {error}"));
        }
        if hello.get("runtime").and_then(Value::as_str) != Some("gpu-native") {
            return Err("native GPU hello did not identify gpu-native runtime".to_string());
        }
        Ok(Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            identity: hello,
        })
    }

    fn identity(&self) -> &Value {
        &self.identity
    }

    fn analyze(
        &mut self,
        state: &GameState,
        actor: usize,
        config: &Config,
    ) -> Result<NativeGpuDecision, String> {
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        let request = native_gpu_request(state, actor as u8, config);
        let message = json!({ "type": "analyze", "id": id, "request": request });
        let payload = serde_json::to_vec(&message)
            .map_err(|error| format!("failed to encode native GPU request: {error}"))?;
        let length = u32::try_from(payload.len())
            .map_err(|_| "native GPU request exceeded protocol size".to_string())?;
        self.stdin
            .write_all(&length.to_ne_bytes())
            .and_then(|_| self.stdin.write_all(&payload))
            .and_then(|_| self.stdin.flush())
            .map_err(|error| format!("failed to write native GPU request: {error}"))?;

        let mut length_bytes = [0u8; 4];
        self.stdout
            .read_exact(&mut length_bytes)
            .map_err(|error| format!("failed to read native GPU response header: {error}"))?;
        let response_length = u32::from_ne_bytes(length_bytes) as usize;
        let mut response_bytes = vec![0u8; response_length];
        self.stdout
            .read_exact(&mut response_bytes)
            .map_err(|error| format!("failed to read native GPU response: {error}"))?;
        let envelope: Value = serde_json::from_slice(&response_bytes)
            .map_err(|error| format!("invalid native GPU response JSON: {error}"))?;
        if envelope.get("id").and_then(Value::as_u64) != Some(id) {
            return Err(format!("native GPU response id did not match request {id}"));
        }
        if let Some(error) = envelope.get("error").and_then(Value::as_str) {
            return Err(format!("native GPU analysis failed: {error}"));
        }
        let response = envelope
            .get("response")
            .cloned()
            .ok_or_else(|| "native GPU response omitted response payload".to_string())?;
        let action_value = response
            .get("chosen")
            .ok_or_else(|| "native GPU response omitted chosen action".to_string())?;
        let action = native_gpu_action(action_value)?;
        if !state.legal_actions().contains(&action) {
            return Err(format!(
                "native GPU returned illegal action {action:?} at state {:016x}",
                state.state_hash()
            ));
        }
        let root_trace = native_gpu_root_trace(&response, &action);
        let action_count = response
            .get("actions")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        let posterior_particles = response
            .get("rustPosteriorParticles")
            .and_then(Value::as_u64)
            .and_then(|value| usize::try_from(value).ok())
            .unwrap_or(0);
        let rollouts = response
            .get("rollouts")
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let rollout_steps = response
            .get("effectiveEffort")
            .and_then(|effort| effort.get("gpu"))
            .and_then(|gpu| gpu.get("rolloutSteps"))
            .and_then(Value::as_u64)
            .unwrap_or(0);
        let deadline_reached = response
            .get("deadlineReached")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        Ok(NativeGpuDecision {
            action,
            response,
            root_trace,
            action_count,
            posterior_particles,
            rollouts,
            rollout_steps,
            deadline_reached,
        })
    }
}

impl Drop for NativeGpuClient {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn native_phase(phase: Phase) -> (&'static str, Option<u8>) {
    match phase {
        Phase::SetupSettlement => ("setup-settlement", None),
        Phase::SetupRoad { settlement } => ("setup-road", Some(settlement)),
        Phase::PreRoll => ("pre-roll", None),
        Phase::RollChance => ("roll-chance", None),
        Phase::Discard => ("discard", None),
        Phase::MoveRobber => ("move-robber", None),
        Phase::ResolveSteal { victim } => ("resolve-steal", Some(victim)),
        Phase::Main => ("main", None),
        Phase::DevelopmentChance => ("development-chance", None),
        Phase::TradeResponses => ("trade-responses", None),
        Phase::Finished => ("finished", None),
    }
}

fn native_port(port: Option<Port>) -> i8 {
    match port {
        None => -1,
        Some(Port::Generic) => 5,
        Some(Port::Resource(resource)) => resource as i8,
    }
}

fn native_gpu_request(state: &GameState, actor: u8, config: &Config) -> Value {
    let particles = belief_particles(
        state,
        actor,
        config.belief_particles,
        state.observation_hash(actor) ^ config.seed,
    );
    let players = state
        .players
        .iter()
        .map(|player| {
            json!({
                "resources": player.resources,
                "development": player.development,
                "boughtDevelopment": player.bought_development,
                "publicVictoryPoints": player.public_victory_points,
                "playedKnights": player.played_knights,
                "roadsLeft": player.roads_left,
                "settlementsLeft": player.settlements_left,
                "citiesLeft": player.cities_left,
                "hasLongestRoad": player.has_longest_road,
                "hasLargestArmy": player.has_largest_army,
                "playedDevelopmentThisTurn": player.played_development_this_turn,
                "policyProfile": player.policy_profile,
            })
        })
        .collect::<Vec<_>>();
    let worlds = particles
        .iter()
        .map(|particle| {
            json!({
                "weight": particle.weight,
                "hands": particle.state.players.iter().map(|player| player.resources).collect::<Vec<_>>(),
                "development": particle.state.players.iter().map(|player| player.development).collect::<Vec<_>>(),
                "boughtDevelopment": particle.state.players.iter().map(|player| player.bought_development).collect::<Vec<_>>(),
                "developmentDeck": particle.state.development_deck,
                "bank": particle.state.bank,
            })
        })
        .collect::<Vec<_>>();
    let (phase, phase_parameter) = native_phase(state.phase);
    let (robber_return_phase, _) = native_phase(state.robber_return_phase);
    let trade = state.trade.map(|trade| {
        json!({
            "creator": trade.creator,
            "recipients": trade.recipients,
            "give": trade.give,
            "receive": trade.receive,
            "accepted": trade.accepted,
            "rejected": trade.rejected,
        })
    });
    let last_rejected_trade = state.last_rejected_trade.map(|trade| {
        json!({
            "give": trade.give,
            "receive": trade.receive,
        })
    });
    let buildings = state
        .buildings
        .iter()
        .map(|building| match building {
            None => -1,
            Some(Building::Settlement(player)) => i16::from(*player) * 2,
            Some(Building::City(player)) => i16::from(*player) * 2 + 1,
        })
        .collect::<Vec<_>>();
    let roads = state
        .roads
        .iter()
        .map(|owner| owner.map_or(-1, i16::from))
        .collect::<Vec<_>>();
    let board = json!({
        "hexes": state.board.hexes.iter().map(|hex| json!({
            "resource": hex.resource.map_or(-1, |resource| resource as i8),
            "number": hex.number,
        })).collect::<Vec<_>>(),
        "vertices": state.board.vertices.iter().map(|vertex| json!({
            "adjacentHexes": vertex.adjacent_hexes,
            "adjacentVertices": vertex.adjacent_vertices,
            "adjacentEdges": vertex.adjacent_edges,
            "port": native_port(vertex.port),
        })).collect::<Vec<_>>(),
        "edges": state.board.edges.iter().map(|edge| json!({
            "vertices": edge.vertices,
            "adjacentHexes": edge.adjacent_hexes,
        })).collect::<Vec<_>>(),
    });
    json!({
        "state": {
            "board": board,
            "players": players,
            "worlds": worlds,
            "buildings": buildings,
            "roads": roads,
            "bank": state.bank,
            "bankVisible": state.bank_is_public,
            "developmentDeck": state.development_deck,
            "playedDevelopment": state.played_development,
            "robberHex": state.robber_hex,
            "currentPlayer": state.current_player,
            "phase": phase,
            "phaseParameter": phase_parameter,
            "turn": state.turn,
            "lastRoll": state.last_roll,
            "victoryTarget": state.victory_target,
            "cardDiscardLimit": state.card_discard_limit,
            "friendlyRobber": state.friendly_robber,
            "setupStep": state.setup_step,
            "discardRemaining": state.discard_remaining,
            "discardCursor": state.discard_cursor,
            "robberReturnPhase": robber_return_phase,
            "trade": trade,
            "tradeCursor": state.trade_cursor,
            "domesticTradeUsed": state.domestic_trade_used,
            "playerTradesEnabled": state.player_trades_enabled,
            "domesticTradeDisabled": state.domestic_trade_disabled,
            "domesticTradeEmbargoes": state.domestic_trade_embargoes,
            "longestRoadHolder": state.longest_road_holder,
            "largestArmyHolder": state.largest_army_holder,
        },
        "effort": {
            "decisionTimeMs": 4000,
            "tactical": { "maxDepth": 14, "nodeBudget": 900 },
            "cpu": { "maxDepth": 5, "rootCap": 10, "nodesPerDepthWave": 8000 },
            "gpu": { "rootCap": 12, "rolloutBudget": 384, "rolloutSteps": 96 },
        },
        "lastRejectedTrade": last_rejected_trade,
        "rootExclusions": [],
        "iterations": 384,
        "maxNodes": 8000,
        "rolloutActions": 96,
        "tacticalDepth": 14,
        "tacticalNodes": 900,
        "timeBudgetMs": 4000,
        "seed": state.observation_hash(actor) ^ config.seed,
        "mode": "maxn",
        "depth": 5,
        "branchCap": 10,
        "ponder": false,
    })
}

fn native_u8(value: &Value, field: &str) -> Result<u8, String> {
    value
        .get(field)
        .and_then(Value::as_u64)
        .and_then(|value| u8::try_from(value).ok())
        .ok_or_else(|| format!("native GPU action omitted valid {field}"))
}

fn native_optional_u8(value: &Value, field: &str) -> Result<Option<u8>, String> {
    match value.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .map(Some)
            .ok_or_else(|| format!("native GPU action has invalid {field}")),
    }
}

fn native_hand(value: &Value, field: &str) -> Result<[u8; 5], String> {
    let values = value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| format!("native GPU action omitted {field}"))?;
    if values.len() != 5 {
        return Err(format!(
            "native GPU action {field} must contain five resources"
        ));
    }
    let mut hand = [0u8; 5];
    for (index, value) in values.iter().enumerate() {
        hand[index] = value
            .as_u64()
            .and_then(|value| u8::try_from(value).ok())
            .ok_or_else(|| format!("native GPU action {field}[{index}] is invalid"))?;
    }
    Ok(hand)
}

fn native_resource(value: &Value, field: &str) -> Result<Resource, String> {
    let index = native_u8(value, field)? as usize;
    Resource::ALL
        .get(index)
        .copied()
        .ok_or_else(|| format!("native GPU action has invalid {field} resource {index}"))
}

fn native_gpu_action(value: &Value) -> Result<Action, String> {
    let kind = value
        .get("kind")
        .and_then(Value::as_str)
        .ok_or_else(|| "native GPU action omitted kind".to_string())?;
    Ok(match kind {
        "place-settlement" => Action::PlaceSettlement {
            vertex: native_u8(value, "first")?,
        },
        "place-road" => Action::PlaceRoad {
            edge: native_u8(value, "first")?,
        },
        "roll" => Action::Roll,
        "resolve-roll" => Action::ResolveRoll {
            value: native_u8(value, "first")?,
        },
        "discard" => Action::Discard {
            cards: native_hand(value, "cards")?,
        },
        "move-robber" => Action::MoveRobber {
            hex: native_u8(value, "first")?,
            victim: native_optional_u8(value, "player")?,
        },
        "resolve-steal" => Action::ResolveSteal {
            victim: native_u8(value, "player")?,
            resource: native_resource(value, "resource")?,
        },
        "build-road" => Action::BuildRoad {
            edge: native_u8(value, "first")?,
        },
        "build-settlement" => Action::BuildSettlement {
            vertex: native_u8(value, "first")?,
        },
        "build-city" => Action::BuildCity {
            vertex: native_u8(value, "first")?,
        },
        "buy-development" => Action::BuyDevelopment,
        "resolve-development" => {
            let index = native_u8(value, "resource")? as usize;
            Action::ResolveDevelopment {
                card: DevCard::ALL.get(index).copied().ok_or_else(|| {
                    format!("native GPU action has invalid development card {index}")
                })?,
            }
        }
        "play-knight" => Action::PlayKnight {
            hex: native_u8(value, "first")?,
            victim: native_optional_u8(value, "player")?,
        },
        "play-road-building" => Action::PlayRoadBuilding {
            first: native_u8(value, "first")?,
            second: native_optional_u8(value, "second")?,
        },
        "play-year-of-plenty" => Action::PlayYearOfPlenty {
            first: native_resource(value, "resource")?,
            second: native_resource(value, "otherResource")?,
        },
        "play-monopoly" => Action::PlayMonopoly {
            resource: native_resource(value, "resource")?,
        },
        "maritime-trade" => Action::MaritimeTrade {
            give: native_resource(value, "resource")?,
            receive: native_resource(value, "otherResource")?,
            ratio: native_u8(value, "first")?,
        },
        "offer-trade" => Action::OfferTrade {
            recipients: native_u8(value, "first")?,
            give: native_hand(value, "cards")?,
            receive: native_hand(value, "receiveCards")?,
        },
        "respond-trade" => Action::RespondTrade {
            accept: value
                .get("accept")
                .and_then(Value::as_bool)
                .ok_or_else(|| "native GPU action omitted accept".to_string())?,
        },
        "counter-trade" => Action::CounterTrade {
            give: native_hand(value, "cards")?,
            receive: native_hand(value, "receiveCards")?,
        },
        "confirm-trade" => Action::ConfirmTrade {
            partner: native_u8(value, "player")?,
        },
        "cancel-trade" => Action::CancelTrade,
        "end-turn" => Action::EndTurn,
        other => return Err(format!("unsupported native GPU action kind {other}")),
    })
}

fn native_gpu_action_string(value: &Value) -> Option<String> {
    native_gpu_action(value)
        .ok()
        .map(|action| format!("{action:?}"))
}

fn native_gpu_replacement(value: Option<&Value>) -> Option<RootReplacementTrace> {
    let value = value?;
    Some(RootReplacementTrace {
        from: native_gpu_action_string(value.get("from")?)?,
        to: native_gpu_action_string(value.get("to")?)?,
    })
}

fn native_gpu_provenance(response: &Value) -> Option<RootProvenanceTrace> {
    let provenance = response.get("rootProvenance")?;
    let retained_roots = provenance
        .get("retainedRoots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| entry.get("action").and_then(native_gpu_action_string))
        .collect::<Vec<_>>();
    let pruned_roots = provenance
        .get("prunedRoots")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(PrunedRootTrace {
                action: native_gpu_action_string(entry.get("action")?)?,
                reason: entry.get("reason")?.as_str()?.to_string(),
            })
        })
        .collect::<Vec<_>>();
    let root_evidence = provenance
        .get("rootEvidence")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|entry| {
            Some(RootEvidenceTrace {
                action: native_gpu_action_string(entry.get("action")?)?,
                promotion_reason: entry
                    .get("promotionReason")
                    .and_then(Value::as_str)
                    .map(str::to_string),
                admitted_by_promotion: entry
                    .get("admittedByPromotion")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
                trade_risk_posterior: entry
                    .get("tradeRiskPosterior")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                dirty_monopoly_posterior: entry
                    .get("dirtyMonopolyPosterior")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                trade_hard_veto_posterior: entry
                    .get("tradeHardVetoPosterior")
                    .and_then(Value::as_f64)
                    .unwrap_or(0.0) as f32,
                trade_hard_veto: entry
                    .get("tradeHardVeto")
                    .and_then(Value::as_bool)
                    .unwrap_or(false),
            })
        })
        .collect::<Vec<_>>();
    Some(RootProvenanceTrace {
        retained_roots,
        pruned_roots,
        root_evidence,
        search_winner: provenance
            .get("searchWinner")
            .and_then(native_gpu_action_string),
        exact_family_replacement: native_gpu_replacement(provenance.get("exactFamilyReplacement")),
        safety_replacement: native_gpu_replacement(provenance.get("safetyReplacement")),
    })
}

fn native_gpu_root_trace(response: &Value, action: &Action) -> RootDecisionTrace {
    RootDecisionTrace {
        action: format!("{action:?}"),
        selected_value: None,
        best_value: None,
        regret: None,
        provenance: native_gpu_provenance(response),
    }
}

fn search_belief_particles(state: &GameState, config: &Config) -> Option<Vec<BeliefParticle>> {
    if config.perfect_information_search {
        None
    } else {
        Some(belief_particles(
            state,
            state.actor(),
            config.belief_particles,
            state.observation_hash(state.actor()) ^ config.seed,
        ))
    }
}

fn information_mode(config: &Config) -> &'static str {
    if config.perfect_information_search {
        "perfect-information"
    } else {
        "weighted-belief"
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PhaseSnapshot {
    SetupSettlement,
    SetupRoad { settlement: u8 },
    PreRoll,
    RollChance,
    Discard,
    MoveRobber,
    ResolveSteal { victim: u8 },
    Main,
    DevelopmentChance,
    TradeResponses,
    Finished,
}

impl From<Phase> for PhaseSnapshot {
    fn from(value: Phase) -> Self {
        match value {
            Phase::SetupSettlement => Self::SetupSettlement,
            Phase::SetupRoad { settlement } => Self::SetupRoad { settlement },
            Phase::PreRoll => Self::PreRoll,
            Phase::RollChance => Self::RollChance,
            Phase::Discard => Self::Discard,
            Phase::MoveRobber => Self::MoveRobber,
            Phase::ResolveSteal { victim } => Self::ResolveSteal { victim },
            Phase::Main => Self::Main,
            Phase::DevelopmentChance => Self::DevelopmentChance,
            Phase::TradeResponses => Self::TradeResponses,
            Phase::Finished => Self::Finished,
        }
    }
}

impl From<PhaseSnapshot> for Phase {
    fn from(value: PhaseSnapshot) -> Self {
        match value {
            PhaseSnapshot::SetupSettlement => Self::SetupSettlement,
            PhaseSnapshot::SetupRoad { settlement } => Self::SetupRoad { settlement },
            PhaseSnapshot::PreRoll => Self::PreRoll,
            PhaseSnapshot::RollChance => Self::RollChance,
            PhaseSnapshot::Discard => Self::Discard,
            PhaseSnapshot::MoveRobber => Self::MoveRobber,
            PhaseSnapshot::ResolveSteal { victim } => Self::ResolveSteal { victim },
            PhaseSnapshot::Main => Self::Main,
            PhaseSnapshot::DevelopmentChance => Self::DevelopmentChance,
            PhaseSnapshot::TradeResponses => Self::TradeResponses,
            PhaseSnapshot::Finished => Self::Finished,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum BuildingSnapshot {
    Settlement { player: u8 },
    City { player: u8 },
}

impl From<Building> for BuildingSnapshot {
    fn from(value: Building) -> Self {
        match value {
            Building::Settlement(player) => Self::Settlement { player },
            Building::City(player) => Self::City { player },
        }
    }
}

impl From<BuildingSnapshot> for Building {
    fn from(value: BuildingSnapshot) -> Self {
        match value {
            BuildingSnapshot::Settlement { player } => Self::Settlement(player),
            BuildingSnapshot::City { player } => Self::City(player),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerStateSnapshot {
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
    policy_profile: [u8; 5],
}

impl From<&PlayerState> for PlayerStateSnapshot {
    fn from(value: &PlayerState) -> Self {
        Self {
            resources: value.resources,
            development: value.development,
            bought_development: value.bought_development,
            public_victory_points: value.public_victory_points,
            played_knights: value.played_knights,
            roads_left: value.roads_left,
            settlements_left: value.settlements_left,
            cities_left: value.cities_left,
            has_longest_road: value.has_longest_road,
            has_largest_army: value.has_largest_army,
            played_development_this_turn: value.played_development_this_turn,
            policy_profile: value.policy_profile,
        }
    }
}

impl From<PlayerStateSnapshot> for PlayerState {
    fn from(value: PlayerStateSnapshot) -> Self {
        Self {
            resources: value.resources,
            development: value.development,
            bought_development: value.bought_development,
            public_victory_points: value.public_victory_points,
            played_knights: value.played_knights,
            roads_left: value.roads_left,
            settlements_left: value.settlements_left,
            cities_left: value.cities_left,
            has_longest_road: value.has_longest_road,
            has_largest_army: value.has_largest_army,
            played_development_this_turn: value.played_development_this_turn,
            policy_profile: value.policy_profile,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeOfferSnapshot {
    creator: u8,
    recipients: u8,
    give: [u8; 5],
    receive: [u8; 5],
    accepted: u8,
    rejected: u8,
}

impl From<TradeOffer> for TradeOfferSnapshot {
    fn from(value: TradeOffer) -> Self {
        Self {
            creator: value.creator,
            recipients: value.recipients,
            give: value.give,
            receive: value.receive,
            accepted: value.accepted,
            rejected: value.rejected,
        }
    }
}

impl From<TradeOfferSnapshot> for TradeOffer {
    fn from(value: TradeOfferSnapshot) -> Self {
        Self {
            creator: value.creator,
            recipients: value.recipients,
            give: value.give,
            receive: value.receive,
            accepted: value.accepted,
            rejected: value.rejected,
        }
    }
}

fn player_trades_enabled_default() -> bool {
    true
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameStateSnapshot {
    players: Vec<PlayerStateSnapshot>,
    buildings: Vec<Option<BuildingSnapshot>>,
    roads: Vec<Option<u8>>,
    bank: [u8; 5],
    bank_is_public: bool,
    development_deck: [u8; 5],
    played_development: [u8; 5],
    robber_hex: u8,
    current_player: u8,
    phase: PhaseSnapshot,
    turn: u16,
    last_roll: u8,
    victory_target: u8,
    card_discard_limit: u8,
    friendly_robber: bool,
    setup_step: u8,
    discard_remaining: [u8; 4],
    discard_cursor: u8,
    robber_return_phase: PhaseSnapshot,
    free_roads: u8,
    domestic_trade_used: bool,
    domestic_trade_count: u8,
    #[serde(default = "player_trades_enabled_default")]
    player_trades_enabled: bool,
    #[serde(default)]
    domestic_trade_embargoes: u16,
    last_rejected_trade: Option<TradeOfferSnapshot>,
    trade: Option<TradeOfferSnapshot>,
    trade_cursor: u8,
    trade_negotiation_round: u8,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
}

impl GameStateSnapshot {
    fn capture(state: &GameState) -> Self {
        Self {
            players: state
                .players
                .iter()
                .map(PlayerStateSnapshot::from)
                .collect(),
            buildings: state
                .buildings
                .iter()
                .map(|building| building.map(BuildingSnapshot::from))
                .collect(),
            roads: state.roads.clone(),
            bank: state.bank,
            bank_is_public: state.bank_is_public,
            development_deck: state.development_deck,
            played_development: state.played_development,
            robber_hex: state.robber_hex,
            current_player: state.current_player,
            phase: state.phase.into(),
            turn: state.turn,
            last_roll: state.last_roll,
            victory_target: state.victory_target,
            card_discard_limit: state.card_discard_limit,
            friendly_robber: state.friendly_robber,
            setup_step: state.setup_step,
            discard_remaining: state.discard_remaining,
            discard_cursor: state.discard_cursor,
            robber_return_phase: state.robber_return_phase.into(),
            free_roads: state.free_roads,
            domestic_trade_used: state.domestic_trade_used,
            domestic_trade_count: state.domestic_trade_count,
            player_trades_enabled: state.player_trades_enabled,
            domestic_trade_embargoes: state.domestic_trade_embargoes,
            last_rejected_trade: state.last_rejected_trade.map(TradeOfferSnapshot::from),
            trade: state.trade.map(TradeOfferSnapshot::from),
            trade_cursor: state.trade_cursor,
            trade_negotiation_round: state.trade_negotiation_round,
            longest_road_holder: state.longest_road_holder,
            largest_army_holder: state.largest_army_holder,
        }
    }

    fn restore(self, board_seed: u64, players: u8) -> GameState {
        let mut state = GameState::standard(board_seed, players);
        state.players = self.players.into_iter().map(PlayerState::from).collect();
        state.buildings = self
            .buildings
            .into_iter()
            .map(|building| building.map(Building::from))
            .collect();
        state.roads = self.roads;
        state.bank = self.bank;
        state.bank_is_public = self.bank_is_public;
        state.development_deck = self.development_deck;
        state.played_development = self.played_development;
        state.robber_hex = self.robber_hex;
        state.current_player = self.current_player;
        state.phase = self.phase.into();
        state.turn = self.turn;
        state.last_roll = self.last_roll;
        state.victory_target = self.victory_target;
        state.card_discard_limit = self.card_discard_limit;
        state.friendly_robber = self.friendly_robber;
        state.setup_step = self.setup_step;
        state.discard_remaining = self.discard_remaining;
        state.discard_cursor = self.discard_cursor;
        state.robber_return_phase = self.robber_return_phase.into();
        state.free_roads = self.free_roads;
        state.domestic_trade_used = self.domestic_trade_used;
        state.domestic_trade_count = self.domestic_trade_count;
        state.player_trades_enabled = self.player_trades_enabled;
        state.domestic_trade_embargoes = self.domestic_trade_embargoes;
        state.last_rejected_trade = self.last_rejected_trade.map(TradeOffer::from);
        state.trade = self.trade.map(TradeOffer::from);
        state.trade_cursor = self.trade_cursor;
        state.trade_negotiation_round = self.trade_negotiation_round;
        state.longest_road_holder = self.longest_road_holder;
        state.largest_army_holder = self.largest_army_holder;
        state
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ArenaSearchProfileSnapshot {
    iterations: u32,
    rollout_actions: u16,
    max_turns: u16,
    belief_particles: usize,
    strategic_particle_limit: usize,
    maxn_depth: u8,
    maxn_branch: usize,
    maxn_nodes: u32,
    maxn_time_ms: u32,
    opening_nodes: u32,
    opening_time_ms: u32,
    trade_response_nodes: u32,
    trade_response_time_ms: u32,
    player_trades_enabled: bool,
    information_mode: String,
}

impl ArenaSearchProfileSnapshot {
    fn capture(config: &Config) -> Self {
        Self {
            iterations: config.iterations,
            rollout_actions: config.rollout_actions,
            max_turns: config.max_turns,
            belief_particles: config.belief_particles,
            strategic_particle_limit: config.strategic_particle_limit,
            maxn_depth: config.maxn_depth,
            maxn_branch: config.maxn_branch,
            maxn_nodes: config
                .maxn_nodes
                .unwrap_or_else(|| (config.iterations * 160).clamp(4_000, 80_000)),
            maxn_time_ms: config.maxn_time_ms,
            opening_nodes: config.opening_nodes,
            opening_time_ms: config.opening_time_ms,
            trade_response_nodes: config.trade_response_nodes,
            trade_response_time_ms: config.trade_response_time_ms,
            player_trades_enabled: config.player_trades_enabled,
            information_mode: information_mode(config).to_string(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChallengeSnapshot {
    schema_version: u8,
    kind: String,
    snapshot_id: String,
    board_seed: u64,
    chance_seed: u64,
    players: u8,
    state_hash: String,
    chance_rng_state: u64,
    policy_rng_states: Vec<u64>,
    target_seat: u8,
    source_block: u32,
    source_rotation: u8,
    turn: u16,
    source_engines: Vec<String>,
    source_git_sha: String,
    source_build_dirty: bool,
    engine_revision: String,
    search_profile: ArenaSearchProfileSnapshot,
    target_public_victory_points: u8,
    target_victory_points: u8,
    target_evaluator_win_value: f32,
    game_state: GameStateSnapshot,
}

#[allow(clippy::too_many_arguments)]
fn capture_challenge_snapshot(
    state: &GameState,
    board_seed: u64,
    chance_seed: u64,
    chance_rng: &SplitMix64,
    policy_rngs: &[SplitMix64],
    engines: &[Engine],
    config: &Config,
    source_block: u32,
    source_rotation: u8,
    captured_targets: &[bool; 4],
    captured_hashes: &HashSet<u64>,
) -> Option<ChallengeSnapshot> {
    if state.is_terminal()
        || state.phase != Phase::PreRoll
        || state.turn < u16::from(config.players) * 5
    {
        return None;
    }
    let target = state.current_player as usize;
    if target >= config.players as usize || captured_targets[target] {
        return None;
    }
    let state_hash = state.state_hash();
    if captured_hashes.contains(&state_hash) {
        return None;
    }
    let public_points = state.players[..config.players as usize]
        .iter()
        .map(|player| player.public_victory_points)
        .collect::<Vec<_>>();
    let target_public_points = public_points[target];
    let minimum = public_points.iter().copied().min().unwrap_or(0);
    let leader = public_points.iter().copied().max().unwrap_or(0);
    let is_last = target_public_points == minimum;
    let behind_by_two = leader.saturating_sub(target_public_points) >= 2;
    if !is_last && !behind_by_two {
        return None;
    }
    let evaluator = evaluate(state)[target];
    if evaluator > 0.25 {
        return None;
    }
    let snapshot_id = format!(
        "{}p-b{}-r{}-s{}-t{}-{:016x}",
        config.players, source_block, source_rotation, target, state.turn, state_hash
    );
    Some(ChallengeSnapshot {
        schema_version: 1,
        kind: "colonist-native-takeover-challenge".to_string(),
        snapshot_id,
        board_seed,
        chance_seed,
        players: config.players,
        state_hash: format!("{state_hash:016x}"),
        chance_rng_state: chance_rng.state(),
        policy_rng_states: policy_rngs.iter().map(SplitMix64::state).collect(),
        target_seat: target as u8,
        source_block,
        source_rotation,
        turn: state.turn,
        source_engines: engines
            .iter()
            .map(|engine| engine.as_str().to_string())
            .collect(),
        source_git_sha: config.build_git_sha.to_string(),
        source_build_dirty: config.build_dirty,
        engine_revision: ENGINE_REVISION.to_string(),
        search_profile: ArenaSearchProfileSnapshot::capture(config),
        target_public_victory_points: target_public_points,
        target_victory_points: state.players[target].victory_points(),
        target_evaluator_win_value: evaluator,
        game_state: GameStateSnapshot::capture(state),
    })
}

fn choose_action(
    engine: Engine,
    state: &GameState,
    rng: &mut SplitMix64,
    config: &Config,
    persistent_searches: &mut [Option<Mcts>],
) -> EngineChoice {
    let actions = state.legal_actions();
    let search_particles = engine
        .uses_search_information()
        .then(|| search_belief_particles(state, config))
        .flatten();
    let ordinary_nodes = config
        .maxn_nodes
        .unwrap_or_else(|| (config.iterations * 160).clamp(4_000, 80_000));
    let (nodes, time_ms) = if matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        (config.opening_nodes, config.opening_time_ms)
    } else if state.phase == Phase::TradeResponses {
        (config.trade_response_nodes, config.trade_response_time_ms)
    } else {
        (ordinary_nodes, config.maxn_time_ms)
    };
    let depth_config = BeliefDepthConfig {
        maximum_depth: config.maxn_depth,
        branch_cap: config.maxn_branch,
        maximum_nodes: nodes,
        time_budget_ms: time_ms,
        strategic_particle_limit: config.strategic_particle_limit,
    };
    match engine {
        Engine::Random => EngineChoice::simple(actions[rng.range(actions.len())].clone()),
        Engine::Weighted => EngineChoice::simple(choose_rollout_action(state, &actions, rng)),
        Engine::MaxN | Engine::AlphaBeta => {
            let paranoid = engine == Engine::AlphaBeta;
            if let Some(particles) = search_particles.as_deref() {
                let report = if paranoid {
                    search_weighted_belief_paranoid_with_config(particles, depth_config)
                } else {
                    match config.evaluator_backend {
                        EvaluatorBackend::Cpu => {
                            search_weighted_belief_maxn_with_config(particles, depth_config)
                        }
                        // The dedicated opening solver is public-state search,
                        // not the recursive MaxN leaf path. Keep it on the CPU
                        // oracle and disclose that boundary in --help/output.
                        EvaluatorBackend::Cuda
                            if matches!(
                                state.phase,
                                Phase::SetupSettlement | Phase::SetupRoad { .. }
                            ) =>
                        {
                            search_weighted_belief_maxn_with_config(particles, depth_config)
                        }
                        EvaluatorBackend::Cuda => {
                            #[cfg(feature = "cuda-exact")]
                            {
                                let evaluator = CUDA_EXACT_EVALUATOR
                                    .get()
                                    .expect("CUDA evaluator initialized before arena workers");
                                search_weighted_belief_maxn_cuda_with_config_mutex(
                                    evaluator,
                                    particles,
                                    depth_config,
                                )
                            }
                            #[cfg(not(feature = "cuda-exact"))]
                            unreachable!("CUDA backend is rejected during argument parsing")
                        }
                    }
                }
                .expect("arena belief particles share one public observation");
                EngineChoice {
                    action: report.chosen.clone().unwrap_or_else(|| actions[0].clone()),
                    root_value: Some(report.value),
                    nodes: report.nodes,
                    depth: report.depth,
                    posterior_particles: report.posterior_particles,
                    strategic_particles: report.particles,
                    deadline_reached: report.deadline_reached,
                    provenance: Some(root_provenance_trace(&report.provenance)),
                    action_values: report.actions,
                    search: None,
                }
            } else {
                let report = if paranoid {
                    search_paranoid_bounded_timed(
                        state,
                        state.actor(),
                        config.maxn_depth,
                        config.maxn_branch,
                        nodes,
                        time_ms,
                    )
                } else {
                    search_maxn_bounded_timed(
                        state,
                        config.maxn_depth,
                        config.maxn_branch,
                        nodes,
                        time_ms,
                    )
                };
                EngineChoice {
                    action: report.chosen.clone().unwrap_or_else(|| actions[0].clone()),
                    root_value: Some(report.value),
                    nodes: report.nodes,
                    depth: report.depth,
                    posterior_particles: 1,
                    strategic_particles: 1,
                    deadline_reached: report.deadline_reached,
                    action_values: report.actions,
                    provenance: None,
                    search: None,
                }
            }
        }
        Engine::Uct | Engine::Puct => {
            let actor = state.actor() as usize;
            let search = persistent_searches[actor]
                .get_or_insert_with(|| Mcts::new(SearchConfig::default(), state));
            let report = run_mcts_search(
                state,
                rng.next_u64(),
                search,
                config.iterations,
                config.rollout_actions,
                search_particles.as_deref(),
                if engine == Engine::Uct {
                    SearchMode::Uct
                } else {
                    SearchMode::Puct
                },
            );
            EngineChoice {
                action: report.chosen.clone().unwrap_or_else(|| actions[0].clone()),
                root_value: Some(report.root_value),
                nodes: report.statistics.nodes as u32,
                depth: 0,
                posterior_particles: search_particles.as_ref().map_or(1, Vec::len),
                strategic_particles: search_particles.as_ref().map_or(1, Vec::len),
                deadline_reached: false,
                action_values: Vec::new(),
                provenance: None,
                search: Some(report),
            }
        }
    }
}

fn run_mcts_search(
    state: &GameState,
    seed: u64,
    search: &mut Mcts,
    iterations: u32,
    rollout_actions: u16,
    belief_particles: Option<&[BeliefParticle]>,
    mode: SearchMode,
) -> SearchReport {
    let search_config = SearchConfig {
        iterations,
        max_nodes: (iterations as usize * 40).max(5_000),
        rollout_actions,
        tactical_depth: 14,
        tactical_nodes: 4_000,
        seed,
        mode,
        ..SearchConfig::default()
    };
    search.reconfigure(search_config);
    if let Some(particles) = belief_particles {
        search
            .search_weighted_belief(particles)
            .expect("arena belief particles share one public observation")
    } else {
        search.search(state)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProgressMechanism {
    Knight = 0,
    RoadBuilding = 1,
    YearOfPlenty = 2,
    Monopoly = 3,
}

impl ProgressMechanism {
    fn from_action(action: &Action) -> Option<Self> {
        match action {
            Action::PlayKnight { .. } => Some(Self::Knight),
            Action::PlayRoadBuilding { .. } => Some(Self::RoadBuilding),
            Action::PlayYearOfPlenty { .. } => Some(Self::YearOfPlenty),
            Action::PlayMonopoly { .. } => Some(Self::Monopoly),
            _ => None,
        }
    }

    const fn index(self) -> usize {
        self as usize
    }
}

#[derive(Clone, Debug)]
struct ProgressCounterfactual {
    mechanism: ProgressMechanism,
    state: GameState,
    diverged: bool,
    dirty_monopoly: bool,
}

fn progress_counterfactual(
    before: &GameState,
    after: &GameState,
    mechanism: ProgressMechanism,
    dirty_monopoly: bool,
) -> Option<ProgressCounterfactual> {
    if mechanism == ProgressMechanism::RoadBuilding {
        return None;
    }
    let mut counterfactual = after.clone();
    match mechanism {
        ProgressMechanism::Knight => {
            counterfactual.robber_hex = before.robber_hex;
            counterfactual.largest_army_holder = before.largest_army_holder;
            for player in 0..before.board.num_players as usize {
                counterfactual.players[player].played_knights =
                    before.players[player].played_knights;
                counterfactual.players[player].has_largest_army =
                    before.players[player].has_largest_army;
                counterfactual.players[player].public_victory_points =
                    before.players[player].public_victory_points;
            }
        }
        ProgressMechanism::YearOfPlenty | ProgressMechanism::Monopoly => {
            counterfactual.bank = before.bank;
            for player in 0..before.board.num_players as usize {
                counterfactual.players[player].resources = before.players[player].resources;
            }
        }
        ProgressMechanism::RoadBuilding => unreachable!(),
    }
    Some(ProgressCounterfactual {
        mechanism,
        state: counterfactual,
        diverged: false,
        dirty_monopoly,
    })
}

fn advance_progress_counterfactual(sequence: &mut ProgressCounterfactual, action: &Action) {
    if sequence.diverged {
        return;
    }
    let restore_resources = sequence.mechanism == ProgressMechanism::Knight
        && matches!(action, Action::ResolveSteal { .. });
    let resources_before = restore_resources.then(|| {
        sequence
            .state
            .players
            .iter()
            .map(|player| player.resources)
            .collect::<Vec<_>>()
    });
    if sequence.state.apply(action).is_err() {
        sequence.diverged = true;
        return;
    }
    if let Some(resources_before) = resources_before {
        for (player, resources) in sequence.state.players.iter_mut().zip(resources_before) {
            player.resources = resources;
        }
    }
}

fn expansion_portfolios(state: &GameState) -> [f32; 4] {
    let mut values = [0.0; 4];
    for player in 0..state.board.num_players {
        values[player as usize] = expansion_option_value(state, player).portfolio_value;
    }
    values
}

fn longest_road_lengths(state: &GameState) -> [u8; 4] {
    let mut lengths = [0; 4];
    for player in 0..state.board.num_players {
        lengths[player as usize] = state.longest_road_length(player);
    }
    lengths
}

#[derive(Clone, Debug, Default)]
struct GameMetrics {
    roads: [u32; 4],
    settlements: [u32; 4],
    cities: [u32; 4],
    development_bought: [u32; 4],
    dead_roads: [u32; 4],
    road_cuts: [u32; 4],
    award_transfers: [u32; 4],
    expansion_denial_events: [u32; 4],
    expansion_portfolio_denied: [f32; 4],
    expansion_protection_events: [u32; 4],
    progress_card_plays: [[u32; 4]; 4],
    progress_card_conversions: [[u32; 4]; 4],
    monopoly_cards_transferred: [u32; 4],
    dirty_monopoly_sequences: [u32; 4],
    one_turn_closeouts: [u32; 4],
    maritime_trades: [u32; 4],
    offers: [u32; 4],
    accepts: [u32; 4],
    counters: [u32; 4],
    end_turns_over_seven: [u32; 4],
    cards_lost_to_sevens: [u32; 4],
    robber_blocked_production: [u32; 4],
    first_decisions: [Option<RootDecisionTrace>; 4],
    native_gpu_initial_diagnostics: [Option<Value>; 4],
    decision_count: [u32; 4],
    decision_time: [Duration; 4],
    search_decision_count: [u32; 4],
    search_nodes: [u64; 4],
    search_depth: [u64; 4],
    posterior_particles: [u64; 4],
    strategic_particles: [u64; 4],
    search_deadlines: [u32; 4],
    search_action_values: [u64; 4],
    trade_value_sum: [f32; 4],
    calibration_brier_sum: [f32; 4],
    calibration_log_loss_sum: [f32; 4],
    calibration_count: [u32; 4],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrajectorySample {
    schema_version: u8,
    board_seed: u64,
    chance_seed: u64,
    block: u32,
    seat_rotation: u8,
    turn: u16,
    phase: String,
    actor: u8,
    engines: Vec<&'static str>,
    public_victory_points: Vec<u8>,
    actual_victory_points: Vec<u8>,
    production_pips: Vec<f32>,
    best_settlement_roads: Vec<u8>,
    expansion_value: Vec<f32>,
    settlements: Vec<u8>,
    cities: Vec<u8>,
    roads_built: Vec<u8>,
    development_hand: Vec<u8>,
    unplayed_action_cards: Vec<u8>,
    played_knights: Vec<u8>,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
    offers: Vec<u32>,
    accepts: Vec<u32>,
    estimated_win_value: Vec<f32>,
}

fn capture_trajectory_sample(
    state: &GameState,
    board_seed: u64,
    chance_seed: u64,
    engines: &[Engine],
    metrics: &GameMetrics,
) -> TrajectorySample {
    let players = state.board.num_players as usize;
    let win_values = evaluate(state);
    TrajectorySample {
        schema_version: 1,
        board_seed,
        chance_seed,
        block: 0,
        seat_rotation: 0,
        turn: state.turn,
        phase: format!("{:?}", state.phase),
        actor: state.actor(),
        engines: engines.iter().map(|engine| engine.as_str()).collect(),
        public_victory_points: state.players[..players]
            .iter()
            .map(|player| player.public_victory_points)
            .collect(),
        actual_victory_points: state.players[..players]
            .iter()
            .map(|player| player.victory_points())
            .collect(),
        production_pips: (0..players)
            .map(|player| production_pips(state, player as u8).iter().sum())
            .collect(),
        best_settlement_roads: (0..players)
            .map(|player| expansion_option_value(state, player as u8).roads_required)
            .collect(),
        expansion_value: (0..players)
            .map(|player| expansion_option_value(state, player as u8).portfolio_value)
            .collect(),
        settlements: state.players[..players]
            .iter()
            .map(|player| 5u8.saturating_sub(player.settlements_left))
            .collect(),
        cities: state.players[..players]
            .iter()
            .map(|player| 4u8.saturating_sub(player.cities_left))
            .collect(),
        roads_built: state.players[..players]
            .iter()
            .map(|player| 15u8.saturating_sub(player.roads_left))
            .collect(),
        development_hand: state.players[..players]
            .iter()
            .map(|player| player.development.iter().copied().sum())
            .collect(),
        unplayed_action_cards: state.players[..players]
            .iter()
            .map(|player| {
                player.development[0].saturating_sub(player.bought_development[0])
                    + player.development[2].saturating_sub(player.bought_development[2])
                    + player.development[3].saturating_sub(player.bought_development[3])
                    + player.development[4].saturating_sub(player.bought_development[4])
            })
            .collect(),
        played_knights: state.players[..players]
            .iter()
            .map(|player| player.played_knights)
            .collect(),
        longest_road_holder: state.longest_road_holder,
        largest_army_holder: state.largest_army_holder,
        offers: metrics.offers[..players].to_vec(),
        accepts: metrics.accepts[..players].to_vec(),
        estimated_win_value: win_values[..players].to_vec(),
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpertActionSample {
    key: String,
    features: Vec<f32>,
    visits: u32,
    policy: f32,
    search_value: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpertSample {
    schema_version: u8,
    state_hash: String,
    board_seed: u64,
    chance_seed: u64,
    turn: u16,
    phase: String,
    actor: u8,
    actor_victory_points: u8,
    players: u8,
    engine: String,
    state_features: Vec<f32>,
    actions: Vec<ExpertActionSample>,
    root_search_value: [f32; 4],
    winner: u8,
    outcome: [f32; 4],
    terminal: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TradeSample {
    schema_version: u8,
    state_hash: String,
    board_seed: u64,
    chance_seed: u64,
    turn: u16,
    actor: u8,
    engine: String,
    features: Vec<f32>,
    accepted: bool,
    countered: bool,
}

#[derive(Clone, Debug)]
struct GameResult {
    winner: u8,
    turns: u16,
    actions: u32,
    cutoff: bool,
    points: [u8; 4],
    ranks: [f32; 4],
    metrics: GameMetrics,
    expert_samples: Vec<ExpertSample>,
    trade_samples: Vec<TradeSample>,
    trajectory_samples: Vec<TrajectorySample>,
    challenge_snapshots: Vec<ChallengeSnapshot>,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
}

#[derive(Clone, Debug)]
struct ArenaResult {
    block: u32,
    seat: u8,
    engines: Vec<Engine>,
    game: GameResult,
}

#[derive(Default)]
struct CandidateMetrics {
    seats: u32,
    points: u64,
    ranks: f64,
    roads: u64,
    settlements: u64,
    cities: u64,
    development_bought: u64,
    dead_roads: u64,
    road_cuts: u64,
    award_transfers: u64,
    expansion_denial_events: u64,
    expansion_portfolio_denied: f64,
    expansion_protection_events: u64,
    progress_card_plays: [u64; 4],
    progress_card_conversions: [u64; 4],
    monopoly_cards_transferred: u64,
    dirty_monopoly_sequences: u64,
    one_turn_closeouts: u64,
    maritime_trades: u64,
    offers: u64,
    accepts: u64,
    counters: u64,
    end_turns_over_seven: u64,
    cards_lost_to_sevens: u64,
    robber_blocked_production: u64,
    decisions: u64,
    decision_nanos: u128,
    search_decisions: u64,
    search_nodes: u64,
    search_depth: u64,
    posterior_particles: u64,
    strategic_particles: u64,
    search_deadlines: u64,
    search_action_values: u64,
    trade_value_sum: f64,
    calibration_brier_sum: f64,
    calibration_log_loss_sum: f64,
    calibration_count: u64,
}

#[derive(Default)]
struct CheckpointEngineMetrics {
    wins: u32,
    seats: u32,
    points: u64,
    ranks: f64,
}

struct PartialArenaMetrics {
    completed_games: u32,
    cutoffs: u32,
    block_completions: Vec<u8>,
    engines: [CheckpointEngineMetrics; 6],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointEngineStanding {
    engine: &'static str,
    wins: u32,
    win_share: f64,
    seat_samples: u32,
    seat_share: f64,
    mean_rank: f64,
    mean_victory_points: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CheckpointGame {
    block: u32,
    rotation: u8,
    engines: Vec<&'static str>,
    winner: u8,
    winner_engine: &'static str,
    points: Vec<u8>,
    ranks: Vec<f32>,
    turns: u16,
    actions: u32,
    cutoff: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArenaCheckpoint {
    schema_version: u8,
    kind: &'static str,
    simulator: &'static str,
    complete: bool,
    completed_games: u32,
    terminal_games: u32,
    scheduled_games: u32,
    completion_share: f64,
    completed_matched_blocks: u32,
    players: u8,
    blocks: u32,
    candidate: &'static str,
    baseline: &'static str,
    lineup: Option<Vec<&'static str>>,
    seed: u64,
    iterations: u32,
    rollout_actions: u16,
    max_turns: u16,
    belief_particles: usize,
    strategic_particle_limit: usize,
    maxn_depth: u8,
    maxn_branch: usize,
    maxn_nodes: u32,
    maxn_time_ms: u32,
    opening_nodes: u32,
    opening_time_ms: u32,
    trade_response_nodes: u32,
    trade_response_time_ms: u32,
    player_trades_enabled: bool,
    evaluator_backend: &'static str,
    engine_revision: &'static str,
    build_git_sha: &'static str,
    build_dirty: bool,
    information_mode: &'static str,
    perfect_information_search: bool,
    validate: bool,
    cutoffs: u32,
    elapsed_ms: u128,
    engine_standings: Vec<CheckpointEngineStanding>,
    last_game: CheckpointGame,
}

impl PartialArenaMetrics {
    fn new(blocks: u32) -> Self {
        Self {
            completed_games: 0,
            cutoffs: 0,
            block_completions: vec![0; blocks as usize],
            engines: std::array::from_fn(|_| CheckpointEngineMetrics::default()),
        }
    }

    fn record(&mut self, result: &ArenaResult) {
        self.completed_games += 1;
        self.cutoffs += u32::from(result.game.cutoff);
        self.block_completions[result.block as usize] += 1;

        let winner_engine = result.engines[result.game.winner as usize];
        if !result.game.cutoff {
            self.engines[winner_engine as usize].wins += 1;
        }
        for (player, engine) in result.engines.iter().enumerate() {
            let metrics = &mut self.engines[*engine as usize];
            metrics.seats += 1;
            metrics.points += result.game.points[player] as u64;
            metrics.ranks += result.game.ranks[player] as f64;
        }
    }

    fn snapshot(
        &self,
        result: &ArenaResult,
        config: &Config,
        scheduled_games: u32,
        elapsed: Duration,
    ) -> ArenaCheckpoint {
        let games = self.completed_games.saturating_sub(self.cutoffs).max(1) as f64;
        let total_seats = (self.completed_games * u32::from(config.players)).max(1) as f64;
        let engine_standings = [
            Engine::Random,
            Engine::Weighted,
            Engine::MaxN,
            Engine::AlphaBeta,
            Engine::Uct,
            Engine::Puct,
        ]
        .into_iter()
        .map(|engine| {
            let metrics = &self.engines[engine as usize];
            let seats = metrics.seats.max(1) as f64;
            CheckpointEngineStanding {
                engine: engine.as_str(),
                wins: metrics.wins,
                win_share: metrics.wins as f64 / games,
                seat_samples: metrics.seats,
                seat_share: metrics.seats as f64 / total_seats,
                mean_rank: metrics.ranks / seats,
                mean_victory_points: metrics.points as f64 / seats,
            }
        })
        .collect();
        let winner_engine = result.engines[result.game.winner as usize];
        ArenaCheckpoint {
            schema_version: 1,
            kind: "colonist-native-arena-checkpoint",
            simulator: "colonist-native",
            complete: self.completed_games == scheduled_games,
            completed_games: self.completed_games,
            terminal_games: self.completed_games.saturating_sub(self.cutoffs),
            scheduled_games,
            completion_share: self.completed_games as f64 / scheduled_games.max(1) as f64,
            completed_matched_blocks: self
                .block_completions
                .iter()
                .filter(|games| **games == config.players)
                .count() as u32,
            players: config.players,
            blocks: config.blocks,
            candidate: config.candidate.as_str(),
            baseline: config.baseline.as_str(),
            lineup: config.lineup.as_ref().map(|lineup| {
                lineup
                    .iter()
                    .map(|engine| engine.as_str())
                    .collect::<Vec<_>>()
            }),
            seed: config.seed,
            iterations: config.iterations,
            rollout_actions: config.rollout_actions,
            max_turns: config.max_turns,
            belief_particles: config.belief_particles,
            strategic_particle_limit: config.strategic_particle_limit,
            maxn_depth: config.maxn_depth,
            maxn_branch: config.maxn_branch,
            maxn_nodes: config
                .maxn_nodes
                .unwrap_or_else(|| (config.iterations * 160).clamp(4_000, 80_000)),
            maxn_time_ms: config.maxn_time_ms,
            opening_nodes: config.opening_nodes,
            opening_time_ms: config.opening_time_ms,
            trade_response_nodes: config.trade_response_nodes,
            trade_response_time_ms: config.trade_response_time_ms,
            player_trades_enabled: config.player_trades_enabled,
            evaluator_backend: config.evaluator_backend.as_str(),
            engine_revision: ENGINE_REVISION,
            build_git_sha: config.build_git_sha,
            build_dirty: config.build_dirty,
            information_mode: information_mode(config),
            perfect_information_search: config.perfect_information_search,
            validate: config.validate,
            cutoffs: self.cutoffs,
            elapsed_ms: elapsed.as_millis(),
            engine_standings,
            last_game: CheckpointGame {
                block: result.block,
                rotation: result.seat,
                engines: result
                    .engines
                    .iter()
                    .map(|engine| engine.as_str())
                    .collect(),
                winner: result.game.winner,
                winner_engine: winner_engine.as_str(),
                points: result.game.points[..config.players as usize].to_vec(),
                ranks: result.game.ranks[..config.players as usize].to_vec(),
                turns: result.game.turns,
                actions: result.game.actions,
                cutoff: result.game.cutoff,
            },
        }
    }
}

fn write_checkpoint<W: Write>(writer: &mut W, checkpoint: &ArenaCheckpoint) {
    serde_json::to_writer(&mut *writer, checkpoint).expect("arena checkpoint must serialize");
    writer
        .write_all(b"\n")
        .expect("arena checkpoint must be writable");
    writer.flush().expect("arena checkpoint must flush");
}

fn compact_engine_metrics(metrics: &[CandidateMetrics; 6]) -> String {
    [
        Engine::Random,
        Engine::Weighted,
        Engine::MaxN,
        Engine::AlphaBeta,
        Engine::Uct,
        Engine::Puct,
    ]
    .into_iter()
    .map(|engine| {
        let metric = &metrics[engine as usize];
        let seats = metric.seats.max(1) as f64;
        let searches = metric.search_decisions.max(1) as f64;
        let progress_plays = metric.progress_card_plays.iter().sum::<u64>();
        let progress_conversions = metric.progress_card_conversions.iter().sum::<u64>();
        format!(
            "\"{}\":{{\"seatSamples\":{},\"meanRank\":{:.6},\"meanVictoryPoints\":{:.6},\"searchSamples\":{},\"meanSearchNodes\":{:.3},\"meanSearchDepth\":{:.3},\"meanPosteriorParticles\":{:.3},\"meanStrategicParticles\":{:.3},\"searchDeadlineShare\":{:.6},\"meanRootActions\":{:.3},\"roadCutsPerSeat\":{:.6},\"awardTransfersPerSeat\":{:.6},\"expansionDenialEventsPerSeat\":{:.6},\"meanExpansionPortfolioValueDenied\":{:.6},\"expansionProtectionEventsPerSeat\":{:.6},\"progressCardConversionRate\":{:.6},\"monopolyCardsTransferredPerSeat\":{:.6},\"dirtyMonopolySequencesPerSeat\":{:.6},\"oneTurnCloseoutsPerSeat\":{:.6}}}",
            engine.as_str(),
            metric.seats,
            metric.ranks / seats,
            metric.points as f64 / seats,
            metric.search_decisions,
            metric.search_nodes as f64 / searches,
            metric.search_depth as f64 / searches,
            metric.posterior_particles as f64 / searches,
            metric.strategic_particles as f64 / searches,
            metric.search_deadlines as f64 / searches,
            metric.search_action_values as f64 / searches,
            metric.road_cuts as f64 / seats,
            metric.award_transfers as f64 / seats,
            metric.expansion_denial_events as f64 / seats,
            metric.expansion_portfolio_denied / seats,
            metric.expansion_protection_events as f64 / seats,
            progress_conversions as f64 / progress_plays.max(1) as f64,
            metric.monopoly_cards_transferred as f64 / seats,
            metric.dirty_monopoly_sequences as f64 / seats,
            metric.one_turn_closeouts as f64 / seats,
        )
    })
    .collect::<Vec<_>>()
    .join(",")
}

fn initialized_game(
    board_seed: u64,
    chance_seed: u64,
    players: u8,
) -> (GameState, SplitMix64, Vec<SplitMix64>) {
    let mut state = GameState::standard(board_seed, players);
    // Rotate explicit bounded-rational styles independently of engine family.
    // Paired seat rotations therefore test road-heavy, development-heavy,
    // trade-happy, and trade-resistant behavior instead of overfitting every
    // search policy to one perfectly symmetric opponent model.
    const POLICY_PROFILES: [[u8; 5]; 6] = [
        [51, 51, 51, 51, 51],
        [45, 92, 28, 38, 45],
        [45, 30, 92, 35, 45],
        [45, 45, 45, 96, 25],
        [45, 45, 45, 15, 96],
        [70, 55, 55, 55, 55],
    ];
    for player in 0..players as usize {
        let profile = ((board_seed ^ (player as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15))
            % POLICY_PROFILES.len() as u64) as usize;
        state.players[player].policy_profile = POLICY_PROFILES[profile];
    }
    let chance_rng = SplitMix64::new(chance_seed);
    let policy_rngs = (0..players)
        .map(|player| SplitMix64::new(chance_seed ^ ((player as u64 + 1) * 0x9e37_79b9)))
        .collect::<Vec<_>>();
    (state, chance_rng, policy_rngs)
}

fn play_game(
    board_seed: u64,
    chance_seed: u64,
    engines: &[Engine],
    config: &Config,
    source: Option<(u32, u8)>,
) -> GameResult {
    let (mut state, chance_rng, policy_rngs) =
        initialized_game(board_seed, chance_seed, config.players);
    state.player_trades_enabled = config.player_trades_enabled;
    play_game_from_state(
        board_seed,
        chance_seed,
        engines,
        config,
        source,
        state,
        chance_rng,
        policy_rngs,
        None,
    )
}

#[allow(clippy::too_many_arguments)]
fn play_game_from_state(
    board_seed: u64,
    chance_seed: u64,
    engines: &[Engine],
    config: &Config,
    source: Option<(u32, u8)>,
    mut state: GameState,
    mut chance_rng: SplitMix64,
    mut policy_rngs: Vec<SplitMix64>,
    mut native_gpu: Option<&mut NativeGpuClient>,
) -> GameResult {
    state.player_trades_enabled = config.player_trades_enabled;
    let mut actions = 0u32;
    let mut metrics = GameMetrics::default();
    let mut progress_sequence = None::<ProgressCounterfactual>;
    let mut trade_resource_balance = [[0i32; 5]; 4];
    let mut calibration = Vec::<(u8, f32)>::new();
    let mut expert_samples = Vec::<ExpertSample>::new();
    let mut trade_samples = Vec::<TradeSample>::new();
    let mut trajectory_samples = Vec::<TrajectorySample>::new();
    let mut last_trajectory_turn = u16::MAX;
    let mut persistent_searches = (0..config.players)
        .map(|_| None)
        .collect::<Vec<Option<Mcts>>>();
    let mut expert_searches = (0..config.players)
        .map(|_| None)
        .collect::<Vec<Option<Mcts>>>();
    let mut challenge_snapshots = Vec::<ChallengeSnapshot>::new();
    let mut captured_challenge_targets = [false; 4];
    let mut captured_challenge_hashes = HashSet::<u64>::new();
    while !state.is_terminal() && state.turn <= config.max_turns {
        if config.challenge_output.is_some()
            && let Some((source_block, source_rotation)) = source
            && let Some(snapshot) = capture_challenge_snapshot(
                &state,
                board_seed,
                chance_seed,
                &chance_rng,
                &policy_rngs,
                engines,
                config,
                source_block,
                source_rotation,
                &captured_challenge_targets,
                &captured_challenge_hashes,
            )
        {
            let target = snapshot.target_seat as usize;
            let hash = state.state_hash();
            captured_challenge_targets[target] = true;
            captured_challenge_hashes.insert(hash);
            challenge_snapshots.push(snapshot);
        }
        if config.trajectory_output.is_some()
            && state.turn != last_trajectory_turn
            && matches!(
                state.phase,
                Phase::PreRoll | Phase::Main | Phase::SetupSettlement
            )
        {
            trajectory_samples.push(capture_trajectory_sample(
                &state,
                board_seed,
                chance_seed,
                engines,
                &metrics,
            ));
            last_trajectory_turn = state.turn;
        }
        let action = if state.node_kind() == NodeKind::Chance {
            state
                .sample_chance(&mut chance_rng)
                .expect("chance state must have an outcome")
        } else {
            let actor = state.actor() as usize;
            let started = Instant::now();
            let legal_actions = state.legal_actions();
            let takeover_target = config.takeover_target_seat == Some(actor as u8);
            let first_root_pending = takeover_target && metrics.first_decisions[actor].is_none();
            let choice = if first_root_pending && legal_actions.len() == 1 {
                // Forced protocol steps are not strategic roots. Execute them
                // directly so native-GPU takeover never falls through to CPU
                // MaxN before the first real fork.
                EngineChoice::simple(legal_actions[0].clone())
            } else if first_root_pending && config.takeover_native_gpu_host.is_some() {
                let decision = native_gpu
                    .as_deref_mut()
                    .expect("native GPU takeover configured its host")
                    .analyze(&state, actor, config)
                    .unwrap_or_else(|error| {
                        panic!(
                            "native GPU takeover failed at state {:016x}: {error}",
                            state.state_hash()
                        )
                    });
                metrics.first_decisions[actor] = Some(decision.root_trace.clone());
                metrics.native_gpu_initial_diagnostics[actor] = Some(decision.response.clone());
                metrics.search_decision_count[actor] += 1;
                metrics.search_nodes[actor] = metrics.search_nodes[actor]
                    .saturating_add(decision.rollouts.saturating_mul(decision.rollout_steps));
                metrics.posterior_particles[actor] = metrics.posterior_particles[actor]
                    .saturating_add(decision.posterior_particles as u64);
                metrics.strategic_particles[actor] = metrics.strategic_particles[actor]
                    .saturating_add(decision.posterior_particles as u64);
                metrics.search_deadlines[actor] += u32::from(decision.deadline_reached);
                metrics.search_action_values[actor] = metrics.search_action_values[actor]
                    .saturating_add(decision.action_count as u64);
                EngineChoice::simple(decision.action)
            } else {
                let decision_engine = if config.takeover_random_followup
                    && takeover_target
                    && metrics.first_decisions[actor].is_some()
                {
                    Engine::Random
                } else {
                    engines[actor]
                };
                choose_action(
                    decision_engine,
                    &state,
                    &mut policy_rngs[actor],
                    config,
                    &mut persistent_searches,
                )
            };
            metrics.decision_time[actor] += started.elapsed();
            if metrics.first_decisions[actor].is_none() && choice.action_values.len() > 1 {
                metrics.first_decisions[actor] = Some(choice.root_trace(actor));
            }
            metrics.decision_count[actor] += 1;
            if choice.root_value.is_some() {
                metrics.search_decision_count[actor] += 1;
                metrics.search_nodes[actor] += u64::from(choice.nodes);
                metrics.search_depth[actor] += u64::from(choice.depth);
                metrics.posterior_particles[actor] += choice.posterior_particles as u64;
                metrics.strategic_particles[actor] += choice.strategic_particles as u64;
                metrics.search_deadlines[actor] += u32::from(choice.deadline_reached);
                metrics.search_action_values[actor] += choice.action_values.len() as u64;
            }
            let should_record_expert = config.expert_output.is_some()
                && metrics.decision_count[actor] % config.expert_stride == 0;
            let teacher_report = if should_record_expert && config.expert_iterations > 0 {
                let teacher_particles = search_belief_particles(&state, config);
                let search = expert_searches[actor]
                    .get_or_insert_with(|| Mcts::new(SearchConfig::default(), &state));
                Some(run_mcts_search(
                    &state,
                    state.observation_hash(actor as u8) ^ config.seed ^ 0x4558_5045_5254_5055,
                    search,
                    config.expert_iterations,
                    config.expert_rollout_actions.max(config.rollout_actions),
                    teacher_particles.as_deref(),
                    SearchMode::Puct,
                ))
            } else {
                None
            };
            if state.phase == Phase::PreRoll
                && let Some(root_value) = choice.root_value
            {
                calibration.push((actor as u8, root_value[actor]));
            }
            if should_record_expert
                && let Some(report) = teacher_report.as_ref().or(choice.search.as_ref())
            {
                let graph = encode_heterogeneous_graph(&state, actor as u8, false);
                let state_features = pool_heterogeneous_graph(&graph, actor as u8).to_vec();
                let visit_total = report
                    .actions
                    .iter()
                    .map(|sample| sample.visits)
                    .sum::<u32>()
                    .max(1);
                let actions = report
                    .actions
                    .iter()
                    .map(|sample| ExpertActionSample {
                        key: format!("{:?}", sample.action),
                        features: encode_action(&state, &sample.action).to_vec(),
                        visits: sample.visits,
                        policy: sample.visits as f32 / visit_total as f32,
                        search_value: sample.value,
                    })
                    .collect();
                expert_samples.push(ExpertSample {
                    schema_version: 1,
                    state_hash: format!("{:016x}", state.observation_hash(actor as u8),),
                    board_seed,
                    chance_seed,
                    turn: state.turn,
                    phase: format!("{:?}", state.phase),
                    actor: actor as u8,
                    actor_victory_points: state.players[actor].victory_points(),
                    players: config.players,
                    engine: if teacher_report.is_some() {
                        "puct-teacher".to_string()
                    } else {
                        engines[actor].as_str().to_string()
                    },
                    state_features,
                    actions,
                    root_search_value: report.root_value,
                    winner: u8::MAX,
                    outcome: [0.0; 4],
                    terminal: false,
                });
            }
            if config.trade_output.is_some()
                && matches!(
                    choice.action,
                    Action::RespondTrade { .. } | Action::CounterTrade { .. }
                )
                && let Some(features) = trade_acceptance_features(&state, actor as u8)
            {
                trade_samples.push(TradeSample {
                    schema_version: 1,
                    state_hash: format!("{:016x}", state.observation_hash(actor as u8)),
                    board_seed,
                    chance_seed,
                    turn: state.turn,
                    actor: actor as u8,
                    engine: engines[actor].as_str().to_string(),
                    features: features.to_vec(),
                    accepted: matches!(choice.action, Action::RespondTrade { accept: true }),
                    countered: matches!(choice.action, Action::CounterTrade { .. }),
                });
            }
            choice.action
        };
        let actor = state.actor() as usize;
        let turn_owner = state.current_player as usize;
        let spatial_action = matches!(
            action,
            Action::BuildRoad { .. }
                | Action::BuildSettlement { .. }
                | Action::PlayRoadBuilding { .. }
        );
        let road_lengths_before = spatial_action.then(|| longest_road_lengths(&state));
        let expansion_before = spatial_action.then(|| expansion_portfolios(&state));
        let expansion_protection =
            matches!(
                action,
                Action::BuildRoad { .. } | Action::BuildSettlement { .. }
            ) && compute_spatial_root_impacts(&state, actor as u8, std::slice::from_ref(&action))
                .actions
                .first()
                .is_some_and(|impact| {
                    impact.promotion == Some(RootPromotionReason::CriticalExpansionProtection)
                });
        let longest_road_holder_before = state.longest_road_holder;
        let largest_army_holder_before = state.largest_army_holder;
        let progress_mechanism = ProgressMechanism::from_action(&action);
        let progress_before = progress_mechanism.map(|_| state.clone());
        let monopoly_transfer = if let Action::PlayMonopoly { resource } = &action {
            (0..state.board.num_players as usize)
                .filter(|player| *player != turn_owner)
                .map(|player| u32::from(state.players[player].resources[resource.index()]))
                .sum::<u32>()
        } else {
            0
        };
        let dirty_monopoly_reclaim = if let Action::PlayMonopoly { resource } = &action {
            actor == turn_owner && trade_resource_balance[turn_owner][resource.index()] > 0
        } else {
            false
        };
        let confirmed_trade_flow = if let Action::ConfirmTrade { partner } = &action {
            state.trade.and_then(|trade| {
                let turn_owner = state.current_player;
                if turn_owner == trade.creator {
                    Some((turn_owner as usize, trade.give, trade.receive))
                } else if turn_owner == *partner {
                    Some((turn_owner as usize, trade.receive, trade.give))
                } else {
                    None
                }
            })
        } else {
            None
        };
        match &action {
            Action::PlaceRoad { .. } | Action::BuildRoad { .. } => {
                metrics.roads[actor] += 1;
                if matches!(action, Action::BuildRoad { .. })
                    && action_prior(&state, &action, actor as u8) <= 0.20
                {
                    metrics.dead_roads[actor] += 1;
                }
            }
            Action::PlaceSettlement { .. } | Action::BuildSettlement { .. } => {
                metrics.settlements[actor] += 1;
            }
            Action::BuildCity { .. } => metrics.cities[actor] += 1,
            Action::BuyDevelopment => metrics.development_bought[actor] += 1,
            Action::MaritimeTrade { .. } => metrics.maritime_trades[actor] += 1,
            Action::OfferTrade { .. } => metrics.offers[actor] += 1,
            Action::RespondTrade { accept: true } => metrics.accepts[actor] += 1,
            Action::CounterTrade { .. } => metrics.counters[actor] += 1,
            Action::EndTurn if state.players[actor].resource_total() > 7 => {
                metrics.end_turns_over_seven[actor] += 1;
            }
            Action::Discard { cards } => {
                metrics.cards_lost_to_sevens[actor] +=
                    cards.iter().copied().map(u32::from).sum::<u32>();
            }
            Action::ResolveRoll { value } if *value != 7 => {
                let robber = state.robber_hex as usize;
                if state.board.hexes[robber].number == *value {
                    for (vertex, building) in state.buildings.iter().enumerate() {
                        let Some(building) = building else {
                            continue;
                        };
                        if state.board.vertices[vertex]
                            .adjacent_hexes
                            .contains(&(robber as u8))
                        {
                            metrics.robber_blocked_production[building.player() as usize] +=
                                building.production_multiplier() as u32;
                        }
                    }
                }
            }
            _ => {}
        }
        let trade_actor = if matches!(action, Action::ConfirmTrade { .. }) {
            Some(actor as u8)
        } else {
            None
        };
        let trade_before = trade_actor.map(|player| strategic_utility(&state, player));
        state.apply(&action).unwrap_or_else(|error| {
            panic!(
                "illegal arena action {action:?} at hash {:016x}: {error}",
                state.state_hash()
            )
        });
        if let (Some(player), Some(before)) = (trade_actor, trade_before) {
            metrics.trade_value_sum[player as usize] += strategic_utility(&state, player) - before;
        }
        if let Some((player, outgoing, incoming)) = confirmed_trade_flow {
            for resource in 0..5 {
                trade_resource_balance[player][resource] +=
                    i32::from(outgoing[resource]) - i32::from(incoming[resource]);
            }
        }

        let actor_award_acquired = (longest_road_holder_before != Some(turn_owner as u8)
            && state.longest_road_holder == Some(turn_owner as u8))
            || (largest_army_holder_before != Some(turn_owner as u8)
                && state.largest_army_holder == Some(turn_owner as u8));
        let mut award_transfer_count = 0u32;
        for (before, after) in [
            (longest_road_holder_before, state.longest_road_holder),
            (largest_army_holder_before, state.largest_army_holder),
        ] {
            if let (Some(before), Some(after)) = (before, after)
                && before != after
            {
                award_transfer_count += 1;
            }
        }
        metrics.award_transfers[actor] += award_transfer_count;

        if let Some(before) = road_lengths_before {
            let after = longest_road_lengths(&state);
            if (0..state.board.num_players as usize)
                .filter(|player| *player != actor)
                .any(|player| after[player] < before[player])
            {
                metrics.road_cuts[actor] += 1;
            }
        }
        if let Some(before) = expansion_before {
            let after = expansion_portfolios(&state);
            let denied = (0..state.board.num_players as usize)
                .filter(|player| *player != actor)
                .map(|player| (before[player] - after[player]).max(0.0))
                .fold(0.0, f32::max);
            if denied > f32::EPSILON {
                metrics.expansion_denial_events[actor] += 1;
                metrics.expansion_portfolio_denied[actor] += denied;
            }
        }
        if expansion_protection {
            metrics.expansion_protection_events[actor] += 1;
        }

        if monopoly_transfer > 0 {
            metrics.monopoly_cards_transferred[turn_owner] += monopoly_transfer;
        }

        let material_progress_conversion = matches!(
            action,
            Action::BuildRoad { .. } | Action::BuildSettlement { .. } | Action::BuildCity { .. }
        ) || actor_award_acquired
            || state.winner() == Some(turn_owner as u8);
        if let Some(mechanism) = progress_mechanism {
            metrics.progress_card_plays[turn_owner][mechanism.index()] += 1;
            let immediate_conversion = mechanism == ProgressMechanism::RoadBuilding
                || (mechanism == ProgressMechanism::Knight && material_progress_conversion);
            if immediate_conversion {
                metrics.progress_card_conversions[turn_owner][mechanism.index()] += 1;
                progress_sequence = None;
            } else {
                progress_sequence = progress_counterfactual(
                    progress_before
                        .as_ref()
                        .expect("progress action captured its pre-transition state"),
                    &state,
                    mechanism,
                    mechanism == ProgressMechanism::Monopoly
                        && dirty_monopoly_reclaim
                        && monopoly_transfer > 0,
                );
            }
        } else if let Some(sequence) = progress_sequence.as_mut() {
            advance_progress_counterfactual(sequence, &action);
            if material_progress_conversion && sequence.diverged {
                metrics.progress_card_conversions[turn_owner][sequence.mechanism.index()] += 1;
                if sequence.mechanism == ProgressMechanism::Monopoly && sequence.dirty_monopoly {
                    metrics.dirty_monopoly_sequences[turn_owner] += 1;
                }
                progress_sequence = None;
            }
        }
        if state.winner() == Some(turn_owner as u8) {
            metrics.one_turn_closeouts[turn_owner] += 1;
        }
        if matches!(action, Action::EndTurn) {
            progress_sequence = None;
            trade_resource_balance[turn_owner] = [0; 5];
        }
        actions += 1;
        if config.validate {
            state.validate().unwrap_or_else(|error| {
                panic!(
                    "invariant failed after {action:?} at hash {:016x}: {error}",
                    state.state_hash()
                )
            });
        }
        if config.takeover_root_only
            && config
                .takeover_target_seat
                .is_some_and(|seat| metrics.first_decisions[seat as usize].is_some())
        {
            break;
        }
    }
    let terminal_winner = state.winner();
    let winner = terminal_winner.unwrap_or_else(|| {
        let values = evaluate(&state);
        values
            .iter()
            .take(config.players as usize)
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(player, _)| player as u8)
            .unwrap_or(0)
    });
    if terminal_winner.is_some() {
        for (player, prediction) in calibration {
            let outcome = f32::from(player == winner);
            let clipped = prediction.clamp(1e-6, 1.0 - 1e-6);
            metrics.calibration_brier_sum[player as usize] += (prediction - outcome).powi(2);
            metrics.calibration_log_loss_sum[player as usize] +=
                -(outcome * clipped.ln() + (1.0 - outcome) * (1.0 - clipped).ln());
            metrics.calibration_count[player as usize] += 1;
        }
    }
    for sample in &mut expert_samples {
        sample.winner = winner;
        sample.terminal = terminal_winner.is_some();
        if terminal_winner.is_some() {
            sample.outcome[winner as usize] = 1.0;
        }
    }
    let mut points = [0u8; 4];
    let mut ranks = [0.0f32; 4];
    for (player, point) in points.iter_mut().enumerate().take(config.players as usize) {
        *point = state.players[player].victory_points();
    }
    for player in 0..config.players as usize {
        let better = points
            .iter()
            .take(config.players as usize)
            .filter(|candidate_points| **candidate_points > points[player])
            .count();
        let tied = points
            .iter()
            .take(config.players as usize)
            .filter(|candidate_points| **candidate_points == points[player])
            .count();
        ranks[player] = 1.0 + better as f32 + (tied.saturating_sub(1)) as f32 / 2.0;
    }
    GameResult {
        winner,
        turns: state.turn,
        actions,
        cutoff: terminal_winner.is_none(),
        points,
        ranks,
        metrics,
        expert_samples,
        trade_samples,
        trajectory_samples,
        challenge_snapshots,
        longest_road_holder: state.longest_road_holder,
        largest_army_holder: state.largest_army_holder,
    }
}

fn percentile(sorted: &[f32], probability: f32) -> f32 {
    let index = ((sorted.len() - 1) as f32 * probability).round() as usize;
    sorted[index]
}

fn bootstrap_interval(block_scores: &[f32], seed: u64) -> (f32, f32) {
    if block_scores.is_empty() {
        return (0.0, 0.0);
    }
    let mut rng = SplitMix64::new(seed);
    let mut samples = Vec::with_capacity(2_000);
    for _ in 0..2_000 {
        let mean = (0..block_scores.len())
            .map(|_| block_scores[rng.range(block_scores.len())])
            .sum::<f32>()
            / block_scores.len() as f32;
        samples.push(mean);
    }
    samples.sort_by(f32::total_cmp);
    (percentile(&samples, 0.025), percentile(&samples, 0.975))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TakeoverOutcome {
    schema_version: u8,
    kind: &'static str,
    snapshot_id: String,
    state_hash: String,
    board_seed: u64,
    chance_seed: u64,
    chance_rng_state: u64,
    policy_rng_states: Vec<u64>,
    players: u8,
    target_seat: u8,
    source_block: u32,
    source_rotation: u8,
    source_turn: u16,
    source_engine: String,
    arm: String,
    engine_revision: &'static str,
    build_git_sha: &'static str,
    build_dirty: bool,
    search_profile: ArenaSearchProfileSnapshot,
    decision_backend: &'static str,
    decision_revision: Option<String>,
    native_gpu_identity: Option<Value>,
    continuation_mode: &'static str,
    initial_root: Option<RootDecisionTrace>,
    native_gpu_initial_diagnostics: Option<Value>,
    terminal: bool,
    winner: u8,
    target_win: bool,
    final_rank: f32,
    final_victory_points: u8,
    best_opponent_victory_points: u8,
    victory_point_margin: i16,
    victory_points_gained: u8,
    final_turn: u16,
    turns_elapsed: u16,
    actions: u32,
    cutoff: bool,
    longest_road_acquired: bool,
    largest_army_acquired: bool,
    road_cuts: u32,
    award_transfers: u32,
    expansion_denial_events: u32,
    expansion_portfolio_denied: f32,
    expansion_protection_events: u32,
    progress_card_plays: [u32; 4],
    progress_card_conversions: [u32; 4],
    monopoly_cards_transferred: u32,
    dirty_monopoly_sequences: u32,
    one_turn_closeouts: u32,
    roads: u32,
    settlements: u32,
    cities: u32,
    development_cards_bought: u32,
    domestic_offers: u32,
    trade_accepts: u32,
    counters: u32,
    cards_lost_to_sevens: u32,
    mean_decision_latency_ms: f64,
    search_decisions: u32,
    mean_search_nodes: f64,
    mean_search_depth: f64,
    mean_posterior_particles: f64,
    mean_strategic_particles: f64,
    search_deadline_share: f64,
    illegal_action_failures: u32,
    protocol_failures: u32,
}

fn run_takeover_mode(config: &Config) {
    let input_path = config
        .takeover_input
        .as_ref()
        .expect("takeover input is present in takeover mode");
    let output_path = config
        .takeover_output
        .as_ref()
        .expect("takeover output is present in takeover mode");
    let input = BufReader::new(
        File::open(input_path)
            .unwrap_or_else(|error| panic!("failed to open takeover input {input_path}: {error}")),
    );
    let mut output =
        BufWriter::new(File::create(output_path).unwrap_or_else(|error| {
            panic!("failed to create takeover output {output_path}: {error}")
        }));
    let mut native_gpu = config.takeover_native_gpu_host.as_deref().map(|path| {
        NativeGpuClient::spawn(path).unwrap_or_else(|error| {
            panic!("failed to initialize takeover native GPU host: {error}")
        })
    });
    for (line_index, line) in input.lines().enumerate() {
        let line = line.unwrap_or_else(|error| {
            panic!(
                "failed to read takeover input line {}: {error}",
                line_index + 1
            )
        });
        if line.trim().is_empty() {
            continue;
        }
        let snapshot: ChallengeSnapshot = serde_json::from_str(&line).unwrap_or_else(|error| {
            panic!(
                "invalid takeover snapshot on line {}: {error}",
                line_index + 1
            )
        });
        let mut replay_config = config.clone();
        replay_config.players = snapshot.players;
        // Arena board seeds are source_seed + block * 2. Belief sampling also
        // mixes config.seed, so restore the exact source seed for common-random
        // continuation rather than using the takeover command's default seed.
        replay_config.seed = snapshot
            .board_seed
            .wrapping_sub(u64::from(snapshot.source_block) * 2);
        replay_config.challenge_output = None;
        replay_config.takeover_input = None;
        replay_config.takeover_output = None;
        replay_config.takeover_target_seat = Some(snapshot.target_seat);
        let state = snapshot
            .game_state
            .clone()
            .restore(snapshot.board_seed, snapshot.players);
        state.validate().unwrap_or_else(|error| {
            panic!(
                "restored snapshot {} is invalid: {error}",
                snapshot.snapshot_id
            )
        });
        let expected_hash = u64::from_str_radix(&snapshot.state_hash, 16)
            .expect("snapshot state hash must be hexadecimal");
        assert_eq!(
            state.state_hash(),
            expected_hash,
            "restored snapshot {} changed state hash",
            snapshot.snapshot_id
        );
        assert_eq!(
            state.phase,
            Phase::PreRoll,
            "takeover snapshot {} is not a stable PreRoll boundary",
            snapshot.snapshot_id
        );
        let chance_rng = SplitMix64::from_state(snapshot.chance_rng_state);
        let policy_rngs = snapshot
            .policy_rng_states
            .iter()
            .copied()
            .map(SplitMix64::from_state)
            .collect::<Vec<_>>();
        assert_eq!(policy_rngs.len(), snapshot.players as usize);
        let mut engines = snapshot
            .source_engines
            .iter()
            .map(|engine| {
                Engine::parse(engine).unwrap_or_else(|| panic!("unknown snapshot engine {engine}"))
            })
            .collect::<Vec<_>>();
        assert_eq!(engines.len(), snapshot.players as usize);
        let target = snapshot.target_seat as usize;
        let source_engine = engines[target].as_str().to_string();
        let arm = if config.takeover_native_gpu_host.is_some() {
            "native-gpu".to_string()
        } else if let Some(engine) = config.takeover_engine {
            engines[target] = engine;
            engine.as_str().to_string()
        } else {
            "control".to_string()
        };
        let initial_longest_road_holder = snapshot.game_state.longest_road_holder;
        let initial_largest_army_holder = snapshot.game_state.largest_army_holder;
        let result = play_game_from_state(
            snapshot.board_seed,
            snapshot.chance_seed,
            &engines,
            &replay_config,
            None,
            state,
            chance_rng,
            policy_rngs,
            native_gpu.as_mut(),
        );
        let metrics = &result.metrics;
        let decisions = metrics.decision_count[target].max(1) as f64;
        let searches = metrics.search_decision_count[target].max(1) as f64;
        let best_opponent_victory_points = result.points[..snapshot.players as usize]
            .iter()
            .enumerate()
            .filter(|(player, _)| *player != target)
            .map(|(_, points)| *points)
            .max()
            .unwrap_or(0);
        let victory_point_margin =
            i16::from(result.points[target]) - i16::from(best_opponent_victory_points);
        let outcome = TakeoverOutcome {
            schema_version: 3,
            kind: "colonist-native-takeover-outcome",
            snapshot_id: snapshot.snapshot_id,
            state_hash: snapshot.state_hash,
            board_seed: snapshot.board_seed,
            chance_seed: snapshot.chance_seed,
            chance_rng_state: snapshot.chance_rng_state,
            policy_rng_states: snapshot.policy_rng_states.clone(),
            players: snapshot.players,
            target_seat: snapshot.target_seat,
            source_block: snapshot.source_block,
            source_rotation: snapshot.source_rotation,
            source_turn: snapshot.turn,
            source_engine,
            arm,
            engine_revision: ENGINE_REVISION,
            build_git_sha: replay_config.build_git_sha,
            build_dirty: replay_config.build_dirty,
            search_profile: ArenaSearchProfileSnapshot::capture(&replay_config),
            decision_backend: if config.takeover_native_gpu_host.is_some() {
                "native-gpu"
            } else {
                "arena"
            },
            decision_revision: config.takeover_native_gpu_revision.clone(),
            native_gpu_identity: native_gpu.as_ref().map(|client| client.identity().clone()),
            continuation_mode: if config.takeover_native_gpu_host.is_some()
                && config.takeover_random_followup
            {
                "native-gpu-first-root-then-random"
            } else if config.takeover_random_followup {
                "first-root-then-random"
            } else {
                "target-engine"
            },
            initial_root: metrics.first_decisions[target].clone(),
            native_gpu_initial_diagnostics: metrics.native_gpu_initial_diagnostics[target].clone(),
            terminal: !result.cutoff,
            winner: result.winner,
            target_win: !result.cutoff && result.winner as usize == target,
            final_rank: result.ranks[target],
            final_victory_points: result.points[target],
            best_opponent_victory_points,
            victory_point_margin,
            victory_points_gained: result.points[target]
                .saturating_sub(snapshot.target_victory_points),
            final_turn: result.turns,
            turns_elapsed: result.turns.saturating_sub(snapshot.turn),
            actions: result.actions,
            cutoff: result.cutoff,
            longest_road_acquired: result.longest_road_holder == Some(snapshot.target_seat)
                && initial_longest_road_holder != Some(snapshot.target_seat),
            largest_army_acquired: result.largest_army_holder == Some(snapshot.target_seat)
                && initial_largest_army_holder != Some(snapshot.target_seat),
            road_cuts: metrics.road_cuts[target],
            award_transfers: metrics.award_transfers[target],
            expansion_denial_events: metrics.expansion_denial_events[target],
            expansion_portfolio_denied: metrics.expansion_portfolio_denied[target],
            expansion_protection_events: metrics.expansion_protection_events[target],
            progress_card_plays: metrics.progress_card_plays[target],
            progress_card_conversions: metrics.progress_card_conversions[target],
            monopoly_cards_transferred: metrics.monopoly_cards_transferred[target],
            dirty_monopoly_sequences: metrics.dirty_monopoly_sequences[target],
            one_turn_closeouts: metrics.one_turn_closeouts[target],
            roads: metrics.roads[target],
            settlements: metrics.settlements[target],
            cities: metrics.cities[target],
            development_cards_bought: metrics.development_bought[target],
            domestic_offers: metrics.offers[target],
            trade_accepts: metrics.accepts[target],
            counters: metrics.counters[target],
            cards_lost_to_sevens: metrics.cards_lost_to_sevens[target],
            mean_decision_latency_ms: metrics.decision_time[target].as_nanos() as f64
                / decisions
                / 1_000_000.0,
            search_decisions: metrics.search_decision_count[target],
            mean_search_nodes: metrics.search_nodes[target] as f64 / searches,
            mean_search_depth: metrics.search_depth[target] as f64 / searches,
            mean_posterior_particles: metrics.posterior_particles[target] as f64 / searches,
            mean_strategic_particles: metrics.strategic_particles[target] as f64 / searches,
            search_deadline_share: metrics.search_deadlines[target] as f64 / searches,
            illegal_action_failures: 0,
            protocol_failures: 0,
        };
        serde_json::to_writer(&mut output, &outcome).expect("takeover outcome must serialize");
        output
            .write_all(b"\n")
            .expect("takeover outcome must be writable");
        output.flush().expect("takeover outcome must flush");
    }
}

fn main() {
    let config = parse_config();
    #[cfg(feature = "cuda-exact")]
    if config.evaluator_backend == EvaluatorBackend::Cuda {
        let evaluator = CudaExactEvaluator::new().unwrap_or_else(|error| {
            panic!("failed to initialize requested CUDA evaluator: {error}")
        });
        let device = evaluator.device_identity();
        eprintln!(
            "evaluator={} gpu={} ordinal={} compute_capability={}.{}",
            evaluator.backend(),
            device.name,
            device.ordinal,
            device.compute_capability.0,
            device.compute_capability.1,
        );
        if CUDA_EXACT_EVALUATOR.set(Mutex::new(evaluator)).is_err() {
            panic!("CUDA evaluator initialized more than once");
        }
    }
    if config.takeover_input.is_some() {
        run_takeover_mode(&config);
        return;
    }
    let started = Instant::now();
    let mut total_games = 0u32;
    let mut candidate_wins = 0u32;
    let mut cutoffs = 0u32;
    let mut turns = 0u64;
    let mut actions = 0u64;
    let mut block_scores = Vec::with_capacity(config.blocks as usize);
    let mut engine_metrics: [CandidateMetrics; 6] =
        std::array::from_fn(|_| CandidateMetrics::default());

    if !config.json {
        let lineup = config.lineup.as_ref().map_or_else(
            || "none".to_string(),
            |engines| {
                engines
                    .iter()
                    .map(|engine| engine.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            },
        );
        println!(
            "arena candidate={} baseline={} lineup={} information={} evaluator={} player_trades={} players={} blocks={} iterations={} threads={} seed={}",
            config.candidate.as_str(),
            config.baseline.as_str(),
            lineup,
            information_mode(&config),
            config.evaluator_backend.as_str(),
            config.player_trades_enabled,
            config.players,
            config.blocks,
            config.iterations,
            config.threads,
            config.seed
        );
    }
    let jobs = Arc::new(
        (0..config.blocks)
            .flat_map(|block| (0..config.players).map(move |seat| (block, seat)))
            .collect::<Vec<_>>(),
    );
    let scheduled_games = jobs.len() as u32;
    let mut checkpoint_writer = config.checkpoint_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create checkpoint {path}: {error}")),
        )
    });
    let mut partial_metrics = PartialArenaMetrics::new(config.blocks);
    let next_job = Arc::new(AtomicUsize::new(0));
    let shared_config = Arc::new(config.clone());
    let (sender, receiver) = mpsc::channel::<ArenaResult>();
    let worker_count = config.threads.min(jobs.len().max(1));
    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let jobs = Arc::clone(&jobs);
        let next_job = Arc::clone(&next_job);
        let config = Arc::clone(&shared_config);
        let sender = sender.clone();
        workers.push(thread::spawn(move || {
            loop {
                let index = next_job.fetch_add(1, Ordering::Relaxed);
                let Some(&(block, seat)) = jobs.get(index) else {
                    break;
                };
                let board_seed = config.seed.wrapping_add(block as u64 * 2);
                let chance_seed = config.seed.wrapping_add(block as u64 * 2 + 1);
                let engines = if let Some(lineup) = &config.lineup {
                    (0..config.players as usize)
                        .map(|player| lineup[(player + seat as usize) % lineup.len()])
                        .collect::<Vec<_>>()
                } else {
                    let mut engines = vec![config.baseline; config.players as usize];
                    engines[seat as usize] = config.candidate;
                    engines
                };
                let result = play_game(
                    board_seed,
                    chance_seed,
                    &engines,
                    &config,
                    Some((block, seat)),
                );
                if sender
                    .send(ArenaResult {
                        block,
                        seat,
                        engines,
                        game: result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    drop(sender);
    let mut results = Vec::with_capacity(jobs.len());
    for result in receiver {
        partial_metrics.record(&result);
        if let Some(writer) = &mut checkpoint_writer {
            let checkpoint =
                partial_metrics.snapshot(&result, &config, scheduled_games, started.elapsed());
            write_checkpoint(writer, &checkpoint);
        }
        results.push(result);
    }
    for worker in workers {
        worker.join().expect("arena worker panicked");
    }
    results.sort_by_key(|result| (result.block, result.seat));
    let mut block_wins = vec![0u32; config.blocks as usize];
    let mut block_terminal_games = vec![0u32; config.blocks as usize];
    let mut engine_wins = [0u32; 6];
    let mut expert_writer = config.expert_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create expert data {path}: {error}")),
        )
    });
    let mut expert_sample_count = 0u64;
    let mut trade_writer = config.trade_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create trade data {path}: {error}")),
        )
    });
    let mut trade_sample_count = 0u64;
    let mut trajectory_writer = config.trajectory_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create trajectory data {path}: {error}")),
        )
    });
    let mut trajectory_sample_count = 0u64;
    let mut challenge_writer = config.challenge_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create challenge data {path}: {error}")),
        )
    });
    let mut challenge_sample_count = 0u64;
    for result in results {
        if let Some(writer) = &mut challenge_writer {
            for snapshot in &result.game.challenge_snapshots {
                serde_json::to_writer(&mut *writer, snapshot)
                    .expect("challenge snapshot must serialize");
                writer
                    .write_all(b"\n")
                    .expect("challenge data must be writable");
                challenge_sample_count += 1;
            }
        }
        if let Some(writer) = &mut expert_writer {
            for sample in &result.game.expert_samples {
                serde_json::to_writer(&mut *writer, sample).expect("expert sample must serialize");
                writer
                    .write_all(b"\n")
                    .expect("expert data must be writable");
                expert_sample_count += 1;
            }
        }
        if let Some(writer) = &mut trade_writer {
            for sample in &result.game.trade_samples {
                serde_json::to_writer(&mut *writer, sample).expect("trade sample must serialize");
                writer
                    .write_all(b"\n")
                    .expect("trade data must be writable");
                trade_sample_count += 1;
            }
        }
        if let Some(writer) = &mut trajectory_writer {
            for sample in &result.game.trajectory_samples {
                let mut annotated = sample.clone();
                annotated.block = result.block;
                annotated.seat_rotation = result.seat;
                serde_json::to_writer(&mut *writer, &annotated)
                    .expect("trajectory sample must serialize");
                writer
                    .write_all(b"\n")
                    .expect("trajectory data must be writable");
                trajectory_sample_count += 1;
            }
        }
        let winner_engine = result.engines[result.game.winner as usize];
        let won = !result.game.cutoff && winner_engine == config.candidate;
        if !result.game.cutoff {
            block_terminal_games[result.block as usize] += 1;
            block_wins[result.block as usize] += u32::from(won);
            candidate_wins += u32::from(won);
            engine_wins[winner_engine as usize] += 1;
        }
        for (player, engine) in result.engines.iter().enumerate() {
            let candidate_metrics = &mut engine_metrics[*engine as usize];
            let metrics = &result.game.metrics;
            candidate_metrics.seats += 1;
            candidate_metrics.points += result.game.points[player] as u64;
            candidate_metrics.ranks += result.game.ranks[player] as f64;
            candidate_metrics.roads += metrics.roads[player] as u64;
            candidate_metrics.settlements += metrics.settlements[player] as u64;
            candidate_metrics.cities += metrics.cities[player] as u64;
            candidate_metrics.development_bought += metrics.development_bought[player] as u64;
            candidate_metrics.dead_roads += metrics.dead_roads[player] as u64;
            candidate_metrics.road_cuts += metrics.road_cuts[player] as u64;
            candidate_metrics.award_transfers += metrics.award_transfers[player] as u64;
            candidate_metrics.expansion_denial_events +=
                metrics.expansion_denial_events[player] as u64;
            candidate_metrics.expansion_portfolio_denied +=
                metrics.expansion_portfolio_denied[player] as f64;
            candidate_metrics.expansion_protection_events +=
                metrics.expansion_protection_events[player] as u64;
            for mechanism in 0..4 {
                candidate_metrics.progress_card_plays[mechanism] +=
                    metrics.progress_card_plays[player][mechanism] as u64;
                candidate_metrics.progress_card_conversions[mechanism] +=
                    metrics.progress_card_conversions[player][mechanism] as u64;
            }
            candidate_metrics.monopoly_cards_transferred +=
                metrics.monopoly_cards_transferred[player] as u64;
            candidate_metrics.dirty_monopoly_sequences +=
                metrics.dirty_monopoly_sequences[player] as u64;
            candidate_metrics.one_turn_closeouts += metrics.one_turn_closeouts[player] as u64;
            candidate_metrics.maritime_trades += metrics.maritime_trades[player] as u64;
            candidate_metrics.offers += metrics.offers[player] as u64;
            candidate_metrics.accepts += metrics.accepts[player] as u64;
            candidate_metrics.counters += metrics.counters[player] as u64;
            candidate_metrics.end_turns_over_seven += metrics.end_turns_over_seven[player] as u64;
            candidate_metrics.cards_lost_to_sevens += metrics.cards_lost_to_sevens[player] as u64;
            candidate_metrics.robber_blocked_production +=
                metrics.robber_blocked_production[player] as u64;
            candidate_metrics.decisions += metrics.decision_count[player] as u64;
            candidate_metrics.decision_nanos += metrics.decision_time[player].as_nanos();
            candidate_metrics.search_decisions += metrics.search_decision_count[player] as u64;
            candidate_metrics.search_nodes += metrics.search_nodes[player];
            candidate_metrics.search_depth += metrics.search_depth[player];
            candidate_metrics.posterior_particles += metrics.posterior_particles[player];
            candidate_metrics.strategic_particles += metrics.strategic_particles[player];
            candidate_metrics.search_deadlines += metrics.search_deadlines[player] as u64;
            candidate_metrics.search_action_values += metrics.search_action_values[player];
            candidate_metrics.trade_value_sum += metrics.trade_value_sum[player] as f64;
            candidate_metrics.calibration_brier_sum += metrics.calibration_brier_sum[player] as f64;
            candidate_metrics.calibration_log_loss_sum +=
                metrics.calibration_log_loss_sum[player] as f64;
            candidate_metrics.calibration_count += metrics.calibration_count[player] as u64;
        }
        cutoffs += u32::from(result.game.cutoff);
        turns += result.game.turns as u64;
        actions += result.game.actions as u64;
        total_games += 1;
        if !config.quiet {
            println!(
                "game block={block} rotation={seat} winner={} winner_engine={} candidate_win={} turns={} actions={} cutoff={}",
                result.game.winner,
                winner_engine.as_str(),
                won,
                result.game.turns,
                result.game.actions,
                result.game.cutoff,
                block = result.block,
                seat = result.seat,
            );
        }
    }
    if let Some(writer) = &mut expert_writer {
        writer.flush().expect("expert data must flush");
    }
    if let Some(writer) = &mut trade_writer {
        writer.flush().expect("trade data must flush");
    }
    block_scores.extend(
        block_wins
            .into_iter()
            .zip(block_terminal_games)
            .filter(|(_, terminal_games)| *terminal_games > 0)
            .map(|(wins, terminal_games)| wins as f32 / terminal_games as f32),
    );

    let terminal_games = total_games.saturating_sub(cutoffs);
    let win_share = candidate_wins as f32 / terminal_games.max(1) as f32;
    let candidate_seats = config
        .lineup
        .as_ref()
        .map_or(1, |lineup| {
            lineup
                .iter()
                .filter(|engine| **engine == config.candidate)
                .count()
        })
        .max(1);
    let fair_share = candidate_seats as f32 / config.players as f32;
    let (lower, upper) = bootstrap_interval(&block_scores, config.seed ^ 0xb007_57a9);
    let elapsed = started.elapsed();
    let mean_turns = turns as f64 / total_games as f64;
    let mean_actions = actions as f64 / total_games as f64;
    let games_per_second = total_games as f64 / elapsed.as_secs_f64();
    let candidate_metrics = &engine_metrics[config.candidate as usize];
    let compact_engine_metrics = compact_engine_metrics(&engine_metrics);
    if config.json {
        let (gpu_stats, batch_stats, average_batch_size) =
            evaluator_benchmark_metrics(config.evaluator_backend);
        println!(
            concat!(
                "{{",
                "\"schemaVersion\":1,",
                "\"simulator\":\"colonist-native\",",
                "\"candidate\":\"{}\",",
                "\"baseline\":\"{}\",",
                "\"lineup\":{},",
                "\"players\":{},",
                "\"blocks\":{},",
                "\"games\":{},",
                "\"terminalGames\":{},",
                "\"candidateWins\":{},",
                "\"engineWins\":{{\"random\":{},\"weighted\":{},\"maxn\":{},\"alphabeta\":{},\"uct\":{},\"puct\":{}}},",
                "\"engineMetrics\":{{{}}},",
                "\"winShare\":{:.8},",
                "\"fairShare\":{:.8},",
                "\"fairShareDelta\":{:.8},",
                "\"blockedCi95\":{{\"lower\":{:.8},\"upper\":{:.8}}},",
                "\"meanTurns\":{:.4},",
                "\"meanActions\":{:.4},",
                "\"candidateMetrics\":{{",
                "\"seatSamples\":{},",
                "\"decisions\":{},",
                "\"meanRank\":{:.6},",
                "\"meanVictoryPoints\":{:.6},",
                "\"meanRoads\":{:.6},",
                "\"meanSettlements\":{:.6},",
                "\"meanCities\":{:.6},",
                "\"meanDevelopmentCardsBought\":{:.6},",
                "\"meanDeadRoads\":{:.6},",
                "\"meanRoadCuts\":{:.6},",
                "\"meanAwardTransfers\":{:.6},",
                "\"meanExpansionDenialEvents\":{:.6},",
                "\"meanExpansionPortfolioValueDenied\":{:.6},",
                "\"meanExpansionProtectionEvents\":{:.6},",
                "\"progressCardPlays\":{{\"knight\":{},\"roadBuilding\":{},\"yearOfPlenty\":{},\"monopoly\":{}}},",
                "\"progressCardConversions\":{{\"knight\":{},\"roadBuilding\":{},\"yearOfPlenty\":{},\"monopoly\":{}}},",
                "\"progressCardConversionRates\":{{\"knight\":{:.6},\"roadBuilding\":{:.6},\"yearOfPlenty\":{:.6},\"monopoly\":{:.6}}},",
                "\"meanMonopolyCardsTransferred\":{:.6},",
                "\"meanDirtyMonopolySequences\":{:.6},",
                "\"meanOneTurnCloseouts\":{:.6},",
                "\"meanMaritimeTrades\":{:.6},",
                "\"meanDomesticOffers\":{:.6},",
                "\"tradeAcceptanceRate\":{:.6},",
                "\"meanCounters\":{:.6},",
                "\"meanEndTurnsAboveSeven\":{:.6},",
                "\"meanCardsLostToSevens\":{:.6},",
                "\"meanRobberBlockedProduction\":{:.6},",
                "\"meanTradeValue\":{:.6},",
                "\"meanDecisionLatencyMs\":{:.6},",
                "\"calibrationBrier\":{:.8},",
                "\"calibrationLogLoss\":{:.8}",
                "}},",
                "\"expertSamples\":{},",
                "\"tradeSamples\":{},",
                "\"trajectorySamples\":{},",
                "\"challengeSamples\":{},",
                "\"cutoffs\":{},",
                "\"elapsedMs\":{},",
                "\"gamesPerSecond\":{:.6},",
                "\"seed\":{},",
                "\"iterations\":{},",
                "\"rolloutActions\":{},",
                "\"maxTurns\":{},",
                "\"beliefParticles\":{},",
                "\"strategicParticleLimit\":{},",
                "\"maxnDepth\":{},",
                "\"maxnBranch\":{},",
                "\"maxnNodes\":{},",
                "\"maxnTimeMs\":{},",
                "\"playerTradesEnabled\":{},",
                "\"maritimeTradesEnabled\":true,",
                "\"evaluatorBackend\":\"{}\",",
                "\"gpuStats\":{},",
                "\"batchStats\":{},",
                "\"averageBatchSize\":{},",
                "\"engineRevision\":\"{}\",",
                "\"buildGitSha\":\"{}\",",
                "\"buildDirty\":{},",
                "\"informationMode\":\"{}\",",
                "\"perfectInformationSearch\":{},",
                "\"threads\":{},",
                "\"validate\":{}",
                "}}"
            ),
            config.candidate.as_str(),
            config.baseline.as_str(),
            config.lineup.as_ref().map_or_else(
                || "null".to_string(),
                |lineup| format!(
                    "[{}]",
                    lineup
                        .iter()
                        .map(|engine| format!("\"{}\"", engine.as_str()))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            config.players,
            config.blocks,
            total_games,
            terminal_games,
            candidate_wins,
            engine_wins[Engine::Random as usize],
            engine_wins[Engine::Weighted as usize],
            engine_wins[Engine::MaxN as usize],
            engine_wins[Engine::AlphaBeta as usize],
            engine_wins[Engine::Uct as usize],
            engine_wins[Engine::Puct as usize],
            compact_engine_metrics,
            win_share,
            fair_share,
            win_share - fair_share,
            lower,
            upper,
            mean_turns,
            mean_actions,
            candidate_metrics.seats,
            candidate_metrics.decisions,
            candidate_metrics.ranks / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.points as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.roads as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.settlements as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.cities as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.development_bought as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.dead_roads as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.road_cuts as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.award_transfers as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.expansion_denial_events as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.expansion_portfolio_denied / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.expansion_protection_events as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.progress_card_plays[ProgressMechanism::Knight.index()],
            candidate_metrics.progress_card_plays[ProgressMechanism::RoadBuilding.index()],
            candidate_metrics.progress_card_plays[ProgressMechanism::YearOfPlenty.index()],
            candidate_metrics.progress_card_plays[ProgressMechanism::Monopoly.index()],
            candidate_metrics.progress_card_conversions[ProgressMechanism::Knight.index()],
            candidate_metrics.progress_card_conversions[ProgressMechanism::RoadBuilding.index()],
            candidate_metrics.progress_card_conversions[ProgressMechanism::YearOfPlenty.index()],
            candidate_metrics.progress_card_conversions[ProgressMechanism::Monopoly.index()],
            candidate_metrics.progress_card_conversions[ProgressMechanism::Knight.index()] as f64
                / candidate_metrics.progress_card_plays[ProgressMechanism::Knight.index()].max(1)
                    as f64,
            candidate_metrics.progress_card_conversions[ProgressMechanism::RoadBuilding.index()]
                as f64
                / candidate_metrics.progress_card_plays[ProgressMechanism::RoadBuilding.index()]
                    .max(1) as f64,
            candidate_metrics.progress_card_conversions[ProgressMechanism::YearOfPlenty.index()]
                as f64
                / candidate_metrics.progress_card_plays[ProgressMechanism::YearOfPlenty.index()]
                    .max(1) as f64,
            candidate_metrics.progress_card_conversions[ProgressMechanism::Monopoly.index()] as f64
                / candidate_metrics.progress_card_plays[ProgressMechanism::Monopoly.index()].max(1)
                    as f64,
            candidate_metrics.monopoly_cards_transferred as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.dirty_monopoly_sequences as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.one_turn_closeouts as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.maritime_trades as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.offers as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.accepts as f64 / candidate_metrics.offers.max(1) as f64,
            candidate_metrics.counters as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.end_turns_over_seven as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.cards_lost_to_sevens as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.robber_blocked_production as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.trade_value_sum / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.decision_nanos as f64
                / candidate_metrics.decisions.max(1) as f64
                / 1_000_000.0,
            candidate_metrics.calibration_brier_sum
                / candidate_metrics.calibration_count.max(1) as f64,
            candidate_metrics.calibration_log_loss_sum
                / candidate_metrics.calibration_count.max(1) as f64,
            expert_sample_count,
            trade_sample_count,
            trajectory_sample_count,
            challenge_sample_count,
            cutoffs,
            elapsed.as_millis(),
            games_per_second,
            config.seed,
            config.iterations,
            config.rollout_actions,
            config.max_turns,
            config.belief_particles,
            config.strategic_particle_limit,
            config.maxn_depth,
            config.maxn_branch,
            config
                .maxn_nodes
                .unwrap_or_else(|| (config.iterations * 160).clamp(4_000, 80_000)),
            config.maxn_time_ms,
            config.player_trades_enabled,
            config.evaluator_backend.as_str(),
            gpu_stats,
            batch_stats,
            average_batch_size,
            ENGINE_REVISION,
            config.build_git_sha,
            config.build_dirty,
            information_mode(&config),
            config.perfect_information_search,
            config.threads,
            config.validate,
        );
    } else {
        println!(
            "summary games={total_games} terminal_games={terminal_games} candidate_wins={candidate_wins} win_share={win_share:.4} blocked_ci95=[{lower:.4},{upper:.4}] mean_turns={mean_turns:.1} mean_actions={mean_actions:.1} cutoffs={cutoffs} elapsed_ms={} games_per_second={games_per_second:.3}",
            elapsed.as_millis(),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use colonist_catan_core::Phase;

    use super::{
        ArenaResult, Config, Engine, GameMetrics, GameResult, PartialArenaMetrics,
        belief_particles, information_mode, play_game, search_belief_particles, write_checkpoint,
    };

    #[test]
    fn maxn_is_the_default_and_legacy_names_resolve_explicitly() {
        assert_eq!(Engine::parse("strategist"), Some(Engine::Puct));
        assert_eq!(Engine::parse("puct"), Some(Engine::Puct));
        assert_eq!(Engine::parse("deep"), Some(Engine::MaxN));
        assert_eq!(Engine::Puct.as_str(), "puct");
        assert_eq!(Config::default().candidate, Engine::MaxN);
    }

    #[test]
    fn random_arena_game_is_reproducible() {
        let config = Config {
            players: 3,
            max_turns: 40,
            ..Config::default()
        };
        let engines = [Engine::Random; 3];
        let first = play_game(1, 2, &engines, &config, None);
        let second = play_game(1, 2, &engines, &config, None);
        assert_eq!(first.winner, second.winner);
        assert_eq!(first.turns, second.turns);
        assert_eq!(first.actions, second.actions);
    }

    #[test]
    fn priors_rank_generated_actions_without_panicking() {
        let state = colonist_catan_core::GameState::standard(4, 4);
        for action in state.legal_actions() {
            assert!(
                colonist_catan_search::action_prior(&state, &action, state.actor()).is_finite()
            );
        }
    }

    #[test]
    fn arena_beliefs_preserve_the_observers_information_set_and_conservation() {
        let mut state = colonist_catan_core::GameState::standard(17, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        let observer = state.actor();
        let exact_hand = state.players[observer as usize].resources;
        let observation = state.observation_hash(observer);
        let single_particle = belief_particles(&state, observer, 1, 991);
        assert_eq!(single_particle.len(), 1);
        assert_eq!(single_particle[0].weight, 1.0);
        assert_eq!(
            single_particle[0].state.observation_hash(observer),
            observation,
        );
        single_particle[0].state.validate().unwrap();

        let particles = belief_particles(&state, observer, 24, 991);
        assert_eq!(particles.len(), 24);
        for particle in particles {
            assert_eq!(
                particle.state.players[observer as usize].resources,
                exact_hand,
            );
            assert_eq!(particle.state.observation_hash(observer), observation,);
            particle.state.validate().unwrap();
        }
    }

    #[test]
    fn all_search_engines_use_the_configured_information_source() {
        for engine in [Engine::MaxN, Engine::AlphaBeta, Engine::Uct, Engine::Puct] {
            assert!(engine.uses_search_information());
        }
        assert!(!Engine::Random.uses_search_information());
        assert!(!Engine::Weighted.uses_search_information());

        let state = colonist_catan_core::GameState::standard(23, 4);
        let belief_config = Config {
            belief_particles: 7,
            ..Config::default()
        };
        let particles = search_belief_particles(&state, &belief_config).unwrap();
        assert_eq!(particles.len(), 7);
        assert_eq!(information_mode(&belief_config), "weighted-belief");
        assert!(
            particles
                .iter()
                .all(|particle| (particle.weight - 1.0 / 7.0).abs() < f32::EPSILON)
        );

        let oracle_config = Config {
            perfect_information_search: true,
            ..belief_config
        };
        assert!(search_belief_particles(&state, &oracle_config).is_none());
        assert_eq!(information_mode(&oracle_config), "perfect-information");
    }

    #[test]
    fn checkpoints_flush_cumulative_multi_engine_standings_after_every_game() {
        let lineup = vec![
            Engine::Puct,
            Engine::Weighted,
            Engine::MaxN,
            Engine::AlphaBeta,
        ];
        let config = Config {
            players: 4,
            blocks: 1,
            lineup: Some(lineup.clone()),
            validate: true,
            ..Config::default()
        };
        let game = GameResult {
            winner: 2,
            turns: 91,
            actions: 412,
            cutoff: false,
            points: [7, 5, 10, 8],
            ranks: [3.0, 4.0, 1.0, 2.0],
            metrics: GameMetrics::default(),
            expert_samples: Vec::new(),
            trade_samples: Vec::new(),
            trajectory_samples: Vec::new(),
            challenge_snapshots: Vec::new(),
            longest_road_holder: None,
            largest_army_holder: None,
        };
        let mut partial = PartialArenaMetrics::new(config.blocks);
        let mut output = Vec::new();

        for rotation in 0..4 {
            let result = ArenaResult {
                block: 0,
                seat: rotation,
                engines: lineup.clone(),
                game: game.clone(),
            };
            partial.record(&result);
            let checkpoint = partial.snapshot(&result, &config, 4, Duration::from_millis(1_500));
            write_checkpoint(&mut output, &checkpoint);
        }

        let lines = std::str::from_utf8(&output)
            .unwrap()
            .lines()
            .collect::<Vec<_>>();
        assert_eq!(lines.len(), 4);
        for line in &lines {
            serde_json::from_str::<serde_json::Value>(line).unwrap();
        }
        let first_checkpoint = serde_json::from_str::<serde_json::Value>(lines[0]).unwrap();
        assert_eq!(first_checkpoint["completedGames"], 1);
        assert_eq!(first_checkpoint["complete"], false);
        assert_eq!(first_checkpoint["completedMatchedBlocks"], 0);
        let final_checkpoint =
            serde_json::from_str::<serde_json::Value>(lines.last().unwrap()).unwrap();
        assert_eq!(final_checkpoint["completedGames"], 4);
        assert_eq!(final_checkpoint["scheduledGames"], 4);
        assert_eq!(final_checkpoint["complete"], true);
        assert_eq!(final_checkpoint["completedMatchedBlocks"], 1);
        assert_eq!(final_checkpoint["cutoffs"], 0);
        assert_eq!(final_checkpoint["informationMode"], "weighted-belief");
        assert_eq!(final_checkpoint["lastGame"]["winnerEngine"], "maxn");

        let standings = final_checkpoint["engineStandings"].as_array().unwrap();
        let maxn = standings
            .iter()
            .find(|standing| standing["engine"] == "maxn")
            .unwrap();
        assert_eq!(maxn["wins"], 4);
        assert_eq!(maxn["winShare"], 1.0);
        assert_eq!(maxn["seatSamples"], 4);
        assert_eq!(maxn["meanRank"], 1.0);
        assert_eq!(maxn["meanVictoryPoints"], 10.0);
    }

    #[test]
    fn cutoff_adjudications_are_censored_from_win_rates() {
        let config = Config {
            players: 3,
            blocks: 1,
            ..Config::default()
        };
        let result = ArenaResult {
            block: 0,
            seat: 0,
            engines: vec![Engine::Puct, Engine::Weighted, Engine::Weighted],
            game: GameResult {
                winner: 0,
                turns: config.max_turns,
                actions: 500,
                cutoff: true,
                points: [9, 8, 7, 0],
                ranks: [1.0, 2.0, 3.0, 0.0],
                metrics: GameMetrics::default(),
                expert_samples: Vec::new(),
                trade_samples: Vec::new(),
                trajectory_samples: Vec::new(),
                challenge_snapshots: Vec::new(),
                longest_road_holder: None,
                largest_army_holder: None,
            },
        };
        let mut partial = PartialArenaMetrics::new(1);
        partial.record(&result);
        let checkpoint = partial.snapshot(&result, &config, 3, Duration::from_secs(1));
        let value = serde_json::to_value(checkpoint).unwrap();

        assert_eq!(value["cutoffs"], 1);
        assert_eq!(value["terminalGames"], 0);
        assert!(
            value["engineStandings"]
                .as_array()
                .unwrap()
                .iter()
                .all(|standing| standing["wins"] == 0)
        );
    }
}
