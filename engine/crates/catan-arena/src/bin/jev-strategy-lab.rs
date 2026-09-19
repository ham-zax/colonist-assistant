use std::collections::BTreeMap;
use std::env;
use std::fs::{self, File};
use std::io::{BufWriter, Write};

use colonist_catan_arena::belief_particles;
use colonist_catan_core::{
    Action, Board, Building, CITY_COST, DEVELOPMENT_COST, DevCard, Edge, GameState, Hex, NodeKind,
    Phase, PlayerState, Port, ROAD_COST, Resource, ResourceHand, SETTLEMENT_COST, SplitMix64,
    Vertex,
};
use colonist_catan_search::{
    BeliefDepthConfig, BeliefParticle, StrategyPolicy, evaluate, expansion_option_value,
    expected_discard_loss, marginal_development_value, production_pips,
    search_weighted_belief_maxn_with_config, strategic_utility,
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct Config {
    players: u8,
    games: u32,
    seed: u64,
    output: String,
    player_trades_enabled: bool,
    max_turns: u16,
    search_depth: u8,
    ordinary_nodes: u32,
    chance_seed: Option<u64>,
    continuation_seed: Option<u64>,
    force_decision_index: Option<u32>,
    force_settlement_vertex: Option<u8>,
    force_action: Option<String>,
    replay_prefix: Option<String>,
    replay_game: u32,
    state_spec: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: 3,
            games: 2,
            seed: 9_190_001,
            output: "benchmark-results/jev-lab/decisions.jsonl".to_string(),
            player_trades_enabled: false,
            max_turns: 220,
            search_depth: 3,
            ordinary_nodes: 48_000,
            chance_seed: None,
            continuation_seed: None,
            force_decision_index: None,
            force_settlement_vertex: None,
            force_action: None,
            replay_prefix: None,
            replay_game: 0,
            state_spec: None,
        }
    }
}

