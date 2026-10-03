//! Focused positions extracted from the sea8653 live recording (2026-10-03).
//! Geometry, public pieces, our hand/cards, and resource-belief worlds follow
//! the recording. Hidden opponent development cards are a caller-visible
//! hypothesis, not a claim about their real identities. Dice use M0 here;
//! these are regression positions, not an exact historical balanced-dice replay.

use colonist_catan_core::{Board, Building, Edge, GameState, Hex, Phase, Port, Resource, Vertex};
use serde_json::Value;

use crate::BeliefParticle;

fn fixture() -> Value {
    serde_json::from_str(include_str!("test_fixtures/sea8653.json")).unwrap()
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

/// Uses the first recorded resource world and a plausible all-VP opponent
/// development hypothesis. Use `particles_with_development` to vary that
/// hypothesis explicitly; never interpret the default as known hidden cards.
pub(crate) fn state(decision: &str) -> GameState {
    let data = fixture();
    let position = &data["positions"][decision];
    let mut state = GameState::new(board(&data["board"]), 15);
    state.phase = if position["pre_roll"].as_bool().unwrap() {
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
    for building in position["buildings"].as_array().unwrap() {
        let player = byte(&building[1]);
        state.buildings[byte(&building[0]) as usize] = Some(if building[2].as_bool().unwrap() {
            Building::City(player)
        } else {
            Building::Settlement(player)
        });
    }
    for road in position["roads"].as_array().unwrap() {
        state.roads[byte(&road[0]) as usize] = Some(byte(&road[1]));
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
    fn recorded_free_pair_legality_and_network_lengths() {
        let mut state = state("D235");
        assert_eq!(state.players[0].victory_points(), 13);
        assert_eq!(state.players[0].resources, [0; 5]);
        assert_eq!(state.trade_ratios(0), [3; 5]);
        assert_eq!(state.longest_road_length(0), 4);
        assert_eq!(state.longest_road_length(1), 10);
        state
            .apply(&Action::PlayRoadBuilding {
                first: edge("e:1,0,1"),
                second: Some(edge("e:1,0,0")),
            })
            .unwrap();
        assert_eq!(state.longest_road_length(0), 8);
        assert_eq!(state.longest_road_holder, Some(1));
        assert_eq!(state.winner(), None);
    }

    #[test]
    fn recorded_trade_cycle_is_legal_but_destroys_two_lumber_and_two_grain() {
        let mut traded = state("D221");
        let mut direct = traded.clone();
        assert_eq!(traded.players[0].resources, [2, 0, 1, 3, 1]);
        assert!(direct.legal_actions().contains(&Action::BuyDevelopment));
        direct.apply(&Action::BuyDevelopment).unwrap();
        traded
            .apply(&Action::MaritimeTrade {
                give: Resource::Grain,
                receive: Resource::Lumber,
                ratio: 3,
            })
            .unwrap();
        assert!(!traded.legal_actions().contains(&Action::BuyDevelopment));
        traded
            .apply(&Action::MaritimeTrade {
                give: Resource::Lumber,
                receive: Resource::Grain,
                ratio: 3,
            })
            .unwrap();
        traded.apply(&Action::BuyDevelopment).unwrap();
        assert_eq!(direct.players[0].resources, [2, 0, 0, 2, 0]);
        assert_eq!(traded.players[0].resources, [0; 5]);
    }
}
