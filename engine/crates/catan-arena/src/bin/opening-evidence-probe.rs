use std::env;
use std::fs;

use colonist_catan_core::{
    Board, Building, Edge, GameState, Hex, Phase, PlayerState, Port, Resource, Vertex,
};
use colonist_catan_search::{OpeningConfig, expansion_option_value, solve_opening};
use serde::Deserialize;
use serde_json::json;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Spec {
    num_players: u8,
    victory_target: u8,
    setup_step: u8,
    current_player: u8,
    root_player: u8,
    player_trades_enabled: bool,
    domestic_trade_disabled_mask: u8,
    bank: [u8; 5],
    hexes: Vec<HexSpec>,
    vertices: Vec<VertexSpec>,
    edges: Vec<EdgeSpec>,
    buildings: Vec<BuildingSpec>,
    roads: Vec<RoadSpec>,
    players: Vec<PlayerSpec>,
}

#[derive(Deserialize)]
struct HexSpec {
    id: String,
    resource: Option<String>,
    number: u8,
    coord: [i8; 2],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexSpec {
    id: String,
    adjacent_hexes: Vec<u8>,
    adjacent_vertices: Vec<u8>,
    adjacent_edges: Vec<u8>,
    port: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeSpec {
    id: String,
    vertices: [u8; 2],
    adjacent_hexes: Vec<u8>,
}

#[derive(Deserialize)]
struct BuildingSpec {
    vertex: u8,
    player: u8,
    kind: String,
}

#[derive(Deserialize)]
struct RoadSpec {
    edge: u8,
    player: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerSpec {
    resources: [u8; 5],
    public_victory_points: u8,
    roads_built: u8,
    settlements_built: u8,
    cities_built: u8,
}

fn resource(value: &str) -> Resource {
    match value {
        "lumber" => Resource::Lumber,
        "brick" => Resource::Brick,
        "wool" => Resource::Wool,
        "grain" => Resource::Grain,
        "ore" => Resource::Ore,
        other => panic!("unknown resource {other}"),
    }
}

fn port(value: Option<&str>) -> Option<Port> {
    match value {
        None => None,
        Some("generic") => Some(Port::Generic),
        Some(value) => Some(Port::Resource(resource(value))),
    }
}

fn main() {
    let path = env::args().nth(1).expect("state spec path required");
    let spec: Spec =
        serde_json::from_str(&fs::read_to_string(&path).expect("read spec")).expect("parse spec");

    let hex_ids = spec
        .hexes
        .iter()
        .map(|hex| hex.id.clone())
        .collect::<Vec<_>>();
    let vertex_ids = spec
        .vertices
        .iter()
        .map(|vertex| vertex.id.clone())
        .collect::<Vec<_>>();
    let edge_ids = spec
        .edges
        .iter()
        .map(|edge| edge.id.clone())
        .collect::<Vec<_>>();

    let board = Board {
        num_players: spec.num_players,
        hexes: spec
            .hexes
            .iter()
            .map(|hex| Hex {
                resource: hex.resource.as_deref().map(resource),
                number: hex.number,
                coord: (hex.coord[0], hex.coord[1]),
            })
            .collect(),
        vertices: spec
            .vertices
            .iter()
            .map(|vertex| Vertex {
                adjacent_hexes: vertex.adjacent_hexes.clone(),
                adjacent_vertices: vertex.adjacent_vertices.clone(),
                adjacent_edges: vertex.adjacent_edges.clone(),
                port: port(vertex.port.as_deref()),
            })
            .collect(),
        edges: spec
            .edges
            .iter()
            .map(|edge| Edge {
                vertices: edge.vertices,
                adjacent_hexes: edge.adjacent_hexes.clone(),
            })
            .collect(),
    };

    let mut state = GameState::new(board, spec.victory_target);
    state.setup_step = spec.setup_step;
    state.current_player = spec.current_player;
    state.phase = Phase::SetupSettlement;
    state.player_trades_enabled = spec.player_trades_enabled;
    state.domestic_trade_disabled = spec.domestic_trade_disabled_mask;
    state.bank = spec.bank;

    state.players = spec
        .players
        .iter()
        .map(|player| {
            let mut result = PlayerState::new();
            result.resources = player.resources;
            result.public_victory_points = player.public_victory_points;
            result.roads_left = 15u8.saturating_sub(player.roads_built);
            result.settlements_left = 5u8.saturating_sub(player.settlements_built);
            result.cities_left = 4u8.saturating_sub(player.cities_built);
            result
        })
        .collect();

    for building in &spec.buildings {
        state.buildings[building.vertex as usize] = Some(match building.kind.as_str() {
            "settlement" => Building::Settlement(building.player),
            "city" => Building::City(building.player),
            other => panic!("unknown building kind {other}"),
        });
    }
    for road in &spec.roads {
        state.roads[road.edge as usize] = Some(road.player);
    }

    state
        .validate()
        .expect("reconstructed D3 state must validate");

    let report = solve_opening(
        &state,
        spec.root_player,
        OpeningConfig {
            maximum_nodes: 12_000,
            root_width: 24,
            opponent_width: 4,
            time_budget_ms: 2_500,
            opponent_maximizes: true,
            rollout_horizon: 16,
            rollout_count: 0,
        },
    );

    let action_label = |action: &colonist_catan_core::Action| match action {
        colonist_catan_core::Action::PlaceSettlement { vertex } => {
            format!("PlaceSettlement {}", vertex_ids[*vertex as usize])
        }
        colonist_catan_core::Action::PlaceRoad { edge } => {
            format!("PlaceRoad {}", edge_ids[*edge as usize])
        }
        other => format!("{other:?}"),
    };

    let chosen = report.chosen.as_ref().map(&action_label);
    let initial_root_roads = state
        .roads
        .iter()
        .enumerate()
        .filter_map(|(edge, owner)| (*owner == Some(spec.root_player)).then_some(edge as u8))
        .collect::<Vec<_>>();
    let endpoint_expansion = |candidate: &colonist_catan_search::OpeningActionValue| {
        let evidence = candidate.evidence.as_ref()?;
        let mut after_root = state.clone();
        after_root.apply(&candidate.action).ok()?;
        let new_road = evidence
            .road_edges
            .into_iter()
            .flatten()
            .find(|edge| !initial_root_roads.contains(edge))?;
        after_root
            .apply(&colonist_catan_core::Action::PlaceRoad { edge: new_road })
            .ok()?;

        let target = evidence.expansion_term;
        let mut matches = Vec::new();
        for settlement in after_root.legal_actions() {
            let colonist_catan_core::Action::PlaceSettlement { .. } = settlement else {
                continue;
            };
            let mut after_settlement = after_root.clone();
            if after_settlement.apply(&settlement).is_err() {
                continue;
            }
            for road in after_settlement.legal_actions() {
                let colonist_catan_core::Action::PlaceRoad { .. } = road else {
                    continue;
                };
                let mut endpoint = after_settlement.clone();
                if endpoint.apply(&road).is_err() {
                    continue;
                }
                let expansion = expansion_option_value(&endpoint, spec.root_player);
                let shared_term = expansion.value * 0.32 + expansion.portfolio_value * 0.22;
                let term = shared_term * evidence.expansion_realization;
                matches.push((
                    (term - target).abs(),
                    action_label(&settlement),
                    action_label(&road),
                    expansion,
                    shared_term,
                    term,
                ));
            }
        }
        matches.sort_by(|left, right| left.0.total_cmp(&right.0));
        Some(
            matches
                .into_iter()
                .take(3)
                .map(|(delta, settlement, road, expansion, shared_term, term)| {
                    json!({
                        "deltaFromEvidenceTerm": delta,
                        "opponentSettlement": settlement,
                        "opponentRoad": road,
                        "sharedExpansionTerm": shared_term,
                        "openingExpansionTerm": term,
                        "bestVertex": expansion.vertex.map(|id| vertex_ids[id as usize].clone()),
                        "roadsRequired": expansion.roads_required,
                        "bestValue": expansion.value,
                        "survivalProbability": expansion.survival_probability,
                        "portfolioValue": expansion.portfolio_value,
                        "optionCount": expansion.option_count,
                    })
                })
                .collect::<Vec<_>>(),
        )
    };
    let actions = report
        .actions
        .iter()
        .map(|candidate| {
            json!({
                "action": action_label(&candidate.action),
                "value": candidate.value,
                "endpointComplete": candidate.endpoint_complete,
                "authoritative": candidate.authoritative,
                "nodeBudget": candidate.node_budget,
                "visitedNodes": candidate.visited_nodes,
                "endpointExpansionMatches": endpoint_expansion(candidate),
                "evidence": candidate.evidence.as_ref().map(|evidence| json!({
                    "productionPips": evidence.production_pips,
                    "startingHand": evidence.starting_hand,
                    "maritimeRatios": evidence.maritime_ratios,
                    "buildEtaRolls": evidence.build_eta_rolls,
                    "settlementVertices": evidence.settlement_vertices.map(|vertex| vertex.map(|id| vertex_ids[id as usize].clone())),
                    "roadEdges": evidence.road_edges.map(|edge| edge.map(|id| edge_ids[id as usize].clone())),
                    "victoryTerm": evidence.victory_term,
                    "productionDiversityTerm": evidence.production_diversity_term,
                    "buildAccessTerm": evidence.build_access_term,
                    "conversionEfficiencyTerm": evidence.conversion_efficiency_term,
                    "portBuildGain": evidence.port_build_gain,
                    "expansionTerm": evidence.expansion_term,
                    "expansionVertex": evidence.expansion_vertex.map(|id| vertex_ids[id as usize].clone()),
                    "expansionRoadsRequired": evidence.expansion_roads_required,
                    "expansionProjectEtaRolls": evidence.expansion_project_eta_rolls,
                    "expansionProjectConversionEfficiency": evidence.expansion_project_conversion_efficiency,
                    "expansionRealization": evidence.expansion_realization,
                    "expansionPortBuildGain": evidence.expansion_port_build_gain,
                    "scarcityTerm": evidence.scarcity_term,
                    "concentrationPenalty": evidence.concentration_penalty,
                    "causalDenialTerm": evidence.causal_denial_term,
                    "ownValue": evidence.own_value,
                    "rivalValue": evidence.rival_value,
                    "rivalWeight": evidence.rival_weight,
                })),
            })
        })
        .collect::<Vec<_>>();

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "chosen": chosen,
            "nodes": report.nodes,
            "completedSetups": report.completed_setups,
            "complete": report.complete,
            "deadlineReached": report.deadline_reached,
            "actions": actions,
            "hexIds": hex_ids,
        }))
        .unwrap()
    );
}
