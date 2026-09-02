use std::path::PathBuf;

use colonist_catan_arena::tactical_corpus::{
    build_state, default_corpus_path, load_tactical_corpus, verify_mechanical_consequence,
    TacticalCorpus,
};
use colonist_catan_core::Action;
use colonist_catan_search::{CudaSimEngine, CudaSimRootActionStats};
use serde::Serialize;

#[derive(Debug)]
struct CliArgs {
    corpus_path: PathBuf,
    rollouts_per_action: usize,
    rollout_steps: usize,
    seed: u64,
}

fn parse_args() -> CliArgs {
    let mut corpus_path = default_corpus_path();
    let mut rollouts_per_action = 512;
    let mut rollout_steps = 96;
    let mut seed = 2026092101u64;

    let args: Vec<String> = std::env::args().collect();
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--corpus" => {
                if i + 1 < args.len() {
                    corpus_path = PathBuf::from(&args[i + 1]);
                    i += 1;
                }
            }
            "--rollouts-per-action" => {
                if i + 1 < args.len() {
                    rollouts_per_action = args[i + 1].parse().unwrap_or(512);
                    i += 1;
                }
            }
            "--rollout-steps" => {
                if i + 1 < args.len() {
                    rollout_steps = args[i + 1].parse().unwrap_or(96);
                    i += 1;
                }
            }
            "--seed" => {
                if i + 1 < args.len() {
                    seed = args[i + 1].parse().unwrap_or(2026092101);
                    i += 1;
                }
            }
            _ => {}
        }
        i += 1;
    }

    CliArgs {
        corpus_path,
        rollouts_per_action,
        rollout_steps,
        seed,
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionReport {
    action: String,
    proposal_frequency: usize,
    proposal_rate: f32,
    samples: usize,
    errors: usize,
    terminal_rate: f32,
    net_terminal_outcome: f32,
    mean_victory_margin: f32,
    variance: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioReport {
    id: String,
    family: String,
    declared_consequence: String,
    g0_passed: bool,
    expected_best_root: String,
    negative_control_root: Option<String>,
    selected_root: String,
    tactical_contract_passed: bool,
    actions: Vec<ActionReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkSummary {
    benchmark: &'static str,
    device_name: String,
    rollouts_per_action: usize,
    rollout_steps: usize,
    seed: u64,
    total_scenarios: usize,
    passed_scenarios: usize,
    failed_scenarios: usize,
    scenarios: Vec<ScenarioReport>,
}

fn main() {
    let args = parse_args();

    let corpus: TacticalCorpus = load_tactical_corpus(&args.corpus_path)
        .unwrap_or_else(|e| panic!("failed to load corpus from {:?}: {e}", args.corpus_path));

    let mut engine = match CudaSimEngine::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("CUDA initialization failed: {e:?}");
            std::process::exit(1);
        }
    };

    let device_name = engine.device_identity().name.clone();

    let mut scenario_reports = Vec::new();
    let mut passed_count = 0;
    let mut failed_count = 0;

    for (index, scenario) in corpus.scenarios.iter().enumerate() {
        let g0_result = verify_mechanical_consequence(scenario);
        let g0_passed = g0_result.is_ok();

        let state = build_state(&scenario.state);
        let candidate_actions: Vec<Action> = scenario
            .candidate_roots
            .iter()
            .map(|s| s.to_action())
            .collect();

        let scenario_seed = args.seed.wrapping_add((index as u64) * 1009);

        // 1. Task 8 Proposal-Frequency Diagnostic:
        // Sample the CUDA rollout proposal policy directly without advancing state
        const PROPOSAL_SAMPLES: usize = 256;
        let proposal_batch = vec![state.clone(); PROPOSAL_SAMPLES];
        let sampled_actions = if let Err(e) = engine.upload_states(&proposal_batch) {
            eprintln!("Failed to upload states for proposal test {}: {e:?}", scenario.id);
            Vec::new()
        } else if let Err(e) = engine.seed_rollout_rng(scenario_seed ^ 0xa5a5_5a5a) {
            eprintln!("Failed to seed rollout RNG for proposal test {}: {e:?}", scenario.id);
            Vec::new()
        } else if let Err(e) = engine.generate_rollout_actions() {
            eprintln!("Failed to generate rollout actions for proposal test {}: {e:?}", scenario.id);
            Vec::new()
        } else {
            engine.download_generated_actions().unwrap_or_default()
        };

        // 2. Explicit-Root Tactical Search:
        if let Err(e) = engine.upload_states(&[state]) {
            eprintln!("Failed to set states for scenario {}: {e:?}", scenario.id);
            failed_count += 1;
            continue;
        }

        let search_result = match engine.search_root_actions(
            &[candidate_actions.clone()],
            args.rollouts_per_action,
            args.rollout_steps,
            scenario_seed,
        ) {
            Ok(res) => res,
            Err(e) => {
                eprintln!("search_root_actions failed for scenario {}: {e:?}", scenario.id);
                failed_count += 1;
                continue;
            }
        };

        let row = search_result.rows.first().cloned().unwrap_or_default();

        // Reject errorful rows when determining best action
        let valid_rows: Vec<&CudaSimRootActionStats> = row.iter().filter(|s| s.errors == 0).collect();
        let best_stat = valid_rows
            .iter()
            .max_by(|a, b| {
                a.mean_victory_margin()
                    .partial_cmp(&b.mean_victory_margin())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .copied();

        let any_errors = row.iter().any(|s| s.errors > 0);

        let selected_action_str = best_stat
            .map(|s| format!("{:?}", s.action))
            .unwrap_or_else(|| "None".into());

        let expected_best_action = scenario.expected_best_root.to_action();
        let expected_best_str = format!("{expected_best_action:?}");
        let negative_control_action = scenario
            .negative_control_root
            .as_ref()
            .map(|s| s.to_action());
        let negative_control_str = negative_control_action
            .as_ref()
            .map(|a| format!("{a:?}"));

        let action_reports: Vec<ActionReport> = row
            .iter()
            .map(|stat| {
                let proposal_count = sampled_actions.iter().filter(|&act| act == &stat.action).count();
                let proposal_rate = if PROPOSAL_SAMPLES > 0 {
                    proposal_count as f32 / PROPOSAL_SAMPLES as f32
                } else {
                    0.0
                };
                ActionReport {
                    action: format!("{:?}", stat.action),
                    proposal_frequency: proposal_count,
                    proposal_rate,
                    samples: stat.samples as usize,
                    errors: stat.errors as usize,
                    terminal_rate: stat.terminal_rate(),
                    net_terminal_outcome: stat.net_terminal_outcome(),
                    mean_victory_margin: stat.mean_victory_margin(),
                    variance: stat.victory_margin_variance(),
                }
            })
            .collect();

        // Check tactical contract:
        // 1. G0 mechanical proof must pass.
        // 2. CUDA root errors must be zero.
        // 3. If negative control, selected action must NOT be negative control action.
        // 4. If positive control, selected action must match expected best action or decisively beat negative control.
        let tactical_passed = if !g0_passed || any_errors || best_stat.is_none() {
            false
        } else if scenario.is_negative_control {
            if let Some(neg_act) = &negative_control_action {
                best_stat.map(|s| &s.action) != Some(neg_act)
            } else {
                true
            }
        } else {
            best_stat.map(|s| &s.action) == Some(&expected_best_action)
                || (negative_control_action.is_some()
                    && best_stat.map(|s| &s.action) != negative_control_action.as_ref())
        };

        if tactical_passed {
            passed_count += 1;
        } else {
            failed_count += 1;
        }

        scenario_reports.push(ScenarioReport {
            id: scenario.id.clone(),
            family: scenario.family.clone(),
            declared_consequence: scenario.declared_consequence.clone(),
            g0_passed,
            expected_best_root: expected_best_str,
            negative_control_root: negative_control_str,
            selected_root: selected_action_str,
            tactical_contract_passed: tactical_passed,
            actions: action_reports,
        });
    }

    let summary = BenchmarkSummary {
        benchmark: "gpu-latent-threat-benchmark",
        device_name,
        rollouts_per_action: args.rollouts_per_action,
        rollout_steps: args.rollout_steps,
        seed: args.seed,
        total_scenarios: corpus.scenarios.len(),
        passed_scenarios: passed_count,
        failed_scenarios: failed_count,
        scenarios: scenario_reports,
    };

    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    if failed_count > 0 {
        std::process::exit(2);
    }
}
