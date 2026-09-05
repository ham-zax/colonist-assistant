use colonist_catan_core::{
    Action, CITY_COST, DEVELOPMENT_COST, GameState, ResourceHand, ROAD_COST, SETTLEMENT_COST,
};

/// Deterministic production/whole-batch proxy, not a stochastic first-passage ETA.
/// This function is mirrored in cuda/exact_eval.cu and checked by evaluator parity.
pub(crate) fn build_fundable_at_rolls(
    production: &[f32; 5],
    hand: &ResourceHand,
    ratios: &ResourceHand,
    cost: &ResourceHand,
    rolls: f32,
) -> bool {
    let mut missing = 0u32;
    let mut capacity = 0u32;
    for resource in 0..5 {
        let produced = (production[resource].max(0.0) * rolls.max(0.0) / 36.0).floor() as u32;
        let available = u32::from(hand[resource]) + produced;
        let required = u32::from(cost[resource]);
        let reserved = available.min(required);
        missing += required - reserved;
        capacity += (available - reserved) / u32::from(ratios[resource].max(1));
    }
    capacity >= missing
}

pub(crate) fn build_eta_rolls(
    production: &[f32; 5],
    hand: &ResourceHand,
    ratios: &ResourceHand,
    cost: &ResourceHand,
) -> f32 {
    if build_fundable_at_rolls(production, hand, ratios, cost, 0.0) {
        return 0.0;
    }
    if production.iter().map(|value| value.max(0.0)).sum::<f32>() <= f32::EPSILON {
        return f32::INFINITY;
    }
    let mut high = 18.0;
    while high < 9216.0 && !build_fundable_at_rolls(production, hand, ratios, cost, high) {
        high *= 2.0;
    }
    if !build_fundable_at_rolls(production, hand, ratios, cost, high) {
        return f32::INFINITY;
    }
    let mut low = 0.0;
    for _ in 0..28 {
        let mid = (low + high) * 0.5;
        if build_fundable_at_rolls(production, hand, ratios, cost, mid) {
            high = mid;
        } else {
            low = mid;
        }
    }
    high
}

/// Fraction of cards retained in a build rather than burned replacing resources
/// with no native production. Throughput/bottleneck timing is priced by ETA,
/// separately; a productive 2:1 port can legitimately improve this efficiency.
pub(crate) fn build_conversion_efficiency(
    production: &[f32; 5],
    ratios: &ResourceHand,
    cost: &ResourceHand,
) -> f32 {
    let required = cost.iter().map(|value| u32::from(*value)).sum::<u32>();
    if required == 0 {
        return 1.0;
    }
    let best_ratio = production
        .iter()
        .zip(ratios)
        .filter(|(pips, _)| **pips > 0.0)
        .map(|(_, ratio)| u32::from(*ratio))
        .min();
    let Some(best_ratio) = best_ratio else { return 0.0; };
    let cards = cost.iter().enumerate().map(|(resource, count)| {
        u32::from(*count) * if production[resource] > 0.0 { 1 } else { best_ratio }
    }).sum::<u32>();
    required as f32 / cards.max(required) as f32
}

/// Guaranteed bank stock using only the actor's hand and public opponent hand
/// totals. Never inspect sampled opponent resource identities in action policy.
pub(crate) fn guaranteed_hidden_bank_lower_bound(state: &GameState, player: u8) -> ResourceHand {
    let others = state.players.iter().enumerate()
        .filter(|(index, _)| *index != usize::from(player))
        .map(|(_, other)| other.resources.iter().map(|count| u16::from(*count)).sum::<u16>())
        .sum::<u16>();
    std::array::from_fn(|resource| {
        19u16.saturating_sub(others + u16::from(state.players[usize::from(player)].resources[resource])) as u8
    })
}

#[cfg(test)]
fn guaranteed_hidden_bank_mask(state: &GameState, player: u8) -> u8 {
    guaranteed_hidden_bank_lower_bound(state, player).iter().enumerate()
        .fold(0, |mask, (resource, count)| mask | (u8::from(*count > 0) << resource))
}

fn observable_bank(state: &GameState, player: u8) -> ResourceHand {
    if state.bank_is_public { state.bank } else { guaranteed_hidden_bank_lower_bound(state, player) }
}

const BUILD_TARGETS: [(ResourceHand, f32); 4] = [
    (SETTLEMENT_COST, 1.35),
    (ROAD_COST, 0.80),
    (CITY_COST, 0.95),
    (DEVELOPMENT_COST, 0.85),
];

