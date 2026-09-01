use std::time::Instant;

use colonist_catan_arena::{belief_particles, exact_parity_corpus};
use colonist_catan_search::{
    BeliefDepthConfig, BeliefDepthResult, CudaExactEvaluator,
    search_weighted_belief_maxn_cuda_with_config, search_weighted_belief_maxn_with_config,
};
use serde::Serialize;

const PARTICLES: usize = 24;
const VALUE_TOLERANCE: f32 = 1e-5;
const CASES: [&str; 6] = [
    "3p-development-cards",
    "3p-longest-road-race",
    "3p-no-player-trades",
    "4p-development-cards",
    "4p-large-hands-discard-pressure",
    "4p-no-player-trades",
];

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct CaseReport {
    case: String,
    players: u8,
    observer: u8,
    cpu_action: Option<String>,
    cuda_action: Option<String>,
    exact_action_match: bool,
    tolerance_tie: bool,
    max_abs_error: f32,
    cpu_nodes: u32,
    cuda_nodes: u32,
    cpu_depth: u8,
    cuda_depth: u8,
    cpu_ms: f64,
    cuda_ms: f64,
    failures: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    kind: &'static str,
    evaluator: &'static str,
    cpu_oracle: &'static str,
    fixed_node_budget: u32,
    wall_clock_cutoff_ms: u32,
    belief_particles: usize,
    value_tolerance: f32,
    parity: bool,
    exact_action_matches: usize,
    tolerance_ties: usize,
    max_abs_error: f32,
    gpu_name: String,
    gpu_batches: u64,
    gpu_states: u64,
    average_gpu_batch_size: f64,
    cases: Vec<CaseReport>,
}

fn search_config() -> BeliefDepthConfig {
    BeliefDepthConfig {
        maximum_depth: 4,
        branch_cap: 8,
        maximum_nodes: 4_000,
        time_budget_ms: 0,
        strategic_particle_limit: PARTICLES,
    }
}

fn action_label(action: &Option<colonist_catan_core::Action>) -> Option<String> {
    action.as_ref().map(|action| format!("{action:?}"))
}

fn chosen_actor_value(
    result: &BeliefDepthResult,
    action: &colonist_catan_core::Action,
    actor: usize,
) -> Option<f32> {
    result
        .actions
        .iter()
        .find(|candidate| &candidate.action == action)
        .map(|candidate| candidate.value[actor])
}

fn mismatch_is_tolerance_tie(
    cpu: &BeliefDepthResult,
    cuda: &BeliefDepthResult,
    actor: usize,
) -> bool {
    let (Some(cpu_action), Some(cuda_action)) = (&cpu.chosen, &cuda.chosen) else {
        return false;
    };
    let Some(cpu_leader) = chosen_actor_value(cpu, cpu_action, actor) else {
        return false;
    };
    let Some(cpu_alternative) = chosen_actor_value(cpu, cuda_action, actor) else {
        return false;
    };
    let Some(cuda_leader) = chosen_actor_value(cuda, cuda_action, actor) else {
        return false;
    };
    let Some(cuda_alternative) = chosen_actor_value(cuda, cpu_action, actor) else {
        return false;
    };
    (cpu_leader - cpu_alternative).abs() <= VALUE_TOLERANCE
        && (cuda_leader - cuda_alternative).abs() <= VALUE_TOLERANCE
}

