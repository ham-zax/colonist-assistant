use colonist_catan_core::{Action, Building, GameState, Port};

use crate::eval::production_pips;
use crate::policy::{ActionClass, action_class};
use crate::root_impact::compute_spatial_root_impacts;

pub const HEX_FEATURES: usize = 10;
pub const VERTEX_FEATURES: usize = 15;
pub const EDGE_FEATURES: usize = 9;
pub const PLAYER_FEATURES: usize = 39;
pub const GLOBAL_FEATURES: usize = 23;
pub const STRATEGIC_FEATURE_SCHEMA_VERSION: u8 = 2;
pub const BASE_ACTION_FEATURES: usize = 48;
pub const ROOT_IMPACT_FEATURES: usize = 4;
pub const ACTION_FEATURES: usize = BASE_ACTION_FEATURES + ROOT_IMPACT_FEATURES;
pub const STATE_FEATURES: usize = GLOBAL_FEATURES
    + PLAYER_FEATURES * 4
    + HEX_FEATURES * 2
    + VERTEX_FEATURES * 2
    + EDGE_FEATURES * 2;

#[derive(Clone, Debug)]
pub struct HeterogeneousGraphFeatures {
    pub hexes: Vec<[f32; HEX_FEATURES]>,
    pub vertices: Vec<[f32; VERTEX_FEATURES]>,
    pub edges: Vec<[f32; EDGE_FEATURES]>,
    pub players: Vec<[f32; PLAYER_FEATURES]>,
    pub global: [f32; GLOBAL_FEATURES],
}

fn phase_index(state: &GameState) -> usize {
    use colonist_catan_core::Phase;
    match state.phase {
        Phase::SetupSettlement => 0,
        Phase::SetupRoad { .. } => 1,
        Phase::PreRoll | Phase::RollChance => 2,
        Phase::Discard => 3,
        Phase::MoveRobber | Phase::ResolveSteal { .. } => 4,
        Phase::Main | Phase::DevelopmentChance => 5,
        Phase::TradeResponses => 6,
        Phase::Finished => 7,
    }
}

fn legal_maps(state: &GameState) -> (Vec<bool>, Vec<bool>, Vec<bool>) {
    let mut settlement = vec![false; state.board.vertices.len()];
    let mut city = vec![false; state.board.vertices.len()];
    let mut road = vec![false; state.board.edges.len()];
    for action in state.legal_actions() {
        match action {
            Action::PlaceSettlement { vertex } | Action::BuildSettlement { vertex } => {
                settlement[vertex as usize] = true;
            }
            Action::BuildCity { vertex } => city[vertex as usize] = true,
            Action::PlaceRoad { edge } | Action::BuildRoad { edge } => {
                road[edge as usize] = true;
            }
            _ => {}
        }
    }
    (settlement, city, road)
}