/// Exact minimum number of bank trades at fixed ratios. Reserve the target's
/// required cards first. Each missing card needs one receipt; every sale must
/// come from whole surplus batches. Reselling a receipt cannot improve capacity
/// when all ratios are >=2. A resource whose hand+bank total is below the target
/// is impossible to fund. These facts give an O(5) solution, independently
/// checked against legal-trade BFS in the tests.
pub(crate) fn minimum_maritime_trades_to_fund(
    hand: &ResourceHand,
    bank: &ResourceHand,
    ratios: &ResourceHand,
    cost: &ResourceHand,
) -> Option<u8> {
    let mut missing = 0u16;
    let mut capacity = 0u16;
    for resource in 0..5 {
        let deficit = cost[resource].saturating_sub(hand[resource]);
        if deficit > bank[resource] {
            return None;
        }
        missing += u16::from(deficit);
        capacity += u16::from(hand[resource].saturating_sub(cost[resource]) / ratios[resource].clamp(2, 4));
    }
    if capacity < missing { return None; }
    u8::try_from(missing).ok()
}

fn closure_value(state: &GameState, player: u8) -> f32 {
    let hand = &state.players[player as usize].resources;
    let ratios = state.trade_ratios(player);
    BUILD_TARGETS
        .iter()
        .map(|(cost, importance)| {
            minimum_maritime_trades_to_fund(hand, &observable_bank(state, player), &ratios, cost)
                .map(|trades| *importance / (1.0 + trades as f32))
                .unwrap_or(0.0)
        })
        .sum()
}

pub(crate) fn settlement_maritime_distance(state: &GameState, player: u8) -> Option<u8> {
    let observable_bank = observable_bank(state, player);
    minimum_maritime_trades_to_fund(
        &state.players[player as usize].resources,
        &observable_bank,
        &state.trade_ratios(player),
        &SETTLEMENT_COST,
    )
}

pub(crate) fn maritime_advances_direct_build_closure(
    state: &GameState,
    action: &Action,
    player: u8,
) -> bool {
    let Action::MaritimeTrade {
        give,
        receive,
        ratio,
    } = action
    else {
        return false;
    };
    let give_index = give.index();
    let receive_index = receive.index();
    let hand = &state.players[player as usize].resources;
    BUILD_TARGETS.iter().any(|(cost, _)| {
        hand[receive_index] < cost[receive_index]
            && hand[give_index] >= ratio.saturating_add(cost[give_index])
    })
}

/// Positive only when an atomic maritime action improves deterministic access
/// to at least one complete build. A destructive swap or an inverse leg gets no
/// progress credit even though it remains legal in the rules engine.
pub(crate) fn maritime_build_closure_progress(
    state: &GameState,
    action: &Action,
    player: u8,
) -> f32 {
    if !matches!(action, Action::MaritimeTrade { .. }) {
        return 0.0;
    }
    let before = closure_value(state, player);
    let before_settlement = settlement_maritime_distance(state, player);
    let mut next = state.clone();
    if next.apply(action).is_err() {
        return 0.0;
    }
    let closure_gain = (closure_value(&next, player) - before).max(0.0);
    let settlement_gain = match (
        before_settlement,
        settlement_maritime_distance(&next, player),
    ) {
        (Some(before), Some(after)) if after < before => (before - after) as f32 * 0.55,
        (None, Some(after)) => 0.35 / (1.0 + after as f32),
        _ => 0.0,
    };
    closure_gain + settlement_gain
}

/// Immediate build value respects piece/connectivity availability. The shared
/// mask is also used by the authoritative strategic evaluator.
pub(crate) fn immediate_build_closure_value(state: &GameState, player: u8) -> f32 {
    let mask = crate::eval::build_target_mask(state, player);
    let bank = observable_bank(state, player);
    let ratios = state.trade_ratios(player);
    BUILD_TARGETS.iter().zip([mask[1], mask[0], mask[2], mask[3]])
        .filter(|(_, enabled)| *enabled)
        .map(|((cost, importance), _)| {
            minimum_maritime_trades_to_fund(&state.players[usize::from(player)].resources, &bank, &ratios, cost)
                .map_or(0.0, |trades| importance / (1.0 + f32::from(trades)))
        }).sum()
}

