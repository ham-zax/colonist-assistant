use std::sync::Arc;

use colonist_catan_core::{
    Action, Board, Building, Edge, GameState, Hex, Phase, Port, Resource, Vertex,
};

use super::{opening_build_economy, opening_build_economy_from_inputs, opening_position_bonus};
use crate::{OpeningConfig, OpeningReport, production_pips, solve_opening};

const TRADE5301_HEXES: [((i8, i8), Option<Resource>, u8); 19] = [
    ((0, -2), Some(Resource::Wool), 10),
    ((-1, -1), Some(Resource::Lumber), 9),
    ((-2, 0), Some(Resource::Brick), 12),
    ((-2, 1), Some(Resource::Wool), 11),
    ((-2, 2), Some(Resource::Grain), 4),
    ((-1, 2), Some(Resource::Brick), 8),
    ((0, 2), Some(Resource::Brick), 5),
    ((1, 1), Some(Resource::Ore), 2),
    ((2, 0), Some(Resource::Grain), 6),
    ((2, -1), Some(Resource::Lumber), 3),
    ((2, -2), None, 0),
    ((1, -2), Some(Resource::Grain), 8),
    ((0, -1), Some(Resource::Grain), 5),
    ((-1, 0), Some(Resource::Lumber), 6),
    ((-1, 1), Some(Resource::Lumber), 3),
    ((0, 1), Some(Resource::Wool), 10),
    ((1, 0), Some(Resource::Wool), 9),
    ((1, -1), Some(Resource::Ore), 4),
    ((0, 0), Some(Resource::Ore), 11),
];

const TOWN1088_HEXES: [((i8, i8), Option<Resource>, u8); 19] = [
    ((0, -2), Some(Resource::Lumber), 4),
    ((-1, -1), Some(Resource::Ore), 8),
    ((-2, 0), Some(Resource::Brick), 5),
    ((-2, 1), Some(Resource::Ore), 2),
    ((-2, 2), Some(Resource::Wool), 6),
    ((-1, 2), Some(Resource::Lumber), 3),
    ((0, 2), None, 0),
    ((1, 1), Some(Resource::Lumber), 8),
    ((2, 0), Some(Resource::Grain), 10),
    ((2, -1), Some(Resource::Ore), 9),
    ((2, -2), Some(Resource::Lumber), 12),
    ((1, -2), Some(Resource::Brick), 11),
    ((0, -1), Some(Resource::Grain), 3),
    ((-1, 0), Some(Resource::Wool), 10),
    ((-1, 1), Some(Resource::Wool), 9),
    ((0, 1), Some(Resource::Grain), 4),
    ((1, 0), Some(Resource::Grain), 5),
    ((1, -1), Some(Resource::Brick), 6),
    ((0, 0), Some(Resource::Wool), 11),
];

const TASK394_HEXES: [((i8, i8), Option<Resource>, u8); 19] = [
    ((0, -2), Some(Resource::Lumber), 9),
    ((-1, -1), Some(Resource::Grain), 10),
    ((-2, 0), Some(Resource::Brick), 8),
    ((-2, 1), Some(Resource::Wool), 3),
    ((-2, 2), Some(Resource::Ore), 6),
    ((-1, 2), Some(Resource::Lumber), 2),
    ((0, 2), Some(Resource::Wool), 5),
    ((1, 1), None, 0),
    ((2, 0), Some(Resource::Wool), 8),
    ((2, -1), Some(Resource::Brick), 4),
    ((2, -2), Some(Resource::Lumber), 11),
    ((1, -2), Some(Resource::Ore), 12),
    ((0, -1), Some(Resource::Lumber), 5),
    ((-1, 0), Some(Resource::Brick), 4),
    ((-1, 1), Some(Resource::Grain), 9),
    ((0, 1), Some(Resource::Grain), 10),
    ((1, 0), Some(Resource::Grain), 3),
    ((1, -1), Some(Resource::Ore), 6),
    ((0, 0), Some(Resource::Wool), 11),
];

const TRADE5301_PORTS: [(&str, Port); 9] = [
    ("e:0,-2,0", Port::Resource(Resource::Wool)),
    ("e:-1,-1,1", Port::Generic),
    ("e:-2,1,1", Port::Generic),
    ("e:-2,2,2", Port::Resource(Resource::Grain)),
    ("e:-1,3,0", Port::Generic),
    ("e:1,2,0", Port::Resource(Resource::Ore)),
    ("e:3,0,1", Port::Resource(Resource::Brick)),
    ("e:3,-2,2", Port::Generic),
    ("e:2,-3,2", Port::Resource(Resource::Lumber)),
];

