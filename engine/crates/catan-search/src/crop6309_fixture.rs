//! Focused positions extracted from the crop6309 live recording (2026-10-03).
//! Geometry, public pieces, our hand/cards, and resource-belief worlds follow
//! the recording. Hidden opponent development cards are a caller-visible
//! hypothesis, not a claim about their real identities. Dice use M0 here;
//! these are regression positions, not an exact historical balanced-dice replay.
//!
//! The recording does not export `victory_target`; 15 is inferred for this
//! two-player game. Use [`state_with_target`] for 10/14/15
//! sensitivity checks. Snapshot frames satisfy
//! `frame_ms <= decision_ms`, except two documented pre-decision inclusions
//! (see `meta.proven_inclusions` in the JSON): the D4 P1 setup road and the
//! D66 D63-built road, both proven to predate their decisions by setup
//! legality and hand conservation respectively.

use colonist_catan_core::{
    Action, Board, Building, Edge, GameState, Hex, Phase, Port, Resource, Vertex,
};
use serde_json::Value;

use crate::BeliefParticle;

/// Inferred, not exported: the recording never states the victory target.
/// Fifteen is inferred for this two-player game; vary with `state_with_target`.
pub(crate) const INFERRED_VICTORY_TARGET: u8 = 15;

fn fixture() -> Value {
    serde_json::from_str(include_str!("test_fixtures/crop6309.json")).unwrap()
}

fn byte(value: &Value) -> u8 {
    u8::try_from(value.as_u64().unwrap()).unwrap()
}

fn hand(value: &Value) -> [u8; 5] {
    std::array::from_fn(|index| byte(&value[index]))
}

fn board(data: &Value) -> Board {
    let hexes = data["hexes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|hex| Hex {
            resource: hex["resource"]
                .as_u64()
                .map(|index| Resource::ALL[index as usize]),
            number: byte(&hex["number"]),
            coord: (
                hex["coord"][0].as_i64().unwrap() as i8,
                hex["coord"][1].as_i64().unwrap() as i8,
            ),
        })
        .collect();
    let edges: Vec<Edge> = data["edges"]
        .as_array()
        .unwrap()
        .iter()
        .map(|edge| Edge {
            vertices: [byte(&edge[0]), byte(&edge[1])],
            adjacent_hexes: Vec::new(),
        })
        .collect();
    let vertices = data["vertices"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let adjacent_edges = edges
                .iter()
                .enumerate()
                .filter(|(_, edge)| edge.vertices.contains(&(index as u8)))
                .map(|(index, _)| index as u8)
                .collect::<Vec<_>>();
            let adjacent_vertices = adjacent_edges
                .iter()
                .map(|edge| {
                    edges[*edge as usize]
                        .vertices
                        .iter()
                        .copied()
                        .find(|vertex| *vertex != index as u8)
                        .unwrap()
                })
                .collect();
            let port = vertex["port"].as_str().map(|name| {
                if name == "generic" {
                    Port::Generic
                } else {
                    let resource = ["lumber", "brick", "wool", "grain", "ore"]
                        .iter()
                        .position(|candidate| *candidate == name)
                        .unwrap();
                    Port::Resource(Resource::ALL[resource])
                }
            });
            Vertex {
                adjacent_hexes: vertex["hexes"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(byte)
                    .collect(),
                adjacent_vertices,
                adjacent_edges,
                port,
            }
        })
        .collect::<Vec<_>>();
    let edges = edges
        .into_iter()
        .map(|mut edge| {
            edge.adjacent_hexes = vertices[edge.vertices[0] as usize]
                .adjacent_hexes
                .iter()
                .copied()
                .filter(|hex| {
                    vertices[edge.vertices[1] as usize]
                        .adjacent_hexes
                        .contains(hex)
                })
                .collect();
            edge
        })
        .collect();
    Board {
        num_players: 2,
        hexes,
        vertices,
        edges,
    }
}

/// Recorded setup prefix, in order: (settlement, road) pairs alternate P0,
/// P1, P1. D2 needs none of these; D4 needs all six.
fn setup_prefix() -> [(&'static str, &'static str); 3] {
    [
        ("v:0,-1,0", "e:0,-1,0"),
        ("v:0,0,0", "e:0,0,0"),
        ("v:-1,-1,1", "e:-1,0,1"),
    ]
}

