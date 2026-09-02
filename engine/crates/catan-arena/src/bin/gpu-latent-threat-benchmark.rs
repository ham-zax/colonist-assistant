use std::path::PathBuf;

use colonist_catan_arena::tactical_corpus::{
    build_state, default_corpus_path, load_tactical_corpus, rebalance_tactical_bank,
    verify_mechanical_consequence, TacticalCorpus,
};
use colonist_catan_core::{Action, Phase};
use colonist_catan_search::CudaSimEngine;
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
struct ProposalProbeReport {
    action: String,
    legal: bool,
    proposal_frequency: usize,
    proposal_rate: f32,
    errors: usize,
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
    proposal_errors: usize,
    proposal_probe: Option<ProposalProbeReport>,
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

        let state = build_state(&scenario.state)
            .unwrap_or_else(|error| panic!("invalid tactical scenario {}: {error}", scenario.id));
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
        let mut proposal_errors = 0usize;
        let sampled_actions = if let Err(e) = engine.upload_states(&proposal_batch) {
            eprintln!("Failed to upload states for proposal test {}: {e:?}", scenario.id);
            proposal_errors = 1;
            Vec::new()
        } else if let Err(e) = engine.seed_rollout_rng(scenario_seed ^ 0xa5a5_5a5a) {
            eprintln!("Failed to seed rollout RNG for proposal test {}: {e:?}", scenario.id);
            proposal_errors = 1;
            Vec::new()
        } else if let Err(e) = engine.generate_rollout_actions() {
            eprintln!("Failed to generate rollout actions for proposal test {}: {e:?}", scenario.id);
            proposal_errors = 1;
            Vec::new()
        } else {
            match engine.download_generated_actions() {
                Ok(actions) => actions,
                Err(e) => {
                    eprintln!("Failed to download proposal actions for {}: {e:?}", scenario.id);
                    proposal_errors = 1;
                    Vec::new()
                }
            }
        };

        let proposal_probe_report = if let Some(probe) = &scenario.proposal_probe {
            let mut probe_state = state.clone();
            probe_state.current_player = probe.player;
            probe_state.phase = Phase::Main;
            probe_state.players[probe.player as usize].resources = probe.resources;
            let probe_action = probe.action.to_action();
            let mut errors = 0usize;
            let valid = match rebalance_tactical_bank(&mut probe_state)
                .and_then(|_| probe_state.validate())
            {
                Ok(()) => true,
                Err(error) => {
                    eprintln!("Invalid proposal probe for {}: {error}", scenario.id);
                    errors = 1;
                    false
                }
            };
            let legal = valid && probe_state.legal_actions().contains(&probe_action);
            let generated = if !legal {
                Vec::new()
            } else {
                let probe_batch = vec![probe_state; PROPOSAL_SAMPLES];
                if engine.upload_states(&probe_batch).is_err()
                    || engine.seed_rollout_rng(scenario_seed ^ 0x5a5a_a5a5).is_err()
                    || engine.generate_rollout_actions().is_err()
                {
                    errors = 1;
                    Vec::new()
                } else {
                    match engine.download_generated_actions() {
                        Ok(actions) => actions,
                        Err(_) => {
                            errors = 1;
                            Vec::new()
                        }
                    }
                }
            };
            let proposal_frequency = generated
                .iter()
                .filter(|action| **action == probe_action)
                .count();
            Some(ProposalProbeReport {
                action: format!("{probe_action:?}"),
                legal,
                proposal_frequency,
                proposal_rate: proposal_frequency as f32 / PROPOSAL_SAMPLES as f32,
                errors,
            })
        } else {
            None
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
        let best_stat = search_result.best_actions().into_iter().next().flatten();
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

        let proposal_probe_ok = proposal_probe_report
            .as_ref()
            .is_none_or(|probe| probe.legal && probe.errors == 0);

        // Check only the scenario's declared mechanical/ordering contract.
        // `best_actions()` uses the benchmark's required explicit-root ordering:
        // net terminal outcome, then VP margin, then shorter mean game length.
        let tactical_passed = if !g0_passed || proposal_errors > 0 || !proposal_probe_ok || any_errors {
            false
        } else if best_stat.map(|stat| &stat.action) != Some(&expected_best_action) {
            false
        } else if scenario.is_negative_control {
            negative_control_action.as_ref().is_some_and(|negative| {
                row.iter().find(|stat| &stat.action == negative).is_some_and(|negative_stat| {
                    let best = best_stat.expect("expected root was selected");
                    best.net_terminal_outcome() > negative_stat.net_terminal_outcome() + 0.001
                        || ((best.net_terminal_outcome() - negative_stat.net_terminal_outcome()).abs()
                            <= 0.001
                            && best.mean_victory_margin()
                                > negative_stat.mean_victory_margin() + 0.001)
                })
            })
        } else {
            true
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
            proposal_errors,
            proposal_probe: proposal_probe_report,
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
