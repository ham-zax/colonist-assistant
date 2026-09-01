//! Narrow structured-clone WebAssembly boundary.
//!
//! JavaScript validates the Colonist DOM and sends one compact snapshot.
//! Search performs no callback into JavaScript and returns one small report.

use std::cell::RefCell;

use colonist_catan_core::{
    Action, Board, Building, Edge, GameState, Phase, PlayerState, Port, Resource, SplitMix64,
    TradeOffer, Vertex,
};
use colonist_catan_search::{
    ActionStats, BeliefParticle, BeliefSearchProvenance, ENGINE_REVISION, ExactActionFamily,
    ExactActionValue, ExactDecisionResult, Mcts, RootPruneReason, SearchConfig, SearchMode,
    SearchReport, SearchStatistics, TacticalResult, choose_rollout_action, evaluate,
    exact_family_for_action, learned_model_version, learned_trade_model_version,
    safer_end_turn_alternative, search_weighted_belief_maxn_bounded_timed_excluding,
    search_weighted_belief_paranoid_bounded_timed_excluding, solve_belief_current_turn,
    solve_exact_belief_excluding,
};
use serde::{Deserialize, Serialize};
use wasm_bindgen::prelude::*;

thread_local! {
    static PERSISTENT_SEARCH: RefCell<Vec<Mcts>> = const { RefCell::new(Vec::new()) };
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HexInput {
    resource: i8,
    number: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VertexInput {
    adjacent_hexes: Vec<u8>,
    adjacent_vertices: Vec<u8>,
    adjacent_edges: Vec<u8>,
    port: i8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EdgeInput {
    vertices: [u8; 2],
    adjacent_hexes: Vec<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct BoardInput {
    hexes: Vec<HexInput>,
    vertices: Vec<VertexInput>,
    edges: Vec<EdgeInput>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerInput {
    resources: [u8; 5],
    development: [u8; 5],
    bought_development: [u8; 5],
    public_victory_points: u8,
    played_knights: u8,
    roads_left: u8,
    settlements_left: u8,
    cities_left: u8,
    has_longest_road: bool,
    has_largest_army: bool,
    played_development_this_turn: bool,
    policy_profile: Option<[u8; 5]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct WorldInput {
    weight: Option<f32>,
    hands: Vec<[u8; 5]>,
    development: Option<Vec<[u8; 5]>>,
    bought_development: Option<Vec<[u8; 5]>>,
    development_deck: Option<[u8; 5]>,
    bank: Option<[u8; 5]>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeInput {
    creator: u8,
    recipients: u8,
    give: [u8; 5],
    receive: [u8; 5],
    accepted: u8,
    rejected: u8,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateInput {
    board: BoardInput,
    players: Vec<PlayerInput>,
    worlds: Vec<WorldInput>,
    buildings: Vec<i8>,
    roads: Vec<i8>,
    bank: [u8; 5],
    bank_visible: Option<bool>,
    development_deck: [u8; 5],
    played_development: [u8; 5],
    robber_hex: u8,
    current_player: u8,
    phase: String,
    phase_parameter: Option<u8>,
    turn: u16,
    last_roll: u8,
    victory_target: u8,
    card_discard_limit: Option<u8>,
    friendly_robber: Option<bool>,
    setup_step: u8,
    discard_remaining: [u8; 4],
    discard_cursor: u8,
    robber_return_phase: String,
    trade: Option<TradeInput>,
    trade_cursor: u8,
    domestic_trade_used: bool,
    player_trades_enabled: Option<bool>,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RejectedTradeInput {
    give: [u8; 5],
    receive: [u8; 5],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RootExclusionInput {
    kind: String,
    give: [u8; 5],
    receive: [u8; 5],
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Request {
    state: StateInput,
    last_rejected_trade: Option<RejectedTradeInput>,
    #[serde(default)]
    root_exclusions: Vec<RootExclusionInput>,
    iterations: Option<u32>,
    max_nodes: Option<usize>,
    rollout_actions: Option<u16>,
    tactical_depth: Option<u8>,
    tactical_nodes: Option<u32>,
    time_budget_ms: Option<u32>,
    seed: Option<u64>,
    mode: Option<String>,
    depth: Option<u8>,
    branch_cap: Option<usize>,
    ponder: Option<bool>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RequestedMode {
    Maxn,
    Weighted,
    Puct,
    Uct,
    AlphaBeta,
}

impl RequestedMode {
    fn parse(value: Option<&str>) -> Result<Self, JsValue> {
        match value.unwrap_or("maxn") {
            "maxn" | "deep" => Ok(Self::Maxn),
            "weighted" => Ok(Self::Weighted),
            // Retain old request names only as an explicit experimental PUCT
            // compatibility path. The packaged live worker never selects it.
            "puct" | "strategist" => Ok(Self::Puct),
            "uct" => Ok(Self::Uct),
            "alpha-beta" => Ok(Self::AlphaBeta),
            other => Err(JsValue::from_str(&format!("unknown search mode: {other}"))),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Maxn => "maxn",
            Self::Weighted => "weighted",
            Self::Puct => "puct",
            Self::Uct => "uct",
            Self::AlphaBeta => "alpha-beta",
        }
    }

    fn mcts_mode(self) -> SearchMode {
        if self == Self::Uct {
            SearchMode::Uct
        } else {
            SearchMode::Puct
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionOutput {
    kind: &'static str,
    first: Option<u8>,
    second: Option<u8>,
    player: Option<u8>,
    resource: Option<u8>,
    other_resource: Option<u8>,
    cards: Option<[u8; 5]>,
    receive_cards: Option<[u8; 5]>,
    accept: Option<bool>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionStatisticsOutput {
    action: ActionOutput,
    visits: u32,
    availability: u32,
    availability_weight: f32,
    legal_weight: f32,
    prior: f32,
    value: [f32; 4],
    lower_confidence_value: [f32; 4],
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
enum DecisionAuthority {
    ExactMandatory,
    TacticalProven,
    DeepMaxn,
    WeightedPolicy,
    ExactFamily,
    SafetyOverride,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ActionReplacementOutput {
    from: ActionOutput,
    to: ActionOutput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RankedRootOutput {
    action: ActionOutput,
    rank: usize,
    prior: f32,
    planner_value: Option<f32>,
    planner_completion_mass: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RetainedRootOutput {
    action: ActionOutput,
    pre_truncation_rank: Option<usize>,
    prior: f32,
    node_budget_per_particle: u32,
    allocated_nodes: u32,
    planner_value: Option<f32>,
    planner_completion_mass: Option<f32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct PrunedRootOutput {
    action: ActionOutput,
    pre_truncation_rank: Option<usize>,
    reason: &'static str,
}

#[derive(Serialize, Default)]
#[serde(rename_all = "camelCase")]
struct RootProvenanceOutput {
    ranked_root_count: usize,
    ranked_roots: Vec<RankedRootOutput>,
    retained_roots: Vec<RetainedRootOutput>,
    pruned_root_count: usize,
    pruned_roots: Vec<PrunedRootOutput>,
    exact_family_replacement: Option<ActionReplacementOutput>,
    safety_replacement: Option<ActionReplacementOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AuthorityTraceOutput {
    initial_authority: DecisionAuthority,
    exact_family: Option<&'static str>,
    exact_family_replacement: Option<ActionReplacementOutput>,
    safety_replacement: Option<ActionReplacementOutput>,
}

struct ResponseDiagnostics {
    rust_posterior_particles: usize,
    rust_search_particles: usize,
    root_provenance: RootProvenanceOutput,
    authority_trace: AuthorityTraceOutput,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Response {
    engine_revision: &'static str,
    authority: DecisionAuthority,
    learned_model_version: &'static str,
    trade_model_version: &'static str,
    algorithm: &'static str,
    chosen: Option<ActionOutput>,
    root_value: [f32; 4],
    tactical_win_probability: f32,
    tactical_lower_bound: f32,
    tactical_proven: bool,
    tactical_line: Vec<ActionOutput>,
    exact_decision: bool,
    exact_worlds: usize,
    actions: Vec<ActionStatisticsOutput>,
    iterations: u32,
    nodes: usize,
    deepest_decision_depth: u16,
    rollouts: u32,
    particles: usize,
    wasm_particles: usize,
    rust_posterior_particles: usize,
    rust_search_particles: usize,
    root_provenance: RootProvenanceOutput,
    authority_trace: AuthorityTraceOutput,
    effective_particle_count: f32,
    deadline_reached: bool,
}

fn resource(value: i8) -> Result<Option<Resource>, JsValue> {
    Ok(match value {
        -1 => None,
        0 => Some(Resource::Lumber),
        1 => Some(Resource::Brick),
        2 => Some(Resource::Wool),
        3 => Some(Resource::Grain),
        4 => Some(Resource::Ore),
        _ => return Err(JsValue::from_str("invalid resource code")),
    })
}

fn port(value: i8) -> Result<Option<Port>, JsValue> {
    Ok(match value {
        -1 => None,
        5 => Some(Port::Generic),
        0..=4 => Some(Port::Resource(
            resource(value)?.expect("resource port code is exact"),
        )),
        _ => return Err(JsValue::from_str("invalid port code")),
    })
}

fn phase(value: &str, parameter: Option<u8>) -> Result<Phase, JsValue> {
    Ok(match value {
        "setup-settlement" => Phase::SetupSettlement,
        "setup-road" => Phase::SetupRoad {
            settlement: parameter.ok_or_else(|| JsValue::from_str("setup road needs anchor"))?,
        },
        "pre-roll" => Phase::PreRoll,
        "roll-chance" => Phase::RollChance,
        "discard" => Phase::Discard,
        "move-robber" => Phase::MoveRobber,
        "resolve-steal" => Phase::ResolveSteal {
            victim: parameter.ok_or_else(|| JsValue::from_str("steal needs victim"))?,
        },
        "main" => Phase::Main,
        "development-chance" => Phase::DevelopmentChance,
        "trade-responses" => Phase::TradeResponses,
        "finished" => Phase::Finished,
        _ => return Err(JsValue::from_str("invalid phase")),
    })
}

fn player(input: PlayerInput) -> PlayerState {
    PlayerState {
        resources: input.resources,
        development: input.development,
        bought_development: input.bought_development,
        public_victory_points: input.public_victory_points,
        played_knights: input.played_knights,
        roads_left: input.roads_left,
        settlements_left: input.settlements_left,
        cities_left: input.cities_left,
        has_longest_road: input.has_longest_road,
        has_largest_army: input.has_largest_army,
        played_development_this_turn: input.played_development_this_turn,
        policy_profile: input.policy_profile.unwrap_or([51; 5]),
    }
}

fn game_states(
    input: StateInput,
    last_rejected_trade: Option<RejectedTradeInput>,
) -> Result<Vec<BeliefParticle>, JsValue> {
    let num_players = input.players.len() as u8;
    let board = Board {
        num_players,
        hexes: input
            .board
            .hexes
            .into_iter()
            .enumerate()
            .map(|(index, hex)| {
                Ok(colonist_catan_core::Hex {
                    resource: resource(hex.resource)?,
                    number: hex.number,
                    coord: (index as i8, 0),
                })
            })
            .collect::<Result<Vec<_>, JsValue>>()?,
        vertices: input
            .board
            .vertices
            .into_iter()
            .map(|vertex| {
                Ok(Vertex {
                    adjacent_hexes: vertex.adjacent_hexes,
                    adjacent_vertices: vertex.adjacent_vertices,
                    adjacent_edges: vertex.adjacent_edges,
                    port: port(vertex.port)?,
                })
            })
            .collect::<Result<Vec<_>, JsValue>>()?,
        edges: input
            .board
            .edges
            .into_iter()
            .map(|edge| Edge {
                vertices: edge.vertices,
                adjacent_hexes: edge.adjacent_hexes,
            })
            .collect(),
    };
    let buildings = input
        .buildings
        .into_iter()
        .map(|piece| {
            if piece < 0 {
                None
            } else {
                let owner = piece as u8 / 2;
                Some(if (piece as u8).is_multiple_of(2) {
                    Building::Settlement(owner)
                } else {
                    Building::City(owner)
                })
            }
        })
        .collect::<Vec<_>>();
    let roads = input
        .roads
        .into_iter()
        .map(|owner| (owner >= 0).then_some(owner as u8))
        .collect::<Vec<_>>();
    let players = input.players.into_iter().map(player).collect::<Vec<_>>();
    let game_phase = phase(&input.phase, input.phase_parameter)?;
    let robber_return_phase = phase(&input.robber_return_phase, None)?;
    let trade = input.trade.map(|trade| TradeOffer {
        creator: trade.creator,
        recipients: trade.recipients,
        give: trade.give,
        receive: trade.receive,
        accepted: trade.accepted,
        rejected: trade.rejected,
    });
    let worlds = if input.worlds.is_empty() {
        vec![WorldInput {
            weight: Some(1.0),
            hands: players.iter().map(|state| state.resources).collect(),
            development: None,
            bought_development: None,
            development_deck: None,
            bank: None,
        }]
    } else {
        input.worlds
    };
    let mut result = Vec::with_capacity(worlds.len());
    for world in worlds {
        if world.hands.len() != players.len()
            || world
                .development
                .as_ref()
                .is_some_and(|cards| cards.len() != players.len())
            || world
                .bought_development
                .as_ref()
                .is_some_and(|cards| cards.len() != players.len())
        {
            return Err(JsValue::from_str("world player count mismatch"));
        }
        let mut world_players = players.clone();
        for (state, hand) in world_players.iter_mut().zip(world.hands) {
            state.resources = hand;
        }
        if let Some(development) = world.development {
            for (state, cards) in world_players.iter_mut().zip(development) {
                state.development = cards;
            }
        }
        if let Some(bought_development) = world.bought_development {
            for (state, cards) in world_players.iter_mut().zip(bought_development) {
                state.bought_development = cards;
            }
        }
        let state = GameState {
            board: board.clone().into(),
            players: world_players,
            buildings: buildings.clone(),
            roads: roads.clone(),
            bank: world.bank.unwrap_or(input.bank),
            bank_is_public: input.bank_visible.unwrap_or(true),
            development_deck: world.development_deck.unwrap_or(input.development_deck),
            played_development: input.played_development,
            robber_hex: input.robber_hex,
            current_player: input.current_player,
            phase: game_phase,
            turn: input.turn,
            last_roll: input.last_roll,
            victory_target: input.victory_target,
            card_discard_limit: input.card_discard_limit.unwrap_or(7),
            friendly_robber: input.friendly_robber.unwrap_or(false),
            setup_step: input.setup_step,
            discard_remaining: input.discard_remaining,
            discard_cursor: input.discard_cursor,
            robber_return_phase,
            free_roads: 0,
            domestic_trade_used: input.domestic_trade_used,
            // The live adapter's boolean means this turn's offer budget is
            // exhausted. Mapping it to one left the simulator's second
            // negotiation round open and made trivial end-turn states
            // expand a full tree of redundant offers.
            domestic_trade_count: if input.domestic_trade_used { 2 } else { 0 },
            player_trades_enabled: input.player_trades_enabled.unwrap_or(true),
            last_rejected_trade: last_rejected_trade.as_ref().map(|rejected| TradeOffer {
                creator: input.current_player,
                recipients: ((1u8 << num_players) - 1) & !(1u8 << input.current_player),
                give: rejected.give,
                receive: rejected.receive,
                accepted: 0,
                rejected: ((1u8 << num_players) - 1) & !(1u8 << input.current_player),
            }),
            trade,
            trade_cursor: input.trade_cursor,
            trade_negotiation_round: 0,
            longest_road_holder: input.longest_road_holder,
            largest_army_holder: input.largest_army_holder,
        };
        state
            .validate()
            .map_err(|error| JsValue::from_str(&format!("invalid search state: {error}")))?;
        result.push(BeliefParticle {
            weight: world.weight.unwrap_or(1.0).max(0.0),
            state,
        });
    }
    Ok(result)
}

fn action(action: Action) -> ActionOutput {
    let mut output = ActionOutput {
        kind: "unknown",
        first: None,
        second: None,
        player: None,
        resource: None,
        other_resource: None,
        cards: None,
        receive_cards: None,
        accept: None,
    };
    match action {
        Action::PlaceSettlement { vertex } => {
            output.kind = "place-settlement";
            output.first = Some(vertex);
        }
        Action::PlaceRoad { edge } => {
            output.kind = "place-road";
            output.first = Some(edge);
        }
        Action::Roll => output.kind = "roll",
        Action::ResolveRoll { value } => {
            output.kind = "resolve-roll";
            output.first = Some(value);
        }
        Action::Discard { cards } => {
            output.kind = "discard";
            output.cards = Some(cards);
        }
        Action::MoveRobber { hex, victim } => {
            output.kind = "move-robber";
            output.first = Some(hex);
            output.player = victim;
        }
        Action::ResolveSteal { victim, resource } => {
            output.kind = "resolve-steal";
            output.player = Some(victim);
            output.resource = Some(resource as u8);
        }
        Action::BuildRoad { edge } => {
            output.kind = "build-road";
            output.first = Some(edge);
        }
        Action::BuildSettlement { vertex } => {
            output.kind = "build-settlement";
            output.first = Some(vertex);
        }
        Action::BuildCity { vertex } => {
            output.kind = "build-city";
            output.first = Some(vertex);
        }
        Action::BuyDevelopment => output.kind = "buy-development",
        Action::ResolveDevelopment { card } => {
            output.kind = "resolve-development";
            output.resource = Some(card as u8);
        }
        Action::PlayKnight { hex, victim } => {
            output.kind = "play-knight";
            output.first = Some(hex);
            output.player = victim;
        }
        Action::PlayRoadBuilding { first, second } => {
            output.kind = "play-road-building";
            output.first = Some(first);
            output.second = second;
        }
        Action::PlayYearOfPlenty { first, second } => {
            output.kind = "play-year-of-plenty";
            output.resource = Some(first as u8);
            output.other_resource = Some(second as u8);
        }
        Action::PlayMonopoly { resource } => {
            output.kind = "play-monopoly";
            output.resource = Some(resource as u8);
        }
        Action::MaritimeTrade {
            give,
            receive,
            ratio,
        } => {
            output.kind = "maritime-trade";
            output.first = Some(ratio);
            output.resource = Some(give as u8);
            output.other_resource = Some(receive as u8);
        }
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } => {
            output.kind = "offer-trade";
            output.first = Some(recipients);
            output.cards = Some(give);
            output.receive_cards = Some(receive);
        }
        Action::RespondTrade { accept } => {
            output.kind = "respond-trade";
            output.accept = Some(accept);
        }
        Action::CounterTrade { give, receive } => {
            output.kind = "counter-trade";
            output.cards = Some(give);
            output.receive_cards = Some(receive);
        }
        Action::ConfirmTrade { partner } => {
            output.kind = "confirm-trade";
            output.player = Some(partner);
        }
        Action::CancelTrade => output.kind = "cancel-trade",
        Action::EndTurn => output.kind = "end-turn",
    }
    output
}

fn root_prune_reason(reason: RootPruneReason) -> &'static str {
    match reason {
        RootPruneReason::RootExcluded => "root-excluded",
        RootPruneReason::BranchTruncated => "branch-truncated",
        RootPruneReason::TradeSafety => "trade-safety",
        RootPruneReason::ExactFamilyCollapsed => "exact-family-collapsed",
    }
}

fn exact_family_label(family: ExactActionFamily) -> &'static str {
    match family {
        ExactActionFamily::Mandatory => "mandatory",
        ExactActionFamily::Knight => "knight",
        ExactActionFamily::Monopoly => "monopoly",
        ExactActionFamily::YearOfPlenty => "year-of-plenty",
        ExactActionFamily::RoadBuilding => "road-building",
    }
}

fn replacement_output(replacement: (Action, Action)) -> ActionReplacementOutput {
    ActionReplacementOutput {
        from: action(replacement.0),
        to: action(replacement.1),
    }
}

fn root_provenance_output(provenance: BeliefSearchProvenance) -> RootProvenanceOutput {
    RootProvenanceOutput {
        ranked_root_count: provenance.ranked_root_count,
        ranked_roots: provenance
            .ranked_roots
            .into_iter()
            .map(|candidate| RankedRootOutput {
                action: action(candidate.action),
                rank: candidate.rank,
                prior: candidate.prior,
                planner_value: candidate.planner_value,
                planner_completion_mass: candidate.planner_completion_mass,
            })
            .collect(),
        retained_roots: provenance
            .retained_roots
            .into_iter()
            .map(|candidate| RetainedRootOutput {
                action: action(candidate.action),
                pre_truncation_rank: candidate.pre_truncation_rank,
                prior: candidate.prior,
                node_budget_per_particle: candidate.node_budget_per_particle,
                allocated_nodes: candidate.allocated_nodes,
                planner_value: candidate.planner_value,
                planner_completion_mass: candidate.planner_completion_mass,
            })
            .collect(),
        pruned_root_count: provenance.pruned_root_count,
        pruned_roots: provenance
            .pruned_roots
            .into_iter()
            .map(|candidate| PrunedRootOutput {
                action: action(candidate.action),
                pre_truncation_rank: candidate.pre_truncation_rank,
                reason: root_prune_reason(candidate.reason),
            })
            .collect(),
        exact_family_replacement: provenance.exact_family_replacement.map(replacement_output),
        safety_replacement: provenance.safety_replacement.map(replacement_output),
    }
}

fn weighted_policy_report(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
    seed: u64,
) -> SearchReport {
    let state = &particles[0].state;
    let actor = state.actor();
    let observed = state.observed_state(actor);
    let actions = observed
        .legal_actions()
        .into_iter()
        .filter(|action| !root_exclusions.contains(action))
        .collect::<Vec<_>>();
    let mut rng = SplitMix64::new(seed);
    let chosen =
        (!actions.is_empty()).then(|| choose_rollout_action(&observed, &actions, &mut rng));
    let root_value = evaluate(&observed);
    let statistics = chosen
        .iter()
        .map(|selected| {
            let mut next = observed.clone();
            let value = if next.apply(selected).is_ok() {
                evaluate(&next)
            } else {
                root_value
            };
            ActionStats {
                action: selected.clone(),
                visits: 1,
                availability: particles.len() as u32,
                availability_weight: 1.0,
                legal_weight: 1.0,
                prior: 1.0,
                value,
                lower_confidence_value: value,
            }
        })
        .collect();
    SearchReport {
        chosen,
        root_value,
        actions: statistics,
        tactical: TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        },
        exact: ExactDecisionResult::default(),
        statistics: SearchStatistics {
            iterations: 1,
            nodes: 1,
            deepest_decision_depth: 0,
            rollouts: 0,
            effective_particle_count: effective_particle_count(particles),
            deadline_reached: false,
        },
    }
}

fn basic_response_diagnostics(
    particles: usize,
    authority: DecisionAuthority,
) -> ResponseDiagnostics {
    ResponseDiagnostics {
        rust_posterior_particles: particles,
        rust_search_particles: particles,
        root_provenance: RootProvenanceOutput::default(),
        authority_trace: AuthorityTraceOutput {
            initial_authority: authority,
            exact_family: None,
            exact_family_replacement: None,
            safety_replacement: None,
        },
    }
}

fn response(
    report: SearchReport,
    particles: usize,
    algorithm: &'static str,
    authority: DecisionAuthority,
    diagnostics: ResponseDiagnostics,
) -> Response {
    Response {
        engine_revision: ENGINE_REVISION,
        authority,
        learned_model_version: learned_model_version(),
        trade_model_version: learned_trade_model_version(),
        algorithm,
        chosen: report.chosen.map(action),
        root_value: report.root_value,
        tactical_win_probability: report.tactical.win_probability,
        tactical_lower_bound: report.tactical.lower_bound,
        tactical_proven: report.tactical.proven,
        tactical_line: report
            .tactical
            .principal_line
            .into_iter()
            .map(action)
            .collect(),
        exact_decision: report.exact.applicable,
        exact_worlds: report.exact.worlds,
        actions: report
            .actions
            .into_iter()
            .map(|statistics| ActionStatisticsOutput {
                action: action(statistics.action),
                visits: statistics.visits,
                availability: statistics.availability,
                availability_weight: statistics.availability_weight,
                legal_weight: statistics.legal_weight,
                prior: statistics.prior,
                value: statistics.value,
                lower_confidence_value: statistics.lower_confidence_value,
            })
            .collect(),
        iterations: report.statistics.iterations,
        nodes: report.statistics.nodes,
        deepest_decision_depth: report.statistics.deepest_decision_depth,
        rollouts: report.statistics.rollouts,
        particles,
        wasm_particles: particles,
        rust_posterior_particles: diagnostics.rust_posterior_particles,
        rust_search_particles: diagnostics.rust_search_particles,
        root_provenance: diagnostics.root_provenance,
        authority_trace: diagnostics.authority_trace,
        effective_particle_count: report.statistics.effective_particle_count,
        deadline_reached: report.statistics.deadline_reached,
    }
}

fn effective_particle_count(particles: &[BeliefParticle]) -> f32 {
    let total = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let squared = particles
        .iter()
        .map(|particle| {
            let weight = particle.weight.max(0.0) / total;
            weight * weight
        })
        .sum::<f32>();
    1.0 / squared.max(f32::EPSILON)
}

fn exact_single_action(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
) -> Option<ExactDecisionResult> {
    let first = particles.first()?;
    let legal = first
        .state
        .legal_actions()
        .into_iter()
        .filter(|action| !root_exclusions.contains(action))
        .collect::<Vec<_>>();
    if legal.len() != 1 {
        return None;
    }
    let chosen = legal[0].clone();
    if particles.iter().any(|particle| {
        particle
            .state
            .legal_actions()
            .into_iter()
            .filter(|action| !root_exclusions.contains(action))
            .collect::<Vec<_>>()
            != [chosen.clone()]
    }) {
        return None;
    }
    let total_weight = particles
        .iter()
        .map(|particle| particle.weight.max(0.0))
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut value = [0.0; 4];
    let mut lower_bound = [f32::INFINITY; 4];
    for particle in particles {
        let weight = particle.weight.max(0.0) / total_weight;
        if weight <= f32::EPSILON {
            continue;
        }
        let mut next = particle.state.clone();
        next.apply(&chosen)
            .expect("the sole legal action must transition");
        let evaluated = evaluate(&next);
        for player in 0..4 {
            value[player] += evaluated[player] * weight;
            lower_bound[player] = lower_bound[player].min(evaluated[player]);
        }
    }
    let actor = first.state.actor() as usize;
    Some(ExactDecisionResult {
        applicable: true,
        chosen: Some(chosen.clone()),
        actions: vec![ExactActionValue {
            action: chosen,
            value,
            lower_bound,
            legal_weight: 1.0,
            decision_score: value[actor],
            lower_score: lower_bound[actor],
        }],
        worlds: particles.len(),
    })
}

/// Mandatory protocol decisions and positions with one universally legal
/// action are exact. Returning them before any long-range search keeps roll,
/// end-turn, discard, robber/victim, and trade-response latency independent of
/// the strategic simulation budget.
fn exact_mandatory_report(
    particles: &[BeliefParticle],
    root_exclusions: &[Action],
) -> Option<SearchReport> {
    let mandatory =
        solve_exact_belief_excluding(particles, ExactActionFamily::Mandatory, root_exclusions);
    let exact = if mandatory.applicable {
        mandatory
    } else {
        exact_single_action(particles, root_exclusions)?
    };
    if !exact.applicable {
        return None;
    }
    let chosen = exact.chosen.clone();
    let root_value = exact
        .actions
        .iter()
        .find(|candidate| Some(&candidate.action) == chosen.as_ref())
        .map(|candidate| candidate.value)
        .unwrap_or([0.0; 4]);
    let actions = exact
        .actions
        .iter()
        .map(|candidate| ActionStats {
            action: candidate.action.clone(),
            visits: particles.len() as u32,
            availability: (candidate.legal_weight * particles.len() as f32).round() as u32,
            availability_weight: candidate.legal_weight,
            legal_weight: candidate.legal_weight,
            prior: 0.0,
            value: candidate.value,
            lower_confidence_value: candidate.lower_bound,
        })
        .collect();
    Some(SearchReport {
        chosen,
        root_value,
        actions,
        tactical: TacticalResult {
            win_probability: 0.0,
            lower_bound: 0.0,
            principal_line: Vec::new(),
            nodes: 0,
            proven: false,
        },
        exact,
        statistics: SearchStatistics {
            iterations: 0,
            nodes: 0,
            deepest_decision_depth: 0,
            rollouts: 0,
            effective_particle_count: effective_particle_count(particles),
            deadline_reached: false,
        },
    })
}

fn root_exclusion_actions(
    inputs: &[RootExclusionInput],
    state: &GameState,
) -> Result<Vec<Action>, JsValue> {
    let recipients = ((1u8 << state.board.num_players) - 1) & !(1u8 << state.actor());
    inputs
        .iter()
        .map(|input| match input.kind.as_str() {
            "offer-trade" => Ok(Action::OfferTrade {
                recipients,
                give: input.give,
                receive: input.receive,
            }),
            "counter-trade" => Ok(Action::CounterTrade {
                give: input.give,
                receive: input.receive,
            }),
            other => Err(JsValue::from_str(&format!(
                "unsupported root exclusion kind: {other}"
            ))),
        })
        .collect()
}

#[wasm_bindgen]
pub fn analyze(request: JsValue) -> Result<JsValue, JsValue> {
    let request: Request = serde_wasm_bindgen::from_value(request)
        .map_err(|error| JsValue::from_str(&error.to_string()))?;
    let mode = RequestedMode::parse(request.mode.as_deref())?;
    let ponder = request.ponder.unwrap_or(false);
    let particles = game_states(request.state, request.last_rejected_trade)?;
    let root_exclusions = root_exclusion_actions(&request.root_exclusions, &particles[0].state)?;
    let algorithm = mode.label();
    if mode == RequestedMode::Weighted {
        let authority = DecisionAuthority::WeightedPolicy;
        let report = weighted_policy_report(
            &particles,
            &root_exclusions,
            request.seed.unwrap_or(0x0043_4154_414e),
        );
        return serde_wasm_bindgen::to_value(&response(
            report,
            particles.len(),
            algorithm,
            authority,
            basic_response_diagnostics(particles.len(), authority),
        ))
        .map_err(|error| JsValue::from_str(&error.to_string()));
    }
    if !ponder && let Some(report) = exact_mandatory_report(&particles, &root_exclusions) {
        return serde_wasm_bindgen::to_value(&response(
            report,
            particles.len(),
            algorithm,
            DecisionAuthority::ExactMandatory,
            basic_response_diagnostics(particles.len(), DecisionAuthority::ExactMandatory),
        ))
        .map_err(|error| JsValue::from_str(&error.to_string()));
    }
    let config = SearchConfig {
        iterations: request.iterations.unwrap_or(2_400).clamp(16, 50_000),
        max_nodes: request.max_nodes.unwrap_or(60_000).clamp(1_000, 250_000),
        rollout_actions: request.rollout_actions.unwrap_or(260).clamp(24, 800),
        tactical_depth: request.tactical_depth.unwrap_or(18).clamp(4, 32),
        tactical_nodes: request.tactical_nodes.unwrap_or(12_000).clamp(100, 100_000),
        time_budget_ms: request.time_budget_ms.unwrap_or(2_800).clamp(250, 10_000),
        seed: request.seed.unwrap_or(0x0043_4154_414e),
        mode: mode.mcts_mode(),
        ..SearchConfig::default()
    };
    let opening = matches!(
        particles[0].state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    );
    let (report, authority, diagnostics) =
        if matches!(mode, RequestedMode::Maxn | RequestedMode::AlphaBeta) || opening {
            let depth = request.depth.unwrap_or(3).clamp(1, 6);
            let branch_cap = request.branch_cap.unwrap_or(12).clamp(2, 32);
            let maximum_nodes = request.max_nodes.unwrap_or(48_000).clamp(1_000, 250_000) as u32;
            let depth_report = if mode == RequestedMode::AlphaBeta {
                search_weighted_belief_paranoid_bounded_timed_excluding(
                    &particles,
                    depth,
                    branch_cap,
                    maximum_nodes,
                    config.time_budget_ms,
                    &root_exclusions,
                )
            } else {
                search_weighted_belief_maxn_bounded_timed_excluding(
                    &particles,
                    depth,
                    branch_cap,
                    maximum_nodes,
                    config.time_budget_ms,
                    &root_exclusions,
                )
            }
            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?;
            let rust_posterior_particles = depth_report.posterior_particles;
            let rust_search_particles = depth_report.particles;
            let depth_safety_replacement = depth_report.provenance.safety_replacement.clone();
            let retained_root_priors = depth_report.provenance.retained_roots.clone();
            let root_provenance = root_provenance_output(depth_report.provenance.clone());
            let tactical_particles = particles
                .iter()
                .map(|particle| (&particle.state, particle.weight))
                .collect::<Vec<_>>();
            let tactical = solve_belief_current_turn(
                &tactical_particles,
                request.tactical_depth.unwrap_or(18).clamp(4, 32),
                request.tactical_nodes.unwrap_or(12_000).clamp(100, 100_000),
            );
            let actions = depth_report
                .actions
                .into_iter()
                .map(|candidate| {
                    let prior = retained_root_priors
                        .iter()
                        .find(|root| root.action == candidate.action)
                        .map_or(0.0, |root| root.prior);
                    ActionStats {
                        action: candidate.action,
                        visits: particles.len() as u32,
                        availability: (candidate.legal_weight * particles.len() as f32).round()
                            as u32,
                        availability_weight: candidate.legal_weight,
                        legal_weight: candidate.legal_weight,
                        prior,
                        value: candidate.value,
                        lower_confidence_value: candidate.lower_confidence_value,
                    }
                })
                .collect::<Vec<_>>();
            let mut exact = solve_exact_belief_excluding(
                &particles,
                ExactActionFamily::Mandatory,
                &root_exclusions,
            );
            let mut authority = if exact.applicable {
                DecisionAuthority::ExactMandatory
            } else if tactical.proven {
                DecisionAuthority::TacticalProven
            } else if depth_safety_replacement.is_some() {
                DecisionAuthority::SafetyOverride
            } else {
                DecisionAuthority::DeepMaxn
            };
            let initial_authority = authority;
            let mut exact_family = None;
            let mut exact_family_replacement = None;
            let mut safety_replacement = depth_safety_replacement.map(replacement_output);
            let mut chosen = if exact.applicable {
                exact.chosen.clone()
            } else if tactical.proven {
                tactical.principal_line.first().cloned()
            } else {
                depth_report.chosen
            };
            if !exact.applicable
                && !tactical.proven
                && let Some(family) = chosen.as_ref().and_then(exact_family_for_action)
            {
                exact_family = Some(exact_family_label(family));
                let before = chosen.clone();
                exact = solve_exact_belief_excluding(&particles, family, &root_exclusions);
                if let Some(exact_chosen) = exact.chosen.clone() {
                    if before.as_ref() != Some(&exact_chosen)
                        && let Some(previous) = before
                    {
                        exact_family_replacement = Some(ActionReplacementOutput {
                            from: action(previous),
                            to: action(exact_chosen.clone()),
                        });
                    }
                    chosen = Some(exact_chosen);
                    authority = DecisionAuthority::ExactFamily;
                }
            }
            // This is the final arbitration gate: when it changes the selected
            // action, downstream telemetry/execution must retain safety-override
            // authority rather than being relabeled by an earlier family solver.
            if chosen == Some(Action::EndTurn)
                && let Some(safer) = safer_end_turn_alternative(
                    &particles[0].state,
                    particles[0].state.actor() as usize,
                    &actions,
                    Some(&particles),
                )
            {
                if safer != Action::EndTurn {
                    safety_replacement = Some(ActionReplacementOutput {
                        from: action(Action::EndTurn),
                        to: action(safer.clone()),
                    });
                }
                chosen = Some(safer);
                authority = DecisionAuthority::SafetyOverride;
            }
            let diagnostics = ResponseDiagnostics {
                rust_posterior_particles,
                rust_search_particles,
                root_provenance,
                authority_trace: AuthorityTraceOutput {
                    initial_authority,
                    exact_family,
                    exact_family_replacement,
                    safety_replacement,
                },
            };
            (
                SearchReport {
                    chosen,
                    root_value: depth_report.value,
                    actions,
                    tactical,
                    exact,
                    statistics: SearchStatistics {
                        iterations: particles.len() as u32,
                        nodes: depth_report.nodes as usize,
                        deepest_decision_depth: depth_report.depth as u16,
                        rollouts: 0,
                        effective_particle_count: effective_particle_count(&particles),
                        deadline_reached: depth_report.deadline_reached,
                    },
                },
                authority,
                diagnostics,
            )
        } else {
            let mut groups = Vec::<(u64, Vec<BeliefParticle>)>::new();
            for particle in &particles {
                let identity = particle.state.observation_hash(particle.state.actor());
                if let Some((_, members)) = groups
                    .iter_mut()
                    .find(|(candidate, _)| *candidate == identity)
                {
                    members.push(particle.clone());
                } else {
                    groups.push((identity, vec![particle.clone()]));
                }
            }
            if !ponder && groups.len() > 1 {
                return Err(JsValue::from_str(
                    "root observation mismatch outside pondering mode",
                ));
            }
            let report = PERSISTENT_SEARCH.with(|slot| {
                let mut forest = slot.borrow_mut();
                let group_count = groups.len().max(1) as u32;
                let mut selected: Option<(f32, SearchReport)> = None;
                for (identity, group) in groups {
                    let group_weight = group
                        .iter()
                        .map(|particle| particle.weight.max(0.0))
                        .sum::<f32>();
                    let mut group_config = config.clone();
                    if ponder {
                        group_config.iterations = (group_config.iterations / group_count).max(16);
                        group_config.max_nodes =
                            (group_config.max_nodes / group_count as usize).max(1_000);
                        group_config.tactical_nodes =
                            (group_config.tactical_nodes / group_count).max(100);
                    }
                    let search_index = forest
                        .iter()
                        .position(|search| search.contains_identity(identity))
                        .unwrap_or_else(|| {
                            if forest.len() >= 12 {
                                forest.remove(0);
                            }
                            forest.push(Mcts::new(group_config.clone(), &group[0].state));
                            forest.len() - 1
                        });
                    let search = &mut forest[search_index];
                    search.reconfigure(group_config);
                    let report = if group.len() == 1 {
                        search.search(&group[0].state)
                    } else {
                        search
                            .search_weighted_belief(&group)
                            .map_err(|error| JsValue::from_str(&format!("{error:?}")))?
                    };
                    if selected
                        .as_ref()
                        .is_none_or(|(weight, _)| group_weight > *weight)
                    {
                        selected = Some((group_weight, report));
                    }
                }
                selected
                    .map(|(_, report)| report)
                    .ok_or_else(|| JsValue::from_str("pondering produced no search group"))
            })?;
            let authority = DecisionAuthority::DeepMaxn;
            let diagnostics = basic_response_diagnostics(particles.len(), authority);
            (report, authority, diagnostics)
        };
    serde_wasm_bindgen::to_value(&response(
        report,
        particles.len(),
        algorithm,
        authority,
        diagnostics,
    ))
    .map_err(|error| JsValue::from_str(&error.to_string()))
}

#[wasm_bindgen]
pub fn engine_version() -> String {
    ENGINE_REVISION.to_string()
}
