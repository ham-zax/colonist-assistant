use std::env;

use colonist_catan_core::{Action, GameState};
use colonist_catan_search::{OpeningActionValue, OpeningConfig, OpeningReport, solve_opening};
use serde_json::{Value, json};

#[derive(Clone, Copy)]
struct Config {
    seed: u64,
    players: u8,
    player_trades_enabled: bool,
    maximum_nodes: u32,
    root_width: usize,
    opponent_width: usize,
}

fn parse_config() -> Config {
    let mut config = Config {
        seed: 0,
        players: 4,
        player_trades_enabled: false,
        maximum_nodes: 12_000,
        root_width: 24,
        opponent_width: 4,
    };
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < args.len() {
        let value = args.get(index + 1).map(String::as_str);
        match args[index].as_str() {
            "--seed" => {
                config.seed = value
                    .and_then(|item| item.parse().ok())
                    .expect("valid seed")
            }
            "--players" => {
                config.players = value
                    .and_then(|item| item.parse().ok())
                    .expect("valid players")
            }
            "--maximum-nodes" => {
                config.maximum_nodes = value
                    .and_then(|item| item.parse().ok())
                    .expect("valid maximum nodes")
            }
            "--root-width" => {
                config.root_width = value
                    .and_then(|item| item.parse().ok())
                    .expect("valid root width")
            }
            "--opponent-width" => {
                config.opponent_width = value
                    .and_then(|item| item.parse().ok())
                    .expect("valid opponent width")
            }
            "--player-trades" => {
                config.player_trades_enabled = true;
                index += 1;
                continue;
            }
            "--no-player-trades" => {
                config.player_trades_enabled = false;
                index += 1;
                continue;
            }
            "--help" | "-h" => {
                println!(
                    "opening-generated-probe --seed N --players 2|3|4 \
                     [--player-trades|--no-player-trades] [--maximum-nodes N] \
                     [--root-width N] [--opponent-width N]"
                );
                std::process::exit(0);
            }
            other => panic!("unknown argument {other}"),
        }
        index += 2;
    }
    assert!((2..=4).contains(&config.players));
    config
}

fn action_json(action: &Action) -> Value {
    match action {
        Action::PlaceSettlement { vertex } => json!({
            "kind": "place_settlement",
            "vertex": vertex,
            "debug": format!("{action:?}"),
        }),
        Action::PlaceRoad { edge } => json!({
            "kind": "place_road",
            "edge": edge,
            "debug": format!("{action:?}"),
        }),
        _ => json!({"kind": "other", "debug": format!("{action:?}")}),
    }
}

fn candidate_json(candidate: &OpeningActionValue) -> Value {
    json!({
        "action": action_json(&candidate.action),
        "engineValueDiagnosticOnly": candidate.value,
        "endpointComplete": candidate.endpoint_complete,
        "authoritative": candidate.authoritative,
        "nodeBudget": candidate.node_budget,
        "visitedNodes": candidate.visited_nodes,
        "evidence": candidate.evidence.as_ref().map(|evidence| json!({
            "productionPips": evidence.production_pips,
            "startingHand": evidence.starting_hand,
            "maritimeRatios": evidence.maritime_ratios,
            "buildEtaRolls": evidence.build_eta_rolls,
            "settlementVertices": evidence.settlement_vertices,
            "roadEdges": evidence.road_edges,
            "victoryTerm": evidence.victory_term,
            "productionDiversityTerm": evidence.production_diversity_term,
            "buildAccessTerm": evidence.build_access_term,
            "conversionEfficiencyTerm": evidence.conversion_efficiency_term,
            "prospectivePortBuildGain": evidence.port_build_gain,
            "expansionTerm": evidence.expansion_term,
            "expansionVertex": evidence.expansion_vertex,
            "expansionRoadsRequired": evidence.expansion_roads_required,
            "expansionProjectEtaRolls": evidence.expansion_project_eta_rolls,
            "expansionProjectConversionEfficiency": evidence.expansion_project_conversion_efficiency,
            "expansionRealization": evidence.expansion_realization,
            "expansionProspectivePortBuildGain": evidence.expansion_port_build_gain,
            "scarcityTerm": evidence.scarcity_term,
            "concentrationPenalty": evidence.concentration_penalty,
            "causalDenialTerm": evidence.causal_denial_term,
            "ownValue": evidence.own_value,
            "rivalValue": evidence.rival_value,
            "rivalWeight": evidence.rival_weight,
        })),
    })
}

fn report_json(report: &OpeningReport) -> Value {
    json!({
        "chosenAction": report.chosen.as_ref().map(action_json),
        "nodes": report.nodes,
        "completedSetups": report.completed_setups,
        "complete": report.complete,
        "deadlineReached": report.deadline_reached,
        "candidates": report.actions.iter().map(candidate_json).collect::<Vec<_>>(),
    })
}

fn opening_config(config: Config) -> OpeningConfig {
    OpeningConfig {
        maximum_nodes: config.maximum_nodes,
        root_width: config.root_width,
        opponent_width: config.opponent_width,
        time_budget_ms: 0,
        opponent_maximizes: true,
        rollout_horizon: 16,
        rollout_count: 0,
    }
}

fn main() {
    let config = parse_config();
    let mut state = GameState::randomized_base_v1(config.seed, config.players);
    state.player_trades_enabled = config.player_trades_enabled;
    state.domestic_trade_disabled = if config.player_trades_enabled {
        0
    } else {
        (1u8 << config.players) - 1
    };
    state
        .validate()
        .expect("generated opening state must validate");

    let root = state.actor();
    let state_hash = format!("{:016x}", state.observation_hash(root));
    let settlement_report = solve_opening(&state, root, opening_config(config));
    let road_report = settlement_report.chosen.as_ref().and_then(|settlement| {
        let mut after_settlement = state.clone();
        after_settlement.apply(settlement).ok()?;
        Some(solve_opening(
            &after_settlement,
            root,
            opening_config(config),
        ))
    });

    println!(
        "{}",
        serde_json::to_string(&json!({
            "schemaVersion": 1,
            "kind": "opening-generated-deterministic-evidence",
            "featureSemanticsVersion": "opening-a2-causal-v1@8496200",
            "generator": "legacy-randomized-v1",
            "boardSeed": config.seed,
            "players": config.players,
            "playerTradesEnabled": config.player_trades_enabled,
            "splitGroup": format!("generated-seed-{}", config.seed),
            "stateHash": state_hash,
            "rootPlayer": root,
            "searchConfig": {
                "maximumNodes": config.maximum_nodes,
                "rootWidth": config.root_width,
                "opponentWidth": config.opponent_width,
                "timeBudgetMs": 0,
                "opponentMaximizes": true,
                "rolloutCount": 0,
            },
            "settlementDecision": report_json(&settlement_report),
            "setupRoadDecision": road_report.as_ref().map(report_json),
            "labelStatus": "unlabeled_deterministic_diagnostic",
            "labelAuthority": null,
        }))
        .expect("generated evidence must serialize")
    );
}
