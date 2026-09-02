//! Road-only tactical scenario corpus definition, loader, and mechanical verifier.
//!
//! The authoritative corpus is defined in `tests/fixtures/latent-threat-tactical-corpus.json`.
//! This module loads the fixture, constructs valid game states, and strictly verifies
//! G0 mechanical consequences.

use std::path::{Path, PathBuf};

use colonist_catan_core::{Action, Building, GameState, Phase, ResourceHand};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalActionSpec {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertex: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<u8>,
}

impl TacticalActionSpec {
    pub fn to_action(&self) -> Action {
        match self.kind.as_str() {
            "build-settlement" | "place-settlement" => Action::BuildSettlement {
                vertex: self.vertex.expect("vertex required for build-settlement"),
            },
            "build-road" | "place-road" => Action::BuildRoad {
                edge: self.edge.expect("edge required for build-road"),
            },
            "end-turn" => Action::EndTurn,
            "roll" => Action::Roll,
            other => panic!("unsupported tactical action spec kind: {other}"),
        }
    }

    pub fn from_action(action: &Action) -> Self {
        match action {
            Action::BuildSettlement { vertex } | Action::PlaceSettlement { vertex } => Self {
                kind: "build-settlement".into(),
                vertex: Some(*vertex),
                edge: None,
            },
            Action::BuildRoad { edge } | Action::PlaceRoad { edge } => Self {
                kind: "build-road".into(),
                vertex: None,
                edge: Some(*edge),
            },
            Action::EndTurn => Self {
                kind: "end-turn".into(),
                vertex: None,
                edge: None,
            },
            Action::Roll => Self {
                kind: "roll".into(),
                vertex: None,
                edge: None,
            },
            other => panic!("unsupported action for tactical action spec: {other:?}"),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadBuildingSpec {
    pub player: u8,
    pub vertex: u8,
    pub kind: String, // "settlement" or "city"
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoadEdgeSpec {
    pub player: u8,
    pub edge: u8,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalStateSpec {
    pub board_seed: u64,
    pub players: u8,
    pub active_player: u8,
    pub roads: Vec<RoadEdgeSpec>,
    pub buildings: Vec<RoadBuildingSpec>,
    pub resources: Vec<ResourceHand>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalProposalProbe {
    pub player: u8,
    pub resources: ResourceHand,
    pub action: TacticalActionSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalScenario {
    pub id: String,
    pub description: String,
    pub family: String,
    pub state: TacticalStateSpec,
    pub candidate_roots: Vec<TacticalActionSpec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proposal_probe: Option<TacticalProposalProbe>,
    pub expected_best_root: TacticalActionSpec,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub negative_control_root: Option<TacticalActionSpec>,
    pub declared_consequence: String,
    pub is_negative_control: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalCorpus {
    pub description: String,
    pub schema_version: u32,
    pub scenarios: Vec<TacticalScenario>,
}

/// Locates the authoritative fixture path.
pub fn default_corpus_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures/latent-threat-tactical-corpus.json")
}

/// Loads the authoritative tactical corpus from a JSON file.
pub fn load_tactical_corpus(path: &Path) -> Result<TacticalCorpus, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read corpus file {:?}: {e}", path))?;
    serde_json::from_str(&content)
        .map_err(|e| format!("failed to deserialize corpus JSON: {e}"))
}

fn rebalance_bank_from_hands(state: &mut GameState) -> Result<(), String> {
    for resource in 0..5 {
        let held = state
            .players
            .iter()
            .map(|player| player.resources[resource] as u16)
            .sum::<u16>();
        let bank = 19u16.checked_sub(held).ok_or_else(|| {
            format!("resource {resource} hands exceed the 19-card supply: {held}")
        })?;
        state.bank[resource] = bank as u8;
    }
    Ok(())
}

/// Constructs and validates a resource-conserving GameState from a TacticalStateSpec.
pub fn build_state(spec: &TacticalStateSpec) -> Result<GameState, String> {
    let mut state = GameState::standard(spec.board_seed, spec.players);
    state.current_player = spec.active_player;
    state.phase = Phase::Main;
    state.domestic_trade_used = false;
    state.domestic_trade_count = 0;
    state.trade = None;
    state.last_rejected_trade = None;
    state.trade_negotiation_round = 0;

    // Reset initial resources and set specified hands
    for p in 0..spec.players as usize {
        state.players[p].resources = [0; 5];
    }
    for (p, &hand) in spec.resources.iter().enumerate() {
        if p < state.players.len() {
            state.players[p].resources = hand;
        }
    }

    // Place roads
    for road in &spec.roads {
        state.roads[road.edge as usize] = Some(road.player);
        state.players[road.player as usize].roads_left =
            state.players[road.player as usize].roads_left.saturating_sub(1);
    }

    // Place buildings
    for bld in &spec.buildings {
        let building = match bld.kind.as_str() {
            "settlement" => {
                state.players[bld.player as usize].settlements_left =
                    state.players[bld.player as usize].settlements_left.saturating_sub(1);
                state.players[bld.player as usize].public_victory_points += 1;
                Building::Settlement(bld.player)
            }
            "city" => {
                state.players[bld.player as usize].cities_left =
                    state.players[bld.player as usize].cities_left.saturating_sub(1);
                state.players[bld.player as usize].public_victory_points += 2;
                Building::City(bld.player)
            }
            other => panic!("unknown building kind: {other}"),
        };
        state.buildings[bld.vertex as usize] = Some(building);
    }

    state.update_longest_road();
    rebalance_bank_from_hands(&mut state)?;
    state
        .validate()
        .map_err(|error| format!("invalid tactical state: {error}"))?;
    Ok(state)
}

/// Rebalances the public bank after a benchmark-only hand override.
pub fn rebalance_tactical_bank(state: &mut GameState) -> Result<(), String> {
    rebalance_bank_from_hands(state)
}

/// Checks if a vertex can legally receive a settlement under public occupancy and distance rules.
fn settlement_vertex_open(state: &GameState, vertex: usize) -> bool {
    if state.buildings[vertex].is_some() {
        return false;
    }
    !state.board.vertices[vertex]
        .adjacent_vertices
        .iter()
        .any(|&neighbor| state.buildings[neighbor as usize].is_some())
}

fn player_can_settle_vertex(state: &GameState, player: u8, vertex: u8) -> bool {
    settlement_vertex_open(state, vertex as usize)
        && state.board.vertices[vertex as usize]
            .adjacent_edges
            .iter()
            .any(|&e| state.roads[e as usize] == Some(player))
}

fn shortest_route_distance(state: &GameState, player: u8, start: u8, goal: u8) -> Option<u8> {
    if start == goal {
        return Some(0);
    }
    let mut min_cost = vec![u8::MAX; state.board.vertices.len()];
    let mut deque = std::collections::VecDeque::new();
    min_cost[start as usize] = 0;
    deque.push_back((start, 0u8));

    while let Some((curr, cost)) = deque.pop_front() {
        if curr == goal {
            return Some(cost);
        }
        if cost > min_cost[curr as usize] {
            continue;
        }
        for &e in &state.board.vertices[curr as usize].adjacent_edges {
            let owner = state.roads[e as usize];
            if owner.is_some_and(|o| o != player) {
                continue;
            }
            let [a, b] = state.board.edges[e as usize].vertices;
            let next_v = if a == curr { b } else { a };
            if state.buildings[next_v as usize].is_some_and(|bld| bld.player() != player && next_v != goal) {
                continue;
            }
            let edge_cost = if owner == Some(player) { 0 } else { 1 };
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

/// Strictly verifies G0 mechanical consequences for a tactical scenario.
pub fn verify_mechanical_consequence(scenario: &TacticalScenario) -> Result<(), String> {
    let base = build_state(&scenario.state)?;
    let best_action = scenario.expected_best_root.to_action();

    let mut next = base.clone();
    if let Err(e) = next.apply(&best_action) {
        return Err(format!("expected root action {:?} is illegal: {e:?}", best_action));
    }

    match scenario.declared_consequence.as_str() {
        "strips_longest_road" => {
            let rival = 1u8;
            if base.longest_road_holder != Some(rival) {
                return Err(format!("base state does not have player {rival} holding longest road"));
            }
            if next.longest_road_holder == Some(rival) {
                return Err(format!("next state still has player {rival} holding longest road"));
            }
        }
        "transfers_longest_road" => {
            let actor = scenario.state.active_player;
            if base.longest_road_holder == Some(actor) {
                return Err(format!("base state already has actor {actor} holding longest road"));
            }
            if next.longest_road_holder != Some(actor) {
                return Err(format!("next state does not transfer longest road to actor {actor}"));
            }
        }
        "bypassed_cycle_retains_award" => {
            let rival = 1u8;
            if base.longest_road_holder != Some(rival) {
                return Err(format!("base state does not have player {rival} holding longest road"));
            }
            if next.longest_road_holder != Some(rival) {
                return Err(format!("cut on bypassed cycle unexpectedly stripped award from {rival}"));
            }
        }
        "preserves_expansion_lane" => {
            let actor = scenario.state.active_player;
            let Action::BuildRoad { edge } = best_action else {
                return Err("preserves_expansion_lane requires a BuildRoad action".into());
            };
            if next.longest_road_holder != base.longest_road_holder {
                return Err("expansion-lane control is confounded by a Longest Road transfer".into());
            }
            let [a, b] = base.board.edges[edge as usize].vertices;
            // Identify the newly reached vertex that was not reachable before
            let target_vertex = if player_can_settle_vertex(&base, actor, a) {
                b
            } else {
                a
            };
            // In base, target_vertex must not be reachable for settlement placement
            if player_can_settle_vertex(&base, actor, target_vertex) {
                return Err(format!("base state already connects target vertex {target_vertex}"));
            }
            // In next, target_vertex must be open and legally available for settlement
            if !settlement_vertex_open(&next, target_vertex as usize) {
                return Err(format!("target vertex {target_vertex} is not open for settlement"));
            }
            if !player_can_settle_vertex(&next, actor, target_vertex) {
                return Err(format!("next state cannot connect a settlement on target vertex {target_vertex}"));
            }
            let settlement = Action::BuildSettlement {
                vertex: target_vertex,
            };
            if !next.legal_actions().contains(&settlement) {
                return Err(format!(
                    "defensive road does not unlock a legal same-turn settlement on vertex {target_vertex}"
                ));
            }
        }
        "detour_available" => {
            // Scenario 5: An apparent choke edge is challenged by an alternative detour.
            let neg_action = scenario
                .negative_control_root
                .as_ref()
                .ok_or_else(|| "detour_available requires a negative_control_root".to_string())?
                .to_action();
            let Action::BuildRoad { edge: choke_edge } = neg_action else {
                return Err("detour_available negative control must be a BuildRoad choke action".into());
            };
            // Simulate blocking the apparent choke edge with actor's road
            let mut choked = base.clone();
            if let Err(e) = choked.apply(&neg_action) {
                return Err(format!("choke action {:?} is illegal: {e:?}", neg_action));
            }
            let rival = 1u8;
            // Identify rival's route endpoints: from rival settlement (vertex 3) to unbuilt destination vertex 17
            let start_v = 3u8;
            let goal_v = 17u8;

            let d_base = shortest_route_distance(&base, rival, start_v, goal_v)
                .ok_or_else(|| format!("rival {rival} cannot reach goal {goal_v} in base state"))?;
            let d_choked = shortest_route_distance(&choked, rival, start_v, goal_v)
                .ok_or_else(|| format!("rival {rival} cannot reach goal {goal_v} after choke"))?;

            // The choke edge did not lengthen rival's route; a parallel detour of equal cost exists
            if d_choked > d_base {
                return Err(format!("choke edge {choke_edge} increased distance from {d_base} to {d_choked}"));
            }
        }
        "negative_control_vanity_branch" => {
            let actor = scenario.state.active_player;
            let neg_action = scenario
                .negative_control_root
                .as_ref()
                .ok_or_else(|| "vanity branch requires a negative_control_root".to_string())?
                .to_action();
            let mut vanity_state = base.clone();
            if let Err(e) = vanity_state.apply(&neg_action) {
                return Err(format!("vanity road action {:?} is illegal: {e:?}", neg_action));
            }
            if vanity_state.longest_road_length(actor) > base.longest_road_length(actor) {
                return Err("vanity branch unexpectedly increased longest road length".into());
            }
            if vanity_state.longest_road_holder == Some(actor) && base.longest_road_holder != Some(actor) {
                return Err("vanity branch unexpectedly claimed longest road".into());
            }
        }
        other => {
            return Err(format!("unknown declared consequence: {other}"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checked_in_corpus_passes_mechanical_g0() {
        let path = default_corpus_path();
        let corpus = load_tactical_corpus(&path).expect("failed to load checked-in tactical corpus");
        assert_eq!(corpus.scenarios.len(), 6);
        for scenario in &corpus.scenarios {
            let res = verify_mechanical_consequence(scenario);
            assert!(res.is_ok(), "scenario {} failed G0: {:?}", scenario.id, res.err());
        }
    }
}
