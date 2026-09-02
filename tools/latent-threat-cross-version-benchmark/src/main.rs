use std::path::{Path, PathBuf};
use std::process::Command;

use colonist_catan_core::{Action, GameState, Phase, SETTLEMENT_COST};
use colonist_catan_search::CudaSimEngine;
use serde::Serialize;

#[path = "../../../engine/crates/catan-arena/src/tactical_corpus.rs"]
mod tactical_corpus;

use tactical_corpus::{
    TacticalCorpus, TacticalScenario, apply_hidden_variant, build_state, load_tactical_corpus,
    rebalance_tactical_bank, verify_mechanical_consequence,
};

const DEFAULT_ROLLOUTS_PER_ACTION: usize = 512;
const DEFAULT_ROLLOUT_STEPS: usize = 96;
const DEFAULT_SEED: u64 = 2026092101;
const DEFAULT_PROPOSAL_SAMPLES: usize = 256;
const SCENARIO_SEED_STRIDE: u64 = 1009;
const PROPOSAL_SEED_XOR: u64 = 0xa5a5_5a5a;
const PROPOSAL_PROBE_SEED_XOR: u64 = 0x5a5a_a5a5;
const OBSERVATION_SAFETY_SEED_XOR: u64 = 0x3c3c_c3c3;
const COMPARISON_ORDERING: &str =
    "net terminal outcome > mean VP margin > shorter mean game length";