/// Observation-ready heterogeneous graph input. `observer` receives an exact
/// hand and development inventory; opponents receive only public totals unless
/// `perfect_information` is explicitly requested for oracle/self-play data.
pub fn encode_heterogeneous_graph(
    state: &GameState,
    observer: u8,
    perfect_information: bool,
) -> HeterogeneousGraphFeatures {
    let (legal_settlement, legal_city, legal_road) = legal_maps(state);
    let hidden_bank_value = state.bank.iter().copied().sum::<u8>() as f32 / (19.0 * 5.0);
    let hexes: Vec<[f32; HEX_FEATURES]> = state
        .board
        .hexes
        .iter()
        .enumerate()
        .map(|(index, hex)| {
            let mut row = [0.0; HEX_FEATURES];
            if let Some(resource) = hex.resource {
                row[resource.index()] = 1.0;
                row[8] = if state.bank_is_public {
                    state.bank[resource.index()] as f32 / 19.0
                } else {
                    hidden_bank_value
                };
            }
            row[5] = match hex.number {
                2 | 12 => 1.0 / 36.0,
                3 | 11 => 2.0 / 36.0,
                4 | 10 => 3.0 / 36.0,
                5 | 9 => 4.0 / 36.0,
                6 | 8 => 5.0 / 36.0,
                _ => 0.0,
            };
            row[6] = f32::from(index == state.robber_hex as usize);
            row[7] = f32::from(hex.resource.is_none());
            row[9] = hex.number as f32 / 12.0;
            row
        })
        .collect();

    let vertices = state
        .board
        .vertices
        .iter()
        .enumerate()
        .map(|(index, vertex)| {
            let mut row = [0.0; VERTEX_FEATURES];
            if let Some(building) = state.buildings[index] {
                let owner = building.player() as usize;
                if owner < 4 {
                    row[owner] = 1.0;
                }
                row[4] = f32::from(matches!(building, Building::Settlement(_)));
                row[5] = f32::from(matches!(building, Building::City(_)));
            } else {
                row[6] = 1.0;
            }
            row[7] = f32::from(legal_settlement[index]);
            row[8] = f32::from(legal_city[index]);
            match vertex.port {
                Some(Port::Generic) => row[9] = 1.0,
                Some(Port::Resource(resource)) => {
                    row[10] = 1.0;
                    row[11] = resource.index() as f32 / 4.0;
                }
                None => {}
            }
            row[12] = vertex.adjacent_hexes.len() as f32 / 3.0;
            row[13] = vertex.adjacent_edges.len() as f32 / 3.0;
            row[14] = vertex
                .adjacent_hexes
                .iter()
                .map(|hex| hexes[*hex as usize][5])
                .sum::<f32>();
            row
        })
        .collect();

    let edges = state
        .board
        .edges
        .iter()
        .enumerate()
        .map(|(index, edge)| {
            let mut row = [0.0; EDGE_FEATURES];
            if let Some(owner) = state.roads[index] {
                if owner < 4 {
                    row[owner as usize] = 1.0;
                }
            } else {
                row[4] = 1.0;
            }
            row[5] = f32::from(legal_road[index]);
            row[6] = edge.adjacent_hexes.len() as f32 / 2.0;
            row[7] = f32::from(edge.vertices.iter().any(|vertex| {
                state.buildings[*vertex as usize]
                    .is_some_and(|building| building.player() != state.actor())
            }));
            row[8] = f32::from(edge.vertices.iter().any(|vertex| {
                state.buildings[*vertex as usize]
                    .is_some_and(|building| building.player() == state.actor())
            }));
            row
        })
        .collect();

    let players = (0..state.board.num_players)
        .map(|player| {
            let public = &state.players[player as usize];
            let production = production_pips(state, player);
            let ratios = state.trade_ratios(player);
            let reveal_private = perfect_information || player == observer;
            let mut row = [0.0; PLAYER_FEATURES];
            row[0] = public.public_victory_points as f32 / state.victory_target as f32;
            row[1] = public.resource_total() as f32 / 20.0;
            row[2] = public.development.iter().copied().sum::<u8>() as f32 / 10.0;
            row[3] = public.played_knights as f32 / 6.0;
            row[4] = public.roads_left as f32 / 15.0;
            row[5] = public.settlements_left as f32 / 5.0;
            row[6] = public.cities_left as f32 / 4.0;
            row[7] = f32::from(public.has_longest_road);
            row[8] = f32::from(public.has_largest_army);
            for index in 0..5 {
                row[9 + index] = if reveal_private {
                    public.resources[index] as f32 / 12.0
                } else {
                    public.resource_total() as f32 / 60.0
                };
                row[14 + index] = production[index] / 15.0;
                row[19 + index] = (4 - ratios[index]) as f32 / 2.0;
                row[24 + index] = if reveal_private {
                    public.development[index] as f32 / 5.0
                } else {
                    public.development.iter().copied().sum::<u8>() as f32 / 25.0
                };
            }
            row[29] = state.longest_road_length(player) as f32 / 15.0;
            row[30] = production.iter().sum::<f32>() / 45.0;
            row[31] = f32::from(player == observer);
            row[32] = f32::from(player == state.actor());
            row[33] = f32::from(public.resource_total() > state.card_discard_limit);
            for (index, posterior) in public.policy_profile.iter().enumerate() {
                row[34 + index] = *posterior as f32 / 255.0;
            }
            row
        })
        .collect();

    let mut global = [0.0; GLOBAL_FEATURES];
    global[0] = state.board.num_players as f32 / 4.0;
    global[1] = state.turn as f32 / 200.0;
    global[2] = state.last_roll as f32 / 12.0;
    global[3] = state.current_player as f32 / 3.0;
    global[4] = observer as f32 / 3.0;
    global[5] = state.development_deck.iter().copied().sum::<u8>() as f32 / 25.0;
    for index in 0..8 {
        global[6 + index] = f32::from(phase_index(state) == index);
    }
    global[14] = f32::from(state.domestic_trade_used);
    global[15] = state.domestic_trade_count as f32 / 3.0;
    global[16] = state.robber_hex as f32 / state.board.hexes.len().max(1) as f32;
    global[17] = state.victory_target as f32 / 15.0;
    for index in 0..5 {
        global[18 + index] =
            state.played_development[index] as f32 / [14.0, 5.0, 2.0, 2.0, 2.0][index];
    }

    HeterogeneousGraphFeatures {
        hexes,
        vertices,
        edges,
        players,
        global,
    }
}