const TOWN1088_PORTS: [(&str, Port); 9] = [
    ("e:0,-2,0", Port::Resource(Resource::Lumber)),
    ("e:-1,-1,1", Port::Resource(Resource::Brick)),
    ("e:-2,1,1", Port::Resource(Resource::Wool)),
    ("e:-2,2,2", Port::Resource(Resource::Ore)),
    ("e:-1,3,0", Port::Generic),
    ("e:1,2,0", Port::Generic),
    ("e:3,0,1", Port::Generic),
    ("e:3,-2,2", Port::Resource(Resource::Grain)),
    ("e:2,-3,2", Port::Generic),
];

const TASK394_PORTS: [(&str, Port); 9] = [
    ("e:0,-2,0", Port::Generic),
    ("e:-1,-1,1", Port::Resource(Resource::Ore)),
    ("e:-2,1,1", Port::Generic),
    ("e:-2,2,2", Port::Resource(Resource::Grain)),
    ("e:-1,3,0", Port::Generic),
    ("e:1,2,0", Port::Resource(Resource::Wool)),
    ("e:3,0,1", Port::Resource(Resource::Brick)),
    ("e:3,-2,2", Port::Resource(Resource::Lumber)),
    ("e:2,-3,2", Port::Generic),
];

const VERTEX_LABELS: [&str; 54] = [
    "v:0,-2,0",
    "v:1,-3,1",
    "v:0,-1,0",
    "v:0,-2,1",
    "v:-1,-1,0",
    "v:0,-3,1",
    "v:-1,0,0",
    "v:-1,-1,1",
    "v:-2,0,0",
    "v:-1,-2,1",
    "v:-2,1,0",
    "v:-2,0,1",
    "v:-3,1,0",
    "v:-2,-1,1",
    "v:-1,0,1",
    "v:-2,2,0",
    "v:-2,1,1",
    "v:-3,2,0",
    "v:-1,1,1",
    "v:-2,3,0",
    "v:-2,2,1",
    "v:-3,3,0",
    "v:-1,2,0",
    "v:0,1,1",
    "v:-1,3,0",
    "v:-1,2,1",
    "v:0,2,0",
    "v:1,1,1",
    "v:0,3,0",
    "v:0,2,1",
    "v:1,1,0",
    "v:2,0,1",
    "v:1,2,0",
    "v:1,0,1",
    "v:2,0,0",
    "v:3,-1,1",
    "v:2,1,0",
    "v:2,-1,1",
    "v:2,-1,0",
    "v:3,-2,1",
    "v:1,0,0",
    "v:2,-2,1",
    "v:2,-2,0",
    "v:3,-3,1",
    "v:1,-1,0",
    "v:2,-3,1",
    "v:1,-2,0",
    "v:1,-2,1",
    "v:0,0,0",
    "v:0,-1,1",
    "v:-1,1,0",
    "v:0,0,1",
    "v:0,1,0",
    "v:1,-1,1",
];

const EDGE_LABELS: [&str; 72] = [
    "e:1,-3,2",
    "e:1,-2,1",
    "e:0,-1,0",
    "e:0,-2,2",
    "e:0,-2,1",
    "e:0,-2,0",
    "e:0,-1,1",
    "e:-1,0,0",
    "e:-1,-1,2",
    "e:-1,-1,1",
    "e:-1,-1,0",
    "e:-1,0,1",
    "e:-2,1,0",
    "e:-2,0,2",
    "e:-2,0,1",
    "e:-2,0,0",
    "e:-1,0,2",
    "e:-1,1,1",
    "e:-2,2,0",
    "e:-2,1,2",
    "e:-2,1,1",
    "e:-1,1,2",
    "e:-1,2,1",
    "e:-2,3,0",
    "e:-2,2,2",
    "e:-2,2,1",
    "e:0,1,2",
    "e:0,2,1",
    "e:-1,3,0",
    "e:-1,2,2",
    "e:-1,2,0",
    "e:1,1,2",
    "e:1,2,1",
    "e:0,3,0",
    "e:0,2,2",
    "e:0,2,0",
    "e:2,0,2",
    "e:2,1,1",
    "e:1,2,0",
    "e:1,1,1",
    "e:1,1,0",
    "e:3,-1,2",
    "e:3,0,1",
    "e:2,1,0",
    "e:2,0,1",
    "e:2,0,0",
    "e:3,-2,2",
    "e:3,-1,1",
    "e:2,-1,2",
    "e:2,-1,1",
    "e:2,-1,0",
    "e:3,-3,2",
    "e:3,-2,1",
    "e:2,-2,2",
    "e:2,-2,1",
    "e:2,-2,0",
    "e:2,-3,2",
    "e:1,-1,0",
    "e:1,-2,2",
    "e:1,-2,0",
    "e:1,-1,1",
    "e:0,0,0",
    "e:0,-1,2",
    "e:0,0,1",
    "e:-1,1,0",
    "e:0,0,2",
    "e:0,1,1",
    "e:1,0,2",
    "e:0,1,0",
    "e:1,0,1",
    "e:1,0,0",
    "e:1,-1,2",
];

