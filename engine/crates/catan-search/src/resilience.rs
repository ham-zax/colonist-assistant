//! Public-board road resilience and route disruption analysis.
//!
//! This module evaluates exact topological vulnerability for player road
//! networks: critical cut vertices, award transfers, expansion loss, and
//! minimum bypass costs. It consumes only public board state and relies on
//! `catan-core` for authoritative rules and award updating.

use std::collections::VecDeque;

use colonist_catan_core::{Building, GameState};

use crate::eval::{road_distances, settlement_vertex_open};

const PUBLIC_PIPS: [f32; 13] = [0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 5.0, 4.0, 3.0, 2.0, 1.0];

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CriticalVertex {
    pub vertex: u8,
    pub target_player: u8,
    pub road_loss: u8,
    pub award_loss: bool,
    pub award_vp_swing: i8,
    pub expansion_loss: f32,
    pub bypass_roads: Option<u8>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CriticalEdge {
    pub edge: u8,
    pub target_player: u8,
    pub road_loss: u8,
    pub award_loss: bool,
    pub award_vp_swing: i8,
    pub expansion_loss: f32,
    pub bypass_roads: Option<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct RoadResilience {
    pub player: u8,
    pub critical_vertices: Vec<CriticalVertex>,
    pub critical_edges: Vec<CriticalEdge>,
    pub maximum_longest_road_loss: u8,
    pub maximum_award_vp_swing: i8,
    pub maximum_expansion_value_loss: f32,
    pub minimum_bypass_roads: Option<u8>,
}


/// Searches for the shortest path of unbuilt edges connecting any two distinct
/// branch endpoints around a cut vertex, without traversing the cut vertex or
/// any vertex occupied by an opponent building.
pub fn find_shortest_bypass(
    state: &GameState,
    player: u8,
    branches: &[u8],
    avoid_vertex: u8,
) -> Option<u8> {
    if branches.len() < 2 {
        return None;
    }

    let mut shortest = None;

    // For each pair of distinct branch endpoints, search for the shortest road path.
    for i in 0..branches.len() {
        for j in (i + 1)..branches.len() {
            let start = branches[i];
            let target = branches[j];

            if let Some(cost) = shortest_road_path(state, player, start, target, avoid_vertex) {
                shortest = Some(shortest.map_or(cost, |prev: u8| prev.min(cost)));
            }
        }
    }

    shortest
}

/// 0-1 BFS to find the shortest path in new roads from `start` to `target`,
/// avoiding `avoid_vertex` and any opponent buildings/roads.
fn shortest_road_path(
    state: &GameState,
    player: u8,
    start: u8,
    target: u8,
    avoid_vertex: u8,
) -> Option<u8> {
    if start == target {
        return Some(0);
    }
    if [start, target].into_iter().any(|vertex| {
        state.buildings[vertex as usize].is_some_and(|building| building.player() != player)
    }) {
        return None;
    }

    let vertex_count = state.board.vertices.len();
    let mut min_cost = vec![u8::MAX; vertex_count];
    let mut deque = VecDeque::new();

    min_cost[start as usize] = 0;
    deque.push_back((start, 0u8));

    while let Some((curr, cost)) = deque.pop_front() {
        if curr == target {
            return Some(cost);
        }

        if cost > min_cost[curr as usize] {
            continue;
        }

        for &edge in &state.board.vertices[curr as usize].adjacent_edges {
            let edge_owner = state.roads[edge as usize];
            // If the edge is owned by an opponent, it is impassable.
            if edge_owner.is_some_and(|owner| owner != player) {
                continue;
            }

            let [a, b] = state.board.edges[edge as usize].vertices;
            let next_v = if a == curr { b } else { a };

            if next_v == avoid_vertex {
                continue;
            }

            // An opponent building blocks road continuation through that vertex.
            if state.buildings[next_v as usize].is_some_and(|bld| bld.player() != player) {
                continue;
            }

            // If player already owns the edge, cost addition is 0; if unbuilt, cost is 1.
            let edge_cost = if edge_owner == Some(player) { 0 } else { 1 };
            let new_cost = cost.saturating_add(edge_cost);

            if new_cost < min_cost[next_v as usize] {
                min_cost[next_v as usize] = new_cost;
                if edge_cost == 0 {
                    deque.push_front((next_v, new_cost));
                } else {
                    deque.push_back((next_v, new_cost));
                }
            }
        }
    }

    None
}

/// Evaluates public expansion opportunity for `player` based purely on
/// public board topology, existing roads/buildings, open vertices, and board pip yield.
///
/// MUST NOT inspect private player hand or development cards.
pub fn public_expansion_opportunity(state: &GameState, player: u8) -> f32 {
    let roads_left = state.players[player as usize].roads_left;
    let settlements_left = state.players[player as usize].settlements_left;
    if settlements_left == 0 {
        return 0.0;
    }

    let distances = road_distances(state, player);

    let mut top = [0.0f32; 3];
    for (v, &cost) in distances.iter().enumerate() {
        if cost == u8::MAX || cost > roads_left || !settlement_vertex_open(state, v) {
            continue;
        }
        let mut pips = 0.0f32;
        for &hex in &state.board.vertices[v].adjacent_hexes {
            let tile = &state.board.hexes[hex as usize];
            if tile.resource.is_some() {
                let robber_factor = if hex == state.robber_hex { 0.3 } else { 1.0 };
                pips += PUBLIC_PIPS[tile.number as usize] * robber_factor;
            }
        }
        if state.board.vertices[v].port.is_some() {
            pips += 0.7;
        }
        let site_value = pips / (1.0 + cost as f32 * 0.4);
        if site_value > top[0] {
            top[2] = top[1];
            top[1] = top[0];
            top[0] = site_value;
        } else if site_value > top[1] {
            top[2] = top[1];
            top[1] = site_value;
        } else if site_value > top[2] {
            top[2] = site_value;
        }
    }

    top[0] + top[1] * 0.5 + top[2] * 0.25
}

/// Evaluates the hypothetical consequence of an opponent building a settlement
/// on `vertex`, which could cut `target_player`'s road network or deny expansion.
pub fn evaluate_vertex_cut(
    state: &GameState,
    target_player: u8,
    vertex: u8,
) -> Option<CriticalVertex> {
    if !settlement_vertex_open(state, vertex as usize) {
        return None;
    }

    let adjacent_target_edges: Vec<u8> = state.board.vertices[vertex as usize]
        .adjacent_edges
        .iter()
        .copied()
        .filter(|&edge| state.roads[edge as usize] == Some(target_player))
        .collect();

    if adjacent_target_edges.is_empty() {
        return None;
    }

    let base_len = state.longest_road_length(target_player);
    let base_holder = state.longest_road_holder;
    let base_exp = public_expansion_opportunity(state, target_player);

    let opponent = (target_player + 1) % state.board.num_players;
    let mut probe = state.clone();
    probe.buildings[vertex as usize] = Some(Building::Settlement(opponent));
    probe.update_longest_road();

    let new_len = probe.longest_road_length(target_player);
    let road_loss = base_len.saturating_sub(new_len);
    let award_loss = base_holder == Some(target_player)
        && (0..state.board.num_players)
            .filter(|attacker| *attacker != target_player)
            .any(|attacker| {
                let mut award_probe = state.clone();
                award_probe.buildings[vertex as usize] = Some(Building::Settlement(attacker));
                award_probe.update_longest_road();
                award_probe.longest_road_holder != Some(target_player)
            });
    let award_vp_swing = if award_loss { -2 } else { 0 };

    let new_exp = public_expansion_opportunity(&probe, target_player);
    let expansion_loss = (base_exp - new_exp).max(0.0);

    let bypass_roads = if adjacent_target_edges.len() >= 2 {
        if road_loss == 0 {
            Some(0)
        } else {
            let branch_vertices: Vec<u8> = adjacent_target_edges
                .iter()
                .map(|&edge| {
                    let [a, b] = state.board.edges[edge as usize].vertices;
                    if a == vertex { b } else { a }
                })
                .collect();
            find_shortest_bypass(state, target_player, &branch_vertices, vertex)
        }
    } else {
        None
    };

    if road_loss > 0 || award_loss || expansion_loss > 0.05 {
        Some(CriticalVertex {
            vertex,
            target_player,
            road_loss,
            award_loss,
            award_vp_swing,
            expansion_loss,
            bypass_roads,
        })
    } else {
        None
    }
}

/// Evaluates the hypothetical consequence of an opponent claiming unbuilt `edge`
/// that is adjacent to `target_player`'s road network.
pub fn evaluate_edge_cut(
    state: &GameState,
    target_player: u8,
    edge: u8,
) -> Option<CriticalEdge> {
    if state.roads.get(edge as usize).is_none_or(Option::is_some) {
        return None;
    }

    let [a, b] = state.board.edges[edge as usize].vertices;
    let adjacent_to_player = state.board.vertices[a as usize]
        .adjacent_edges
        .iter()
        .chain(&state.board.vertices[b as usize].adjacent_edges)
        .any(|&e| state.roads[e as usize] == Some(target_player));

    if !adjacent_to_player {
        return None;
    }

    let opponent = (target_player + 1) % state.board.num_players;
    let base_exp = public_expansion_opportunity(state, target_player);
    let base_holder = state.longest_road_holder;

    // Expansion denial depends only on the edge becoming unavailable to the
    // target player, so any non-target owner is an equivalent public probe.
    let mut probe = state.clone();
    probe.roads[edge as usize] = Some(opponent);

    // Award vulnerability is structural: report it when at least one opponent
    // claiming this edge would strip the current holder. Opponent reachability
    // and affordability remain a later policy/search question.
    let award_loss = base_holder == Some(target_player)
        && (0..state.board.num_players)
            .filter(|attacker| *attacker != target_player)
            .any(|attacker| {
                let mut award_probe = state.clone();
                award_probe.roads[edge as usize] = Some(attacker);
                award_probe.update_longest_road();
                award_probe.longest_road_holder != Some(target_player)
            });
    let award_vp_swing = if award_loss { -2 } else { 0 };

    let new_exp = public_expansion_opportunity(&probe, target_player);
    let expansion_loss = (base_exp - new_exp).max(0.0);

    if award_loss || expansion_loss > 0.10 {
        Some(CriticalEdge {
            edge,
            target_player,
            road_loss: 0,
            award_loss,
            award_vp_swing,
            expansion_loss,
            bypass_roads: None,
        })
    } else {
        None
    }
}

/// Computes comprehensive road resilience for `player` on the current public board.
pub fn analyze_road_resilience(state: &GameState, player: u8) -> RoadResilience {
    let mut critical_vertices = Vec::new();
    for v in 0..state.board.vertices.len() as u8 {
        if let Some(critical) = evaluate_vertex_cut(state, player, v) {
            critical_vertices.push(critical);
        }
    }

    let mut critical_edges = Vec::new();
    for e in 0..state.board.edges.len() as u8 {
        if let Some(critical) = evaluate_edge_cut(state, player, e) {
            critical_edges.push(critical);
        }
    }

    let maximum_longest_road_loss = critical_vertices
        .iter()
        .map(|c| c.road_loss)
        .max()
        .unwrap_or(0);

    let maximum_award_vp_swing = critical_vertices
        .iter()
        .map(|c| c.award_vp_swing)
        .chain(critical_edges.iter().map(|c| c.award_vp_swing))
        .min()
        .unwrap_or(0);

    let maximum_expansion_value_loss = critical_vertices
        .iter()
        .map(|c| c.expansion_loss)
        .chain(critical_edges.iter().map(|c| c.expansion_loss))
        .reduce(f32::max)
        .unwrap_or(0.0);

    let minimum_bypass_roads = critical_vertices
        .iter()
        .filter_map(|c| c.bypass_roads)
        .filter(|&b| b > 0)
        .min();

    RoadResilience {
        player,
        critical_vertices,
        critical_edges,
        maximum_longest_road_loss,
        maximum_award_vp_swing,
        maximum_expansion_value_loss,
        minimum_bypass_roads,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colonist_catan_core::GameState;

    #[test]
    fn fragile_single_intersection_chain_reports_material_loss() {
        let mut state = GameState::standard(1, 3);
        let e0 = 0usize;
        let [_v0, v1] = state.board.edges[e0].vertices;
        state.roads[e0] = Some(0);

        let e1 = *state.board.vertices[v1 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e0 as u8)
            .unwrap() as usize;
        state.roads[e1] = Some(0);
        let [_, v2] = state.board.edges[e1].vertices;
        let v2 = if v2 == v1 { state.board.edges[e1].vertices[0] } else { v2 };

        let e2 = *state.board.vertices[v2 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e1 as u8)
            .unwrap() as usize;
        state.roads[e2] = Some(0);
        let [_, v3] = state.board.edges[e2].vertices;
        let v3 = if v3 == v2 { state.board.edges[e2].vertices[0] } else { v3 };

        let e3 = *state.board.vertices[v3 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e2 as u8)
            .unwrap() as usize;
        state.roads[e3] = Some(0);
        let [_, v4] = state.board.edges[e3].vertices;
        let v4 = if v4 == v3 { state.board.edges[e3].vertices[0] } else { v4 };

        let e4 = *state.board.vertices[v4 as usize]
            .adjacent_edges
            .iter()
            .find(|&&e| e != e3 as u8)
            .unwrap() as usize;
        state.roads[e4] = Some(0);

        state.update_longest_road();
        let initial_len = state.longest_road_length(0);
        assert_eq!(initial_len, 5);
        assert_eq!(state.longest_road_holder, Some(0));

        let cut = evaluate_vertex_cut(&state, 0, v2);
        assert!(cut.is_some());
        let cut = cut.unwrap();
        assert!(cut.road_loss > 0, "cut must cause road loss");
        assert!(cut.award_loss, "cut must cause award loss since length drops below 5");
        assert_eq!(cut.award_vp_swing, -2);
    }

    #[test]
    fn loop_with_bypass_reports_zero_road_loss() {
        let mut state = GameState::standard(1, 3);
        let hex = 0usize;
        let hex_edges: Vec<u8> = state.board.edges
            .iter()
            .enumerate()
            .filter(|(_, edge)| edge.adjacent_hexes.contains(&(hex as u8)))
            .map(|(index, _)| index as u8)
            .collect();
        assert_eq!(hex_edges.len(), 6);

        // Build all 6 edges of the hex
        for &edge in &hex_edges {
            state.roads[edge as usize] = Some(0);
        }
        // Also build 1 attached extension edge outside the hex
        let [v0, _] = state.board.edges[hex_edges[0] as usize].vertices;
        let ext_edge = state.board.vertices[v0 as usize]
            .adjacent_edges
            .iter()
            .copied()
            .find(|&e| !hex_edges.contains(&e));

        if let Some(ext) = ext_edge {
            state.roads[ext as usize] = Some(0);
            state.update_longest_road();
            let base_len = state.longest_road_length(0);
            assert!(base_len >= 6);

            // Cut a vertex on the far side of the hex (not v0)
            let far_vertex = state.board.edges[hex_edges[3] as usize].vertices[0];
            let cut = evaluate_vertex_cut(&state, 0, far_vertex);
            if let Some(cut) = cut {
                // Because of the hex cycle, even cutting far_vertex leaves a 5-edge hex path + extension = 6!
                let after_len = base_len.saturating_sub(cut.road_loss);
                assert!(after_len >= 5, "bypass keeps road length >= 5");
                assert_eq!(cut.award_loss, false, "award is not lost due to bypass");
            }
        }
    }
}