fn class_index(class: ActionClass) -> usize {
    match class {
        ActionClass::Mandatory => 0,
        ActionClass::Settlement => 1,
        ActionClass::City => 2,
        ActionClass::ExpansionRoad => 3,
        ActionClass::Development => 4,
        ActionClass::DomesticTrade => 5,
        ActionClass::MaritimeTrade => 6,
        ActionClass::Trophy => 7,
        ActionClass::HandSafety => 8,
        ActionClass::EndTurn => 9,
    }
}

fn encode_action_base(state: &GameState, action: &Action) -> [f32; ACTION_FEATURES] {
    let mut row = [0.0; ACTION_FEATURES];
    row[class_index(action_class(action))] = 1.0;
    let pip_value = |number: u8| match number {
        2 | 12 => 1.0,
        3 | 11 => 2.0,
        4 | 10 => 3.0,
        5 | 9 => 4.0,
        6 | 8 => 5.0,
        _ => 0.0,
    };
    let encode_vertex_target = |row: &mut [f32; ACTION_FEATURES], vertex: usize| {
        let target = &state.board.vertices[vertex];
        for hex in &target.adjacent_hexes {
            let tile = &state.board.hexes[*hex as usize];
            if let Some(resource) = tile.resource {
                let pips = pip_value(tile.number);
                row[32 + resource.index()] += pips / 15.0;
                row[37] += pips / 15.0;
            }
        }
        match target.port {
            Some(Port::Generic) => row[38] = 1.0,
            Some(Port::Resource(resource)) => {
                row[39] = 1.0;
                row[40] = (resource.index() + 1) as f32 / 5.0;
            }
            None => {}
        }
        if let Some(building) = state.buildings[vertex] {
            if building.player() == state.actor() {
                row[41] = 1.0;
            } else {
                row[42] = 1.0;
            }
        } else {
            row[43] = 1.0;
        }
    };
    match action {
        Action::PlaceSettlement { vertex }
        | Action::BuildSettlement { vertex }
        | Action::BuildCity { vertex } => {
            row[10] = *vertex as f32 / state.board.vertices.len().max(1) as f32;
            row[11] = state.board.vertices[*vertex as usize].adjacent_hexes.len() as f32 / 3.0;
            encode_vertex_target(&mut row, *vertex as usize);
        }
        Action::PlaceRoad { edge } | Action::BuildRoad { edge } => {
            row[12] = *edge as f32 / state.board.edges.len().max(1) as f32;
            let target = &state.board.edges[*edge as usize];
            if let Some(vertex) = target.vertices.iter().copied().max_by(|left, right| {
                let score = |candidate: u8| {
                    state.board.vertices[candidate as usize]
                        .adjacent_hexes
                        .iter()
                        .map(|hex| pip_value(state.board.hexes[*hex as usize].number))
                        .sum::<f32>()
                };
                score(*left).total_cmp(&score(*right))
            }) {
                encode_vertex_target(&mut row, vertex as usize);
            }
            let actor = state.actor();
            let connections = target
                .vertices
                .iter()
                .map(|vertex| {
                    let building = f32::from(
                        state.buildings[*vertex as usize]
                            .is_some_and(|piece| piece.player() == actor),
                    );
                    let roads = state.board.vertices[*vertex as usize]
                        .adjacent_edges
                        .iter()
                        .filter(|candidate| state.roads[**candidate as usize] == Some(actor))
                        .count() as f32;
                    building + roads
                })
                .sum::<f32>();
            row[44] = (connections / 4.0).clamp(0.0, 1.0);
        }
        Action::MoveRobber { hex, victim } | Action::PlayKnight { hex, victim } => {
            row[13] = *hex as f32 / state.board.hexes.len().max(1) as f32;
            row[14] = victim.map_or(0.0, |player| (player + 1) as f32 / 4.0);
            let tile = &state.board.hexes[*hex as usize];
            if let Some(resource) = tile.resource {
                row[32 + resource.index()] = pip_value(tile.number) / 5.0;
            }
            row[37] = pip_value(tile.number) / 5.0;
            row[43] = f32::from(*hex != state.robber_hex);
        }
        Action::PlayRoadBuilding { first, second } => {
            row[12] = *first as f32 / state.board.edges.len().max(1) as f32;
            row[15] = second.map_or(0.0, |edge| {
                (edge + 1) as f32 / state.board.edges.len().max(1) as f32
            });
        }
        Action::PlayYearOfPlenty { first, second } => {
            row[16 + first.index()] += 0.5;
            row[16 + second.index()] += 0.5;
        }
        Action::PlayMonopoly { resource } => row[16 + resource.index()] = 1.0,
        Action::MaritimeTrade {
            give,
            receive,
            ratio,
        } => {
            row[16 + give.index()] = -(*ratio as f32 / 4.0);
            row[16 + receive.index()] = 1.0;
            row[21] = *ratio as f32 / 4.0;
        }
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } => {
            for index in 0..5 {
                row[16 + index] = receive[index] as f32 / 2.0 - give[index] as f32 / 2.0;
            }
            row[22] = recipients.count_ones() as f32 / 3.0;
        }
        Action::CounterTrade { give, receive } => {
            for index in 0..5 {
                row[16 + index] = receive[index] as f32 / 2.0 - give[index] as f32 / 2.0;
            }
        }
        Action::RespondTrade { accept } => row[23] = f32::from(*accept),
        Action::ConfirmTrade { partner } => row[24] = (*partner + 1) as f32 / 4.0,
        Action::ResolveRoll { value } => row[25] = *value as f32 / 12.0,
        Action::ResolveSteal { resource, .. } => row[16 + resource.index()] = 1.0,
        Action::Discard { cards } => {
            for index in 0..5 {
                row[16 + index] = -(cards[index] as f32 / 10.0);
            }
        }
        _ => {}
    }
    row[26] = state.players[state.actor() as usize].resource_total() as f32 / 20.0;
    row[27] =
        state.players[state.actor() as usize].victory_points() as f32 / state.victory_target as f32;
    row[28] = state.turn as f32 / 200.0;
    row[29] = f32::from(state.actor() == state.current_player);
    row[30] = f32::from(
        state.players[state.actor() as usize].resource_total() > state.card_discard_limit,
    );
    row[31] = 1.0;
    row[45] = match action {
        Action::BuildRoad { .. } => 2.0 / 8.0,
        Action::BuildSettlement { .. } => 4.0 / 8.0,
        Action::BuildCity { .. } => 5.0 / 8.0,
        Action::BuyDevelopment => 3.0 / 8.0,
        Action::MaritimeTrade { ratio, .. } => ratio.saturating_sub(1) as f32 / 8.0,
        Action::OfferTrade { give, receive, .. } | Action::CounterTrade { give, receive } => {
            (give.iter().copied().sum::<u8>() as f32 - receive.iter().copied().sum::<u8>() as f32)
                / 8.0
        }
        Action::Discard { cards } => cards.iter().copied().sum::<u8>() as f32 / 8.0,
        _ => 0.0,
    };
    row[46] += match action {
        Action::PlaceSettlement { .. }
        | Action::BuildSettlement { .. }
        | Action::BuildCity { .. } => 0.1,
        _ => 0.0,
    };
    row[47] = f32::from({
        let hand = state.players[state.actor() as usize].resources;
        match action {
            Action::BuildRoad { .. } => hand[0] >= 1 && hand[1] >= 1,
            Action::BuildSettlement { .. } => {
                hand[0] >= 1 && hand[1] >= 1 && hand[2] >= 1 && hand[3] >= 1
            }
            Action::BuildCity { .. } => hand[3] >= 2 && hand[4] >= 3,
            Action::BuyDevelopment => hand[2] >= 1 && hand[3] >= 1 && hand[4] >= 1,
            _ => false,
        }
    });
    row
}

