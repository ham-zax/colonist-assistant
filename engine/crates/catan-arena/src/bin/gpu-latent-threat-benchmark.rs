use std::path::PathBuf;

use colonist_catan_arena::tactical_corpus::{
    apply_hidden_variant, build_state, default_corpus_path, load_tactical_corpus,
    rebalance_tactical_bank, verify_mechanical_consequence, TacticalCorpus,
};
use colonist_catan_core::{Action, GameState};
use colonist_catan_search::{
    posterior_expected_tactical_threat_weight, posterior_immediate_threat_weight, CudaSimEngine,
};
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
    min_proposal_rate: Option<f32>,
    max_proposal_rate: Option<f32>,
    contract_passed: bool,
    errors: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ThreatProbeReport {
    expected_weight: f32,
    strict_observed_weight: f32,
    expected_observed_weight: f32,
    contract_passed: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ObservationSafetyReport {
    actor: u8,
    observation_hash: u64,
    variant_observation_hash: u64,
    mismatched_proposals: usize,
    samples: usize,
    contract_passed: bool,
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
    threat_probe: Option<ThreatProbeReport>,
    observation_safety: Option<ObservationSafetyReport>,
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
    proposal_samples: usize,
    scenario_seed_stride: u64,
    proposal_seed_xor: u64,
    proposal_probe_seed_xor: u64,
    observation_safety_seed_xor: u64,
    comparison_ordering: &'static str,
    total_scenarios: usize,
    passed_scenarios: usize,
    failed_scenarios: usize,
    scenarios: Vec<ScenarioReport>,
}

const PROPOSAL_SAMPLES: usize = 256;
const SCENARIO_SEED_STRIDE: u64 = 1009;
const PROPOSAL_SEED_XOR: u64 = 0xa5a5_5a5a;
const PROPOSAL_PROBE_SEED_XOR: u64 = 0x5a5a_a5a5;
const OBSERVATION_SAFETY_SEED_XOR: u64 = 0x3c3c_c3c3;

fn same_action_kind(left: &Action, right: &Action) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn sample_proposals(
    engine: &mut CudaSimEngine,
    state: &GameState,
    seed: u64,
) -> Result<Vec<Action>, String> {
    engine
        .upload_states(&vec![state.clone(); PROPOSAL_SAMPLES])
        .map_err(|error| format!("upload failed: {error:?}"))?;
    engine
        .seed_rollout_rng(seed)
        .map_err(|error| format!("rng seed failed: {error:?}"))?;
    engine
        .generate_rollout_actions()
        .map_err(|error| format!("proposal generation failed: {error:?}"))?;
    engine
        .download_generated_actions()
        .map_err(|error| format!("proposal download failed: {error:?}"))
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

        let scenario_seed = args
            .seed
            .wrapping_add((index as u64) * SCENARIO_SEED_STRIDE);

        // 1. Task 8 Proposal-Frequency Diagnostic:
        // Sample the CUDA rollout proposal policy directly without advancing state.
        let mut proposal_errors = 0usize;
        let sampled_actions = match sample_proposals(
            &mut engine,
            &state,
            scenario_seed ^ PROPOSAL_SEED_XOR,
        ) {
            Ok(actions) => actions,
            Err(error) => {
                eprintln!("Proposal test failed for {}: {error}", scenario.id);
                proposal_errors = 1;
                Vec::new()
            }
        };

        let proposal_probe_report = if let Some(probe) = &scenario.proposal_probe {
            let mut probe_state = state.clone();
            probe_state.current_player = probe.player;
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
                match sample_proposals(
                    &mut engine,
                    &probe_state,
                    scenario_seed ^ PROPOSAL_PROBE_SEED_XOR,
                ) {
                    Ok(actions) => actions,
                    Err(error) => {
                        eprintln!("Proposal probe failed for {}: {error}", scenario.id);
                        errors = 1;
                        Vec::new()
                    }
                }
            };
            let proposal_frequency = generated
                .iter()
                .filter(|action| {
                    if probe.match_kind {
                        same_action_kind(action, &probe_action)
                    } else {
                        **action == probe_action
                    }
                })
                .count();
            let proposal_rate = proposal_frequency as f32 / PROPOSAL_SAMPLES as f32;
            let contract_passed = legal
                && errors == 0
                && probe
                    .min_proposal_rate
                    .is_none_or(|minimum| proposal_rate + 1e-6 >= minimum)
                && probe
                    .max_proposal_rate
                    .is_none_or(|maximum| proposal_rate <= maximum + 1e-6);
            Some(ProposalProbeReport {
                action: format!("{probe_action:?}"),
                legal,
                proposal_frequency,
                proposal_rate,
                min_proposal_rate: probe.min_proposal_rate,
                max_proposal_rate: probe.max_proposal_rate,
                contract_passed,
                errors,
            })
        } else {
            None
        };

        let threat_probe_report = scenario.threat_probe.as_ref().map(|probe| {
            let mut worlds = Vec::new();
            if probe.variants.is_empty() {
                worlds.push((state.clone(), 1.0));
            } else {
                for variant in &probe.variants {
                    let world = apply_hidden_variant(&state, variant).unwrap_or_else(|error| {
                        panic!("invalid threat variant {} / {}: {error}", scenario.id, variant.id)
                    });
                    worlds.push((world, variant.weight));
                }
            }
            let strict_observed_weight = posterior_immediate_threat_weight(
                worlds.iter().map(|(world, weight)| (world, *weight)),
                probe.protected_player,
            );
            let expected_observed_weight = posterior_expected_tactical_threat_weight(
                worlds.iter().map(|(world, weight)| (world, *weight)),
                probe.protected_player,
            );
            ThreatProbeReport {
                expected_weight: probe.expected_weight,
                strict_observed_weight,
                expected_observed_weight,
                contract_passed: (expected_observed_weight - probe.expected_weight).abs() <= 1e-4,
            }
        });

        let observation_safety_report = scenario.observation_safety_probe.as_ref().map(|probe| {
            let mut errors = 0usize;
            let variant = apply_hidden_variant(&state, &probe.variant).unwrap_or_else(|error| {
                panic!(
                    "invalid observation-safety variant {} / {}: {error}",
                    scenario.id, probe.variant.id
                )
            });
            let observation_hash = state.observation_hash(probe.actor);
            let variant_observation_hash = variant.observation_hash(probe.actor);
            let seed = scenario_seed ^ OBSERVATION_SAFETY_SEED_XOR;
            let base_actions = match sample_proposals(&mut engine, &state, seed) {
                Ok(actions) => actions,
                Err(error) => {
                    eprintln!("Observation-safety base proposal failed for {}: {error}", scenario.id);
                    errors += 1;
                    Vec::new()
                }
            };
            let variant_actions = match sample_proposals(&mut engine, &variant, seed) {
                Ok(actions) => actions,
                Err(error) => {
                    eprintln!("Observation-safety variant proposal failed for {}: {error}", scenario.id);
                    errors += 1;
                    Vec::new()
                }
            };
            let mismatched_proposals = base_actions
                .iter()
                .zip(&variant_actions)
                .filter(|(left, right)| left != right)
                .count();
            ObservationSafetyReport {
                actor: probe.actor,
                observation_hash,
                variant_observation_hash,
                mismatched_proposals,
                samples: PROPOSAL_SAMPLES,
                contract_passed: errors == 0
                    && observation_hash == variant_observation_hash
                    && base_actions.len() == PROPOSAL_SAMPLES
                    && variant_actions.len() == PROPOSAL_SAMPLES
                    && mismatched_proposals == 0,
                errors,
            }
        });

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
            .is_none_or(|probe| probe.contract_passed);
        let threat_probe_ok = threat_probe_report
            .as_ref()
            .is_none_or(|probe| probe.contract_passed);
        let observation_safety_ok = observation_safety_report
            .as_ref()
            .is_none_or(|probe| probe.contract_passed);

        // Check only the scenario's declared mechanical/ordering contract.
        // `best_actions()` uses the benchmark's required explicit-root ordering:
        // net terminal outcome, then VP margin, then shorter mean game length.
        let tactical_passed = if !g0_passed
            || proposal_errors > 0
            || !proposal_probe_ok
            || !threat_probe_ok
            || !observation_safety_ok
            || any_errors
        {
            false
        } else if scenario.enforce_explicit_root_ordering
            && best_stat.map(|stat| &stat.action) != Some(&expected_best_action)
        {
            false
        } else if scenario.enforce_explicit_root_ordering && scenario.is_negative_control {
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
            threat_probe: threat_probe_report,
            observation_safety: observation_safety_report,
            actions: action_reports,
        });
    }

    let summary = BenchmarkSummary {
        benchmark: "gpu-latent-threat-benchmark",
        device_name,
        rollouts_per_action: args.rollouts_per_action,
        rollout_steps: args.rollout_steps,
        seed: args.seed,
        proposal_samples: PROPOSAL_SAMPLES,
        scenario_seed_stride: SCENARIO_SEED_STRIDE,
        proposal_seed_xor: PROPOSAL_SEED_XOR,
        proposal_probe_seed_xor: PROPOSAL_PROBE_SEED_XOR,
        observation_safety_seed_xor: OBSERVATION_SAFETY_SEED_XOR,
        comparison_ordering: "net terminal outcome > mean VP margin > shorter mean game length",
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