#[derive(Debug)]
struct CliArgs {
    corpus_path: PathBuf,
    rollouts_per_action: usize,
    rollout_steps: usize,
    seed: u64,
    proposal_samples: usize,
    benchmark_harness_sha: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct RootReport {
    action: String,
    proposal_frequency: usize,
    proposal_rate: f32,
    samples: u32,
    errors: u32,
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
    match_mode: &'static str,
    proposal_frequency: usize,
    proposal_rate: f32,
    min_proposal_rate: Option<f32>,
    max_proposal_rate: Option<f32>,
    frozen_contract_met: bool,
    errors: usize,
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
struct PosteriorVariantReport {
    id: String,
    weight: f32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioReport {
    id: String,
    family: String,
    declared_consequence: String,
    is_negative_control: bool,
    g0_passed: bool,
    posterior_variants: Vec<PosteriorVariantReport>,
    candidate_roots: Vec<String>,
    expected_best_root: String,
    negative_control_root: Option<String>,
    selected_root: String,
    expected_root_selected: bool,
    proposal_errors: usize,
    proposal_probe: Option<ProposalProbeReport>,
    observation_safety: Option<ObservationSafetyReport>,
    roots: Vec<RootReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkSummary {
    benchmark: &'static str,
    engine_under_test_sha: String,
    benchmark_harness_sha: String,
    engine_production_dirty: bool,
    engine_ptx_blob: String,
    corpus_blob: String,
    corpus_schema_version: u32,
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
    g0_failures: usize,
    cuda_failures: usize,
    scenarios: Vec<ScenarioReport>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("benchmark crate must live at tools/<name>")
        .to_path_buf()
}

fn default_corpus_path() -> PathBuf {
    repo_root().join("tests/fixtures/latent-threat-tactical-corpus.json")
}

fn parse_args() -> CliArgs {
    let mut corpus_path = default_corpus_path();
    let mut rollouts_per_action = DEFAULT_ROLLOUTS_PER_ACTION;
    let mut rollout_steps = DEFAULT_ROLLOUT_STEPS;
    let mut seed = DEFAULT_SEED;
    let mut proposal_samples = DEFAULT_PROPOSAL_SAMPLES;
    let mut benchmark_harness_sha = std::env::var("BENCHMARK_HARNESS_SHA").ok();

    let args = std::env::args().collect::<Vec<_>>();
    let mut index = 1usize;
    while index < args.len() {
        let value = args.get(index + 1).cloned();
        match args[index].as_str() {
            "--corpus" => corpus_path = PathBuf::from(value.expect("--corpus requires a path")),
            "--rollouts-per-action" => {
                rollouts_per_action = value
                    .expect("--rollouts-per-action requires a value")
                    .parse()
                    .expect("invalid rollouts-per-action");
            }
            "--rollout-steps" => {
                rollout_steps = value
                    .expect("--rollout-steps requires a value")
                    .parse()
                    .expect("invalid rollout-steps");
            }
            "--seed" => {
                seed = value
                    .expect("--seed requires a value")
                    .parse()
                    .expect("invalid seed");
            }
            "--proposal-samples" => {
                proposal_samples = value
                    .expect("--proposal-samples requires a value")
                    .parse()
                    .expect("invalid proposal-samples");
            }
            "--benchmark-harness-sha" => {
                benchmark_harness_sha = Some(value.expect("--benchmark-harness-sha requires a SHA"));
            }
            other => panic!("unknown argument: {other}"),
        }
        index += 2;
    }

    CliArgs {
        corpus_path,
        rollouts_per_action,
        rollout_steps,
        seed,
        proposal_samples,
        benchmark_harness_sha: benchmark_harness_sha
            .expect("set BENCHMARK_HARNESS_SHA or pass --benchmark-harness-sha"),
    }
}

fn git_stdout(root: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .unwrap_or_else(|error| panic!("failed to run git {args:?}: {error}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout)
        .expect("git output must be UTF-8")
        .trim()
        .to_owned()
}

fn engine_production_dirty(root: &Path) -> bool {
    !git_stdout(
        root,
        &[
            "status",
            "--short",
            "--",
            "engine/crates/catan-core",
            "engine/crates/catan-search",
        ],
    )
    .is_empty()
}

fn same_action_kind(left: &Action, right: &Action) -> bool {
    std::mem::discriminant(left) == std::mem::discriminant(right)
}

fn proposal_matches_declared_consequence(
    scenario: &TacticalScenario,
    state: &GameState,
    action: &Action,
) -> bool {
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return false;
    }
    let actor = scenario.state.active_player;
    match scenario.declared_consequence.as_str() {
        "road_building_settlement_win" => next.legal_actions().into_iter().any(|follow_up| {
            if !matches!(follow_up, Action::BuildSettlement { .. }) {
                return false;
            }
            let mut won = next.clone();
            won.apply(&follow_up).is_ok() && won.winner() == Some(actor)
        }),
        "road_building_longest_road_win" => {
            next.longest_road_holder == Some(actor) && next.winner() == Some(actor)
        }
        "road_building_bypass" => {
            let Some(expect) = &scenario.mechanical else {
                return false;
            };
            let (Some(vertex), Some(cut_player), Some(holder)) = (
                expect.bypass_cut_vertex,
                expect.bypass_cut_player,
                expect.bypass_holder,
            ) else {
                return false;
            };
            let mut cut = next;
            cut.current_player = cut_player;
            cut.phase = Phase::Main;
            cut.players[cut_player as usize].resources = SETTLEMENT_COST;
            if rebalance_tactical_bank(&mut cut).is_err() {
                return false;
            }
            let cut_action = Action::BuildSettlement { vertex };
            cut.legal_actions().contains(&cut_action)
                && cut.apply(&cut_action).is_ok()
                && cut.longest_road_holder == Some(holder)
        }
        _ => false,
    }
}

fn sample_proposals(
    engine: &mut CudaSimEngine,
    state: &GameState,
    samples: usize,
    seed: u64,
) -> Result<Vec<Action>, String> {
    engine
        .upload_states(&vec![state.clone(); samples])
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
    let root = repo_root();
    let corpus: TacticalCorpus = load_tactical_corpus(&args.corpus_path)
        .unwrap_or_else(|error| panic!("failed to load corpus {:?}: {error}", args.corpus_path));
    let engine_under_test_sha = git_stdout(&root, &["rev-parse", "HEAD"]);
    let engine_ptx_blob = git_stdout(
        &root,
        &["hash-object", "engine/crates/catan-search/src/cuda/sim.ptx"],
    );
    let corpus_path = args
        .corpus_path
        .strip_prefix(&root)
        .unwrap_or(&args.corpus_path)
        .to_string_lossy()
        .into_owned();
    let corpus_blob = git_stdout(&root, &["hash-object", &corpus_path]);

    let mut engine = CudaSimEngine::new()
        .unwrap_or_else(|error| panic!("CUDA initialization failed: {error:?}"));
    let device_name = engine.device_identity().name.clone();

    let mut g0_failures = 0usize;
    let mut cuda_failures = 0usize;
    let mut scenarios = Vec::with_capacity(corpus.scenarios.len());

    for (index, scenario) in corpus.scenarios.iter().enumerate() {
        let g0_passed = verify_mechanical_consequence(scenario).is_ok();
        if !g0_passed {
            g0_failures += 1;
        }
        let state = build_state(&scenario.state)
            .unwrap_or_else(|error| panic!("invalid scenario {}: {error}", scenario.id));
        let candidate_actions = scenario
            .candidate_roots
            .iter()
            .map(|root| root.to_action())
            .collect::<Vec<_>>();
        let scenario_seed = args
            .seed
            .wrapping_add((index as u64) * SCENARIO_SEED_STRIDE);

        let (sampled_actions, proposal_errors) = match sample_proposals(
            &mut engine,
            &state,
            args.proposal_samples,
            scenario_seed ^ PROPOSAL_SEED_XOR,
        ) {
            Ok(actions) => (actions, 0usize),
            Err(error) => {
                eprintln!("proposal sampling failed for {}: {error}", scenario.id);
                cuda_failures += 1;
                (Vec::new(), 1usize)
            }
        };

        let proposal_probe = scenario.proposal_probe.as_ref().map(|probe| {
            let mut probe_state = state.clone();
            probe_state.current_player = probe.player;
            probe_state.players[probe.player as usize].resources = probe.resources;
            let probe_action = probe.action.to_action();
            let valid = rebalance_tactical_bank(&mut probe_state)
                .and_then(|_| probe_state.validate())
                .is_ok();
            let legal = valid && probe_state.legal_actions().contains(&probe_action);
            let (generated, errors) = if legal {
                match sample_proposals(
                    &mut engine,
                    &probe_state,
                    args.proposal_samples,
                    scenario_seed ^ PROPOSAL_PROBE_SEED_XOR,
                ) {
                    Ok(actions) => (actions, 0usize),
                    Err(error) => {
                        eprintln!("proposal probe failed for {}: {error}", scenario.id);
                        cuda_failures += 1;
                        (Vec::new(), 1usize)
                    }
                }
            } else {
                (Vec::new(), 0usize)
            };
            let proposal_frequency = generated
                .iter()
                .filter(|action| {
                    if probe.match_consequence {
                        proposal_matches_declared_consequence(scenario, &probe_state, action)
                    } else if probe.match_kind {
                        same_action_kind(action, &probe_action)
                    } else {
                        **action == probe_action
                    }
                })
                .count();
            let proposal_rate = proposal_frequency as f32 / args.proposal_samples.max(1) as f32;
            let frozen_contract_met = legal
                && errors == 0
                && probe
                    .min_proposal_rate
                    .is_none_or(|minimum| proposal_rate + 1e-6 >= minimum)
                && probe
                    .max_proposal_rate
                    .is_none_or(|maximum| proposal_rate <= maximum + 1e-6);
            ProposalProbeReport {
                action: format!("{probe_action:?}"),
                legal,
                match_mode: if probe.match_consequence {
                    "declared-consequence"
                } else if probe.match_kind {
                    "action-kind"
                } else {
                    "exact-action"
                },
                proposal_frequency,
                proposal_rate,
                min_proposal_rate: probe.min_proposal_rate,
                max_proposal_rate: probe.max_proposal_rate,
                frozen_contract_met,
                errors,
            }
        });

        let observation_safety = scenario.observation_safety_probe.as_ref().map(|probe| {
            let variant = apply_hidden_variant(&state, &probe.variant)
                .unwrap_or_else(|error| panic!("invalid hidden variant {}: {error}", scenario.id));
            let observation_hash = state.observation_hash(probe.actor);
            let variant_observation_hash = variant.observation_hash(probe.actor);
            let seed = scenario_seed ^ OBSERVATION_SAFETY_SEED_XOR;
            let base_actions = sample_proposals(&mut engine, &state, args.proposal_samples, seed);
            let variant_actions = sample_proposals(&mut engine, &variant, args.proposal_samples, seed);
            let mut errors = 0usize;
            if base_actions.is_err() || variant_actions.is_err() {
                errors = 1;
                cuda_failures += 1;
            }
            let base_actions = base_actions.unwrap_or_default();
            let variant_actions = variant_actions.unwrap_or_default();
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
                samples: args.proposal_samples,
                contract_passed: errors == 0
                    && observation_hash == variant_observation_hash
                    && base_actions.len() == args.proposal_samples
                    && variant_actions.len() == args.proposal_samples
                    && mismatched_proposals == 0,
                errors,
            }
        });

        let search_result = engine
            .upload_states(std::slice::from_ref(&state))
            .and_then(|_| {
                engine.search_root_actions(
                    std::slice::from_ref(&candidate_actions),
                    args.rollouts_per_action,
                    args.rollout_steps,
                    scenario_seed,
                )
            });
        let row = match search_result {
            Ok(result) => result.rows.into_iter().next().unwrap_or_default(),
            Err(error) => {
                eprintln!("explicit-root search failed for {}: {error:?}", scenario.id);
                cuda_failures += 1;
                Vec::new()
            }
        };
        let best = row.iter().max_by(|left, right| {
            left.net_terminal_outcome()
                .total_cmp(&right.net_terminal_outcome())
                .then_with(|| {
                    left.mean_victory_margin()
                        .total_cmp(&right.mean_victory_margin())
                })
                .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
        });
        let selected_root = best
            .map(|stat| format!("{:?}", stat.action))
            .unwrap_or_else(|| "None".to_owned());
        let expected_best = scenario.expected_best_root.to_action();
        let expected_root_selected = best.map(|stat| &stat.action) == Some(&expected_best);
        let roots = row
            .iter()
            .map(|stat| {
                let proposal_frequency = sampled_actions
                    .iter()
                    .filter(|action| *action == &stat.action)
                    .count();
                RootReport {
                    action: format!("{:?}", stat.action),
                    proposal_frequency,
                    proposal_rate: proposal_frequency as f32
                        / args.proposal_samples.max(1) as f32,
                    samples: stat.samples,
                    errors: stat.errors,
                    terminal_rate: stat.terminal_rate(),
                    net_terminal_outcome: stat.net_terminal_outcome(),
                    mean_victory_margin: stat.mean_victory_margin(),
                    variance: stat.victory_margin_variance(),
                }
            })
            .collect();
        let posterior_variants = scenario
            .threat_probe
            .as_ref()
            .map(|probe| {
                probe
                    .variants
                    .iter()
                    .map(|variant| PosteriorVariantReport {
                        id: variant.id.clone(),
                        weight: variant.weight,
                    })
                    .collect()
            })
            .unwrap_or_default();

        scenarios.push(ScenarioReport {
            id: scenario.id.clone(),
            family: scenario.family.clone(),
            declared_consequence: scenario.declared_consequence.clone(),
            is_negative_control: scenario.is_negative_control,
            g0_passed,
            posterior_variants,
            candidate_roots: candidate_actions
                .iter()
                .map(|action| format!("{action:?}"))
                .collect(),
            expected_best_root: format!("{expected_best:?}"),
            negative_control_root: scenario
                .negative_control_root
                .as_ref()
                .map(|root| format!("{:?}", root.to_action())),
            selected_root,
            expected_root_selected,
            proposal_errors,
            proposal_probe,
            observation_safety,
            roots,
        });
    }

    let summary = BenchmarkSummary {
        benchmark: "latent-threat-cross-version-benchmark",
        engine_under_test_sha,
        benchmark_harness_sha: args.benchmark_harness_sha,
        engine_production_dirty: engine_production_dirty(&root),
        engine_ptx_blob,
        corpus_blob,
        corpus_schema_version: corpus.schema_version,
        device_name,
        rollouts_per_action: args.rollouts_per_action,
        rollout_steps: args.rollout_steps,
        seed: args.seed,
        proposal_samples: args.proposal_samples,
        scenario_seed_stride: SCENARIO_SEED_STRIDE,
        proposal_seed_xor: PROPOSAL_SEED_XOR,
        proposal_probe_seed_xor: PROPOSAL_PROBE_SEED_XOR,
        observation_safety_seed_xor: OBSERVATION_SAFETY_SEED_XOR,
        comparison_ordering: COMPARISON_ORDERING,
        total_scenarios: corpus.scenarios.len(),
        g0_failures,
        cuda_failures,
        scenarios,
    };

    println!("{}", serde_json::to_string_pretty(&summary).unwrap());
    if g0_failures > 0 || cuda_failures > 0 {
        std::process::exit(2);
    }
}