fn self_sufficient_production_from_inputs(production: &[f32; 5], ratios: &ResourceHand) -> f32 {
    [(SETTLEMENT_COST, 0.65), (CITY_COST, 0.20), (DEVELOPMENT_COST, 0.15)]
        .iter().map(|(cost, weight)| {
            weight * build_conversion_efficiency(production, ratios, cost)
                / (1.0 + build_eta_rolls(production, &[0; 5], ratios, cost) / 18.0)
        }).sum()
}

pub(crate) fn self_sufficient_production_value(state: &GameState, player: u8) -> f32 {
    self_sufficient_production_from_inputs(&crate::eval::production_pips(state, player), &state.trade_ratios(player))
}

#[cfg(test)]
mod tests {
    use std::collections::{HashSet, VecDeque};

    use super::*;

    fn bfs_maritime_distance(
        hand: &ResourceHand,
        bank: &ResourceHand,
        ratios: &ResourceHand,
        cost: &ResourceHand,
    ) -> Option<u8> {
        let funds = |cards: &ResourceHand| {
            cards
                .iter()
                .zip(cost.iter())
                .all(|(available, required)| available >= required)
        };
        if funds(hand) {
            return Some(0);
        }
        let start = (*hand, *bank);
        let mut queue = VecDeque::from([(start, 0_u8)]);
        let mut seen = HashSet::from([start]);
        while let Some(((cards, stock), distance)) = queue.pop_front() {
            if distance >= 8 {
                continue;
            }
            for give in 0..5 {
                let ratio = ratios[give].clamp(2, 4);
                if cards[give] < ratio {
                    continue;
                }
                for receive in 0..5 {
                    if receive == give || stock[receive] == 0 {
                        continue;
                    }
                    let mut next_cards = cards;
                    let mut next_stock = stock;
                    next_cards[give] -= ratio;
                    next_cards[receive] = next_cards[receive].saturating_add(1);
                    next_stock[give] = next_stock[give].saturating_add(ratio);
                    next_stock[receive] -= 1;
                    if funds(&next_cards) {
                        return Some(distance + 1);
                    }
                    if seen.insert((next_cards, next_stock)) {
                        queue.push_back(((next_cards, next_stock), distance + 1));
                    }
                }
            }
        }
        None
    }