fn apply_setup(state: &mut GameState, settlement: &str, road: &str) {
    let action = Action::PlaceSettlement {
        vertex: vertex(settlement),
    };
    assert!(
        state.legal_actions().contains(&action),
        "recorded setup settlement {settlement} must be legal"
    );
    state.apply(&action).unwrap();
    let action = Action::PlaceRoad { edge: edge(road) };
    assert!(
        state.legal_actions().contains(&action),
        "recorded setup road {road} must be legal"
    );
    state.apply(&action).unwrap();
}

/// Recorded position at the inferred default victory target. Uses the first
/// recorded resource world and a plausible all-VP opponent development
/// hypothesis. Use [`particles_with_development`] to vary that hypothesis
/// explicitly; never interpret the default as known hidden cards.
pub(crate) fn state(decision: &str) -> GameState {
    state_with_target(decision, INFERRED_VICTORY_TARGET)
}

/// Recorded position at an explicit victory target. The recording does not
/// export the target; 15 is inferred for this two-player game, so run 10/14
/// sensitivity through this entry point when the target matters.
pub(crate) fn state_with_target(decision: &str, victory_target: u8) -> GameState {
    let data = fixture();
    let position = &data["positions"][decision];
    let mut state = GameState::new(board(&data["board"]), victory_target);
    state.phase = if position["setup"].as_bool().unwrap() {
        Phase::SetupSettlement
    } else if position["pre_roll"].as_bool().unwrap() {
        Phase::PreRoll
    } else {
        Phase::Main
    };
    state.turn = u16::try_from(position["turn"].as_u64().unwrap()).unwrap();
    state.current_player = 0;
    state.bank_is_public = false;
    state.card_discard_limit = 9;
    state.friendly_robber = true;
    state.player_trades_enabled = false;
    state.robber_hex = byte(&position["robber"]);
    // Exported per decision (decisionContexts.roll); 0 when unrolled (D2/D4).
    state.last_roll = byte(&position["last_roll"]);
    if position["setup"].as_bool().unwrap() {
        if decision == "D4" {
            for (settlement, road) in setup_prefix() {
                apply_setup(&mut state, settlement, road);
            }
        }
        state.turn = u16::try_from(position["turn"].as_u64().unwrap()).unwrap();
        state.current_player = 0;
        // The replay must reproduce exactly the recorded snapshot pieces
        // (including the proven pre-decision P1 road for D4).
        let mut replayed_buildings = state
            .buildings
            .iter()
            .enumerate()
            .filter_map(|(index, building)| {
                building.as_ref().copied().map(|owner| {
                    [
                        index as u8,
                        owner.player(),
                        u8::from(!matches!(owner, Building::Settlement(_))),
                    ]
                })
            })
            .collect::<Vec<_>>();
        replayed_buildings.sort();
        let mut recorded_buildings = position["buildings"]
            .as_array()
            .unwrap()
            .iter()
            .map(|building| {
                [
                    byte(&building[0]),
                    byte(&building[1]),
                    u8::from(building[2].as_bool().unwrap()),
                ]
            })
            .collect::<Vec<_>>();
        recorded_buildings.sort();
        assert_eq!(replayed_buildings, recorded_buildings);
        let mut replayed_roads = state
            .roads
            .iter()
            .enumerate()
            .filter_map(|(index, owner)| {
                owner.as_ref().copied().map(|player| [index as u8, player])
            })
            .collect::<Vec<_>>();
        replayed_roads.sort();
        let mut recorded_roads = position["roads"]
            .as_array()
            .unwrap()
            .iter()
            .map(|road| [byte(&road[0]), byte(&road[1])])
            .collect::<Vec<_>>();
        recorded_roads.sort();
        assert_eq!(replayed_roads, recorded_roads);
    } else {
        for building in position["buildings"].as_array().unwrap() {
            let player = byte(&building[1]);
            state.buildings[byte(&building[0]) as usize] =
                Some(if building[2].as_bool().unwrap() {
                    Building::City(player)
                } else {
                    Building::Settlement(player)
                });
        }
        for road in position["roads"].as_array().unwrap() {
            state.roads[byte(&road[0]) as usize] = Some(byte(&road[1]));
        }
    }
    for player in 0..2 {
        let public = &position["players"][player];
        state.players[player].public_victory_points = byte(&position["public_vp"][player]);
        state.players[player].played_knights = byte(&public["knights"]);
        state.players[player].has_longest_road = public["longest"].as_bool().unwrap();
        state.players[player].has_largest_army = public["largest"].as_bool().unwrap();
        if state.players[player].has_longest_road {
            state.longest_road_holder = Some(player as u8);
        }
        if state.players[player].has_largest_army {
            state.largest_army_holder = Some(player as u8);
        }
        state.players[player].roads_left = 15
            - state
                .roads
                .iter()
                .filter(|owner| **owner == Some(player as u8))
                .count() as u8;
        state.players[player].settlements_left = 5 - state
            .buildings
            .iter()
            .filter(|building| **building == Some(Building::Settlement(player as u8)))
            .count() as u8;
        state.players[player].cities_left = 4 - state
            .buildings
            .iter()
            .filter(|building| **building == Some(Building::City(player as u8)))
            .count() as u8;
    }
    state.players[0].resources = hand(&position["hand"]);
    state.players[0].development = hand(&position["dev"]);
    state.players[0].bought_development = hand(&position["bought"]);
    state.players[0].played_development_this_turn = position["dev_played"].as_bool().unwrap();
    state.players[1].resources = hand(&position["worlds"][0]["hand"]);
    state.players[1].development[1] = byte(&position["players"][1]["dev_total"]);
    state.played_development = hand(&position["played_dev"]);
    reconcile_inventories(&mut state);
    state
}

