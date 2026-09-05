use colonist_catan_core::{
    Building, CITY_COST, DEVELOPMENT_COST, GameState, ROAD_COST, ResourceHand, SETTLEMENT_COST,
};

/// Fixed-point units per public/hidden victory point in the compact GPU rollout
/// cutoff. The score is deliberately not a win probability; it only orders
/// unfinished rollouts after exact terminal evidence.
pub const ROLLOUT_CUTOFF_SCALE: i32 = 1_024;

const NUMBER_PIPS: [i32; 13] = [0, 0, 1, 2, 3, 4, 5, 0, 5, 4, 3, 2, 1];

fn production_units(state: &GameState, player: u8) -> [i32; 5] {
    let mut production = [0i32; 5];
    for (vertex, building) in state.buildings.iter().enumerate() {
        let Some(building) = building.filter(|building| building.player() == player) else {
            continue;
        };
        let multiplier = i32::from(building.production_multiplier());
        for hex in &state.board.vertices[vertex].adjacent_hexes {
            let tile = &state.board.hexes[*hex as usize];
            let Some(resource) = tile.resource else {
                continue;
            };
            let active_eighths = if *hex == state.robber_hex { 1 } else { 8 };
            production[resource.index()] += NUMBER_PIPS[tile.number as usize]
                * multiplier
                * active_eighths;
        }
    }
    production
}

fn remaining_missing_after_maritime(
    hand: &ResourceHand,
    ratios: &ResourceHand,
    bank: &ResourceHand,
    cost: &ResourceHand,
) -> i32 {
    let mut missing = 0i32;
    let mut capacity = 0i32;
    let mut unavailable = 0i32;
    for resource in 0..5 {
        let required = cost[resource];
        let reserved = hand[resource].min(required);
        missing += i32::from(required - reserved);
        unavailable += i32::from((required - reserved).saturating_sub(bank[resource]));
        capacity += i32::from(hand[resource] - reserved)
            / i32::from(ratios[resource].max(1));
    }
    (missing - capacity).max(unavailable)
}

fn build_access(weight: i32, enabled: bool, hand: &ResourceHand, ratios: &ResourceHand, bank: &ResourceHand, cost: &ResourceHand) -> i32 {
    if !enabled {
        return 0;
    }
    weight / (1 + remaining_missing_after_maritime(hand, ratios, bank, cost))
}

fn distance_rule_open(state: &GameState, vertex: usize) -> bool {
    state.buildings[vertex].is_none()
        && state.board.vertices[vertex]
            .adjacent_vertices
            .iter()
            .all(|neighbor| state.buildings[*neighbor as usize].is_none())
}

fn network_touches_vertex(state: &GameState, player: u8, vertex: usize) -> bool {
    state.buildings[vertex].is_some_and(|building| building.player() == player)
        || state.board.vertices[vertex]
            .adjacent_edges
            .iter()
            .any(|edge| state.roads[*edge as usize] == Some(player))
}

fn route_distance_at_most_one(state: &GameState, player: u8, vertex: usize) -> Option<i32> {
    if state.board.vertices[vertex]
        .adjacent_edges
        .iter()
        .any(|edge| state.roads[*edge as usize] == Some(player))
    {
        return Some(0);
    }
    for edge in &state.board.vertices[vertex].adjacent_edges {
        if state.roads[*edge as usize].is_some() {
            continue;
        }
        let endpoints = state.board.edges[*edge as usize].vertices;
        let other = endpoints
            .into_iter()
            .find(|endpoint| usize::from(*endpoint) != vertex)?;
        if state.buildings[other as usize]
            .is_some_and(|building| building.player() != player)
        {
            continue;
        }
        if network_touches_vertex(state, player, other as usize) {
            return Some(1);
        }
    }
    None
}

fn expansion_option_score(state: &GameState, player: u8) -> i32 {
    if state.players[usize::from(player)].settlements_left == 0 { return 0; }
    let mut best = 0i32;
    let existing_production = production_units(state, player);
    let mut missing_settlement_mask = 0u8;
    for (resource, production) in existing_production.iter().enumerate().take(4) {
        if *production == 0 {
            missing_settlement_mask |= 1 << resource;
        }
    }
    let closure_weight = if !state.player_trades_enabled || state.domestic_trade_disabled & (1 << player) != 0 {
        64
    } else {
        40
    };
    for vertex in 0..state.board.vertices.len() {
        if !distance_rule_open(state, vertex) {
            continue;
        }
        let Some(distance) = route_distance_at_most_one(state, player, vertex) else {
            continue;
        };
        if distance > i32::from(state.players[usize::from(player)].roads_left) { continue; }
        let mut pips = 0i32;
        let mut resource_mask = 0u8;
        for hex in &state.board.vertices[vertex].adjacent_hexes {
            let tile = &state.board.hexes[*hex as usize];
            let Some(resource) = tile.resource else {
                continue;
            };
            let active_eighths = if *hex == state.robber_hex { 1 } else { 8 };
            pips += NUMBER_PIPS[tile.number as usize] * active_eighths;
            resource_mask |= 1 << resource.index();
        }
        let port = i32::from(state.board.vertices[vertex].port.is_some()) * 16;
        let closure = (resource_mask & missing_settlement_mask).count_ones() as i32
            * closure_weight;
        let site = pips * 2
            + resource_mask.count_ones() as i32 * 18
            + closure
            + port;
        best = best.max(site / (distance + 1));
    }
    best.min(320)
}