fn compare_results(
    case: String,
    players: u8,
    observer: u8,
    cpu: BeliefDepthResult,
    cuda: BeliefDepthResult,
    cpu_ms: f64,
    cuda_ms: f64,
) -> CaseReport {
    let mut failures = Vec::new();
    let exact_action_match = cpu.chosen == cuda.chosen;
    let tolerance_tie =
        !exact_action_match && mismatch_is_tolerance_tie(&cpu, &cuda, observer as usize);
    if !exact_action_match && !tolerance_tie {
        failures.push("chosen actions differ outside the value tolerance".into());
    }
    if cpu.nodes != cuda.nodes {
        failures.push(format!(
            "node counts differ: CPU {} CUDA {}",
            cpu.nodes, cuda.nodes
        ));
    }
    if cpu.depth != cuda.depth {
        failures.push(format!(
            "search depths differ: CPU {} CUDA {}",
            cpu.depth, cuda.depth
        ));
    }
    if cpu.particles != cuda.particles || cpu.posterior_particles != cuda.posterior_particles {
        failures.push("particle accounting differs".into());
    }
    if cpu.deadline_reached || cuda.deadline_reached {
        failures.push("a wall-clock deadline was reached despite a zero time budget".into());
    }
    if cpu.actions.len() != cuda.actions.len() {
        failures.push(format!(
            "root action counts differ: CPU {} CUDA {}",
            cpu.actions.len(),
            cuda.actions.len()
        ));
    }

    let mut max_abs_error = 0.0f32;
    for cpu_action in &cpu.actions {
        let Some(cuda_action) = cuda
            .actions
            .iter()
            .find(|candidate| candidate.action == cpu_action.action)
        else {
            failures.push(format!(
                "CUDA result omitted action {:?}",
                cpu_action.action
            ));
            continue;
        };
        for (label, cpu_values, cuda_values) in [
            ("value", cpu_action.value, cuda_action.value),
            (
                "lower confidence value",
                cpu_action.lower_confidence_value,
                cuda_action.lower_confidence_value,
            ),
        ] {
            for player in 0..4 {
                let error = (cpu_values[player] - cuda_values[player]).abs();
                max_abs_error = max_abs_error.max(error);
                if !cpu_values[player].is_finite() || !cuda_values[player].is_finite() {
                    failures.push(format!(
                        "{label} for action {:?}, player {player} is non-finite",
                        cpu_action.action
                    ));
                } else if error > VALUE_TOLERANCE {
                    failures.push(format!(
                        "{label} for action {:?}, player {player} differs by {error}",
                        cpu_action.action
                    ));
                }
            }
        }
        let legal_weight_error = (cpu_action.legal_weight - cuda_action.legal_weight).abs();
        max_abs_error = max_abs_error.max(legal_weight_error);
        if legal_weight_error > VALUE_TOLERANCE {
            failures.push(format!(
                "legal weight for action {:?} differs by {legal_weight_error}",
                cpu_action.action
            ));
        }
    }
    for player in 0..4 {
        let error = (cpu.value[player] - cuda.value[player]).abs();
        max_abs_error = max_abs_error.max(error);
        if error > VALUE_TOLERANCE {
            failures.push(format!(
                "chosen value for player {player} differs by {error}"
            ));
        }
    }

    CaseReport {
        case,
        players,
        observer,
        cpu_action: action_label(&cpu.chosen),
        cuda_action: action_label(&cuda.chosen),
        exact_action_match,
        tolerance_tie,
        max_abs_error,
        cpu_nodes: cpu.nodes,
        cuda_nodes: cuda.nodes,
        cpu_depth: cpu.depth,
        cuda_depth: cuda.depth,
        cpu_ms,
        cuda_ms,
        failures,
    }
}

fn main() {
    let corpus = exact_parity_corpus();
    let mut evaluator = CudaExactEvaluator::new().unwrap_or_else(|error| {
        eprintln!("exact-gpu-search-parity: CUDA initialization failed: {error}");
        std::process::exit(1);
    });
    let gpu_name = evaluator.device_identity().name.clone();
    let mut case_reports = Vec::with_capacity(CASES.len());

    for (case_index, case_name) in CASES.into_iter().enumerate() {
        let source = corpus
            .iter()
            .find(|candidate| candidate.name == case_name)
            .unwrap_or_else(|| panic!("missing fixed search parity case {case_name}"));
        let observer = source.state.actor();
        let particles = belief_particles(
            &source.state,
            observer,
            PARTICLES,
            0x5eed_0000_u64 ^ case_index as u64,
        );

        let cpu_started = Instant::now();
        let cpu = search_weighted_belief_maxn_with_config(&particles, search_config())
            .unwrap_or_else(|error| panic!("CPU search failed for {case_name}: {error:?}"));
        let cpu_ms = cpu_started.elapsed().as_secs_f64() * 1_000.0;

        let cuda_started = Instant::now();
        let cuda = search_weighted_belief_maxn_cuda_with_config(
            &mut evaluator,
            &particles,
            search_config(),
        )
        .unwrap_or_else(|error| panic!("CUDA search failed for {case_name}: {error:?}"));
        let cuda_ms = cuda_started.elapsed().as_secs_f64() * 1_000.0;

        case_reports.push(compare_results(
            case_name.into(),
            source.state.board.num_players,
            observer,
            cpu,
            cuda,
            cpu_ms,
            cuda_ms,
        ));
    }

    let stats = evaluator.stats();
    let exact_action_matches = case_reports
        .iter()
        .filter(|case| case.exact_action_match)
        .count();
    let tolerance_ties = case_reports
        .iter()
        .filter(|case| case.tolerance_tie)
        .count();
    let max_abs_error = case_reports
        .iter()
        .map(|case| case.max_abs_error)
        .fold(0.0f32, f32::max);
    let parity = case_reports.iter().all(|case| case.failures.is_empty());
    let report = Report {
        kind: "colonist-exact-gpu-search-parity",
        evaluator: "handcrafted-exact",
        cpu_oracle: "search_weighted_belief_maxn_with_config",
        fixed_node_budget: search_config().maximum_nodes,
        wall_clock_cutoff_ms: search_config().time_budget_ms,
        belief_particles: PARTICLES,
        value_tolerance: VALUE_TOLERANCE,
        parity,
        exact_action_matches,
        tolerance_ties,
        max_abs_error,
        gpu_name,
        gpu_batches: stats.batches,
        gpu_states: stats.states,
        average_gpu_batch_size: stats.average_batch_size(),
        cases: case_reports,
    };
    println!(
        "{}",
        serde_json::to_string(&report).expect("search parity report must serialize")
    );
    if !report.parity {
        std::process::exit(1);
    }
}