fn reconcile_inventories(state: &mut GameState) {
    for resource in 0..5 {
        state.bank[resource] = 19u8
            .checked_sub(
                state
                    .players
                    .iter()
                    .map(|player| player.resources[resource])
                    .sum(),
            )
            .unwrap();
        state.development_deck[resource] = [14u8, 5, 2, 2, 2][resource]
            .checked_sub(
                state.played_development[resource]
                    + state
                        .players
                        .iter()
                        .map(|player| player.development[resource])
                        .sum::<u8>(),
            )
            .unwrap();
    }
}

/// Belief particles with the default all-VP opponent development hypothesis.
/// The hypothesis is explicit in the name chain, not a claim about the real
/// hidden cards; use [`particles_with_development`] to vary it.
pub(crate) fn particles(decision: &str) -> Vec<BeliefParticle> {
    let data = fixture();
    let total = byte(&data["positions"][decision]["players"][1]["dev_total"]);
    particles_with_development(decision, [0, total, 0, 0, 0])
}

pub(crate) fn particles_with_development(
    decision: &str,
    opponent_cards: [u8; 5],
) -> Vec<BeliefParticle> {
    let data = fixture();
    let baseline = state(decision);
    data["positions"][decision]["worlds"]
        .as_array()
        .unwrap()
        .iter()
        .map(|world| {
            let mut state = baseline.clone();
            state.players[1].resources = hand(&world["hand"]);
            state.players[1].development = opponent_cards;
            reconcile_inventories(&mut state);
            BeliefParticle {
                state,
                weight: world["weight"].as_f64().unwrap() as f32,
            }
        })
        .collect()
}

pub(crate) fn edge(id: &str) -> u8 {
    fixture()["board"]["edge_ids"]
        .as_array()
        .unwrap()
        .iter()
        .position(|value| value.as_str() == Some(id))
        .unwrap() as u8
}

pub(crate) fn vertex(id: &str) -> u8 {
    fixture()["board"]["vertex_ids"]
        .as_array()
        .unwrap()
        .iter()
        .position(|value| value.as_str() == Some(id))
        .unwrap() as u8
}

pub(crate) fn hex(id: &str) -> u8 {
    fixture()["board"]["hex_ids"]
        .as_array()
        .unwrap()
        .iter()
        .position(|value| value.as_str() == Some(id))
        .unwrap() as u8
}

#[cfg(test)]
mod tests {
    use super::*;
    use colonist_catan_core::Action;

    #[test]
    fn opening_replay_reaches_recorded_d4() {
        let state = state("D4");
        assert_eq!(state.phase, Phase::SetupSettlement);
        assert_eq!(state.current_player, 0);
        assert_eq!(state.players[0].resources, [0; 5]);
        // P1 starting grant from the second setup settlement (ore8, lumber4,
        // grain3 around v:-1,-1,1).
        assert_eq!(state.players[1].resources, [1, 0, 0, 1, 1]);
        assert_eq!(state.players[0].public_victory_points, 1);
        assert_eq!(state.players[1].public_victory_points, 2);
    }