fn triplet(label: &str, prefix: &str) -> (i8, i8, u8) {
    let mut parts = label
        .strip_prefix(prefix)
        .unwrap_or_else(|| panic!("expected {prefix} label: {label}"))
        .split(',');
    let q = parts.next().unwrap().parse().unwrap();
    let r = parts.next().unwrap().parse().unwrap();
    let kind = parts.next().unwrap().parse().unwrap();
    assert!(parts.next().is_none());
    (q, r, kind)
}

fn recorded_vertex_index(label: &str) -> u8 {
    VERTEX_LABELS
        .iter()
        .position(|candidate| *candidate == label)
        .unwrap_or_else(|| panic!("unknown recorded vertex {label}")) as u8
}

fn vertex_index(_board: &Board, label: &str) -> u8 {
    recorded_vertex_index(label)
}

fn edge_endpoint_labels(label: &str) -> (String, String) {
    let (q, r, orientation) = triplet(label, "e:");
    match orientation {
        0 => (format!("v:{q},{},1", r - 1), format!("v:{q},{r},0")),
        1 => (
            format!("v:{},{},0", q - 1, r + 1),
            format!("v:{q},{},1", r - 1),
        ),
        2 => (format!("v:{q},{r},1"), format!("v:{},{},0", q - 1, r + 1)),
        _ => panic!("unsupported edge orientation in {label}"),
    }
}

fn edge_index(_board: &Board, label: &str) -> u8 {
    EDGE_LABELS
        .iter()
        .position(|candidate| *candidate == label)
        .unwrap_or_else(|| panic!("unknown recorded edge {label}")) as u8
}