fn parse_config() -> Config {
    let mut config = Config::default();
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut i = 0usize;
    while i < args.len() {
        let value = args.get(i + 1).map(String::as_str);
        match args[i].as_str() {
            "--players" => config.players = value.and_then(|v| v.parse().ok()).unwrap_or(3),
            "--games" => config.games = value.and_then(|v| v.parse().ok()).unwrap_or(2),
            "--seed" => config.seed = value.and_then(|v| v.parse().ok()).unwrap_or(9_190_001),
            "--output" => {
                config.output = value.unwrap_or(&config.output).to_string();
            }
            "--max-turns" => {
                config.max_turns = value.and_then(|v| v.parse().ok()).unwrap_or(220);
            }
            "--depth" => {
                config.search_depth = value.and_then(|v| v.parse().ok()).unwrap_or(3);
            }
            "--ordinary-nodes" => {
                config.ordinary_nodes = value.and_then(|v| v.parse().ok()).unwrap_or(48_000);
            }
            "--chance-seed" => {
                config.chance_seed = value.and_then(|v| v.parse().ok());
            }
            "--continuation-seed" => {
                config.continuation_seed = value.and_then(|v| v.parse().ok());
            }
            "--force-decision-index" => {
                config.force_decision_index = value.and_then(|v| v.parse().ok());
            }
            "--force-settlement-vertex" => {
                config.force_settlement_vertex = value.and_then(|v| v.parse().ok());
            }
            "--force-action" => {
                config.force_action = value.map(str::to_owned);
            }
            "--replay-prefix" => {
                config.replay_prefix = value.map(str::to_owned);
            }
            "--replay-game" => {
                config.replay_game = value.and_then(|v| v.parse().ok()).unwrap_or(0);
            }
            "--state-spec" => {
                config.state_spec = value.map(str::to_owned);
            }
            "--player-trades" => {
                config.player_trades_enabled = true;
                i += 1;
                continue;
            }
            "--no-player-trades" => {
                config.player_trades_enabled = false;
                i += 1;
                continue;
            }
            "--help" | "-h" => {
                println!(
                    "jev-strategy-lab [--players 2|3|4] [--games N] [--seed N] \
                     [--output PATH] [--max-turns N] [--depth N] [--ordinary-nodes N] \
                     [--chance-seed N] [--continuation-seed N] \
                     [--force-decision-index N (--force-settlement-vertex V|--force-action SPEC)] \
                     [--replay-prefix JSONL --replay-game N] [--state-spec JSON] \
                     [--player-trades|--no-player-trades]"
                );
                std::process::exit(0);
            }
            unknown => {
                eprintln!("unknown argument: {unknown}");
                std::process::exit(2);
            }
        }
        i += 2;
    }
    assert!((2..=4).contains(&config.players));
    assert!(config.games > 0);
    if config.state_spec.is_some() {
        assert_eq!(
            config.games, 1,
            "--state-spec currently supports exactly one game"
        );
    }
    let forced_spec_count = usize::from(config.force_settlement_vertex.is_some())
        + usize::from(config.force_action.is_some());
    assert!(
        forced_spec_count <= 1,
        "use only one of --force-settlement-vertex or --force-action"
    );
    assert_eq!(
        config.force_decision_index.is_some(),
        forced_spec_count == 1,
        "--force-decision-index must be paired with exactly one forced action specification"
    );
    config
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateSpec {
    num_players: u8,
    victory_target: u8,
    setup_step: u8,
    current_player: u8,
    player_trades_enabled: bool,
    domestic_trade_disabled_mask: u8,
    bank: [u8; 5],
    hexes: Vec<StateHexSpec>,
    vertices: Vec<StateVertexSpec>,
    edges: Vec<StateEdgeSpec>,
    buildings: Vec<StateBuildingSpec>,
    roads: Vec<StateRoadSpec>,
    players: Vec<StatePlayerSpec>,
}

#[derive(Deserialize)]
struct StateHexSpec {
    resource: Option<String>,
    number: u8,
    coord: [i8; 2],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateVertexSpec {
    adjacent_hexes: Vec<u8>,
    adjacent_vertices: Vec<u8>,
    adjacent_edges: Vec<u8>,
    port: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateEdgeSpec {
    vertices: [u8; 2],
    adjacent_hexes: Vec<u8>,
}

#[derive(Deserialize)]
struct StateBuildingSpec {
    vertex: u8,
    player: u8,
    kind: String,
}

#[derive(Deserialize)]
struct StateRoadSpec {
    edge: u8,
    player: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StatePlayerSpec {
    resources: [u8; 5],
    public_victory_points: u8,
    roads_built: u8,
    settlements_built: u8,
    cities_built: u8,
}

fn parse_resource(value: &str) -> Resource {
    match value.to_ascii_lowercase().as_str() {
        "lumber" => Resource::Lumber,
        "brick" => Resource::Brick,
        "wool" => Resource::Wool,
        "grain" => Resource::Grain,
        "ore" => Resource::Ore,
        other => panic!("unknown forced-action resource {other}"),
    }
}

fn parse_port(value: Option<&str>) -> Option<Port> {
    match value {
        None => None,
        Some("generic") => Some(Port::Generic),
        Some(value) => Some(Port::Resource(parse_resource(value))),
    }
}

fn load_state_spec(path: &str) -> GameState {
    let spec: StateSpec = serde_json::from_str(
        &fs::read_to_string(path).unwrap_or_else(|error| panic!("failed to read {path}: {error}")),
    )
    .unwrap_or_else(|error| panic!("failed to parse state spec {path}: {error}"));

    let board = Board {
        num_players: spec.num_players,
        hexes: spec
            .hexes
            .iter()
            .map(|hex| Hex {
                resource: hex.resource.as_deref().map(parse_resource),
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
                port: parse_port(vertex.port.as_deref()),
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
            other => panic!("unknown state-spec building kind {other}"),
        });
    }
    for road in &spec.roads {
        state.roads[road.edge as usize] = Some(road.player);
    }
    state
        .validate()
        .unwrap_or_else(|error| panic!("state spec {path} is invalid: {error}"));
    state
}

fn parse_forced_action(spec: &str) -> Action {
    if spec == "end-turn" {
        return Action::EndTurn;
    }
    if spec == "buy-development" {
        return Action::BuyDevelopment;
    }
    if let Some(edge) = spec.strip_prefix("road:") {
        return Action::BuildRoad {
            edge: edge.parse().expect("forced road edge must be an integer"),
        };
    }
    if let Some(vertex) = spec.strip_prefix("settlement:") {
        return Action::BuildSettlement {
            vertex: vertex
                .parse()
                .expect("forced settlement vertex must be an integer"),
        };
    }
    if let Some(vertex) = spec.strip_prefix("city:") {
        return Action::BuildCity {
            vertex: vertex
                .parse()
                .expect("forced city vertex must be an integer"),
        };
    }
    if let Some(rest) = spec.strip_prefix("maritime:") {
        let mut parts = rest.split(':');
        let give = parse_resource(parts.next().expect("forced maritime give resource missing"));
        let receive = parse_resource(
            parts
                .next()
                .expect("forced maritime receive resource missing"),
        );
        let ratio = parts
            .next()
            .expect("forced maritime ratio missing")
            .parse()
            .expect("forced maritime ratio must be an integer");
        assert!(
            parts.next().is_none(),
            "forced maritime action has too many fields"
        );
        return Action::MaritimeTrade {
            give,
            receive,
            ratio,
        };
    }
    panic!("unsupported --force-action {spec}");
}

fn parse_u8_field(value: &str, prefix: &str) -> u8 {
    value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(" }"))
        .expect("forced replay action shape changed")
        .parse()
        .expect("forced replay action index must be an integer")
}

fn parse_recorded_action(value: &str) -> Action {
    if value == "EndTurn" {
        return Action::EndTurn;
    }
    if value == "BuyDevelopment" {
        return Action::BuyDevelopment;
    }
    if value.starts_with("PlaceSettlement { vertex: ") {
        return Action::PlaceSettlement {
            vertex: parse_u8_field(value, "PlaceSettlement { vertex: "),
        };
    }
    if value.starts_with("PlaceRoad { edge: ") {
        return Action::PlaceRoad {
            edge: parse_u8_field(value, "PlaceRoad { edge: "),
        };
    }
    if value.starts_with("BuildRoad { edge: ") {
        return Action::BuildRoad {
            edge: parse_u8_field(value, "BuildRoad { edge: "),
        };
    }
    if value.starts_with("BuildSettlement { vertex: ") {
        return Action::BuildSettlement {
            vertex: parse_u8_field(value, "BuildSettlement { vertex: "),
        };
    }
    if value.starts_with("BuildCity { vertex: ") {
        return Action::BuildCity {
            vertex: parse_u8_field(value, "BuildCity { vertex: "),
        };
    }
    if let Some(rest) = value
        .strip_prefix("MaritimeTrade { give: ")
        .and_then(|value| value.strip_suffix(" }"))
    {
        let mut fields = rest.split(", ");
        let give = parse_resource(fields.next().expect("recorded maritime give missing"));
        let receive = parse_resource(
            fields
                .next()
                .and_then(|field| field.strip_prefix("receive: "))
                .expect("recorded maritime receive missing"),
        );
        let ratio = fields
            .next()
            .and_then(|field| field.strip_prefix("ratio: "))
            .expect("recorded maritime ratio missing")
            .parse()
            .expect("recorded maritime ratio must be an integer");
        assert!(
            fields.next().is_none(),
            "recorded maritime action has extra fields"
        );
        return Action::MaritimeTrade {
            give,
            receive,
            ratio,
        };
    }
    if let Some(rest) = value
        .strip_prefix("MoveRobber { hex: ")
        .and_then(|value| value.strip_suffix(" }"))
    {
        let (hex, victim) = rest
            .split_once(", victim: ")
            .expect("recorded robber action missing victim");
        let victim = if victim == "None" {
            None
        } else {
            Some(
                victim
                    .strip_prefix("Some(")
                    .and_then(|value| value.strip_suffix(')'))
                    .expect("recorded robber victim shape changed")
                    .parse()
                    .expect("recorded robber victim must be an integer"),
            )
        };
        return Action::MoveRobber {
            hex: hex.parse().expect("recorded robber hex must be an integer"),
            victim,
        };
    }
    if let Some(cards) = value
        .strip_prefix("Discard { cards: [")
        .and_then(|value| value.strip_suffix("] }"))
    {
        let parsed = cards
            .split(", ")
            .map(|card| {
                card.parse::<u8>()
                    .expect("recorded discard card must be an integer")
            })
            .collect::<Vec<_>>();
        assert_eq!(
            parsed.len(),
            5,
            "recorded discard must contain five resources"
        );
        return Action::Discard {
            cards: [parsed[0], parsed[1], parsed[2], parsed[3], parsed[4]],
        };
    }
    panic!("unsupported recorded action {value}");
}

fn load_replay_prefix(
    path: Option<&str>,
    game: u32,
    before_decision: Option<u32>,
) -> BTreeMap<u32, Action> {
    let Some(path) = path else {
        return BTreeMap::new();
    };
    let text = fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read replay prefix {path}: {error}"));
    text.lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|record| record.get("recordType").and_then(Value::as_str) == Some("decision"))
        .filter(|record| record.get("game").and_then(Value::as_u64) == Some(u64::from(game)))
        .filter_map(|record| {
            let index = u32::try_from(record.get("decisionIndex")?.as_u64()?).ok()?;
            if before_decision.is_some_and(|limit| index >= limit) {
                return None;
            }
            let action = parse_recorded_action(record.get("chosenAction")?.as_str()?);
            Some((index, action))
        })
        .collect()
}

fn resource_name(resource: Resource) -> &'static str {
    match resource {
        Resource::Lumber => "lumber",
        Resource::Brick => "brick",
        Resource::Wool => "wool",
        Resource::Grain => "grain",
        Resource::Ore => "ore",
    }
}

fn port_json(port: Option<Port>) -> Value {
    match port {
        None => Value::Null,
        Some(Port::Generic) => json!("3:1"),
        Some(Port::Resource(resource)) => json!(format!("2:1-{}", resource_name(resource))),
    }
}

fn building_json(building: Option<Building>) -> Value {
    match building {
        None => Value::Null,
        Some(Building::Settlement(player)) => {
            json!({"kind": "settlement", "player": player})
        }
        Some(Building::City(player)) => json!({"kind": "city", "player": player}),
    }
}

fn hand_json(hand: ResourceHand) -> Value {
    json!({
        "lumber": hand[Resource::Lumber.index()],
        "brick": hand[Resource::Brick.index()],
        "wool": hand[Resource::Wool.index()],
        "grain": hand[Resource::Grain.index()],
        "ore": hand[Resource::Ore.index()],
    })
}

fn can_pay(hand: &ResourceHand, cost: &ResourceHand) -> bool {
    hand.iter()
        .zip(cost.iter())
        .all(|(have, need)| have >= need)
}

fn add_costs(left: &ResourceHand, right: &ResourceHand) -> ResourceHand {
    let mut result = [0u8; 5];
    for i in 0..5 {
        result[i] = left[i].saturating_add(right[i]);
    }
    result
}

fn affordability_json(particles: &[BeliefParticle], player: u8) -> Value {
    let road_settlement = add_costs(&ROAD_COST, &SETTLEMENT_COST);
    let two_roads = add_costs(&ROAD_COST, &ROAD_COST);
    let settlement_dev = add_costs(&SETTLEMENT_COST, &DEVELOPMENT_COST);
    let mut road = 0.0f32;
    let mut settlement = 0.0f32;
    let mut city = 0.0f32;
    let mut development = 0.0f32;
    let mut road_and_settlement = 0.0f32;
    let mut two_roads_mass = 0.0f32;
    let mut settlement_and_dev = 0.0f32;
    let mut total_weight = 0.0f32;

    for particle in particles {
        let hand = &particle.state.players[player as usize].resources;
        let weight = particle.weight;
        total_weight += weight;
        road += weight * f32::from(can_pay(hand, &ROAD_COST));
        settlement += weight * f32::from(can_pay(hand, &SETTLEMENT_COST));
        city += weight * f32::from(can_pay(hand, &CITY_COST));
        development += weight * f32::from(can_pay(hand, &DEVELOPMENT_COST));
        road_and_settlement += weight * f32::from(can_pay(hand, &road_settlement));
        two_roads_mass += weight * f32::from(can_pay(hand, &two_roads));
        settlement_and_dev += weight * f32::from(can_pay(hand, &settlement_dev));
    }

    let normalize = |value: f32| {
        if total_weight > f32::EPSILON {
            value / total_weight
        } else {
            0.0
        }
    };

    json!({
        "canAffordRoad": normalize(road),
        "canAffordSettlement": normalize(settlement),
        "canAffordCity": normalize(city),
        "canAffordDevelopmentCard": normalize(development),
        "canAffordRoadAndSettlement": normalize(road_and_settlement),
        "canAffordTwoRoads": normalize(two_roads_mass),
        "canAffordSettlementAndDevelopmentCard": normalize(settlement_and_dev),
    })
}

fn actor_development_draw_belief_json(state: &GameState, actor: u8) -> Value {
    const STANDARD_TOTALS: [u8; 5] = [14, 5, 2, 2, 2];

    let own = &state.players[actor as usize];
    let possible = std::array::from_fn::<u8, 5, _>(|card| {
        STANDARD_TOTALS[card]
            .saturating_sub(state.played_development[card])
            .saturating_sub(own.development[card])
    });
    let unseen_total = possible.iter().copied().sum::<u8>();
    let probability = |card: DevCard| {
        if unseen_total == 0 {
            0.0
        } else {
            f32::from(possible[card.index()]) / f32::from(unseen_total)
        }
    };

    json!({
        "interpretation": "Information-set-safe exchangeable belief for the next development-card draw. Uses standard card totals, publicly played cards, the actor's own exact cards, and public deck size; never exact opponent-held or hidden-deck identities.",
        "deckCardsRemaining": state.development_deck.iter().copied().sum::<u8>(),
        "unseenPoolCards": unseen_total,
        "probabilities": {
            "knight": probability(DevCard::Knight),
            "victoryPoint": probability(DevCard::VictoryPoint),
            "roadBuilding": probability(DevCard::RoadBuilding),
            "yearOfPlenty": probability(DevCard::YearOfPlenty),
            "monopoly": probability(DevCard::Monopoly),
        },
    })
}

fn public_state_json(state: &GameState, actor: u8, particles: &[BeliefParticle]) -> Value {
    let hexes = state
        .board
        .hexes
        .iter()
        .enumerate()
        .map(|(id, hex)| {
            json!({
                "id": id,
                "resource": hex.resource.map(resource_name),
                "number": hex.number,
                "coord": [hex.coord.0, hex.coord.1],
                "robber": id == state.robber_hex as usize,
            })
        })
        .collect::<Vec<_>>();

    let vertices = state
        .board
        .vertices
        .iter()
        .enumerate()
        .map(|(id, vertex)| {
            json!({
                "id": id,
                "adjacentHexes": vertex.adjacent_hexes,
                "adjacentVertices": vertex.adjacent_vertices,
                "adjacentEdges": vertex.adjacent_edges,
                "port": port_json(vertex.port),
                "building": building_json(state.buildings[id]),
            })
        })
        .collect::<Vec<_>>();

    let edges = state
        .board
        .edges
        .iter()
        .enumerate()
        .map(|(id, edge)| {
            json!({
                "id": id,
                "vertices": edge.vertices,
                "adjacentHexes": edge.adjacent_hexes,
                "roadOwner": state.roads[id],
            })
        })
        .collect::<Vec<_>>();

    let players = (0..state.board.num_players)
        .map(|player| {
            let p = state.players[player as usize];
            let production = production_pips(state, player);
            let expansion = expansion_option_value(state, player);
            let own = player == actor;
            json!({
                "player": player,
                "isActor": own,
                "publicVictoryPoints": p.public_victory_points,
                "resourceCards": p.resource_total(),
                "resourcesIfActor": if own { hand_json(p.resources) } else { Value::Null },
                "developmentCards": p.development.iter().copied().sum::<u8>(),
                "developmentTypesIfActor": if own { json!(p.development) } else { Value::Null },
                "playedKnights": p.played_knights,
                "roadsBuilt": 15u8.saturating_sub(p.roads_left),
                "settlementsBuilt": 5u8.saturating_sub(p.settlements_left),
                "citiesBuilt": 4u8.saturating_sub(p.cities_left),
                "policyProfile": p.policy_profile,
                "productionPips": {
                    "lumber": production[Resource::Lumber.index()],
                    "brick": production[Resource::Brick.index()],
                    "wool": production[Resource::Wool.index()],
                    "grain": production[Resource::Grain.index()],
                    "ore": production[Resource::Ore.index()],
                },
                "expansion": {
                    "bestVertex": expansion.vertex,
                    "roadsRequired": expansion.roads_required,
                    "bestValue": expansion.value,
                    "survivalProbability": expansion.survival_probability,
                    "portfolioValue": expansion.portfolio_value,
                    "optionCount": expansion.option_count,
                },
                "hiddenResourceBeliefIfOpponent": if own {
                    Value::Null
                } else {
                    affordability_json(particles, player)
                },
            })
        })
        .collect::<Vec<_>>();

    json!({
        "turn": state.turn,
        "phase": format!("{:?}", state.phase),
        "actor": actor,
        "currentPlayer": state.current_player,
        "lastRoll": state.last_roll,
        "victoryTarget": state.victory_target,
        "playerTradesEnabled": state.player_trades_enabled,
        "domesticTradeDisabledMask": state.domestic_trade_disabled,
        "longestRoadHolder": state.longest_road_holder,
        "largestArmyHolder": state.largest_army_holder,
        "bankIfPublic": if state.bank_is_public { hand_json(state.bank) } else { Value::Null },
        "developmentDrawBelief": actor_development_draw_belief_json(state, actor),
        "players": players,
        "hexes": hexes,
        "vertices": vertices,
        "edges": edges,
    })
}

fn immediate_action_diagnostics(state: &GameState, action: &Action, actor: u8) -> Value {
    let before = strategic_utility(state, actor);
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return Value::Null;
    }

    let root_resources = next.players[actor as usize].resources;
    let mut resource_only = state.clone();
    resource_only.players[actor as usize].resources = root_resources;
    let resource_only_utility = strategic_utility(&resource_only, actor);
    let root_discard_loss = expected_discard_loss(&next, actor);
    let root_marginal_development = marginal_development_value(&next, actor);
    if next.node_kind() != NodeKind::Chance {
        let after = strategic_utility(&next, actor);
        return json!({
            "strategicUtilityBefore": before,
            "strategicUtilityAfterExpected": after,
            "strategicUtilityDelta": after - before,
            "resourcesAfterRoot": hand_json(root_resources),
            "resourceOnlyStrategicUtility": resource_only_utility,
            "resourceOnlyDelta": resource_only_utility - before,
            "expectedDiscardLossAfterRoot": root_discard_loss,
            "marginalDevelopmentValueAfterRoot": root_marginal_development,
            "chanceOutcomes": [],
        });
    }

    let chance_actions = next.legal_actions();
    let total_development = next.development_deck.iter().copied().sum::<u8>().max(1) as f32;
    let mut expected_after = 0.0;
    let outcomes = chance_actions
        .iter()
        .filter_map(|chance| {
            let weight = match chance {
                Action::ResolveDevelopment { card } => {
                    next.development_deck[card.index()] as f32 / total_development
                }
                _ => 1.0 / chance_actions.len().max(1) as f32,
            };
            let mut resolved = next.clone();
            resolved.apply(chance).ok()?;
            let utility = strategic_utility(&resolved, actor);
            expected_after += utility * weight;
            Some(json!({
                "action": format!("{chance:?}"),
                "weight": weight,
                "strategicUtility": utility,
                "deltaFromBefore": utility - before,
                "resources": hand_json(resolved.players[actor as usize].resources),
                "expectedDiscardLoss": expected_discard_loss(&resolved, actor),
                "marginalDevelopmentValue": marginal_development_value(&resolved, actor),
            }))
        })
        .collect::<Vec<_>>();
    json!({
        "strategicUtilityBefore": before,
        "strategicUtilityAfterExpected": expected_after,
        "strategicUtilityDelta": expected_after - before,
        "resourcesAfterRoot": hand_json(root_resources),
        "resourceOnlyStrategicUtility": resource_only_utility,
        "resourceOnlyDelta": resource_only_utility - before,
        "expectedDiscardLossAfterRoot": root_discard_loss,
        "marginalDevelopmentValueAfterRoot": root_marginal_development,
        "chanceOutcomes": outcomes,
    })
}