    #[test]
    fn delayed_third_settlement_is_legal_then_built() {
        let delayed = Action::BuildSettlement {
            vertex: vertex("v:-1,2,0"),
        };
        for decision in ["D34", "D36", "D38", "D40"] {
            let before = state(decision);
            assert!(before.players[0].resources.iter().sum::<u8>() >= 4);
            assert!(before.legal_actions().contains(&delayed));
        }
        let mut built = state("D42");
        assert!(built.legal_actions().contains(&delayed));
        built.apply(&delayed).unwrap();
        assert_eq!(built.players[0].resources, [0, 1, 4, 0, 0]);
    }

    #[test]
    fn d63_road_completes_five_road_and_takes_longest() {
        let road = Action::BuildRoad {
            edge: edge("e:1,1,1"),
        };
        let before = state("D63");
        assert_eq!(before.longest_road_length(0), 4);
        assert_eq!(before.longest_road_holder, None);
        assert!(before.legal_actions().contains(&road));
        let after = state("D66");
        assert_eq!(after.players[0].resources, [1, 1, 1, 0, 1]);
        assert_eq!(after.longest_road_length(0), 5);
        assert_eq!(after.longest_road_holder, Some(0));
        assert_eq!(after.players[0].public_victory_points, 5);
    }

    #[test]
    fn recorded_bank_trades_are_legal_at_4_to_1() {
        assert_eq!(state("D97").trade_ratios(0), [4; 5]);
        let ore_to_grain = Action::MaritimeTrade {
            give: Resource::Ore,
            receive: Resource::Grain,
            ratio: 4,
        };
        assert!(state("D97").legal_actions().contains(&ore_to_grain));
        let wool_to_grain = Action::MaritimeTrade {
            give: Resource::Wool,
            receive: Resource::Grain,
            ratio: 4,
        };
        assert!(state("D114").legal_actions().contains(&wool_to_grain));
        assert!(state("D116").legal_actions().contains(&wool_to_grain));
    }

    #[test]
    fn all_recorded_positions_and_particles_validate() {
        for decision in [
            "D2", "D4", "D34", "D36", "D38", "D40", "D42", "D63", "D66", "D97", "D114", "D116",
        ] {
            let base = state(decision);
            base.validate()
                .unwrap_or_else(|err| panic!("{decision} baseline: {err}"));
            for (index, world) in particles(decision).iter().enumerate() {
                world
                    .state
                    .validate()
                    .unwrap_or_else(|err| panic!("{decision} world {index}: {err}"));
            }
        }
    }

    #[test]
    fn recorded_tiles_and_ports_match_board() {
        let recorded = state("D34");
        assert_eq!(recorded.last_roll, 7);
        assert_eq!(state("D2").last_roll, 0);
        assert_eq!(recorded.board.hexes[hex("h:0,2") as usize].resource, None);
        assert_eq!(
            recorded.board.hexes[hex("h:-1,2") as usize].resource,
            Some(Resource::Ore)
        );
        assert_eq!(recorded.board.hexes[hex("h:-1,2") as usize].number, 5);
        assert_eq!(recorded.board.hexes[hex("h:0,1") as usize].number, 10);
        assert_eq!(
            recorded.board.vertices[vertex("v:0,-2,0") as usize].port,
            Some(Port::Generic)
        );
        assert_eq!(
            recorded.board.vertices[vertex("v:-1,3,0") as usize].port,
            Some(Port::Resource(Resource::Ore))
        );
        assert_eq!(
            recorded.board.vertices[vertex("v:0,-1,0") as usize].port,
            None
        );
        assert!(
            recorded.board.edges[edge("e:0,1,2") as usize]
                .vertices
                .contains(&vertex("v:-1,2,0"))
        );
    }

    #[test]
    fn belief_worlds_carry_exact_recorded_weights() {
        for decision in ["D34", "D42", "D66", "D114"] {
            let total: f32 = particles(decision).iter().map(|world| world.weight).sum();
            assert!(
                (total - 1.0).abs() < 1e-6,
                "{decision} weights sum to {total}"
            );
        }
        assert_eq!(particles("D2").len(), 1);
        assert_eq!(particles("D42").len(), 14);
        assert_eq!(particles("D114").len(), 16);
    }
}