fn recorded_state(
    hexes: &[((i8, i8), Option<Resource>, u8); 19],
    ports: &[(&str, Port); 9],
    root: u8,
) -> GameState {
    let board_hexes = hexes
        .iter()
        .map(|(coord, resource, number)| Hex {
            resource: *resource,
            number: *number,
            coord: *coord,
        })
        .collect::<Vec<_>>();
    let mut vertices = VERTEX_LABELS
        .iter()
        .map(|label| {
            let (q, r, orientation) = triplet(label, "v:");
            let adjacent_coords = match orientation {
                0 => [(q, r), (q, r - 1), (q + 1, r - 1)],
                1 => [(q, r), (q, r + 1), (q - 1, r + 1)],
                _ => panic!("unsupported vertex orientation in {label}"),
            };
            let adjacent_hexes = adjacent_coords
                .into_iter()
                .filter_map(|coord| {
                    board_hexes
                        .iter()
                        .position(|hex| hex.coord == coord)
                        .map(|index| index as u8)
                })
                .collect::<Vec<_>>();
            Vertex {
                adjacent_hexes,
                adjacent_vertices: Vec::new(),
                adjacent_edges: Vec::new(),
                port: None,
            }
        })
        .collect::<Vec<_>>();
    let mut edges = EDGE_LABELS
        .iter()
        .map(|label| {
            let (left_label, right_label) = edge_endpoint_labels(label);
            let left = recorded_vertex_index(&left_label);
            let right = recorded_vertex_index(&right_label);
            Edge {
                vertices: [left, right],
                adjacent_hexes: vertices[left as usize]
                    .adjacent_hexes
                    .iter()
                    .copied()
                    .filter(|hex| vertices[right as usize].adjacent_hexes.contains(hex))
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    for (edge_index, edge) in edges.iter().enumerate() {
        let [left, right] = edge.vertices;
        vertices[left as usize].adjacent_vertices.push(right);
        vertices[right as usize].adjacent_vertices.push(left);
        vertices[left as usize]
            .adjacent_edges
            .push(edge_index as u8);
        vertices[right as usize]
            .adjacent_edges
            .push(edge_index as u8);
    }
    let mut board = Board {
        num_players: 4,
        hexes: board_hexes,
        vertices,
        edges: std::mem::take(&mut edges),
    };
    for (edge_label, port) in ports {
        let edge = edge_index(&board, edge_label);
        for vertex in board.edges[edge as usize].vertices {
            board.vertices[vertex as usize].port = Some(*port);
        }
    }

    let mut state = GameState::new(board, 10);
    state.player_trades_enabled = true;
    state.domestic_trade_disabled = 1 << root;
    state
}

fn place_settlement(state: &mut GameState, label: &str) {
    let action = Action::PlaceSettlement {
        vertex: vertex_index(&state.board, label),
    };
    assert!(state.legal_actions().contains(&action), "illegal {label}");
    state.apply(&action).unwrap();
}

fn place_road(state: &mut GameState, label: &str) {
    let action = Action::PlaceRoad {
        edge: edge_index(&state.board, label),
    };
    assert!(state.legal_actions().contains(&action), "illegal {label}");
    state.apply(&action).unwrap();
}

fn settlement_action(state: &GameState, label: &str) -> Action {
    Action::PlaceSettlement {
        vertex: vertex_index(&state.board, label),
    }
}

fn candidate_value(report: &OpeningReport, action: &Action) -> f32 {
    report
        .actions
        .iter()
        .find(|candidate| &candidate.action == action)
        .unwrap_or_else(|| panic!("candidate missing from opening report: {action:?}"))
        .value
}

fn live_opening_config() -> OpeningConfig {
    OpeningConfig {
        maximum_nodes: 12_000,
        root_width: 24,
        opponent_width: 4,
        time_budget_ms: 0,
        opponent_maximizes: true,
        rollout_count: 0,
        ..OpeningConfig::default()
    }
}

fn assert_setup_turn(state: &GameState, root: u8, setup_step: u8) {
    assert_eq!(state.actor(), root);
    assert_eq!(state.setup_step, setup_step);
    assert_eq!(state.phase, Phase::SetupSettlement);
}

fn trade5301_d1() -> GameState {
    let mut state = recorded_state(&TRADE5301_HEXES, &TRADE5301_PORTS, 1);
    place_settlement(&mut state, "v:-1,0,0");
    place_road(&mut state, "e:0,-1,2");
    assert_setup_turn(&state, 1, 1);
    state
}

fn trade5301_d3() -> GameState {
    let mut state = trade5301_d1();
    place_settlement(&mut state, "v:1,0,0");
    place_road(&mut state, "e:2,-1,2");
    place_settlement(&mut state, "v:0,-1,0");
    place_road(&mut state, "e:1,-2,1");
    place_settlement(&mut state, "v:0,1,1");
    place_road(&mut state, "e:0,2,1");
    place_settlement(&mut state, "v:-1,1,1");
    place_road(&mut state, "e:-1,2,0");
    place_settlement(&mut state, "v:1,1,0");
    place_road(&mut state, "e:2,0,1");
    assert_setup_turn(&state, 1, 6);
    state
}

fn town1088_d1() -> GameState {
    let mut state = recorded_state(&TOWN1088_HEXES, &TOWN1088_PORTS, 2);
    place_settlement(&mut state, "v:1,0,0");
    place_road(&mut state, "e:2,-1,1");
    place_settlement(&mut state, "v:-1,-1,1");
    place_road(&mut state, "e:-1,0,0");
    assert_setup_turn(&state, 2, 2);
    state
}

fn town1088_d3() -> GameState {
    let mut state = town1088_d1();
    place_settlement(&mut state, "v:-1,-1,0");
    place_road(&mut state, "e:0,-2,1");
    place_settlement(&mut state, "v:1,1,0");
    place_road(&mut state, "e:2,0,2");
    place_settlement(&mut state, "v:-1,1,1");
    place_road(&mut state, "e:-1,2,1");
    assert_setup_turn(&state, 2, 5);
    state
}

fn task394_d1() -> GameState {
    let mut state = recorded_state(&TASK394_HEXES, &TASK394_PORTS, 1);
    place_settlement(&mut state, "v:0,-2,1");
    place_road(&mut state, "e:0,-1,0");
    assert_setup_turn(&state, 1, 1);
    state
}

fn task394_d3() -> GameState {
    let mut state = task394_d1();
    place_settlement(&mut state, "v:0,1,1");
    place_road(&mut state, "e:0,1,2");
    place_settlement(&mut state, "v:-1,-1,1");
    place_road(&mut state, "e:-1,0,1");
    place_settlement(&mut state, "v:-2,2,0");
    place_road(&mut state, "e:-1,1,2");
    place_settlement(&mut state, "v:0,0,0");
    place_road(&mut state, "e:1,-1,1");
    place_settlement(&mut state, "v:2,-1,1");
    place_road(&mut state, "e:2,0,0");
    assert_setup_turn(&state, 1, 6);
    state
}

#[test]
fn trade5301_d1_partial_continuation_cannot_remain_authoritative() {
    let state = trade5301_d1();
    let historical = settlement_action(&state, "v:1,0,0");
    let report = solve_opening(&state, 1, live_opening_config());

    assert_ne!(
        report.chosen.as_ref(),
        Some(&historical),
        "the historical v:1,0,0 root must not win unless its completed endpoint is actually best",
    );
    if let Some(candidate) = report
        .actions
        .iter()
        .find(|candidate| candidate.action == historical)
    {
        assert!(
            candidate.endpoint_complete,
            "the historical root may enter the authoritative list only when its selected continuation is completion-backed",
        );
    }
}

#[test]
fn recorded_d1_authoritative_candidates_are_completion_backed() {
    for (name, state, root) in [
        ("trade5301", trade5301_d1(), 1),
        ("town1088", town1088_d1(), 2),
        ("task394", task394_d1(), 1),
    ] {
        let report = solve_opening(&state, root, live_opening_config());
        assert!(
            !report.actions.is_empty(),
            "{name} must retain an authoritative root"
        );
        assert!(
            report
                .actions
                .iter()
                .all(|candidate| candidate.endpoint_complete),
            "{name} must not admit a root whose selected returned utility ends in partial setup",
        );
    }
}

#[test]
fn trade5301_weak_brick_closes_a_real_bottleneck_even_with_bank_conversion() {
    let state = trade5301_d3();
    let weak_brick = settlement_action(&state, "v:-2,1,0");
    let higher_pip_no_brick = settlement_action(&state, "v:-1,0,1");
    let report = solve_opening(&state, 1, live_opening_config());

    let weak_vertex = match weak_brick {
        Action::PlaceSettlement { vertex } => vertex,
        _ => unreachable!(),
    };
    assert!(
        state.board.vertices[weak_vertex as usize]
            .adjacent_hexes
            .iter()
            .any(|hex| {
                let tile = &state.board.hexes[*hex as usize];
                tile.resource == Some(Resource::Brick) && tile.number == 12
            })
    );

    let mut weak_state = state.clone();
    weak_state.apply(&weak_brick).unwrap();
    let weak_pips = production_pips(&weak_state, 1);
    let weak_economy = opening_build_economy(&weak_state, 1);
    let mut no_brick_state = state.clone();
    no_brick_state.apply(&higher_pip_no_brick).unwrap();
    let no_brick_pips = production_pips(&no_brick_state, 1);
    let no_brick_economy = opening_build_economy(&no_brick_state, 1);

    assert_eq!(weak_pips, [7.0, 1.0, 6.0, 0.0, 3.0]);
    assert_eq!(weak_pips.iter().sum::<f32>(), 17.0);
    assert_eq!(no_brick_pips, [9.0, 0.0, 6.0, 0.0, 3.0]);
    assert_eq!(no_brick_pips.iter().sum::<f32>(), 18.0);
    assert!(weak_economy.weighted_access > no_brick_economy.weighted_access);
    assert!(no_brick_economy.eta_rolls[0].is_finite());

    assert_eq!(report.chosen.as_ref(), Some(&weak_brick));
    assert!(candidate_value(&report, &weak_brick) > candidate_value(&report, &higher_pip_no_brick));
}

#[test]
fn town1088_second_settlement_keeps_all_five_resource_portfolio() {
    let state = town1088_d3();
    let chosen = settlement_action(&state, "v:0,0,0");
    let legal_count = state.legal_actions().len();
    let report = solve_opening(&state, 2, live_opening_config());
    assert_eq!(report.actions.len(), legal_count);
    assert_eq!(report.chosen.as_ref(), Some(&chosen));

    let mut final_state = state.clone();
    final_state.apply(&chosen).unwrap();
    let pips = production_pips(&final_state, 2);
    assert_eq!(pips, [3.0, 5.0, 2.0, 2.0, 5.0]);
    assert_eq!(pips.iter().sum::<f32>(), 17.0);
    assert!(pips.iter().all(|pips| *pips > 0.0));
}

#[test]
fn task394_second_settlement_keeps_all_five_resource_portfolio() {
    let state = task394_d3();
    let chosen = settlement_action(&state, "v:2,-2,1");
    let legal_count = state.legal_actions().len();
    let report = solve_opening(&state, 1, live_opening_config());
    assert_eq!(report.actions.len(), legal_count);
    assert_eq!(report.chosen.as_ref(), Some(&chosen));

    let mut final_state = state.clone();
    final_state.apply(&chosen).unwrap();
    let pips = production_pips(&final_state, 1);
    assert_eq!(pips, [3.0, 3.0, 4.0, 3.0, 5.0]);
    assert_eq!(pips.iter().sum::<f32>(), 18.0);
    assert!(pips.iter().all(|pips| *pips > 0.0));
}

fn synthetic_two_settlement_state(
    resources: [Resource; 6],
    numbers: [u8; 6],
    port: Option<Port>,
) -> GameState {
    let mut state = GameState::standard(811, 2);
    state.buildings.fill(None);
    state.roads.fill(None);
    state.phase = Phase::PreRoll;
    state.current_player = 0;
    state.players[0].resources = [0; 5];

    let (first, second) = state
        .board
        .vertices
        .iter()
        .enumerate()
        .filter(|(_, vertex)| vertex.adjacent_hexes.len() == 3)
        .find_map(|(first_index, first_vertex)| {
            state
                .board
                .vertices
                .iter()
                .enumerate()
                .find(|(second_index, second_vertex)| {
                    *second_index != first_index
                        && second_vertex.adjacent_hexes.len() == 3
                        && first_vertex
                            .adjacent_hexes
                            .iter()
                            .all(|hex| !second_vertex.adjacent_hexes.contains(hex))
                })
                .map(|(second_index, _)| (first_index, second_index))
        })
        .expect("standard board has two disjoint three-hex vertices");

    state.buildings[first] = Some(Building::Settlement(0));
    state.buildings[second] = Some(Building::Settlement(0));
    let first_hexes = state.board.vertices[first].adjacent_hexes.clone();
    let second_hexes = state.board.vertices[second].adjacent_hexes.clone();
    let selected_hexes = first_hexes
        .iter()
        .chain(second_hexes.iter())
        .copied()
        .collect::<Vec<_>>();
    assert_eq!(selected_hexes.len(), 6);

    state.robber_hex = (0..state.board.hexes.len())
        .map(|hex| hex as u8)
        .find(|hex| !selected_hexes.contains(hex))
        .expect("standard board has an unselected robber destination");
    let board = Arc::make_mut(&mut state.board);
    for vertex in &mut board.vertices {
        vertex.port = None;
    }
    board.vertices[first].port = port;
    for (slot, hex) in selected_hexes.into_iter().enumerate() {
        board.hexes[hex as usize].resource = Some(resources[slot]);
        board.hexes[hex as usize].number = numbers[slot];
    }
    state
}

fn duplicate_strong_number_states() -> (GameState, GameState) {
    let mut base = GameState::standard(821, 2);
    base.buildings.fill(None);
    base.roads.fill(None);
    base.phase = Phase::PreRoll;
    base.current_player = 0;
    base.players[0].resources = [0; 5];

    let mut corner_hexes = Vec::new();
    let mut corners = Vec::new();
    for (index, vertex) in base.board.vertices.iter().enumerate() {
        if vertex.adjacent_hexes.len() != 1 {
            continue;
        }
        let hex = vertex.adjacent_hexes[0];
        if corner_hexes.contains(&hex) {
            continue;
        }
        corner_hexes.push(hex);
        corners.push(index);
        if corners.len() == 3 {
            break;
        }
    }
    assert_eq!(corners.len(), 3);
    let [first, duplicate, unique] = [corners[0], corners[1], corners[2]];
    let first_hex = base.board.vertices[first].adjacent_hexes[0];
    let duplicate_hex = base.board.vertices[duplicate].adjacent_hexes[0];
    let unique_hex = base.board.vertices[unique].adjacent_hexes[0];
    base.robber_hex = (0..base.board.hexes.len())
        .map(|hex| hex as u8)
        .find(|hex| ![first_hex, duplicate_hex, unique_hex].contains(hex))
        .expect("standard board has an unselected robber destination");

    let board = Arc::make_mut(&mut base.board);
    for vertex in &mut board.vertices {
        vertex.port = None;
    }
    board.hexes[first_hex as usize].resource = Some(Resource::Lumber);
    board.hexes[first_hex as usize].number = 6;
    board.hexes[duplicate_hex as usize].resource = Some(Resource::Ore);
    board.hexes[duplicate_hex as usize].number = 6;
    board.hexes[unique_hex as usize].resource = Some(Resource::Ore);
    board.hexes[unique_hex as usize].number = 12;

    let mut duplicate_state = base.clone();
    duplicate_state.buildings[first] = Some(Building::Settlement(0));
    duplicate_state.buildings[duplicate] = Some(Building::Settlement(0));
    let mut unique_state = base;
    unique_state.buildings[first] = Some(Building::Settlement(0));
    unique_state.buildings[unique] = Some(Building::Settlement(0));
    (duplicate_state, unique_state)
}

#[test]
fn bank_four_to_one_conversion_funds_complete_build_costs() {
    let production = [12.0, 0.0, 0.0, 0.0, 0.0];
    let economy = opening_build_economy_from_inputs(&production, &[0; 5], &[4; 5]);

    assert!(economy.eta_rolls[0] > 14.0 && economy.eta_rolls[0] < 16.0);
    assert!(economy.eta_rolls[1] > 38.0 && economy.eta_rolls[1] < 40.0);
    assert!(economy.eta_rolls[2].is_finite());
    assert!(economy.eta_rolls[3].is_finite());
}

#[test]
fn starting_surplus_can_pay_a_missing_resource_through_the_bank() {
    let production = [12.0, 0.0, 0.0, 0.0, 0.0];
    let no_surplus = opening_build_economy_from_inputs(&production, &[1, 0, 0, 0, 0], &[4; 5]);
    let convertible_hand =
        opening_build_economy_from_inputs(&production, &[5, 0, 0, 0, 0], &[4; 5]);

    assert!(no_surplus.eta_rolls[0] > 11.0);
    assert!(convertible_hand.eta_rolls[0] <= f32::EPSILON);
}

#[test]
fn completed_opponent_starting_hand_matches_root_build_economy() {
    let mut root_state = synthetic_two_settlement_state(
        [
            Resource::Lumber,
            Resource::Brick,
            Resource::Wool,
            Resource::Grain,
            Resource::Ore,
            Resource::Grain,
        ],
        [6, 8, 5, 9, 4, 10],
        None,
    );
    let starting_hand = [1, 1, 1, 0, 0];
    root_state.players[0].resources = starting_hand;
    let root = opening_build_economy(&root_state, 0);

    let mut rival_state = root_state.clone();
    for building in &mut rival_state.buildings {
        if matches!(building, Some(Building::Settlement(0))) {
            *building = Some(Building::Settlement(1));
        }
    }
    rival_state.players[0].resources = [0; 5];
    rival_state.players[1].resources = starting_hand;
    let rival = opening_build_economy(&rival_state, 1);

    assert_eq!(
        production_pips(&root_state, 0),
        production_pips(&rival_state, 1)
    );
    assert_eq!(root.maritime_ratios, rival.maritime_ratios);
    assert_eq!(root.eta_rolls, rival.eta_rolls);
    assert_eq!(root.weighted_access, rival.weighted_access);
}

#[test]
fn matching_two_to_one_port_materially_improves_conversion_tempo() {
    let production = [12.0, 0.0, 0.0, 0.0, 0.0];
    let bank = opening_build_economy_from_inputs(&production, &[0; 5], &[4; 5]);
    let lumber_port = opening_build_economy_from_inputs(&production, &[0; 5], &[2, 4, 4, 4, 4]);

    assert!(lumber_port.eta_rolls[0] < bank.eta_rolls[0] * 0.7);
    assert!(lumber_port.weighted_access > bank.weighted_access);
}

#[test]
fn generic_three_to_one_port_improves_conversion_tempo() {
    let production = [12.0, 0.0, 0.0, 0.0, 0.0];
    let bank = opening_build_economy_from_inputs(&production, &[0; 5], &[4; 5]);
    let generic = opening_build_economy_from_inputs(&production, &[0; 5], &[3; 5]);

    assert!(generic.eta_rolls[0] < bank.eta_rolls[0]);
    assert!(generic.weighted_access > bank.weighted_access);
}

#[test]
fn concentrated_port_engine_can_beat_balanced_build_access_without_making_diversity_absolute() {
    let balanced = opening_build_economy_from_inputs(&[3.0; 5], &[0; 5], &[4; 5]);
    let concentrated_bank =
        opening_build_economy_from_inputs(&[24.0, 0.0, 0.0, 0.0, 0.0], &[0; 5], &[4; 5]);
    let concentrated_port =
        opening_build_economy_from_inputs(&[24.0, 0.0, 0.0, 0.0, 0.0], &[0; 5], &[2, 4, 4, 4, 4]);

    assert!(balanced.weighted_access > concentrated_bank.weighted_access);
    assert!(concentrated_port.weighted_access > balanced.weighted_access);

    let balanced_state = synthetic_two_settlement_state(
        [
            Resource::Lumber,
            Resource::Brick,
            Resource::Wool,
            Resource::Grain,
            Resource::Ore,
            Resource::Grain,
        ],
        [4, 10, 5, 3, 9, 12],
        None,
    );
    let concentrated_bank_state =
        synthetic_two_settlement_state([Resource::Lumber; 6], [6, 8, 5, 9, 4, 10], None);
    let concentrated_port_state = synthetic_two_settlement_state(
        [Resource::Lumber; 6],
        [6, 8, 5, 9, 4, 10],
        Some(Port::Resource(Resource::Lumber)),
    );
    assert!(
        opening_position_bonus(&balanced_state, 0)
            > opening_position_bonus(&concentrated_bank_state, 0),
        "balanced direct access can beat concentration when the latter only has 4:1 bank conversion",
    );
    assert!(
        opening_position_bonus(&concentrated_port_state, 0)
            > opening_position_bonus(&balanced_state, 0),
        "a sufficiently strong 2:1 conversion engine must be able to outweigh intrinsic color-count diversity",
    );
}

#[test]
fn duplicate_strong_number_can_beat_a_weak_unique_number() {
    let (duplicate_six, unique_twelve) = duplicate_strong_number_states();
    assert!(
        opening_position_bonus(&duplicate_six, 0) > opening_position_bonus(&unique_twelve, 0),
        "a second productive 6 must not be rejected merely because the roll number is duplicated",
    );
}

#[test]
fn weak_twelve_resource_can_still_beat_more_raw_pips_when_it_closes_a_bottleneck() {
    let mut weak_brick = synthetic_two_settlement_state(
        [
            Resource::Lumber,
            Resource::Lumber,
            Resource::Wool,
            Resource::Grain,
            Resource::Ore,
            Resource::Brick,
        ],
        [6, 8, 9, 5, 4, 12],
        None,
    );
    let mut higher_pips = synthetic_two_settlement_state(
        [
            Resource::Lumber,
            Resource::Lumber,
            Resource::Wool,
            Resource::Grain,
            Resource::Ore,
            Resource::Lumber,
        ],
        [6, 8, 9, 5, 4, 10],
        None,
    );
    weak_brick.players[0].resources = [0, 1, 0, 0, 0];
    higher_pips.players[0].resources = [1, 0, 0, 0, 0];

    let weak_pips = production_pips(&weak_brick, 0).iter().sum::<f32>();
    let high_pips = production_pips(&higher_pips, 0).iter().sum::<f32>();
    assert!(weak_pips < high_pips);
    assert!(
        opening_position_bonus(&weak_brick, 0) > opening_position_bonus(&higher_pips, 0),
        "a brick 12 plus a starting brick can beat more raw pips when it closes road/settlement access",
    );
}

#[test]
fn opening_economy_does_not_assign_a_speculative_domestic_trade_rate() {
    let mut state =
        synthetic_two_settlement_state([Resource::Lumber; 6], [6, 8, 5, 9, 4, 10], None);
    state.player_trades_enabled = true;
    state.domestic_trade_disabled = 0;
    let allowed = opening_build_economy(&state, 0);
    state.domestic_trade_disabled = 1;
    let disabled = opening_build_economy(&state, 0);

    assert_eq!(disabled.maritime_ratios, [4; 5]);
    assert_eq!(allowed.eta_rolls, disabled.eta_rolls);
    assert_eq!(allowed.weighted_access, disabled.weighted_access);
}