/// Encodes candidate actions with authoritative root-level road consequences.
/// Spatial impacts are computed once for the action batch so the learned prior
/// consumes the existing structural owner instead of reconstructing topology.
pub fn encode_actions(state: &GameState, actions: &[Action]) -> Vec<[f32; ACTION_FEATURES]> {
    let mut encoded = actions
        .iter()
        .map(|action| encode_action_base(state, action))
        .collect::<Vec<_>>();
    let needs_spatial = actions.iter().any(|action| {
        matches!(
            action,
            Action::PlaceSettlement { .. }
                | Action::BuildSettlement { .. }
                | Action::PlaceRoad { .. }
                | Action::BuildRoad { .. }
        )
    });
    if !needs_spatial {
        return encoded;
    }

    let report = compute_spatial_root_impacts(state, state.actor(), actions);
    for (row, impact) in encoded.iter_mut().zip(report.actions.iter()) {
        row[BASE_ACTION_FEATURES] =
            (impact.road_delta.longest_road_loss_prevented.max(0) as f32 / 15.0).clamp(0.0, 1.0);
        row[BASE_ACTION_FEATURES + 1] =
            (impact.road_delta.longest_road_loss_inflicted.max(0) as f32 / 15.0).clamp(0.0, 1.0);
        row[BASE_ACTION_FEATURES + 2] =
            (impact.road_delta.award_vp_swing as f32 / 2.0).clamp(-1.0, 1.0);
        row[BASE_ACTION_FEATURES + 3] =
            (impact.road_delta.expansion_portfolio_delta / 30.0).clamp(0.0, 1.0);
    }
    encoded
}