fn settlement_independence(production: &[i32; 5], ratios: &ResourceHand) -> i32 {
    let best_productive_ratio = production
        .iter()
        .enumerate()
        .filter(|(_, units)| **units > 0)
        .map(|(resource, _)| ratios[resource].max(1))
        .min();
    let Some(best_productive_ratio) = best_productive_ratio else {
        return 0;
    };
    let effective_cards = SETTLEMENT_COST
        .iter()
        .enumerate()
        .map(|(resource, required)| {
            i32::from(*required)
                * i32::from(if production[resource] > 0 {
                    1
                } else {
                    best_productive_ratio
                })
        })
        .sum::<i32>();
    320 * 4 / effective_cards.max(4)
}

/// Compact, integer, parity-friendly strategic value for unfinished GPU
/// rollouts. Terminal results remain authoritative. This score supplies the
/// missing economic/position context when no rollout reaches the victory
/// target, especially in long 15-VP games.
pub fn rollout_cutoff_player_score(state: &GameState, player: u8) -> i32 {
    let player_state = &state.players[player as usize];
    let production = production_units(state, player);
    let production_total = production.iter().sum::<i32>();
    let resource_diversity = production.iter().filter(|units| **units > 0).count() as i32;
    let hand_total = i32::from(player_state.resource_total());
    let discard_excess = (hand_total - i32::from(state.card_discard_limit)).max(0);
    let ratios = state.trade_ratios(player);
    let has_settlement = state
        .buildings
        .contains(&Some(Building::Settlement(player)));
    let has_distance_open_site = (0..state.board.vertices.len())
        .any(|vertex| distance_rule_open(state, vertex)
            && route_distance_at_most_one(state, player, vertex) == Some(0));
    let development_available = state.development_deck.iter().copied().sum::<u8>() > 0;
    let development_inventory = player_state
        .development
        .iter()
        .enumerate()
        .filter(|(card, _)| *card != 1)
        .map(|(_, cards)| i32::from(*cards))
        .sum::<i32>();

    i32::from(player_state.victory_points()) * ROLLOUT_CUTOFF_SCALE
        // Production is kept in eighth-pip units so a currently blocked hex
        // retains a small future value without counting as current production.
        + production_total * 4
        + resource_diversity * 20
        + hand_total.min(12) * 10
        - discard_excess * 28
        + development_inventory * 28
        + i32::from(state.longest_road_length(player)) * 16
        + i32::from(player_state.played_knights) * 16
        + build_access(
            90,
            player_state.roads_left > 0,
            &player_state.resources,
            &ratios,
            &state.bank,
            &ROAD_COST,
        )
        + build_access(
            360,
            player_state.settlements_left > 0 && has_distance_open_site,
            &player_state.resources,
            &ratios,
            &state.bank,
            &SETTLEMENT_COST,
        )
        + build_access(
            320,
            player_state.cities_left > 0 && has_settlement,
            &player_state.resources,
            &ratios,
            &state.bank,
            &CITY_COST,
        )
        + build_access(
            180,
            development_available,
            &player_state.resources,
            &ratios,
            &state.bank,
            &DEVELOPMENT_COST,
        )
        + settlement_independence(&production, &ratios)
        + expansion_option_score(state, player)
}

pub fn rollout_cutoff_margin(state: &GameState, actor: u8) -> f32 {
    let actor_score = rollout_cutoff_player_score(state, actor);
    let best_opponent = (0..state.board.num_players)
        .filter(|player| *player != actor)
        .map(|player| rollout_cutoff_player_score(state, player))
        .max()
        .unwrap_or(0);
    (actor_score - best_opponent) as f32 / ROLLOUT_CUTOFF_SCALE as f32
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{GameState, Phase, Resource};

    use super::{rollout_cutoff_margin, rollout_cutoff_player_score};

    fn after_setup(seed: u64) -> GameState {
        let mut state = GameState::standard(seed, 2);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        state.bank = [19; 5];
        state
    }

    #[test]
    fn immediate_settlement_closure_improves_cutoff_value() {
        let mut base = after_setup(79_001);
        base.players[0].resources = [1, 1, 0, 1, 0];
        base.bank = [18, 18, 19, 18, 19];
        let mut closed = base.clone();
        closed.players[0].resources[Resource::Wool.index()] = 1;
        closed.bank[Resource::Wool.index()] = 18;
        assert!(
            rollout_cutoff_player_score(&closed, 0)
                > rollout_cutoff_player_score(&base, 0)
        );
    }

    #[test]
    fn strategic_margin_is_not_only_public_victory_points() {
        let mut state = after_setup(79_003);
        state.players[0].resources = [1, 1, 1, 1, 0];
        state.bank = [18, 18, 18, 18, 19];
        assert_eq!(state.players[0].victory_points(), state.players[1].victory_points());
        assert_ne!(rollout_cutoff_margin(&state, 0), 0.0);
    }
}
