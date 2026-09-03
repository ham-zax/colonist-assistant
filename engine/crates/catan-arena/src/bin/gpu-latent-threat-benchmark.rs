use std::path::PathBuf;

use colonist_catan_arena::tactical_corpus::{
    apply_hidden_variant, build_state, default_corpus_path, load_tactical_corpus,
    rebalance_tactical_bank, verify_mechanical_consequence, TacticalCorpus, TacticalScenario,
};
use colonist_catan_core::{Action, GameState, Phase, SETTLEMENT_COST};
use colonist_catan_search::{
    CudaSimEngine, CudaSimRootActionStats, DecisiveContinuationDiagnostic, HARD_VETO_POSTERIOR,
    TurnPlanConfig, apply_closeout_root_impacts, belief_domestic_trade_assessment,
    compute_spatial_root_impacts, diagnose_decisive_continuation, plan_current_turn,
    posterior_expected_tactical_threat_weight, posterior_immediate_threat_weight,
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
    mean_turn: f32,
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

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PosteriorRootReport {
    action: String,
    net_terminal_outcome: f32,
    mean_victory_margin: f32,
    variance: f32,
    mean_turn: f32,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct StrictSafetyReport {
    action: String,
    threat: Option<String>,
    posterior: f32,
    dirty_monopoly_posterior: f32,
    hard_veto_posterior: f32,
    hard_veto_threshold: f32,
    hard_veto: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PosteriorGridPointReport {
    posterior_mass: f32,
    base_world_weight: f32,
    variant_world_weight: f32,
    immediate_threat_weight: f32,
    expected_tactical_threat_weight: f32,
    selected_root: String,
    roots: Vec<PosteriorRootReport>,
    strict_safety: Option<StrictSafetyReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PosteriorSensitivityReport {
    actor: u8,
    base_world: &'static str,
    variant_id: String,
    setup_action: Option<String>,
    observation_hash: u64,
    variant_observation_hash: u64,
    observation_fixed: bool,
    expected_zero_root: Option<String>,
    expected_full_root: Option<String>,
    contract_passed: bool,
    errors: Vec<String>,
    points: Vec<PosteriorGridPointReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseoutRootReport {
    action: String,
    value: f32,
    completion_mass: f32,
    decisive_completion_mass: f32,
    response_windows: Option<f32>,
    closeout_gain: f32,
    promotion: Option<String>,
    decisive_endpoint: Option<String>,
    diagnostic_response_windows: Option<u8>,
    endpoint_strategic_value: Option<f32>,
    completion_timing: Option<&'static str>,
    diagnostic_transitions: Option<u8>,
    diagnostic_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CloseoutProbeReport {
    same_turn: Option<CloseoutRootReport>,
    delayed: Option<CloseoutRootReport>,
    expected_same_turn_endpoint: Option<String>,
    expected_delayed_endpoint: Option<String>,
    same_endpoint_matched: bool,
    delayed_stronger: bool,
    contract_passed: bool,
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
    posterior_sensitivity: Option<PosteriorSensitivityReport>,
    closeout_probe: Option<CloseoutProbeReport>,
    actions: Vec<ActionReport>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkSummary {
    benchmark: &'static str,
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
    posterior_seed_xor: u64,
    posterior_grid: [f32; 9],
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
const POSTERIOR_SEED_XOR: u64 = 0xc3c3_3c3c;
const POSTERIOR_GRID: [f32; 9] = [0.0, 0.05, 0.15, 0.30, 0.50, 0.75, 0.95, 0.995, 1.0];
const CLOSEOUT_PLANNER_NODES: u32 = 4_000;
const CLOSEOUT_DIAGNOSTIC_TRANSITIONS: u8 = 64;
const CLOSEOUT_DIAGNOSTIC_DEPTH: u8 = 3;
const CLOSEOUT_DIAGNOSTIC_BRANCH_CAP: usize = 12;
const CLOSEOUT_DIAGNOSTIC_NODES: u32 = 2_000;

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

fn mixture_variance(
    base_mean: f32,
    base_variance: f32,
    variant_mean: f32,
    variant_variance: f32,
    variant_weight: f32,
) -> f32 {
    let base_weight = 1.0 - variant_weight;
    let mean = base_weight * base_mean + variant_weight * variant_mean;
    (base_weight * (base_variance + base_mean.powi(2))
        + variant_weight * (variant_variance + variant_mean.powi(2))
        - mean.powi(2))
    .max(0.0)
}

fn posterior_root_report(
    base: &CudaSimRootActionStats,
    variant: &CudaSimRootActionStats,
    variant_weight: f32,
) -> PosteriorRootReport {
    let base_weight = 1.0 - variant_weight;
    let base_margin = base.mean_victory_margin();
    let variant_margin = variant.mean_victory_margin();
    PosteriorRootReport {
        action: format!("{:?}", base.action),
        net_terminal_outcome: base_weight * base.net_terminal_outcome()
            + variant_weight * variant.net_terminal_outcome(),
        mean_victory_margin: base_weight * base_margin + variant_weight * variant_margin,
        variance: mixture_variance(
            base_margin,
            base.victory_margin_variance(),
            variant_margin,
            variant.victory_margin_variance(),
            variant_weight,
        ),
        mean_turn: base_weight * base.mean_turn + variant_weight * variant.mean_turn,
    }
}

fn best_posterior_root(roots: &[PosteriorRootReport]) -> Option<&PosteriorRootReport> {
    roots.iter().max_by(|left, right| {
        left.net_terminal_outcome
            .total_cmp(&right.net_terminal_outcome)
            .then_with(|| left.mean_victory_margin.total_cmp(&right.mean_victory_margin))
            .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
    })
}

fn run_posterior_sensitivity_probe(
    engine: &mut CudaSimEngine,
    scenario: &TacticalScenario,
    state: &GameState,
    rollouts_per_action: usize,
    rollout_steps: usize,
    scenario_seed: u64,
) -> Option<PosteriorSensitivityReport> {
    let probe = scenario.posterior_sensitivity_probe.as_ref()?;
    let mut errors = Vec::new();
    let mut base_world = state.clone();
    let mut variant_world = match apply_hidden_variant(state, &probe.variant) {
        Ok(world) => world,
        Err(error) => {
            errors.push(error);
            state.clone()
        }
    };

    let setup_action = probe.setup_action.as_ref().map(|spec| spec.to_action());
    if let Some(action) = &setup_action {
        if !base_world.legal_actions().contains(action) {
            errors.push(format!("base setup action is not legal: {action:?}"));
        } else if let Err(error) = base_world.apply(action) {
            errors.push(format!("base setup action {action:?} failed: {error:?}"));
        }
        if !variant_world.legal_actions().contains(action) {
            errors.push(format!("variant setup action is not legal: {action:?}"));
        } else if let Err(error) = variant_world.apply(action) {
            errors.push(format!("variant setup action {action:?} failed: {error:?}"));
        }
    }

    let observation_hash = base_world.observation_hash(probe.actor);
    let variant_observation_hash = variant_world.observation_hash(probe.actor);
    let observation_fixed = observation_hash == variant_observation_hash;
    if !observation_fixed {
        errors.push("posterior worlds do not share one public observation".into());
    }

    let candidate_roots = probe
        .candidate_roots
        .iter()
        .map(|spec| spec.to_action())
        .collect::<Vec<_>>();
    if candidate_roots.is_empty() {
        errors.push("posterior probe has no candidate roots".into());
    }
    for action in &candidate_roots {
        if !base_world.legal_actions().contains(action) {
            errors.push(format!("base posterior root is not legal: {action:?}"));
        }
        if !variant_world.legal_actions().contains(action) {
            errors.push(format!("variant posterior root is not legal: {action:?}"));
        }
    }

    let mut points = Vec::new();
    if errors.is_empty() {
        if let Err(error) = engine.upload_states(&[base_world.clone(), variant_world.clone()]) {
            errors.push(format!("posterior world upload failed: {error:?}"));
        } else {
            let rows = vec![candidate_roots.clone(), candidate_roots.clone()];
            match engine.search_root_actions(
                &rows,
                rollouts_per_action,
                rollout_steps,
                scenario_seed ^ POSTERIOR_SEED_XOR,
            ) {
                Ok(search) => {
                    if search.rows.len() != 2
                        || search.rows[0].len() != candidate_roots.len()
                        || search.rows[1].len() != candidate_roots.len()
                    {
                        errors.push("posterior root search returned an unexpected row shape".into());
                    } else if search.rows.iter().flatten().any(|stats| stats.errors > 0) {
                        errors.push("posterior root search reported rollout errors".into());
                    } else {
                        for &posterior_mass in &POSTERIOR_GRID {
                            let base_weight = 1.0 - posterior_mass;
                            let roots = search.rows[0]
                                .iter()
                                .zip(&search.rows[1])
                                .map(|(base, variant)| {
                                    if base.action != variant.action {
                                        errors.push(format!(
                                            "posterior row action mismatch: {:?} vs {:?}",
                                            base.action, variant.action
                                        ));
                                    }
                                    posterior_root_report(base, variant, posterior_mass)
                                })
                                .collect::<Vec<_>>();
                            let selected_root = best_posterior_root(&roots)
                                .map(|root| root.action.clone())
                                .unwrap_or_else(|| "None".into());
                            let immediate_threat_weight = posterior_immediate_threat_weight(
                                [(&base_world, base_weight), (&variant_world, posterior_mass)],
                                probe.actor,
                            );
                            let expected_tactical_threat_weight =
                                posterior_expected_tactical_threat_weight(
                                    [(&base_world, base_weight), (&variant_world, posterior_mass)],
                                    probe.actor,
                                );
                            let strict_safety = probe.strict_safety_action.as_ref().map(|spec| {
                                let action = spec.to_action();
                                let assessment = belief_domestic_trade_assessment(
                                    [(&base_world, base_weight), (&variant_world, posterior_mass)],
                                    &action,
                                );
                                StrictSafetyReport {
                                    action: format!("{action:?}"),
                                    threat: assessment.threat.map(|threat| format!("{threat:?}")),
                                    posterior: assessment.posterior,
                                    dirty_monopoly_posterior: assessment.dirty_monopoly_posterior,
                                    hard_veto_posterior: assessment.hard_veto_posterior,
                                    hard_veto_threshold: HARD_VETO_POSTERIOR,
                                    hard_veto: assessment.hard_veto,
                                }
                            });
                            points.push(PosteriorGridPointReport {
                                posterior_mass,
                                base_world_weight: base_weight,
                                variant_world_weight: posterior_mass,
                                immediate_threat_weight,
                                expected_tactical_threat_weight,
                                selected_root,
                                roots,
                                strict_safety,
                            });
                        }
                    }
                }
                Err(error) => errors.push(format!("posterior root search failed: {error:?}")),
            }
        }
    }

    let expected_zero_root = probe
        .expected_zero_root
        .as_ref()
        .map(|spec| format!("{:?}", spec.to_action()));
    let expected_full_root = probe
        .expected_full_root
        .as_ref()
        .map(|spec| format!("{:?}", spec.to_action()));
    let zero_root = points.first().map(|point| point.selected_root.as_str());
    let five_root = points.get(1).map(|point| point.selected_root.as_str());
    let full_root = points.last().map(|point| point.selected_root.as_str());
    let endpoints_ok = expected_zero_root
        .as_deref()
        .is_none_or(|expected| zero_root == Some(expected))
        && expected_full_root
            .as_deref()
            .is_none_or(|expected| full_root == Some(expected));
    let switch_ok = !probe.require_switch || zero_root != full_root;
    let tiny_probability_ok = !probe.require_five_percent_stable || zero_root == five_root;
    let strict_transition_ok = if probe.require_strict_safety_transition {
        let at = |mass: f32| {
            points
                .iter()
                .find(|point| (point.posterior_mass - mass).abs() < 1e-6)
                .and_then(|point| point.strict_safety.as_ref())
        };
        at(0.95).is_some_and(|safety| !safety.hard_veto)
            && at(0.995).is_some_and(|safety| safety.hard_veto)
            && at(1.0).is_some_and(|safety| safety.hard_veto)
    } else {
        true
    };
    let contract_passed = errors.is_empty()
        && observation_fixed
        && endpoints_ok
        && switch_ok
        && tiny_probability_ok
        && strict_transition_ok;

    Some(PosteriorSensitivityReport {
        actor: probe.actor,
        base_world: "scenarioState",
        variant_id: probe.variant.id.clone(),
        setup_action: setup_action.map(|action| format!("{action:?}")),
        observation_hash,
        variant_observation_hash,
        observation_fixed,
        expected_zero_root,
        expected_full_root,
        contract_passed,
        errors,
        points,
    })
}

fn run_closeout_probe(
    scenario: &TacticalScenario,
    state: &GameState,
) -> Option<CloseoutProbeReport> {
    let probe = scenario.closeout_probe.as_ref()?;
    let same_turn_action = probe.same_turn_root.to_action();
    let delayed_action = probe.delayed_root.to_action();
    let plans = plan_current_turn(
        state,
        TurnPlanConfig {
            maximum_nodes: CLOSEOUT_PLANNER_NODES,
            root_cap: 64,
            ..TurnPlanConfig::default()
        },
    );
    let roots = vec![same_turn_action.clone(), delayed_action.clone()];
    let mut impacts = compute_spatial_root_impacts(state, state.actor(), &roots);
    apply_closeout_root_impacts(&mut impacts, &plans);

    let same_diagnostic = diagnose_decisive_continuation(
        state,
        &same_turn_action,
        CLOSEOUT_DIAGNOSTIC_TRANSITIONS,
        CLOSEOUT_DIAGNOSTIC_DEPTH,
        CLOSEOUT_DIAGNOSTIC_BRANCH_CAP,
        CLOSEOUT_DIAGNOSTIC_NODES,
    );
    let delayed_diagnostic = diagnose_decisive_continuation(
        state,
        &delayed_action,
        CLOSEOUT_DIAGNOSTIC_TRANSITIONS,
        CLOSEOUT_DIAGNOSTIC_DEPTH,
        CLOSEOUT_DIAGNOSTIC_BRANCH_CAP,
        CLOSEOUT_DIAGNOSTIC_NODES,
    );

    let report_for =
        |action: &Action, diagnostic: &Result<DecisiveContinuationDiagnostic, String>| {
            let plan = plans.iter().find(|plan| &plan.first_action == action)?;
            let impact = impacts
                .actions
                .iter()
                .find(|impact| &impact.action == action)?;
            let resolved = diagnostic.as_ref().ok();
            Some(CloseoutRootReport {
                action: format!("{action:?}"),
                value: plan.value,
                completion_mass: plan.completion_mass,
                decisive_completion_mass: plan.decisive_completion_mass,
                response_windows: plan.response_windows,
                closeout_gain: impact.closeout_gain,
                promotion: impact.promotion.map(|promotion| format!("{promotion:?}")),
                decisive_endpoint: resolved.map(|entry| format!("{:?}", entry.decisive_action)),
                diagnostic_response_windows: resolved.map(|entry| entry.response_windows),
                endpoint_strategic_value: resolved.map(|entry| entry.endpoint_strategic_value),
                completion_timing: resolved.map(|entry| {
                    if entry.same_turn {
                        "sameTurn"
                    } else {
                        "delayed"
                    }
                }),
                diagnostic_transitions: resolved.map(|entry| entry.transitions),
                diagnostic_error: diagnostic.as_ref().err().cloned(),
            })
        };
    let same_turn = report_for(&same_turn_action, &same_diagnostic);
    let delayed = report_for(&delayed_action, &delayed_diagnostic);
    let expected_same_turn_endpoint = probe
        .expected_same_turn_endpoint
        .as_ref()
        .map(|spec| format!("{:?}", spec.to_action()));
    let expected_delayed_endpoint = probe
        .expected_delayed_endpoint
        .as_ref()
        .map(|spec| format!("{:?}", spec.to_action()));
    let response_contract = same_turn.as_ref().is_some_and(|root| {
        root.decisive_completion_mass > f32::EPSILON
            && root.response_windows.is_some_and(|windows| windows <= 1e-6)
            && root.completion_timing == Some("sameTurn")
            && root.diagnostic_response_windows == Some(0)
    }) && delayed.as_ref().is_some_and(|root| {
        root.response_windows.is_some_and(|windows| windows >= 1.0)
            && root.completion_timing == Some("delayed")
            && root
                .diagnostic_response_windows
                .is_some_and(|windows| windows >= 1)
    });
    let expected_endpoints_ok = expected_same_turn_endpoint
        .as_deref()
        .is_none_or(|expected| {
            same_turn
                .as_ref()
                .and_then(|root| root.decisive_endpoint.as_deref())
                == Some(expected)
        })
        && expected_delayed_endpoint.as_deref().is_none_or(|expected| {
            delayed
                .as_ref()
                .and_then(|root| root.decisive_endpoint.as_deref())
                == Some(expected)
        });
    let same_endpoint_matched = same_turn
        .as_ref()
        .and_then(|root| root.decisive_endpoint.as_deref())
        .zip(
            delayed
                .as_ref()
                .and_then(|root| root.decisive_endpoint.as_deref()),
        )
        .is_some_and(|(same, later)| same == later);
    let delayed_stronger = same_turn
        .as_ref()
        .and_then(|root| root.endpoint_strategic_value)
        .zip(
            delayed
                .as_ref()
                .and_then(|root| root.endpoint_strategic_value),
        )
        .is_some_and(|(same, later)| later > same + 1e-6);
    let contract_passed = response_contract
        && expected_endpoints_ok
        && (!probe.require_same_endpoint || same_endpoint_matched)
        && (!probe.require_delayed_stronger || delayed_stronger);
    Some(CloseoutProbeReport {
        same_turn,
        delayed,
        expected_same_turn_endpoint,
        expected_delayed_endpoint,
        same_endpoint_matched,
        delayed_stronger,
        contract_passed,
    })
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
                    if probe.match_consequence {
                        proposal_matches_declared_consequence(scenario, &probe_state, action)
                    } else if probe.match_kind {
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

        let posterior_sensitivity_report = run_posterior_sensitivity_probe(
            &mut engine,
            scenario,
            &state,
            args.rollouts_per_action,
            args.rollout_steps,
            scenario_seed,
        );
        let closeout_probe_report = run_closeout_probe(scenario, &state);

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
                    mean_turn: stat.mean_turn,
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
        let posterior_sensitivity_ok = posterior_sensitivity_report
            .as_ref()
            .is_none_or(|probe| probe.contract_passed);
        let closeout_probe_ok = closeout_probe_report
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
            || !posterior_sensitivity_ok
            || !closeout_probe_ok
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
            posterior_sensitivity: posterior_sensitivity_report,
            closeout_probe: closeout_probe_report,
            actions: action_reports,
        });
    }

    let summary = BenchmarkSummary {
        benchmark: "gpu-latent-threat-benchmark",
        corpus_schema_version: corpus.schema_version,
        device_name,
        rollouts_per_action: args.rollouts_per_action,
        rollout_steps: args.rollout_steps,
        seed: args.seed,
        proposal_samples: PROPOSAL_SAMPLES,
        scenario_seed_stride: SCENARIO_SEED_STRIDE,
        proposal_seed_xor: PROPOSAL_SEED_XOR,
        proposal_probe_seed_xor: PROPOSAL_PROBE_SEED_XOR,
        observation_safety_seed_xor: OBSERVATION_SAFETY_SEED_XOR,
        posterior_seed_xor: POSTERIOR_SEED_XOR,
        posterior_grid: POSTERIOR_GRID,
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