pub fn encode_action(state: &GameState, action: &Action) -> [f32; ACTION_FEATURES] {
    encode_actions(state, std::slice::from_ref(action))[0]
}

fn pool_rows<const WIDTH: usize>(rows: &[[f32; WIDTH]], output: &mut Vec<f32>) {
    let mut mean = [0.0; WIDTH];
    let mut maximum = [f32::NEG_INFINITY; WIDTH];
    for row in rows {
        for index in 0..WIDTH {
            mean[index] += row[index];
            maximum[index] = maximum[index].max(row[index]);
        }
    }
    if !rows.is_empty() {
        for value in &mut mean {
            *value /= rows.len() as f32;
        }
    }
    for value in &mut maximum {
        if !value.is_finite() {
            *value = 0.0;
        }
    }
    output.extend(mean);
    output.extend(maximum);
}

/// Fixed-width graph readout used by the compact value/policy network. Raw
/// heterogeneous nodes remain available above for future message passing; this
/// deterministic mean/max readout keeps the browser model tiny and fast.
pub fn pool_heterogeneous_graph(
    graph: &HeterogeneousGraphFeatures,
    observer: u8,
) -> [f32; STATE_FEATURES] {
    let mut pooled = Vec::with_capacity(STATE_FEATURES);
    pooled.extend(graph.global);
    for offset in 0..4 {
        let count = graph.players.len();
        let row = if offset < count && count > 0 {
            graph.players[(observer as usize + offset) % count]
        } else {
            [0.0; PLAYER_FEATURES]
        };
        pooled.extend(row);
    }
    pool_rows(&graph.hexes, &mut pooled);
    pool_rows(&graph.vertices, &mut pooled);
    pool_rows(&graph.edges, &mut pooled);
    pooled
        .try_into()
        .expect("heterogeneous graph readout has a fixed feature width")
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{Action, GameState, Phase};

    use super::{
        ACTION_FEATURES, BASE_ACTION_FEATURES, encode_action, encode_actions,
        encode_heterogeneous_graph, pool_heterogeneous_graph,
    };
    use crate::root_impact::compute_spatial_root_impacts;

    fn install_road_chain(state: &mut GameState, owner: u8, length: usize) -> Vec<u8> {
        assert!(length > 0);
        let mut edges = vec![0u8];
        state.roads[0] = Some(owner);
        let mut vertex = state.board.edges[0].vertices[1];
        while edges.len() < length {
            let previous = *edges.last().unwrap();
            let next = *state.board.vertices[vertex as usize]
                .adjacent_edges
                .iter()
                .find(|&&edge| edge != previous && !edges.contains(&edge))
                .expect("standard board provides a continuation edge");
            state.roads[next as usize] = Some(owner);
            let [left, right] = state.board.edges[next as usize].vertices;
            vertex = if left == vertex { right } else { left };
            edges.push(next);
        }
        edges
    }

    #[test]
    fn strategic_action_tail_matches_authoritative_root_impact() {
        let mut state = GameState::standard(1, 3);
        let chain = install_road_chain(&mut state, 1, 5);
        state.update_longest_road();
        let shared = state.board.edges[chain[1] as usize]
            .vertices
            .into_iter()
            .find(|vertex| state.board.edges[chain[2] as usize].vertices.contains(vertex))
            .expect("adjacent chain edges share a vertex");
        let action = Action::PlaceSettlement { vertex: shared };
        let actions = vec![action.clone(), Action::Roll];

        let source = compute_spatial_root_impacts(&state, state.actor(), &actions);
        let encoded = encode_actions(&state, &actions);
        let impact = &source.actions[0].road_delta;
        let expected = [
            (impact.longest_road_loss_prevented.max(0) as f32 / 15.0).clamp(0.0, 1.0),
            (impact.longest_road_loss_inflicted.max(0) as f32 / 15.0).clamp(0.0, 1.0),
            (impact.award_vp_swing as f32 / 2.0).clamp(-1.0, 1.0),
            (impact.expansion_portfolio_delta / 30.0).clamp(0.0, 1.0),
        ];

        assert_eq!(encoded[0][BASE_ACTION_FEATURES..], expected);
        assert_eq!(encoded[0].len(), ACTION_FEATURES);
        assert!(encoded[0].iter().all(|value| value.is_finite()));
        assert!(encoded[1][BASE_ACTION_FEATURES..].iter().all(|value| *value == 0.0));
    }

    #[test]
    fn parameterized_actions_encode_the_target_board_quality_not_only_its_id() {
        let state = GameState::standard(401, 4);
        let actions = state
            .legal_actions()
            .into_iter()
            .filter(|action| matches!(action, Action::PlaceSettlement { .. }))
            .collect::<Vec<_>>();
        let encoded = actions
            .iter()
            .map(|action| (action, encode_action(&state, action)))
            .collect::<Vec<_>>();
        let minimum = encoded
            .iter()
            .min_by(|left, right| left.1[37].total_cmp(&right.1[37]))
            .expect("opening has settlement actions");
        let maximum = encoded
            .iter()
            .max_by(|left, right| left.1[37].total_cmp(&right.1[37]))
            .expect("opening has settlement actions");
        assert_ne!(minimum.0, maximum.0);
        assert!(
            maximum.1[37] > minimum.1[37],
            "target-local production must distinguish parameterized placements",
        );
    }

    #[test]
    fn graph_readout_hides_third_party_card_identity_but_keeps_policy_posterior() {
        let mut first = GameState::standard(409, 3);
        while matches!(
            first.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = first.legal_actions()[0].clone();
            first.apply(&action).unwrap();
        }
        first.phase = Phase::Main;
        first.current_player = 0;
        first.players[1].resources = [4, 0, 0, 0, 0];
        first.players[1].development = [1, 0, 0, 0, 0];
        let mut second = first.clone();
        second.players[1].resources = [0, 0, 0, 4, 0];
        second.players[1].development = [0, 1, 0, 0, 0];
        assert_eq!(first.observation_hash(0), second.observation_hash(0));
        let first_features =
            pool_heterogeneous_graph(&encode_heterogeneous_graph(&first, 0, false), 0);
        let second_features =
            pool_heterogeneous_graph(&encode_heterogeneous_graph(&second, 0, false), 0);
        assert_eq!(first_features, second_features);

        second.players[1].policy_profile = [255, 0, 0, 0, 0];
        let profiled = pool_heterogeneous_graph(&encode_heterogeneous_graph(&second, 0, false), 0);
        assert_ne!(
            second_features, profiled,
            "inferred opponent policy posterior is a legitimate search feature",
        );
    }
}