    #[test]
    fn closed_form_maritime_distance_matches_explicit_legal_search() {
        let costs = [ROAD_COST, SETTLEMENT_COST, CITY_COST, DEVELOPMENT_COST];
        let mut seed = 0xC0FF_EE12_3456_7890_u64;
        for sample in 0..512 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let mut hand = [0_u8; 5];
            let mut bank = [0_u8; 5];
            let mut ratios = [4_u8; 5];
            for resource in 0..5 {
                hand[resource] = ((seed >> (resource * 6)) & 7) as u8;
                bank[resource] = (((seed >> (resource * 5 + 3)) & 7) + 1) as u8;
                ratios[resource] = 2 + ((seed >> (resource * 3 + 1)) % 3) as u8;
            }
            let cost = &costs[sample % costs.len()];
            assert_eq!(
                minimum_maritime_trades_to_fund(&hand, &bank, &ratios, cost),
                bfs_maritime_distance(&hand, &bank, &ratios, cost),
                "sample={sample}, hand={hand:?}, bank={bank:?}, ratios={ratios:?}, cost={cost:?}",
            );
        }
    }

    #[test]
    fn balanced_settlement_access_beats_equal_total_output_with_no_grain() {
        let ratios = [4; 5];
        let balanced = [3.0, 3.0, 3.0, 3.0, 0.0];
        let grainless = [4.0, 4.0, 4.0, 0.0, 0.0];
        assert!(
            self_sufficient_production_from_inputs(&balanced, &ratios)
                > self_sufficient_production_from_inputs(&grainless, &ratios)
        );
    }

    #[test]
    fn a_real_two_to_one_engine_can_close_a_missing_resource() {
        let concentrated = [8.0, 0.0, 2.0, 2.0, 0.0];
        let no_port = [4; 5];
        let lumber_port = [2, 4, 4, 4, 4];
        assert!(
            self_sufficient_production_from_inputs(&concentrated, &lumber_port)
                > self_sufficient_production_from_inputs(&concentrated, &no_port)
        );
    }

    #[test]
    fn integer_maritime_distance_requires_a_complete_batch() {
        let bank = [19; 5];
        let ratios = [3, 4, 4, 4, 4];
        let cost = [0, 1, 0, 0, 0];
        assert_eq!(
            minimum_maritime_trades_to_fund(&[2, 0, 0, 0, 0], &bank, &ratios, &cost),
            None
        );
        assert_eq!(
            minimum_maritime_trades_to_fund(&[3, 0, 0, 0, 0], &bank, &ratios, &cost),
            Some(1)
        );
    }

    #[test]
    fn maritime_distance_respects_bank_stock() {
        let ratios = [3, 4, 4, 4, 4];
        let cost = [0, 1, 0, 0, 0];
        let mut bank = [19; 5];
        bank[1] = 0;
        assert_eq!(
            minimum_maritime_trades_to_fund(&[6, 0, 0, 0, 0], &bank, &ratios, &cost),
            None
        );
    }

    #[test]
    fn maritime_distance_can_chain_distinct_surplus_resources() {
        let bank = [19; 5];
        let ratios = [4; 5];
        let cost = [0, 0, 1, 1, 0];
        assert_eq!(
            minimum_maritime_trades_to_fund(&[4, 4, 0, 0, 0], &bank, &ratios, &cost),
            Some(2)
        );
    }

    #[test]
    fn hidden_bank_guarantee_is_invariant_to_opponent_resource_identity() {
        let mut left = GameState::standard(19, 2);
        let mut right = left.clone();
        left.players[0].resources = [2, 1, 0, 0, 0];
        right.players[0].resources = left.players[0].resources;
        left.players[1].resources = [10, 0, 0, 0, 0];
        right.players[1].resources = [0, 0, 0, 10, 0];

        assert_eq!(
            guaranteed_hidden_bank_mask(&left, 0),
            guaranteed_hidden_bank_mask(&right, 0)
        );
    }

    #[test]
    fn hidden_bank_guarantee_fails_closed_when_opponents_could_exhaust_resource() {
        let mut state = GameState::standard(23, 2);
        state.players[0].resources = [1, 0, 0, 0, 0];
        state.players[1].resources = [18, 0, 0, 0, 0];

        assert_eq!(guaranteed_hidden_bank_mask(&state, 0) & 1, 0);
    }

    #[test]
    fn five_ore_three_grain_does_not_treat_ore_for_more_grain_as_progress() {
        use colonist_catan_core::{Phase, Resource};

        let mut state = GameState::standard(25, 2);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [0, 0, 0, 3, 5];
        state.bank = [19, 19, 19, 16, 14];
        let action = Action::MaritimeTrade {
            give: Resource::Ore,
            receive: Resource::Grain,
            ratio: 4,
        };

        assert!(!maritime_advances_direct_build_closure(&state, &action, 0));
        assert_eq!(maritime_build_closure_progress(&state, &action, 0), 0.0);
    }

    #[test]
    fn selling_reserved_grain_for_wool_is_not_build_progress() {
        use colonist_catan_core::{Phase, Resource};

        let mut state = GameState::standard(26, 2);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [1, 1, 0, 4, 0];
        state.bank = [18, 18, 19, 15, 19];
        let action = Action::MaritimeTrade {
            give: Resource::Grain,
            receive: Resource::Wool,
            ratio: 4,
        };

        assert!(!maritime_advances_direct_build_closure(&state, &action, 0));
    }

    #[test]
    fn settlement_completing_maritime_trade_has_more_progress_than_a_waste_swap() {
        use colonist_catan_core::{Phase, Resource};

        let mut state = GameState::standard(27, 2);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players[0].resources = [1, 1, 0, 1, 4];
        state.bank = [18, 18, 19, 18, 15];
        let complete_settlement = Action::MaritimeTrade {
            give: Resource::Ore,
            receive: Resource::Wool,
            ratio: 4,
        };
        let surplus_brick = Action::MaritimeTrade {
            give: Resource::Ore,
            receive: Resource::Brick,
            ratio: 4,
        };

        assert!(
            maritime_build_closure_progress(&state, &complete_settlement, 0)
                > maritime_build_closure_progress(&state, &surplus_brick, 0)
        );
    }

    #[test]
    fn hidden_bank_closure_value_is_observation_safe() {
        let mut left = GameState::standard(29, 2);
        let mut right = left.clone();
        left.bank_is_public = false;
        right.bank_is_public = false;
        left.players[0].resources = [4, 1, 0, 1, 0];
        right.players[0].resources = left.players[0].resources;
        left.players[1].resources = [8, 0, 0, 0, 0];
        right.players[1].resources = [0, 0, 0, 8, 0];
        left.bank = [7, 18, 19, 18, 19];
        right.bank = [15, 18, 19, 10, 19];

        assert_eq!(closure_value(&left, 0), closure_value(&right, 0));
    }
}
