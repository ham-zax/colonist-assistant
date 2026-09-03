//! Tactical scenario corpus definition, loader, and mechanical verifier.
//!
//! The authoritative corpus is defined in `tests/fixtures/latent-threat-tactical-corpus.json`.
//! This module loads the fixture, constructs valid game states, and strictly verifies
//! G0 mechanical consequences without duplicating production legality rules.

use std::path::{Path, PathBuf};

use colonist_catan_core::{
    Action, Building, DevCard, GameState, Phase, Resource, ResourceHand, ROAD_COST,
    SETTLEMENT_COST,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalActionSpec {
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vertex: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub edge: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_edge: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resource: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub second_resource: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hex: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub victim: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ratio: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recipients: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub give: Option<ResourceHand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub receive: Option<ResourceHand>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accept: Option<bool>,
}

fn resource(index: u8) -> Resource {
    *Resource::ALL
        .get(index as usize)
        .unwrap_or_else(|| panic!("resource index must be 0..4, found {index}"))
}

impl TacticalActionSpec {
    pub fn to_action(&self) -> Action {
        match self.kind.as_str() {
            "build-settlement" => Action::BuildSettlement {
                vertex: self.vertex.expect("vertex required for build-settlement"),
            },
            "place-settlement" => Action::PlaceSettlement {
                vertex: self.vertex.expect("vertex required for place-settlement"),
            },
            "build-road" => Action::BuildRoad {
                edge: self.edge.expect("edge required for build-road"),
            },
            "place-road" => Action::PlaceRoad {
                edge: self.edge.expect("edge required for place-road"),
            },
            "build-city" => Action::BuildCity {
                vertex: self.vertex.expect("vertex required for build-city"),
            },
            "buy-development" => Action::BuyDevelopment,
            "play-road-building" => Action::PlayRoadBuilding {
                first: self.edge.expect("edge required for play-road-building"),
                second: self.second_edge,
            },
            "play-year-of-plenty" => Action::PlayYearOfPlenty {
                first: resource(self.resource.expect("resource required for play-year-of-plenty")),
                second: resource(
                    self.second_resource
                        .expect("secondResource required for play-year-of-plenty"),
                ),
            },
            "play-monopoly" => Action::PlayMonopoly {
                resource: resource(self.resource.expect("resource required for play-monopoly")),
            },
            "play-knight" => Action::PlayKnight {
                hex: self.hex.expect("hex required for play-knight"),
                victim: self.victim,
            },
            "maritime-trade" => Action::MaritimeTrade {
                give: resource(self.resource.expect("resource required for maritime-trade")),
                receive: resource(
                    self.second_resource
                        .expect("secondResource required for maritime-trade"),
                ),
                ratio: self.ratio.expect("ratio required for maritime-trade"),
            },
            "offer-trade" => Action::OfferTrade {
                recipients: self.recipients.expect("recipients required for offer-trade"),
                give: self.give.expect("give required for offer-trade"),
                receive: self.receive.expect("receive required for offer-trade"),
            },
            "respond-trade" => Action::RespondTrade {
                accept: self.accept.expect("accept required for respond-trade"),
            },
            "end-turn" => Action::EndTurn,
            "roll" => Action::Roll,
            other => panic!("unsupported tactical action spec kind: {other}"),
        }
    }

    pub fn from_action(action: &Action) -> Self {
        let mut spec = Self {
            kind: String::new(),
            vertex: None,
            edge: None,
            second_edge: None,
            resource: None,
            second_resource: None,
            hex: None,
            victim: None,
            ratio: None,
            recipients: None,
            give: None,
            receive: None,
            accept: None,
        };
        match action {
            Action::BuildSettlement { vertex } => {
                spec.kind = "build-settlement".into();
                spec.vertex = Some(*vertex);
            }
            Action::PlaceSettlement { vertex } => {
                spec.kind = "place-settlement".into();
                spec.vertex = Some(*vertex);
            }
            Action::BuildRoad { edge } => {
                spec.kind = "build-road".into();
                spec.edge = Some(*edge);
            }
            Action::PlaceRoad { edge } => {
                spec.kind = "place-road".into();
                spec.edge = Some(*edge);
            }
            Action::BuildCity { vertex } => {
                spec.kind = "build-city".into();
                spec.vertex = Some(*vertex);
            }
            Action::BuyDevelopment => spec.kind = "buy-development".into(),
            Action::PlayRoadBuilding { first, second } => {
                spec.kind = "play-road-building".into();
                spec.edge = Some(*first);
                spec.second_edge = *second;
            }
            Action::PlayYearOfPlenty { first, second } => {
                spec.kind = "play-year-of-plenty".into();
                spec.resource = Some(*first as u8);
                spec.second_resource = Some(*second as u8);
            }
            Action::PlayMonopoly { resource } => {
                spec.kind = "play-monopoly".into();
                spec.resource = Some(*resource as u8);
            }
            Action::PlayKnight { hex, victim } => {
                spec.kind = "play-knight".into();
                spec.hex = Some(*hex);
                spec.victim = *victim;
            }
            Action::EndTurn => spec.kind = "end-turn".into(),
            Action::Roll => spec.kind = "roll".into(),
            other => panic!("unsupported action for tactical action spec: {other:?}"),
        }
        spec
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
    #[serde(default)]
    pub phase: Option<String>,
    #[serde(default = "default_victory_target")]
    pub victory_target: u8,
    pub roads: Vec<RoadEdgeSpec>,
    pub buildings: Vec<RoadBuildingSpec>,
    pub resources: Vec<ResourceHand>,
    #[serde(default)]
    pub development: Vec<[u8; 5]>,
    #[serde(default)]
    pub bought_development: Vec<[u8; 5]>,
    #[serde(default)]
    pub played_knights: Vec<u8>,
    #[serde(default)]
    pub played_development_this_turn: Vec<bool>,
    #[serde(default)]
    pub development_deck: Option<[u8; 5]>,
    #[serde(default = "default_true")]
    pub bank_is_public: bool,
    #[serde(default)]
    pub robber_hex: Option<u8>,
    #[serde(default)]
    pub largest_army_holder: Option<u8>,
    #[serde(default)]
    pub domestic_trade_disabled: u8,
}

const fn default_victory_target() -> u8 {
    10
}

const fn default_true() -> bool {
    true
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalHiddenVariant {
    pub id: String,
    pub weight: f32,
    #[serde(default)]
    pub resources: Option<Vec<ResourceHand>>,
    #[serde(default)]
    pub development: Option<Vec<[u8; 5]>>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalThreatProbe {
    pub protected_player: u8,
    #[serde(default)]
    pub variants: Vec<TacticalHiddenVariant>,
    pub expected_weight: f32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalObservationSafetyProbe {
    pub actor: u8,
    pub variant: TacticalHiddenVariant,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalPosteriorSensitivityProbe {
    pub actor: u8,
    pub variant: TacticalHiddenVariant,
    #[serde(default)]
    pub setup_action: Option<TacticalActionSpec>,
    pub candidate_roots: Vec<TacticalActionSpec>,
    pub expected_zero_root: TacticalActionSpec,
    pub expected_full_root: TacticalActionSpec,
    #[serde(default)]
    pub require_switch: bool,
    #[serde(default = "default_true")]
    pub require_five_percent_stable: bool,
    #[serde(default)]
    pub strict_safety_action: Option<TacticalActionSpec>,
    #[serde(default)]
    pub require_strict_safety_transition: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalCloseoutProbe {
    pub same_turn_root: TacticalActionSpec,
    pub delayed_root: TacticalActionSpec,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalMechanicalExpectations {
    #[serde(default)]
    pub action: Option<TacticalActionSpec>,
    #[serde(default)]
    pub follow_up: Option<TacticalActionSpec>,
    #[serde(default)]
    pub forbidden_action: Option<TacticalActionSpec>,
    #[serde(default)]
    pub winner_after_sequence: Option<u8>,
    #[serde(default)]
    pub longest_road_holder_after_action: Option<u8>,
    #[serde(default)]
    pub largest_army_holder_after_action: Option<u8>,
    #[serde(default)]
    pub largest_army_must_not_be_actor: bool,
    #[serde(default)]
    pub robber_moved_from: Option<u8>,
    #[serde(default)]
    pub bypass_cut_vertex: Option<u8>,
    #[serde(default)]
    pub bypass_cut_player: Option<u8>,
    #[serde(default)]
    pub bypass_holder: Option<u8>,
    #[serde(default)]
    pub development_vp_chance: Option<f32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TacticalProposalProbe {
    pub player: u8,
    pub resources: ResourceHand,
    pub action: TacticalActionSpec,
    #[serde(default)]
    pub match_kind: bool,
    #[serde(default)]
    pub match_consequence: bool,
    #[serde(default)]
    pub min_proposal_rate: Option<f32>,
    #[serde(default)]
    pub max_proposal_rate: Option<f32>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threat_probe: Option<TacticalThreatProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub observation_safety_probe: Option<TacticalObservationSafetyProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posterior_sensitivity_probe: Option<TacticalPosteriorSensitivityProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub closeout_probe: Option<TacticalCloseoutProbe>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mechanical: Option<TacticalMechanicalExpectations>,
    #[serde(default = "default_true")]
    pub enforce_explicit_root_ordering: bool,
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

const DEVELOPMENT_TOTALS: [u8; 5] = [14, 5, 2, 2, 2];

fn phase_from_spec(spec: &TacticalStateSpec) -> Result<Phase, String> {
    match spec.phase.as_deref().unwrap_or("main") {
        "main" => Ok(Phase::Main),
        "pre-roll" => Ok(Phase::PreRoll),
        other => Err(format!("unsupported tactical phase: {other}")),
    }
}

fn configure_development(state: &mut GameState, spec: &TacticalStateSpec) -> Result<(), String> {
    for player in &mut state.players {
        player.development = [0; 5];
        player.bought_development = [0; 5];
        player.played_knights = 0;
        player.played_development_this_turn = false;
    }
    for (player, hand) in spec.development.iter().copied().enumerate() {
        if player < state.players.len() {
            state.players[player].development = hand;
        }
    }
    for (player, hand) in spec.bought_development.iter().copied().enumerate() {
        if player < state.players.len() {
            state.players[player].bought_development = hand;
        }
    }
    for (player, count) in spec.played_knights.iter().copied().enumerate() {
        if player < state.players.len() {
            state.players[player].played_knights = count;
        }
    }
    for (player, played) in spec
        .played_development_this_turn
        .iter()
        .copied()
        .enumerate()
    {
        if player < state.players.len() {
            state.players[player].played_development_this_turn = played;
        }
    }

    let held = |card: usize| {
        state
            .players
            .iter()
            .map(|player| player.development[card] as u16)
            .sum::<u16>()
    };
    let played_knights = state
        .players
        .iter()
        .map(|player| player.played_knights as u16)
        .sum::<u16>();
    state.played_development = [0; 5];
    state.played_development[DevCard::Knight.index()] = played_knights
        .try_into()
        .map_err(|_| "played Knight count overflow".to_string())?;

    if let Some(deck) = spec.development_deck {
        state.development_deck = deck;
        for card in 0..5 {
            let accounted = deck[card] as u16 + held(card);
            state.played_development[card] = (DEVELOPMENT_TOTALS[card] as u16)
                .checked_sub(accounted)
                .ok_or_else(|| format!("development card {card} exceeds supply"))?
                as u8;
        }
        if (state.played_development[DevCard::Knight.index()] as u16) < played_knights {
            return Err("played Knights exceed consumed Knight cards".into());
        }
    } else {
        for card in 0..5 {
            let accounted = held(card) + state.played_development[card] as u16;
            state.development_deck[card] = (DEVELOPMENT_TOTALS[card] as u16)
                .checked_sub(accounted)
                .ok_or_else(|| format!("development card {card} exceeds supply"))?
                as u8;
        }
    }
    Ok(())
}

fn refresh_longest_road_via_public_transition(
    state: &mut GameState,
    roads: &[RoadEdgeSpec],
) -> Result<(), String> {
    if roads.is_empty()
        || (0..state.board.num_players)
            .map(|player| state.longest_road_length(player))
            .max()
            .unwrap_or(0)
            < 5
    {
        state.longest_road_holder = None;
        for player in &mut state.players {
            player.has_longest_road = false;
        }
        return Ok(());
    }

    for road in roads.iter().rev() {
        if state.roads[road.edge as usize] != Some(road.player) {
            continue;
        }
        let mut probe = state.clone();
        probe.roads[road.edge as usize] = None;
        probe.players[road.player as usize].roads_left = probe.players[road.player as usize]
            .roads_left
            .saturating_add(1);
        probe.current_player = road.player;
        probe.phase = Phase::Main;
        probe.players[road.player as usize].resources = ROAD_COST;
        rebalance_bank_from_hands(&mut probe)?;
        let action = Action::BuildRoad { edge: road.edge };
        if !probe.legal_actions().contains(&action) || probe.apply(&action).is_err() {
            continue;
        }

        state.longest_road_holder = probe.longest_road_holder;
        for (target, source) in state.players.iter_mut().zip(&probe.players) {
            target.has_longest_road = source.has_longest_road;
            target.public_victory_points = source.public_victory_points;
        }
        return Ok(());
    }

    Err("unable to refresh Longest Road through a legal public road transition".into())
}

/// Constructs and validates a resource-conserving GameState from a TacticalStateSpec.
pub fn build_state(spec: &TacticalStateSpec) -> Result<GameState, String> {
    let mut state = GameState::standard(spec.board_seed, spec.players);
    state.current_player = spec.active_player;
    state.phase = phase_from_spec(spec)?;
    state.victory_target = spec.victory_target;
    state.bank_is_public = spec.bank_is_public;
    state.domestic_trade_used = false;
    state.domestic_trade_count = 0;
    state.domestic_trade_disabled = spec.domestic_trade_disabled;
    state.trade = None;
    state.last_rejected_trade = None;
    state.trade_negotiation_round = 0;
    if let Some(robber_hex) = spec.robber_hex {
        state.robber_hex = robber_hex;
    }

    for p in 0..spec.players as usize {
        state.players[p].resources = [0; 5];
    }
    for (p, hand) in spec.resources.iter().copied().enumerate() {
        if p < state.players.len() {
            state.players[p].resources = hand;
        }
    }
    configure_development(&mut state, spec)?;

    for road in &spec.roads {
        state.roads[road.edge as usize] = Some(road.player);
        state.players[road.player as usize].roads_left =
            state.players[road.player as usize].roads_left.saturating_sub(1);
    }

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

    refresh_longest_road_via_public_transition(&mut state, &spec.roads)?;
    if let Some(holder) = spec.largest_army_holder {
        if holder >= spec.players || state.players[holder as usize].played_knights < 3 {
            return Err("Largest Army holder must have at least three played Knights".into());
        }
        state.largest_army_holder = Some(holder);
        state.players[holder as usize].has_largest_army = true;
        state.players[holder as usize].public_victory_points = state.players[holder as usize]
            .public_victory_points
            .saturating_add(2);
    }
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

/// Applies one hidden-state benchmark variant while preserving resource and development supply.
pub fn apply_hidden_variant(
    base: &GameState,
    variant: &TacticalHiddenVariant,
) -> Result<GameState, String> {
    let mut state = base.clone();
    if let Some(resources) = &variant.resources {
        for (player, hand) in resources.iter().copied().enumerate() {
            if player < state.players.len() {
                state.players[player].resources = hand;
            }
        }
        rebalance_bank_from_hands(&mut state)?;
    }
    if let Some(development) = &variant.development {
        for (player, hand) in development.iter().copied().enumerate() {
            if player < state.players.len() {
                state.players[player].development = hand;
                state.players[player].bought_development = [0; 5];
            }
        }
        for card in 0..5 {
            let held = state
                .players
                .iter()
                .map(|player| player.development[card] as u16)
                .sum::<u16>();
            state.development_deck[card] = (DEVELOPMENT_TOTALS[card] as u16)
                .checked_sub(held + state.played_development[card] as u16)
                .ok_or_else(|| format!("variant development card {card} exceeds supply"))?
                as u8;
        }
    }
    state
        .validate()
        .map_err(|error| format!("invalid hidden variant {}: {error}", variant.id))?;
    Ok(state)
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

    if let Some(expect) = &scenario.mechanical {
        if let Some(forbidden) = &expect.forbidden_action {
            let forbidden = forbidden.to_action();
            if base.legal_actions().contains(&forbidden) {
                return Err(format!("forbidden action is unexpectedly legal: {forbidden:?}"));
            }
        }

        let action = expect
            .action
            .as_ref()
            .map(TacticalActionSpec::to_action)
            .unwrap_or_else(|| best_action.clone());
        if !base.legal_actions().contains(&action) {
            return Err(format!("expected action is not legal: {action:?}"));
        }
        let mut next = base.clone();
        next.apply(&action)
            .map_err(|error| format!("expected action {action:?} failed: {error:?}"))?;

        if let Some(holder) = expect.longest_road_holder_after_action
            && next.longest_road_holder != Some(holder)
        {
            return Err(format!(
                "Longest Road holder after action was {:?}, expected {holder}",
                next.longest_road_holder
            ));
        }
        if let Some(holder) = expect.largest_army_holder_after_action
            && next.largest_army_holder != Some(holder)
        {
            return Err(format!(
                "Largest Army holder after action was {:?}, expected {holder}",
                next.largest_army_holder
            ));
        }
        if expect.largest_army_must_not_be_actor
            && next.largest_army_holder == Some(scenario.state.active_player)
        {
            return Err("actor unexpectedly acquired Largest Army".into());
        }
        if let Some(previous_robber) = expect.robber_moved_from
            && next.robber_hex == previous_robber
        {
            return Err(format!("robber did not move from hex {previous_robber}"));
        }
        if let Some(expected) = expect.development_vp_chance {
            if next.phase != Phase::DevelopmentChance {
                return Err("development VP chance requires BuyDevelopment to enter chance phase".into());
            }
            let actions = next.legal_actions();
            let total = actions
                .iter()
                .map(|action| next.chance_weight(action) as u32)
                .sum::<u32>();
            let vp = actions
                .iter()
                .filter(|action| {
                    matches!(action, Action::ResolveDevelopment { card: DevCard::VictoryPoint })
                })
                .map(|action| next.chance_weight(action) as u32)
                .sum::<u32>();
            let actual = if total == 0 {
                0.0
            } else {
                vp as f32 / total as f32
            };
            if (actual - expected).abs() > 1e-6 {
                return Err(format!(
                    "development VP chance was {actual:.6}, expected {expected:.6}"
                ));
            }
        }

        if let Some(follow_up) = &expect.follow_up {
            let follow_up = follow_up.to_action();
            if !next.legal_actions().contains(&follow_up) {
                return Err(format!("follow-up is not legal after action: {follow_up:?}"));
            }
            next.apply(&follow_up)
                .map_err(|error| format!("follow-up {follow_up:?} failed: {error:?}"))?;
        }
        if let Some(winner) = expect.winner_after_sequence
            && next.winner() != Some(winner)
        {
            return Err(format!(
                "winner after declared sequence was {:?}, expected {winner}",
                next.winner()
            ));
        }

        if let (Some(vertex), Some(cut_player), Some(holder)) = (
            expect.bypass_cut_vertex,
            expect.bypass_cut_player,
            expect.bypass_holder,
        ) {
            let mut cut = next.clone();
            cut.current_player = cut_player;
            cut.phase = Phase::Main;
            cut.players[cut_player as usize].resources = SETTLEMENT_COST;
            rebalance_bank_from_hands(&mut cut)?;
            let cut_action = Action::BuildSettlement { vertex };
            if !cut.legal_actions().contains(&cut_action) {
                return Err(format!("declared bypass cut is not legal: {cut_action:?}"));
            }
            cut.apply(&cut_action)
                .map_err(|error| format!("declared bypass cut failed: {error:?}"))?;
            if cut.longest_road_holder != Some(holder) {
                return Err(format!(
                    "bypass cut changed holder to {:?}, expected {holder}",
                    cut.longest_road_holder
                ));
            }
        }
        return Ok(());
    }

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
        assert_eq!(corpus.scenarios.len(), 25);
        for scenario in &corpus.scenarios {
            let res = verify_mechanical_consequence(scenario);
            assert!(res.is_ok(), "scenario {} failed G0: {:?}", scenario.id, res.err());
        }
    }
}
