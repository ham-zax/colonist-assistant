use std::cmp::Ordering;
use std::collections::BinaryHeap;

use colonist_catan_core::{
    CITY_COST, DEVELOPMENT_COST, GameState, ROAD_COST, ResourceHand, SETTLEMENT_COST,
};

const PIPS: [f32; 13] = [
    0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 5.0, 4.0, 3.0, 2.0, 1.0,
];
/// Weak opening prior only. Marginal hand values are derived from the current
/// build race, production, and ports below.
const BASE_RESOURCE_WEIGHTS: [f32; 5] = [0.98, 0.98, 0.73, 1.22, 1.10];
const BUILD_COSTS: [ResourceHand; 4] = [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST];

#[derive(Clone, Copy, Debug, Default)]
pub struct ExpansionOption {
    pub vertex: Option<u8>,
    pub roads_required: u8,
    pub value: f32,
    pub survival_probability: f32,
    /// Combined value of the best three surviving expansion options.
    pub portfolio_value: f32,
    pub option_count: u8,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct TrophyOutlook {
    pub acquire: f32,
    pub retain: f32,
    pub additional_cost: f32,
}

#[derive(Clone, Copy, Debug)]
struct QueueEntry {
    cost: u8,
    vertex: u8,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.cost == other.cost && self.vertex == other.vertex
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .cost
            .cmp(&self.cost)
            .then_with(|| other.vertex.cmp(&self.vertex))
    }
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn contains(hand: &ResourceHand, cost: &ResourceHand) -> bool {
    hand.iter()
        .zip(cost)
        .all(|(available, required)| available >= required)
}

fn deficit(hand: &ResourceHand, cost: &ResourceHand) -> ResourceHand {
    let mut result = [0; 5];
    for index in 0..5 {
        result[index] = cost[index].saturating_sub(hand[index]);
    }
    result
}

fn total(hand: &ResourceHand) -> u8 {
    hand.iter().copied().sum()
}

pub fn production_pips(state: &GameState, player: u8) -> [f32; 5] {
    let mut result = [0.0; 5];
    for (vertex, building) in state.buildings.iter().enumerate() {
        let Some(building) = building else {
            continue;
        };
        if building.player() != player {
            continue;
        }
        for hex in &state.board.vertices[vertex].adjacent_hexes {
            let tile = &state.board.hexes[*hex as usize];
            let Some(resource) = tile.resource else {
                continue;
            };
            // A blocked number is not permanently worthless. A small residual
            // represents the probability that another robber event frees it.
            let active = if *hex == state.robber_hex { 0.12 } else { 1.0 };
            // A publicly depleted bank cannot satisfy a production event.
            // Keep a small future-replenishment floor because builds, trades,
            // and discards can return cards before this number next appears.
            let bank_supply = if state.bank_is_public {
                match state.bank[resource.index()] {
                    0 => 0.15,
                    1 => 0.55,
                    2 => 0.82,
                    _ => 1.0,
                }
            } else {
                1.0
            };
            result[resource.index()] += PIPS[tile.number as usize]
                * building.production_multiplier() as f32
                * active
                * bank_supply;
        }
    }
    result
}

fn dynamic_resource_weights(state: &GameState, player: u8) -> [f32; 5] {
    let player_state = &state.players[player as usize];
    let production = production_pips(state, player);
    let ratios = state.trade_ratios(player);
    let mut weights = BASE_RESOURCE_WEIGHTS;

    let best_deficit = BUILD_COSTS
        .iter()
        .enumerate()
        .map(|(kind, cost)| {
            let missing = deficit(&player_state.resources, cost);
            let weighted = missing
                .iter()
                .enumerate()
                .map(|(index, amount)| {
                    *amount as f32
                        * BASE_RESOURCE_WEIGHTS[index]
                        * (6.0 / (production[index] + 1.5)).clamp(0.55, 2.2)
                })
                .sum::<f32>();
            let plan_value: f32 = [0.35, 1.25, 1.15, 0.72][kind];
            (weighted / plan_value.max(0.1), missing)
        })
        .min_by(|left, right| left.0.total_cmp(&right.0))
        .map(|(_, missing)| missing)
        .unwrap_or([0; 5]);

    for index in 0..5 {
        let scarcity = (6.0 / (production[index] + 2.0)).clamp(0.65, 1.8);
        let bottleneck = if best_deficit[index] > 0 {
            1.0 + 0.36 * best_deficit[index] as f32
        } else {
            0.9
        };
        let port_liquidity = match ratios[index] {
            2 => 1.18,
            3 => 1.08,
            _ => 1.0,
        };
        let surplus = player_state.resources[index].saturating_sub(4) as f32;
        weights[index] = BASE_RESOURCE_WEIGHTS[index] * scarcity * bottleneck * port_liquidity
            / (1.0 + surplus * 0.10);
    }
    weights
}

fn hand_utility_with_weights(hand: &ResourceHand, weights: &[f32; 5]) -> f32 {
    let liquidity = hand
        .iter()
        .enumerate()
        .map(|(index, amount)| *amount as f32 * weights[index])
        .sum::<f32>();
    let completed = BUILD_COSTS
        .iter()
        .enumerate()
        .filter(|(_, cost)| contains(hand, cost))
        .map(|(kind, _)| [0.35, 1.45, 1.35, 0.78][kind])
        .fold(0.0, f32::max);
    let near_plan = BUILD_COSTS
        .iter()
        .enumerate()
        .map(|(kind, cost)| {
            let missing = total(&deficit(hand, cost)) as f32;
            [0.25, 1.10, 1.0, 0.58][kind] / (1.0 + missing)
        })
        .fold(0.0, f32::max);
    liquidity * 0.18 + completed + near_plan
}

/// Fast local value for a prospective hand transition.
///
/// Action priors and trade-response models call this for many nearby bundles.
/// Re-running the complete board/race evaluator for each bundle made one
/// rollout solve every expansion race dozens of times. Board position,
/// trophies, and production are unchanged by a pending resource exchange, so
/// their terms cancel; only hand composition, build completion, and immediate
/// seven exposure need to be compared here.
pub(crate) fn hand_transition_value(
    state: &GameState,
    player: u8,
    resulting_hand: &ResourceHand,
) -> f32 {
    let current = state.players[player as usize].resources;
    let weights = dynamic_resource_weights(state, player);
    let current_value = hand_utility_with_weights(&current, &weights);
    let resulting_value = hand_utility_with_weights(resulting_hand, &weights);
    let overflow = |hand: &ResourceHand| {
        let held = total(hand);
        if held <= state.card_discard_limit {
            0.0
        } else {
            // This is an immediate-transition comparison, not the full orbit
            // loss model used at search leaves. Quadratic exposure preserves
            // the strong incentive to convert a large at-risk hand cheaply.
            let excess = held.saturating_sub(state.card_discard_limit) as f32;
            excess * 0.20 + excess * excess * 0.045
        }
    };
    resulting_value - current_value + overflow(&current) - overflow(resulting_hand)
}

fn enumerate_optimal_kept_utility(
    hand: &ResourceHand,
    discard_count: u8,
    weights: &[f32; 5],
) -> f32 {
    fn visit(
        hand: &ResourceHand,
        weights: &[f32; 5],
        index: usize,
        remaining: u8,
        discarded: &mut ResourceHand,
        best: &mut f32,
    ) {
        if index == 5 {
            if remaining == 0 {
                let mut kept = *hand;
                for resource in 0..5 {
                    kept[resource] -= discarded[resource];
                }
                *best = best.max(hand_utility_with_weights(&kept, weights));
            }
            return;
        }
        for amount in 0..=remaining.min(hand[index]) {
            discarded[index] = amount;
            visit(
                hand,
                weights,
                index + 1,
                remaining - amount,
                discarded,
                best,
            );
        }
        discarded[index] = 0;
    }

    let mut best: f32 = 0.0;
    visit(hand, weights, 0, discard_count, &mut [0; 5], &mut best);
    best
}

fn rolls_before_next_spend(state: &GameState, player: u8) -> u8 {
    if state.current_player == player && matches!(state.phase, colonist_catan_core::Phase::Main) {
        return 0;
    }
    let count = state.board.num_players;
    let distance = (player + count - state.current_player) % count;
    let current_roll_still_pending = matches!(
        state.phase,
        colonist_catan_core::Phase::PreRoll | colonist_catan_core::Phase::RollChance
    );
    if distance == 0 {
        u8::from(current_roll_still_pending)
    } else {
        distance + u8::from(current_roll_still_pending)
    }
}

/// Expected strategic hand utility lost to a seven before the player's next
/// spending window. This is nonlinear, composition-aware, and chooses the best
/// legal discard rather than applying a tiny linear overflow penalty.
pub fn expected_discard_loss(state: &GameState, player: u8) -> f32 {
    let hand = &state.players[player as usize].resources;
    let held = total(hand);
    let rolls = rolls_before_next_spend(state, player);
    if rolls == 0 {
        // During Main the player can still convert the hand. A residual price
        // stops EndTurn from looking harmless when no conversion is selected.
        return 0.04 * held.saturating_sub(state.card_discard_limit) as f32;
    }
    let probability = 1.0 - (5.0_f32 / 6.0).powi(rolls as i32);
    // Condition on the first seven arriving within the available rolls and
    // project the safe production accumulated before it. This catches hands
    // of six or seven that are likely to cross the discard threshold during
    // the intervening orbit without running an expensive dice tree at every
    // search leaf.
    let expected_safe_rolls = {
        let mut numerator = 0.0;
        let mut survival = 1.0;
        for safe_rolls in 0..rolls {
            numerator += safe_rolls as f32 * survival * (1.0 / 6.0);
            survival *= 5.0 / 6.0;
        }
        numerator / probability.max(f32::EPSILON)
    };
    let production = production_pips(state, player);
    let mut projected = *hand;
    let mut expected_added_total = 0.0;
    for resource in 0..5 {
        let expected = production[resource] / 30.0 * expected_safe_rolls;
        expected_added_total += expected;
        projected[resource] = projected[resource].saturating_add(expected.floor() as u8);
    }
    let projected_held = total(&projected);
    if projected_held <= state.card_discard_limit {
        return probability
            * (held as f32 + expected_added_total - state.card_discard_limit as f32)
                .max(0.0)
                .powf(1.35)
            * 0.32;
    }
    let weights = dynamic_resource_weights(state, player);
    let before = hand_utility_with_weights(&projected, &weights);
    let kept = enumerate_optimal_kept_utility(&projected, projected_held / 2, &weights);
    let expected_cards_lost = probability * (projected_held / 2) as f32;
    let overflow = projected_held.saturating_sub(state.card_discard_limit) as f32;
    probability * (before - kept).max(0.0)
        + expected_cards_lost * 0.22
        + probability * overflow.powf(1.35) * 0.15
}

fn settlement_vertex_open(state: &GameState, vertex: usize) -> bool {
    state.buildings[vertex].is_none()
        && state.board.vertices[vertex]
            .adjacent_vertices
            .iter()
            .all(|neighbor| state.buildings[*neighbor as usize].is_none())
}

fn road_distances(state: &GameState, player: u8) -> Vec<u8> {
    let mut distances = vec![u8::MAX; state.board.vertices.len()];
    let mut queue = BinaryHeap::new();
    for (vertex, building) in state.buildings.iter().enumerate() {
        if building.is_some_and(|piece| piece.player() == player) {
            distances[vertex] = 0;
            queue.push(QueueEntry {
                cost: 0,
                vertex: vertex as u8,
            });
        }
    }
    for (edge, owner) in state.roads.iter().enumerate() {
        if *owner != Some(player) {
            continue;
        }
        for vertex in state.board.edges[edge].vertices {
            if distances[vertex as usize] > 0 {
                distances[vertex as usize] = 0;
                queue.push(QueueEntry { cost: 0, vertex });
            }
        }
    }
    while let Some(QueueEntry { cost, vertex }) = queue.pop() {
        if cost != distances[vertex as usize] {
            continue;
        }
        // An opponent building cuts road continuation through this vertex.
        if state.buildings[vertex as usize].is_some_and(|piece| piece.player() != player) {
            continue;
        }
        for edge in &state.board.vertices[vertex as usize].adjacent_edges {
            let edge_owner = state.roads[*edge as usize];
            if edge_owner.is_some_and(|owner| owner != player) {
                continue;
            }
            let next_cost = cost.saturating_add(u8::from(edge_owner.is_none()));
            let [a, b] = state.board.edges[*edge as usize].vertices;
            let next = if a == vertex { b } else { a };
            if next_cost < distances[next as usize] {
                distances[next as usize] = next_cost;
                queue.push(QueueEntry {
                    cost: next_cost,
                    vertex: next,
                });
            }
        }
    }
    distances
}

fn all_route_maps(state: &GameState) -> Vec<Vec<u8>> {
    (0..state.board.num_players)
        .map(|candidate| road_distances(state, candidate))
        .collect()
}

fn turns_until_action(state: &GameState, player: u8) -> f32 {
    let count = state.board.num_players;
    let seats = (player + count - state.current_player) % count;
    let phase_delay = if player == state.current_player
        && matches!(
            state.phase,
            colonist_catan_core::Phase::Main
                | colonist_catan_core::Phase::PreRoll
                | colonist_catan_core::Phase::TradeResponses
        ) {
        0.0
    } else {
        0.15
    };
    seats as f32 + phase_delay
}

fn expansion_arrival_score(
    state: &GameState,
    player: u8,
    roads_required: u8,
    exact_hand_visible: bool,
    production: &[f32; 5],
) -> f32 {
    let mut plan_cost = SETTLEMENT_COST;
    plan_cost[0] = plan_cost[0].saturating_add(roads_required);
    plan_cost[1] = plan_cost[1].saturating_add(roads_required);
    let missing = if exact_hand_visible {
        deficit(&state.players[player as usize].resources, &plan_cost).map(f32::from)
    } else {
        // Rival card identities are hidden. Estimate coverage from their
        // public hand total and production mix instead of consulting this
        // sampled world.
        let public_cards = state.players[player as usize].resource_total() as f32;
        let production_total = production.iter().sum::<f32>();
        std::array::from_fn(|resource| {
            let expected_share = (production[resource] + 1.0) / (production_total + 5.0);
            (plan_cost[resource] as f32 - public_cards * expected_share).max(0.0)
        })
    };
    if missing.iter().sum::<f32>() <= f32::EPSILON {
        return turns_until_action(state, player);
    }
    let production_total = production.iter().sum::<f32>();
    let ratios = state.trade_ratios(player);
    let expected_rolls = missing
        .iter()
        .enumerate()
        .map(|(resource, count)| {
            *count * 36.0
                / (production[resource] + production_total / ratios[resource] as f32 + 0.65)
        })
        .sum::<f32>();
    turns_until_action(state, player)
        + expected_rolls / state.board.num_players.max(1) as f32
        + roads_required as f32 * 0.08
}

fn expansion_site_survival(
    state: &GameState,
    player: u8,
    vertex: usize,
    route_maps: &[Vec<u8>],
    arrival_scores: &[Vec<f32>],
) -> f32 {
    let own_distance = route_maps[player as usize][vertex];
    let own_arrival = arrival_scores[player as usize][own_distance as usize];
    let opponent_arrival = (0..state.board.num_players)
        .filter(|candidate| *candidate != player)
        .filter_map(|candidate| {
            let distance = route_maps[candidate as usize][vertex];
            (distance != u8::MAX
                && distance <= state.players[candidate as usize].roads_left
                && state.players[candidate as usize].settlements_left > 0)
                .then(|| arrival_scores[candidate as usize][distance as usize])
        })
        .fold(f32::INFINITY, f32::min);
    if !opponent_arrival.is_finite() {
        return 0.985;
    }
    if own_arrival <= 0.01 {
        return 0.995;
    }
    // Arrival is measured in full table rotations and includes the resources
    // needed for every road plus the settlement itself. This makes a rival
    // who can place on its current turn a near-certain denial, while a merely
    // adjacent but resource-starved road is no longer treated as an equal race.
    sigmoid((opponent_arrival - own_arrival) * 1.35 - 0.10).clamp(0.01, 0.995)
}

fn expansion_arrival_scores(
    state: &GameState,
    observer: Option<u8>,
    exact_rival_hands: bool,
) -> Vec<Vec<f32>> {
    (0..state.board.num_players)
        .map(|player| {
            let production = production_pips(state, player);
            (0..=state.players[player as usize].roads_left)
                .map(|roads| {
                    expansion_arrival_score(
                        state,
                        player,
                        roads,
                        exact_rival_hands || observer == Some(player),
                        &production,
                    )
                })
                .collect()
        })
        .collect()
}

fn expansion_option_value_with_routes(
    state: &GameState,
    player: u8,
    route_maps: &[Vec<u8>],
) -> ExpansionOption {
    let resource_weights = dynamic_resource_weights(state, player);
    expansion_option_value_with_routes_and_weights(
        state,
        player,
        route_maps,
        &resource_weights,
        Some(player),
        true,
        None,
    )
}

fn expansion_option_value_with_routes_and_weights(
    state: &GameState,
    player: u8,
    route_maps: &[Vec<u8>],
    resource_weights: &[f32; 5],
    observer: Option<u8>,
    exact_rival_hands: bool,
    prepared_arrivals: Option<&[Vec<f32>]>,
) -> ExpansionOption {
    if state.players[player as usize].settlements_left == 0 {
        return ExpansionOption::default();
    }
    // Compute each player's route map once. The earlier implementation ran a
    // fresh Dijkstra for every (candidate vertex, opponent) pair, which made a
    // leaf evaluation dozens of times more expensive than the search itself.
    let distances = &route_maps[player as usize];
    let owned_arrivals;
    let arrival_scores = if let Some(prepared) = prepared_arrivals {
        prepared
    } else {
        owned_arrivals = expansion_arrival_scores(state, observer, exact_rival_hands);
        &owned_arrivals
    };
    let mut best = ExpansionOption::default();
    let mut top = [0.0_f32; 3];
    let mut option_count = 0u8;
    for (vertex, &distance) in distances
        .iter()
        .enumerate()
        .take(state.board.vertices.len())
    {
        if !settlement_vertex_open(state, vertex) {
            continue;
        }
        if distance == u8::MAX || distance > state.players[player as usize].roads_left {
            continue;
        }
        let survival = expansion_site_survival(state, player, vertex, route_maps, arrival_scores);
        let site = vertex_value_with_weights(state, vertex as u8, resource_weights, player);
        let road_cost = distance as f32 * 1.45;
        let value = survival * (site + 5.4) / (1.0 + road_cost * 0.34);
        option_count = option_count.saturating_add(1);
        if value > top[0] {
            top[2] = top[1];
            top[1] = top[0];
            top[0] = value;
        } else if value > top[1] {
            top[2] = top[1];
            top[1] = value;
        } else if value > top[2] {
            top[2] = value;
        }
        if value > best.value {
            best = ExpansionOption {
                vertex: Some(vertex as u8),
                roads_required: distance,
                value,
                survival_probability: survival,
                portfolio_value: 0.0,
                option_count: 0,
            };
        }
    }
    best.portfolio_value = top[1] * 0.55 + top[2] * 0.30;
    best.option_count = option_count.min(3);
    best
}

pub fn expansion_option_value(state: &GameState, player: u8) -> ExpansionOption {
    let route_maps = all_route_maps(state);
    expansion_option_value_with_routes(state, player, &route_maps)
}

pub fn longest_road_outlook(state: &GameState, player: u8) -> TrophyOutlook {
    let own = state.longest_road_length(player) as f32;
    let best_other = (0..state.board.num_players)
        .filter(|candidate| *candidate != player)
        .map(|candidate| state.longest_road_length(candidate) as f32)
        .fold(0.0, f32::max);
    let threshold = 5.0_f32.max(best_other + 1.0);
    let missing = (threshold - own).max(0.0);
    let acquire = if state.longest_road_holder == Some(player) {
        1.0
    } else {
        sigmoid((own - threshold + 0.5) * 1.25)
    };
    let retain = sigmoid((own - best_other - 0.5) * 1.1);
    TrophyOutlook {
        acquire,
        retain,
        additional_cost: missing * 2.0,
    }
}

pub fn largest_army_outlook(state: &GameState, player: u8) -> TrophyOutlook {
    let own = state.players[player as usize].played_knights as f32;
    let ready = state.players[player as usize].development[0]
        .saturating_sub(state.players[player as usize].bought_development[0])
        as f32;
    let best_other = (0..state.board.num_players)
        .filter(|candidate| *candidate != player)
        .map(|candidate| state.players[candidate as usize].played_knights as f32)
        .fold(0.0, f32::max);
    let threshold = 3.0_f32.max(best_other + 1.0);
    let potential = own + ready;
    let remaining_knights = state.development_deck[0] as f32;
    let missing = (threshold - potential).max(0.0);
    let deck_support = if missing <= 0.0 {
        1.0
    } else {
        (remaining_knights / (missing * state.board.num_players as f32 + 1.0)).clamp(0.0, 1.0)
    };
    TrophyOutlook {
        acquire: if state.largest_army_holder == Some(player) {
            1.0
        } else {
            sigmoid((potential - threshold + 0.4) * 1.35) * deck_support
        },
        retain: sigmoid((potential - best_other - 0.4) * 1.1),
        additional_cost: missing * 3.0,
    }
}

fn progress_card_utility(
    state: &GameState,
    player: u8,
    card: usize,
    expansion: ExpansionOption,
) -> f32 {
    let held = state.players[player as usize].development[card] as f32;
    if held <= 0.0 {
        return 0.0;
    }
    let congestion = 1.0 / (1.0 + (held - 1.0).max(0.0) * 0.55);
    let base = match card {
        2 => 0.55 + expansion.value.min(4.0) * 0.16,
        3 => {
            let hand = state.players[player as usize].resources;
            let nearest = BUILD_COSTS
                .iter()
                .map(|cost| total(&deficit(&hand, cost)))
                .min()
                .unwrap_or(2);
            0.65 + (2.0 - nearest.min(2) as f32) * 0.28
        }
        4 => {
            let opponents = state
                .players
                .iter()
                .enumerate()
                .filter(|(index, _)| *index != player as usize)
                .map(|(_, opponent)| opponent.resource_total() as f32)
                .sum::<f32>();
            0.48 + opponents * 0.035
        }
        _ => 0.0,
    };
    held.min(1.0) * base + (held - 1.0).max(0.0) * base * 0.28 * congestion
}

fn development_utility(state: &GameState, player: u8, expansion: ExpansionOption) -> f32 {
    let player_state = &state.players[player as usize];
    let cards = &player_state.development;
    let army = largest_army_outlook(state, player);
    let knights = cards[0] as f32;
    let knight_utility = knights.min(1.0) * (0.28 + army.acquire * 1.15)
        + (knights - 1.0).max(0.0) * (0.12 + army.acquire * 0.24);
    // Victory-point cards are already included exactly once by
    // PlayerState::victory_points and must not receive another inventory term.
    let raw = knight_utility
        + progress_card_utility(state, player, 2, expansion)
        + progress_card_utility(state, player, 3, expansion)
        + progress_card_utility(state, player, 4, expansion);
    let action_cards = [0, 2, 3, 4].into_iter().map(|card| cards[card]).sum::<u8>();
    let newly_bought = [0, 2, 3, 4]
        .into_iter()
        .map(|card| player_state.bought_development[card])
        .sum::<u8>();
    let usable_horizon = state
        .victory_target
        .saturating_sub(player_state.victory_points())
        .saturating_add(1);
    let queue = action_cards.saturating_sub(1) as f32;
    let horizon_excess = action_cards.saturating_sub(usable_horizon) as f32;
    // Only one action card can be played per turn, and cards bought this turn
    // are not immediately playable. Price that queue across card types rather
    // than granting every first Knight/YOP/Monopoly its full standalone value.
    raw / (1.0 + queue * 0.20 + newly_bought as f32 * 0.16 + horizon_excess * 0.30)
}

fn speculative_road_penalty(state: &GameState, player: u8, road: TrophyOutlook) -> f32 {
    let roads_built = 15_u8.saturating_sub(state.players[player as usize].roads_left);
    let buildings = state
        .buildings
        .iter()
        .filter(|building| building.is_some_and(|piece| piece.player() == player))
        .count() as u8;
    // Two setup roads are free of strategic debt. Thereafter, allow a modest
    // route buffer per realized building and a small award allowance; long
    // speculative chains must justify the cards they consumed.
    let award_allowance = if state.longest_road_holder == Some(player) {
        3
    } else if road.acquire * road.retain >= 0.55 {
        2
    } else {
        0
    };
    let supported = buildings.saturating_add(2).saturating_add(award_allowance);
    let excess = roads_built.saturating_sub(supported) as f32;
    excess * 0.48 + excess * excess * 0.035
}

/// Expected marginal utility of the next development-card purchase. This is
/// deck-aware and prices inventory congestion rather than granting every
/// unplayed card a fixed additive bonus.
pub fn marginal_development_value(state: &GameState, player: u8) -> f32 {
    let remaining = state.development_deck.iter().copied().sum::<u8>();
    if remaining == 0 {
        return 0.0;
    }
    let baseline = strategic_utility(state, player);
    let mut expected = 0.0;
    for card in 0..5 {
        let count = state.development_deck[card];
        if count == 0 {
            continue;
        }
        let mut next = state.clone();
        next.players[player as usize].development[card] += 1;
        next.players[player as usize].bought_development[card] += 1;
        expected += count as f32 / remaining as f32 * (strategic_utility(&next, player) - baseline);
    }
    expected.max(0.0)
}

fn canonical_observed_development_deck(state: &GameState, player: u8) -> [u8; 5] {
    const TOTALS: [u8; 5] = [14, 5, 2, 2, 2];
    let deck_total = state.development_deck.iter().copied().sum::<u8>();
    let own = &state.players[player as usize];
    let possible = std::array::from_fn::<u8, 5, _>(|card| {
        TOTALS[card]
            .saturating_sub(state.played_development[card])
            .saturating_sub(own.development[card])
    });
    let possible_total = possible.iter().copied().sum::<u8>().max(1);
    let mut canonical = [0u8; 5];
    let mut remainders = Vec::with_capacity(5);
    let mut assigned = 0u8;
    for card in 0..5 {
        let numerator = possible[card] as u16 * deck_total as u16;
        canonical[card] = (numerator / possible_total as u16) as u8;
        assigned = assigned.saturating_add(canonical[card]);
        remainders.push((card, numerator % possible_total as u16));
    }
    remainders.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    for (card, _) in remainders {
        if assigned >= deck_total {
            break;
        }
        if canonical[card] < possible[card] {
            canonical[card] += 1;
            assigned += 1;
        }
    }
    canonical
}

/// Information-set-safe marginal value of buying a development card.
///
/// The player knows the public played pile, their own exact cards, and the
/// number of cards left. They do not know the exact identities in opponents'
/// hands or the deck. By exchangeability, the next unseen card can be valued
/// from the remaining public/own pool without reading the determinization.
pub fn observed_marginal_development_value(state: &GameState, player: u8) -> f32 {
    const TOTALS: [u8; 5] = [14, 5, 2, 2, 2];
    if state.development_deck.iter().copied().sum::<u8>() == 0 {
        return 0.0;
    }
    let own = &state.players[player as usize];
    let possible = std::array::from_fn::<u8, 5, _>(|card| {
        TOTALS[card]
            .saturating_sub(state.played_development[card])
            .saturating_sub(own.development[card])
    });
    let possible_total = possible.iter().copied().sum::<u8>().max(1);
    let mut observed = state.clone();
    observed.development_deck = canonical_observed_development_deck(state, player);
    let baseline = strategic_utility(&observed, player);
    let mut expected = 0.0;
    for (card, &possible_count) in possible.iter().enumerate() {
        if possible_count == 0 {
            continue;
        }
        let mut next = observed.clone();
        next.players[player as usize].development[card] += 1;
        next.players[player as usize].bought_development[card] += 1;
        expected += possible_count as f32 / possible_total as f32
            * (strategic_utility(&next, player) - baseline);
    }
    expected.max(0.0)
}

fn expected_build_tempo(state: &GameState, player: u8) -> f32 {
    let hand = &state.players[player as usize].resources;
    let production = production_pips(state, player);
    let ratios = state.trade_ratios(player);
    BUILD_COSTS
        .iter()
        .enumerate()
        .map(|(kind, cost)| {
            let missing = deficit(hand, cost);
            let eta = missing
                .iter()
                .enumerate()
                .map(|(index, amount)| {
                    if *amount == 0 {
                        0.0
                    } else {
                        *amount as f32 * 36.0
                            / (production[index]
                                + production.iter().sum::<f32>() / ratios[index] as f32
                                + 0.75)
                    }
                })
                .sum::<f32>();
            [0.32, 1.25, 1.18, 0.68][kind] / (1.0 + eta / 18.0)
        })
        .fold(0.0, f32::max)
}

/// Race-to-win utility used by search and exact tactical endpoint comparison.
/// It intentionally is not exposed as a calibrated win probability.
fn strategic_utility_with_routes_and_knowledge(
    state: &GameState,
    player: u8,
    route_maps: &[Vec<u8>],
    exact_rival_hands: bool,
    prepared_arrivals: Option<&[Vec<f32>]>,
) -> f32 {
    let player_state = &state.players[player as usize];
    let victory = player_state.victory_points() as f32;
    let production = production_pips(state, player);
    let weights = dynamic_resource_weights(state, player);
    let weighted_production = production
        .iter()
        .enumerate()
        .map(|(index, pips)| *pips * weights[index])
        .sum::<f32>();
    let distinct_numbers = state
        .buildings
        .iter()
        .enumerate()
        .filter(|(_, building)| building.is_some_and(|piece| piece.player() == player))
        .flat_map(|(vertex, _)| state.board.vertices[vertex].adjacent_hexes.iter())
        .map(|hex| state.board.hexes[*hex as usize].number)
        .filter(|number| *number != 0)
        .fold(0u16, |mask, number| mask | (1u16 << number))
        .count_ones() as f32;
    let resource_diversity = production.iter().filter(|pips| **pips > 0.0).count() as f32;
    let hand_value = hand_utility_with_weights(&player_state.resources, &weights);
    let expansion = expansion_option_value_with_routes_and_weights(
        state,
        player,
        route_maps,
        &weights,
        Some(player),
        exact_rival_hands,
        prepared_arrivals,
    );
    let road = longest_road_outlook(state, player);
    let army = largest_army_outlook(state, player);
    let port_flexibility = state
        .trade_ratios(player)
        .iter()
        .map(|ratio| (4 - *ratio) as f32)
        .sum::<f32>();
    let points_to_win = state
        .victory_target
        .saturating_sub(player_state.victory_points()) as f32;
    let race_urgency = 1.0 + (4.0 - points_to_win).max(0.0) * 0.18;

    victory * 7.4
        + weighted_production * 0.17
        + distinct_numbers * 0.06
        + resource_diversity * 0.09
        + hand_value * 0.48
        + expected_build_tempo(state, player) * 1.15
        + expansion.value * 0.32
        // Portfolio excludes the best site already counted in `expansion.value`.
        + expansion.portfolio_value * 0.22
        + (road.acquire * road.retain) * 3.2 * race_urgency
        + (army.acquire * army.retain) * 3.2 * race_urgency
        + development_utility(state, player, expansion) * 0.72
        + port_flexibility * 0.07
        - expected_discard_loss(state, player) * 2.4
        - speculative_road_penalty(state, player, road)
}

fn strategic_utility_with_routes(state: &GameState, player: u8, route_maps: &[Vec<u8>]) -> f32 {
    strategic_utility_with_routes_and_knowledge(state, player, route_maps, true, None)
}

pub fn strategic_utility(state: &GameState, player: u8) -> f32 {
    let route_maps = all_route_maps(state);
    strategic_utility_with_routes(state, player, &route_maps)
}

/// Observation-safe race value for another player. It deliberately uses only
/// board production, public piece/trophy state, and public hand/development
/// totals. Plan priors may compare against this value without learning a
/// sampled opponent resource identity that the acting player cannot observe.
pub(crate) fn public_strategic_utility(state: &GameState, player: u8) -> f32 {
    let public = &state.players[player as usize];
    let production = production_pips(state, player);
    let weighted_production = production
        .iter()
        .zip(BASE_RESOURCE_WEIGHTS)
        .map(|(pips, weight)| *pips * weight)
        .sum::<f32>();
    let road = longest_road_outlook(state, player);
    let best_other_army = (0..state.board.num_players)
        .filter(|candidate| *candidate != player)
        .map(|candidate| state.players[candidate as usize].played_knights as f32)
        .fold(0.0, f32::max);
    let public_development_total = public.development.iter().copied().sum::<u8>() as f32;
    // Card identities are private. Estimate the player's Knight inventory
    // from the public development-card total instead of consulting the sampled
    // determinization. This keeps opponent priors observation-correct.
    let expected_held_knights = public_development_total * 14.0 / 25.0;
    let army_threshold = 3.0_f32.max(best_other_army + 1.0);
    let army_potential = public.played_knights as f32 + expected_held_knights;
    let public_army_acquire = if public.has_largest_army {
        1.0
    } else {
        sigmoid((army_potential - army_threshold + 0.35) * 1.05)
    };
    let public_army_retain = sigmoid((army_potential - best_other_army - 0.45) * 0.95);
    let route_maps = all_route_maps(state);
    let expansion = expansion_option_value_with_routes_and_weights(
        state,
        player,
        &route_maps,
        &BASE_RESOURCE_WEIGHTS,
        None,
        false,
        None,
    );
    let hand_total = public.resource_total() as f32;
    let points_to_win = state
        .victory_target
        .saturating_sub(public.public_victory_points) as f32;
    let urgency = 1.0 + (4.0 - points_to_win).max(0.0) * 0.18;
    public.public_victory_points as f32 * 7.4
        + weighted_production * 0.17
        + hand_total.min(12.0) * 0.10
        + public_development_total * 0.22
        + expansion.value * 0.42
        + road.acquire * road.retain * 3.2 * urgency
        + public_army_acquire * public_army_retain * 3.2 * urgency
}

fn evaluate_with_precomputed_routes(
    state: &GameState,
    route_maps: &[Vec<u8>],
    arrival_scores: &[Vec<f32>],
) -> [f32; 4] {
    let count = state.board.num_players as usize;
    let mut result = [0.0; 4];
    let mut logits = [f32::NEG_INFINITY; 4];
    let mut maximum = f32::NEG_INFINITY;
    for (player, logit) in logits.iter_mut().enumerate().take(count) {
        *logit = strategic_utility_with_routes_and_knowledge(
            state,
            player as u8,
            route_maps,
            true,
            Some(arrival_scores),
        );
        maximum = maximum.max(*logit);
    }
    let mut sum = 0.0;
    for (player, value) in result.iter_mut().enumerate().take(count) {
        *value = ((logits[player] - maximum) * 0.50).exp();
        sum += *value;
    }
    if sum > 0.0 {
        for value in result.iter_mut().take(count) {
            *value /= sum;
        }
    }
    if let Some(learned) = crate::model::learned_value(state) {
        for player in 0..count {
            result[player] = result[player] * 0.38 + learned[player] * 0.62;
        }
    }
    result
}

/// Returns a normalized relative strategic-value vector.
///
/// Terminal states are exact one-hot wins. Nonterminal values are useful for
/// multiplayer backup and ranking, but are deliberately not described as
/// calibrated win probabilities in the product UI.
pub fn evaluate(state: &GameState) -> [f32; 4] {
    if let Some(winner) = state.winner() {
        let mut result = [0.0; 4];
        result[winner as usize] = 1.0;
        return result;
    }
    let route_maps = all_route_maps(state);
    let arrival_scores = expansion_arrival_scores(state, None, true);
    evaluate_with_precomputed_routes(state, &route_maps, &arrival_scores)
}

#[cfg(feature = "benchmark-profile")]
#[derive(Clone, Copy, Debug, Default)]
pub struct EvaluateProfile {
    pub route_map_nanos: u64,
    pub arrival_score_nanos: u64,
    pub remaining_utility_nanos: u64,
}

#[cfg(feature = "benchmark-profile")]
pub fn evaluate_profiled(state: &GameState) -> ([f32; 4], EvaluateProfile) {
    use std::time::Instant;

    if let Some(winner) = state.winner() {
        let mut result = [0.0; 4];
        result[winner as usize] = 1.0;
        return (result, EvaluateProfile::default());
    }
    let started = Instant::now();
    let route_maps = all_route_maps(state);
    let route_map_nanos = started.elapsed().as_nanos() as u64;
    let started = Instant::now();
    let arrival_scores = expansion_arrival_scores(state, None, true);
    let arrival_score_nanos = started.elapsed().as_nanos() as u64;
    let started = Instant::now();
    let result = evaluate_with_precomputed_routes(state, &route_maps, &arrival_scores);
    let remaining_utility_nanos = started.elapsed().as_nanos() as u64;
    (
        result,
        EvaluateProfile {
            route_map_nanos,
            arrival_score_nanos,
            remaining_utility_nanos,
        },
    )
}

pub(crate) fn vertex_value(state: &GameState, vertex: u8, player: u8) -> f32 {
    let weights = dynamic_resource_weights(state, player);
    vertex_value_with_weights(state, vertex, &weights, player)
}

fn vertex_value_with_weights(state: &GameState, vertex: u8, weights: &[f32; 5], player: u8) -> f32 {
    let mut value: f32 = 0.0;
    let mut numbers = 0u16;
    let mut resources = 0u8;
    for hex in &state.board.vertices[vertex as usize].adjacent_hexes {
        let tile = &state.board.hexes[*hex as usize];
        let Some(resource) = tile.resource else {
            continue;
        };
        let robber_factor = if *hex == state.robber_hex { 0.30 } else { 1.0 };
        value += PIPS[tile.number as usize] * weights[resource.index()] * robber_factor;
        numbers |= 1 << tile.number;
        resources |= 1 << resource.index();
    }
    value += numbers.count_ones() as f32 * 0.16;
    value += resources.count_ones() as f32 * 0.22;
    if state.board.vertices[vertex as usize].port.is_some() {
        value += state.trade_ratios(player).iter().all(|ratio| *ratio == 4) as u8 as f32 * 0.7;
    }
    value
}

pub(crate) fn city_value(state: &GameState, vertex: u8, player: u8) -> f32 {
    let weights = dynamic_resource_weights(state, player);
    state.board.vertices[vertex as usize]
        .adjacent_hexes
        .iter()
        .filter_map(|hex| {
            let tile = &state.board.hexes[*hex as usize];
            tile.resource.map(|resource| {
                PIPS[tile.number as usize]
                    * weights[resource.index()]
                    * if *hex == state.robber_hex { 0.22 } else { 1.0 }
            })
        })
        .sum()
}

pub(crate) fn robber_denial(state: &GameState, hex: u8, actor: u8) -> f32 {
    if hex == state.robber_hex {
        return -100.0;
    }
    let public_logits = (0..state.board.num_players)
        .map(|player| public_strategic_utility(state, player))
        .collect::<Vec<_>>();
    let maximum = public_logits
        .iter()
        .copied()
        .fold(f32::NEG_INFINITY, f32::max);
    let denominator = public_logits
        .iter()
        .map(|logit| ((*logit - maximum) * 0.50).exp())
        .sum::<f32>()
        .max(f32::EPSILON);
    let mut score = 0.0;
    for (vertex, building) in state.buildings.iter().enumerate() {
        let Some(building) = building else {
            continue;
        };
        if !state.board.vertices[vertex].adjacent_hexes.contains(&hex) {
            continue;
        }
        let owner = building.player();
        let multiplier = building.production_multiplier() as f32;
        let tile = &state.board.hexes[hex as usize];
        let resource_weight = tile
            .resource
            .map(|resource| {
                if owner == actor {
                    dynamic_resource_weights(state, owner)[resource.index()]
                } else {
                    let port_ratio = state.trade_ratios(owner)[resource.index()];
                    BASE_RESOURCE_WEIGHTS[resource.index()] * (1.0 + (4 - port_ratio) as f32 * 0.12)
                }
            })
            .unwrap_or(0.0);
        let threat = ((public_logits[owner as usize] - maximum) * 0.50).exp() / denominator;
        let denial =
            PIPS[tile.number as usize] * multiplier * resource_weight * (1.0 + threat * 1.4);
        if owner == actor {
            score -= denial * 2.0;
        } else {
            score += denial;
        }
    }
    score
}

pub(crate) fn road_frontier_value(state: &GameState, edge: u8, actor: u8) -> f32 {
    if state.roads.get(edge as usize).is_none_or(Option::is_some) {
        return 0.0;
    }
    let observed_option = |position: &GameState| {
        let route_maps = all_route_maps(position);
        let weights = dynamic_resource_weights(position, actor);
        expansion_option_value_with_routes_and_weights(
            position,
            actor,
            &route_maps,
            &weights,
            Some(actor),
            false,
            None,
        )
    };
    // Road priors are selected from the acting player's information set.
    // Rival exact resource identities belong only in leaf determinizations,
    // never in the simulated opponent's action policy.
    let before = observed_option(state);
    let mut after = state.clone();
    after.roads[edge as usize] = Some(actor);
    let option = observed_option(&after);
    // `ExpansionOption::value` already prices road distance; adding another
    // fixed reward for the same distance reduction double-counted every link
    // in a speculative chain.
    let progress = (option.value - before.value).clamp(0.0, 2.2);
    let road_before = longest_road_outlook(state, actor);
    let road_after = longest_road_outlook(&after, actor);
    let trophy = ((road_after.acquire * road_after.retain)
        - (road_before.acquire * road_before.retain))
        .max(0.0)
        * 3.0
        / (1.0 + road_after.additional_cost * 0.32);
    progress + trophy
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{GameState, Phase, Resource, SETTLEMENT_COST};

    use super::{
        all_route_maps, expansion_arrival_scores, expansion_site_survival, expected_discard_loss,
        marginal_development_value, production_pips, public_strategic_utility, road_frontier_value,
        robber_denial, rolls_before_next_spend,
    };

    fn after_setup(seed: u64, players: u8) -> GameState {
        let mut state = GameState::standard(seed, players);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state
    }

    #[test]
    fn twelve_card_waiting_hand_has_large_nonlinear_discard_risk() {
        let mut state = after_setup(47, 4);
        state.phase = Phase::Main;
        state.current_player = 1;
        state.players[0].resources = [3, 3, 2, 2, 2];
        let twelve = expected_discard_loss(&state, 0);
        state.players[0].resources = [2, 2, 1, 2, 1];
        let eight = expected_discard_loss(&state, 0);
        state.players[0].resources = [1, 1, 1, 2, 1];
        let safe = expected_discard_loss(&state, 0);
        assert!(twelve > eight * 1.8);
        assert!(eight > 0.0);
        assert_eq!(safe, 0.0);
    }

    #[test]
    fn discard_risk_respects_the_configured_limit() {
        let mut state = after_setup(48, 4);
        state.card_discard_limit = 9;
        state.current_player = 0;
        state.phase = Phase::Main;

        state.players[0].resources = [9, 0, 0, 0, 0];
        assert_eq!(expected_discard_loss(&state, 0), 0.0);

        state.players[0].resources = [10, 0, 0, 0, 0];
        assert!(expected_discard_loss(&state, 0) > 0.0);
    }

    #[test]
    fn discard_risk_counts_the_full_orbit_and_the_players_next_roll() {
        let mut state = after_setup(49, 4);
        state.current_player = 1;
        state.phase = Phase::PreRoll;
        assert_eq!(rolls_before_next_spend(&state, 0), 4);
        state.phase = Phase::Main;
        assert_eq!(rolls_before_next_spend(&state, 0), 3);
        state.current_player = 0;
        state.phase = Phase::PreRoll;
        assert_eq!(rolls_before_next_spend(&state, 0), 1);
        state.phase = Phase::Main;
        assert_eq!(rolls_before_next_spend(&state, 0), 0);
    }

    #[test]
    fn additional_unplayed_progress_cards_have_diminishing_value() {
        let mut empty = after_setup(53, 3);
        empty.phase = Phase::Main;
        empty.current_player = 0;
        let first = marginal_development_value(&empty, 0);
        empty.players[0].development[4] = 2;
        let congested = marginal_development_value(&empty, 0);
        assert!(first > congested);
    }

    #[test]
    fn action_card_queue_diminishes_value_across_card_types() {
        let mut empty = after_setup(57, 4);
        empty.phase = Phase::Main;
        empty.current_player = 0;
        let first = marginal_development_value(&empty, 0);
        empty.players[0].development[0] = 1;
        empty.players[0].development[2] = 1;
        empty.players[0].development[3] = 1;
        empty.players[0].bought_development[3] = 1;
        let queued = marginal_development_value(&empty, 0);

        assert!(first > queued);
    }

    #[test]
    fn roads_are_distinguished_by_reachable_expansion_value() {
        let mut state = after_setup(59, 4);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources[Resource::Lumber.index()] = 1;
        state.players[0].resources[Resource::Brick.index()] = 1;
        let legal = state
            .legal_actions()
            .into_iter()
            .filter_map(|action| match action {
                colonist_catan_core::Action::BuildRoad { edge } => Some(edge),
                _ => None,
            })
            .collect::<Vec<_>>();
        let mut values = legal
            .into_iter()
            .map(|edge| road_frontier_value(&state, edge, 0))
            .collect::<Vec<_>>();
        values.sort_by(f32::total_cmp);
        assert!(values.len() >= 2);
        assert!(values.last().unwrap() > values.first().unwrap());
    }

    #[test]
    fn opponent_public_value_and_robber_prior_ignore_hidden_card_identity() {
        let mut first = after_setup(67, 4);
        first.phase = Phase::Main;
        first.current_player = 0;
        let mut second = first.clone();
        first.players[1].resources = [4, 0, 0, 0, 0];
        second.players[1].resources = [0, 0, 0, 4, 0];
        first.players[1].development = [2, 0, 0, 0, 0];
        second.players[1].development = [0, 2, 0, 0, 0];
        assert_eq!(
            public_strategic_utility(&first, 1),
            public_strategic_utility(&second, 1),
        );
        for hex in 0..first.board.hexes.len() as u8 {
            if hex == first.robber_hex {
                continue;
            }
            assert_eq!(
                robber_denial(&first, hex, 0),
                robber_denial(&second, hex, 0),
            );
        }
    }

    #[test]
    fn public_bank_depletion_reduces_near_term_production_value() {
        let mut state = after_setup(71, 3);
        state.bank_is_public = true;
        let resource = Resource::ALL
            .into_iter()
            .max_by(|left, right| {
                production_pips(&state, 0)[left.index()]
                    .total_cmp(&production_pips(&state, 0)[right.index()])
            })
            .expect("a resource");
        let supplied = production_pips(&state, 0)[resource.index()];
        state.bank[resource.index()] = 0;
        let depleted = production_pips(&state, 0)[resource.index()];
        assert!(depleted < supplied * 0.3);

        state.bank_is_public = false;
        let hidden = production_pips(&state, 0)[resource.index()];
        assert!(
            hidden > depleted,
            "a hidden bank must not leak its sampled composition into policy features",
        );
    }

    #[test]
    fn contested_site_survival_prices_turn_order_and_complete_build_resources() {
        let mut state = GameState::standard(73, 3);
        state.phase = Phase::Main;
        state.current_player = 1;
        state.buildings.fill(None);
        state.roads.fill(None);
        let (vertex, first, second) = state
            .board
            .vertices
            .iter()
            .enumerate()
            .find_map(|(vertex, candidate)| {
                (candidate.adjacent_edges.len() >= 2).then(|| {
                    (
                        vertex,
                        candidate.adjacent_edges[0],
                        candidate.adjacent_edges[1],
                    )
                })
            })
            .expect("standard topology has a contested vertex");
        state.roads[first as usize] = Some(0);
        state.roads[second as usize] = Some(1);
        state.players[0].roads_left = 14;
        state.players[1].roads_left = 14;
        state.players[0].settlements_left = 5;
        state.players[1].settlements_left = 5;
        state.players[0].resources = SETTLEMENT_COST;
        state.players[1].resources = SETTLEMENT_COST;
        let route_maps = all_route_maps(&state);
        let immediate_rival = expansion_site_survival(
            &state,
            0,
            vertex,
            &route_maps,
            &expansion_arrival_scores(&state, Some(0), true),
        );

        state.players[1].resources = [0; 5];
        let route_maps = all_route_maps(&state);
        let starved_rival = expansion_site_survival(
            &state,
            0,
            vertex,
            &route_maps,
            &expansion_arrival_scores(&state, Some(0), true),
        );
        assert!(
            immediate_rival < 0.2,
            "a rival able to settle the site right now should nearly erase the option",
        );
        assert!(
            starved_rival > immediate_rival + 0.35,
            "an adjacent but resource-starved rival must not be treated as an immediate denial",
        );
    }
}