fn road_intent_json(intent: Option<colonist_catan_search::RoadIntent>) -> Value {
    match intent {
        None => Value::Null,
        Some(intent) => json!({
            "targetVertex": intent.target_vertex,
            "roadsRemaining": intent.roads_remaining,
            "expectedRolls": intent.expected_rolls,
            "survivalProbability": intent.survival_probability,
            "targetValue": intent.target_value,
            "portfolioValue": intent.portfolio_value,
            "frontierGain": intent.frontier_gain,
            "orderingScore": intent.ordering_score(),
        }),
    }
}

fn main() {
    let config = parse_config();
    let replay_prefix = load_replay_prefix(
        config.replay_prefix.as_deref(),
        config.replay_game,
        config.force_decision_index,
    );
    let file = File::create(&config.output)
        .unwrap_or_else(|error| panic!("failed to create {}: {error}", config.output));
    let mut writer = BufWriter::new(file);

    serde_json::to_writer(
        &mut writer,
        &json!({
            "recordType": "labHeader",
            "schemaVersion": 1,
            "players": config.players,
            "games": config.games,
            "baseSeed": config.seed,
            "chanceSeedOverride": config.chance_seed,
            "continuationSeed": config.continuation_seed,
            "forceDecisionIndex": config.force_decision_index,
            "forceSettlementVertex": config.force_settlement_vertex,
            "forceAction": config.force_action,
            "replayPrefix": config.replay_prefix,
            "replayGame": config.replay_game,
            "stateSpec": config.state_spec,
            "playerTradesEnabled": config.player_trades_enabled,
            "search": {
                "beliefParticles": 24,
                "strategicParticles": 12,
                "depth": config.search_depth,
                "branchCap": 12,
                "ordinaryNodes": config.ordinary_nodes,
                "openingNodes": 12_000,
                "openingTimeMs": 1_200,
                "tradeResponseNodes": 2_000,
                "tradeResponseTimeMs": 350,
                "strategyPolicy": "baseline",
            }
        }),
    )
    .expect("header must serialize");
    writer.write_all(b"\n").expect("header newline");

    for game in 0..config.games {
        let board_seed = config.seed.wrapping_add(u64::from(game) * 0x9e37_79b9);
        let chance_seed = config
            .chance_seed
            .map(|seed| seed.wrapping_add(u64::from(game) * 0x1000_0001))
            .unwrap_or(config.seed ^ 0x4348_414e_4345 ^ (u64::from(game) * 0x1000_0001));
        let mut state = config
            .state_spec
            .as_deref()
            .map(load_state_spec)
            .unwrap_or_else(|| GameState::randomized_base_v1(board_seed, config.players));
        state.player_trades_enabled = config.player_trades_enabled;
        let mut chance_rng = SplitMix64::new(chance_seed);
        let continuation_seed = config
            .continuation_seed
            .map(|seed| seed.wrapping_add(u64::from(game) * 0x1000_0001));
        let continuation_base = continuation_seed.unwrap_or(chance_seed);
        let mut roll_rng = SplitMix64::new(continuation_base ^ 0x524f_4c4c_5354_524d);
        let mut development_rng = SplitMix64::new(continuation_base ^ 0x4445_5653_5452_4d31);
        let mut steal_rng = SplitMix64::new(continuation_base ^ 0x5354_4541_4c53_5452);
        let mut decisions = 0u32;
        let mut actions = 0u32;

        while !state.is_terminal() && state.turn <= config.max_turns {
            if state.node_kind() == NodeKind::Chance {
                let use_continuation_streams = continuation_seed.is_some()
                    && config
                        .force_decision_index
                        .is_some_and(|target| decisions > target);
                let action = if use_continuation_streams {
                    let rng = match state.phase {
                        Phase::RollChance => &mut roll_rng,
                        Phase::DevelopmentChance => &mut development_rng,
                        Phase::ResolveSteal { .. } => &mut steal_rng,
                        _ => &mut chance_rng,
                    };
                    state.sample_chance(rng)
                } else {
                    state.sample_chance(&mut chance_rng)
                }
                .expect("chance state must expose an outcome");
                state
                    .apply(&action)
                    .expect("sampled chance action must be legal");
                actions += 1;
                continue;
            }

            let actor = state.actor();
            let legal_actions = state.legal_actions();
            if legal_actions.len() > 1
                && let Some(replayed) = config
                    .force_decision_index
                    .filter(|target| decisions < *target)
                    .and_then(|_| replay_prefix.get(&decisions))
                    .cloned()
            {
                assert!(
                    legal_actions.contains(&replayed),
                    "replayed action {replayed:?} is not legal at decision {decisions}"
                );
                state.apply(&replayed).unwrap_or_else(|error| {
                    panic!("replayed action {replayed:?} failed at decision {decisions}: {error}")
                });
                decisions += 1;
                actions += 1;
                continue;
            }

            let action = if legal_actions.len() == 1 {
                legal_actions[0].clone()
            } else {
                let particles = belief_particles(
                    &state,
                    actor,
                    24,
                    state.observation_hash(actor) ^ config.seed ^ u64::from(decisions),
                );
                let (nodes, time_budget_ms) = if matches!(
                    state.phase,
                    Phase::SetupSettlement | Phase::SetupRoad { .. }
                ) {
                    (12_000, 1_200)
                } else if state.phase == Phase::TradeResponses {
                    (2_000, 350)
                } else {
                    (config.ordinary_nodes, 0)
                };
                let report = search_weighted_belief_maxn_with_config(
                    &particles,
                    BeliefDepthConfig {
                        maximum_depth: config.search_depth,
                        branch_cap: 12,
                        maximum_nodes: nodes,
                        time_budget_ms,
                        strategy_policy: StrategyPolicy::Baseline,
                        strategic_particle_limit: 12,
                    },
                )
                .expect("belief particles must share one observation");
                let engine_chosen = report
                    .chosen
                    .clone()
                    .unwrap_or_else(|| legal_actions[0].clone());
                let forced_action = if config.force_decision_index == Some(decisions) {
                    if let Some(spec) = config.force_action.as_deref() {
                        Some(parse_forced_action(spec))
                    } else {
                        Some(Action::PlaceSettlement {
                            vertex: config
                                .force_settlement_vertex
                                .expect("force vertex paired during config parsing"),
                        })
                    }
                } else {
                    None
                };
                if let Some(forced) = &forced_action {
                    assert!(
                        legal_actions.contains(forced),
                        "forced action {forced:?} is not legal at decision {decisions}"
                    );
                }
                let chosen = forced_action
                    .clone()
                    .unwrap_or_else(|| engine_chosen.clone());

                let candidate_actions = report
                    .actions
                    .iter()
                    .map(|candidate| {
                        let planner = report
                            .provenance
                            .ranked_roots
                            .iter()
                            .find(|diagnostic| diagnostic.action == candidate.action);
                        json!({
                            "action": format!("{:?}", candidate.action),
                            "chosen": candidate.action == chosen,
                            "engineRecommended": candidate.action == engine_chosen,
                            "actorValue": candidate.value[actor as usize],
                            "valueVector": candidate.value,
                            "actorLowerConfidenceValue": candidate.lower_confidence_value[actor as usize],
                            "legalWeight": candidate.legal_weight,
                            "policyPrior": planner.map(|diagnostic| diagnostic.prior),
                            "plannerValue": planner.and_then(|diagnostic| diagnostic.planner_value),
                            "plannerCompletionMass": planner.and_then(|diagnostic| diagnostic.planner_completion_mass),
                            "plannerDecisiveCompletionMass": planner.and_then(|diagnostic| diagnostic.planner_decisive_completion_mass),
                            "plannerResponseWindows": planner.and_then(|diagnostic| diagnostic.planner_response_windows),
                        })
                    })
                    .collect::<Vec<_>>();

                let root_evidence = report
                    .provenance
                    .root_evidence
                    .iter()
                    .map(|evidence| {
                        json!({
                            "action": format!("{:?}", evidence.action),
                            "promotionReason": evidence.promotion_reason.map(|reason| format!("{reason:?}")),
                            "roadIntent": road_intent_json(evidence.road_intent),
                            "introducedRoadFragility": format!("{:?}", evidence.introduced_road_fragility),
                            "roadCutContinuation": format!("{:?}", evidence.road_cut_continuation),
                            "admittedByPromotion": evidence.admitted_by_promotion,
                            "closeoutGain": evidence.closeout_gain,
                            "responseWindows": evidence.response_windows,
                            "decisiveCompletionMass": evidence.decisive_completion_mass,
                            "tradeRiskPosterior": evidence.trade_risk_posterior,
                            "dirtyMonopolyPosterior": evidence.dirty_monopoly_posterior,
                            "tradeHardVetoPosterior": evidence.trade_hard_veto_posterior,
                            "tradeHardVeto": evidence.trade_hard_veto,
                        })
                    })
                    .collect::<Vec<_>>();

                let record = json!({
                    "recordType": "decision",
                    "schemaVersion": 1,
                    "game": game,
                    "boardSeed": board_seed,
                    "chanceSeed": chance_seed,
                    "decisionIndex": decisions,
                    "stateHash": format!("{:016x}", state.observation_hash(actor)),
                    "turn": state.turn,
                    "phase": format!("{:?}", state.phase),
                    "actor": actor,
                    "chosenAction": format!("{:?}", chosen),
                    "engineRecommendedAction": format!("{:?}", engine_chosen),
                    "forcedAction": forced_action.as_ref().map(|action| format!("{action:?}")),
                    "search": {
                        "nodes": report.nodes,
                        "depth": report.depth,
                        "deadlineReached": report.deadline_reached,
                        "posteriorParticles": report.posterior_particles,
                        "searchParticles": report.particles,
                        "rankedRootCount": report.provenance.ranked_root_count,
                        "prunedRootCount": report.provenance.pruned_root_count,
                        "searchWinner": report.provenance.search_winner.as_ref().map(|action| format!("{action:?}")),
                        "safetyReplacement": report.provenance.safety_replacement.as_ref().map(|(from, to)| {
                            json!({"from": format!("{from:?}"), "to": format!("{to:?}")})
                        }),
                    },
                    "candidateActions": candidate_actions,
                    "candidateImmediateDiagnostics": report.actions.iter().map(|candidate| json!({
                        "action": format!("{:?}", candidate.action),
                        "diagnostics": immediate_action_diagnostics(&state, &candidate.action, actor),
                    })).collect::<Vec<_>>(),
                    "rootEvidence": root_evidence,
                    "rootSearchWork": report.provenance.root_search_work.iter().map(|work| json!({
                        "action": format!("{:?}", work.action),
                        "nodes": work.nodes,
                        "completedWaveDepth": work.completed_wave_depth,
                        "cutoffDepthCounts": work.cutoff_depth_counts,
                        "posteriorMassReachingControlledNextDecision": work.posterior_mass_reaching_controlled_next_decision,
                        "posteriorMassReachingTerminal": work.posterior_mass_reaching_terminal,
                    })).collect::<Vec<_>>(),
                    "publicState": public_state_json(&state, actor, &particles),
                });
                serde_json::to_writer(&mut writer, &record).expect("decision must serialize");
                writer.write_all(b"\n").expect("decision newline");
                decisions += 1;
                chosen
            };

            state.apply(&action).unwrap_or_else(|error| {
                panic!("lab selected illegal action {action:?} in game {game}: {error}")
            });
            actions += 1;
        }

        let final_evaluation = evaluate(&state);
        let final_position_metrics = (0..state.board.num_players)
            .map(|player| {
                let production = production_pips(&state, player);
                let expansion = expansion_option_value(&state, player);
                json!({
                    "player": player,
                    "productionPips": {
                        "lumber": production[Resource::Lumber.index()],
                        "brick": production[Resource::Brick.index()],
                        "wool": production[Resource::Wool.index()],
                        "grain": production[Resource::Grain.index()],
                        "ore": production[Resource::Ore.index()],
                    },
                    "totalProductionPips": production.iter().sum::<f32>(),
                    "expansion": {
                        "bestVertex": expansion.vertex,
                        "roadsRequired": expansion.roads_required,
                        "bestValue": expansion.value,
                        "survivalProbability": expansion.survival_probability,
                        "portfolioValue": expansion.portfolio_value,
                        "optionCount": expansion.option_count,
                    },
                })
            })
            .collect::<Vec<_>>();

        serde_json::to_writer(
            &mut writer,
            &json!({
                "recordType": "gameEnd",
                "schemaVersion": 1,
                "game": game,
                "boardSeed": board_seed,
                "chanceSeed": chance_seed,
                "winner": state.winner(),
                "cutoff": !state.is_terminal(),
                "turns": state.turn,
                "actions": actions,
                "decisionsLogged": decisions,
                "publicVictoryPoints": state.players.iter().map(|p| p.public_victory_points).collect::<Vec<_>>(),
                "actualVictoryPoints": state.players.iter().map(|p| p.victory_points()).collect::<Vec<_>>(),
                "evaluation": final_evaluation,
                "positionMetrics": final_position_metrics,
            }),
        )
        .expect("game end must serialize");
        writer.write_all(b"\n").expect("game end newline");
        writer.flush().expect("lab output must flush");
    }
}
