use colonist_catan_core::GameState;

const PIPS: [f32; 13] = [
    0.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 0.0, 5.0, 4.0, 3.0, 2.0, 1.0,
];

/// Probability of drawing no knights from the publicly unknown card pool.
/// Composition of an opponent's sampled hand must not change this estimate.
fn no_knight_probability(population: u16, knights: u16, draws: u16) -> f32 {
    let draws = draws.min(population);
    let non_knights = population.saturating_sub(knights.min(population));
    if draws > non_knights {
        return 0.0;
    }
    (0..draws).fold(1.0, |probability, draw| {
        probability * (non_knights - draw) as f32 / (population - draw) as f32
    })
}

/// One-roll option value of keeping a knight to answer a future
/// block of a single productive tile. Mirrored in cuda/exact_eval.cu.
///
/// This is a bounded insurance proxy, not a calibrated opponent attack model:
/// an opponent may roll seven or hold a knight and target our strongest tile.
/// The premium prices production after opponents get a turn to re-block us,
/// which a retained knight can protect by clearing before our following roll.
/// Newly bought knights mature at EndTurn and cover that future horizon.
/// The all-opponents risk is a coarse full-round proxy even in recursive
/// partial-round states; it is not a turn-order-aware attack forecast. It does not
/// reward withholding a knight while our own production is already blocked.
/// A second held knight preserves this first-knight option, so spending a
/// spare does not discard the reserve option.
pub(crate) fn held_knight_insurance(state: &GameState, player: u8, weights: &[f32; 5]) -> f32 {
    let own = &state.players[player as usize];
    if own.development[0] == 0 {
        return 0.0;
    }
    let mut multipliers = vec![0.0f32; state.board.hexes.len()];
    let mut protected = vec![false; state.board.hexes.len()];
    for (vertex, building) in state.buildings.iter().enumerate() {
        let Some(building) = building else { continue };
        for hex in &state.board.vertices[vertex].adjacent_hexes {
            let hex = *hex as usize;
            if state.friendly_robber
                && state.players[building.player() as usize].public_victory_points < 3
            {
                protected[hex] = true;
            }
            if building.player() == player {
                multipliers[hex] += building.production_multiplier() as f32;
            }
        }
    }
    let robber = state.robber_hex as usize;
    if multipliers[robber] > 0.0
        && state.board.hexes[robber].resource.is_some()
        && state.board.hexes[robber].number > 0
    {
        return 0.0;
    }
    let exposure = state
        .board
        .hexes
        .iter()
        .enumerate()
        .filter_map(|(hex, tile)| {
            let resource = tile.resource?;
            if hex == robber
                || protected[hex]
                || (state.bank_is_public && state.bank[resource.index()] == 0)
            {
                return None;
            }
            Some(multipliers[hex] * PIPS[tile.number as usize] / 36.0 * weights[resource.index()])
        })
        .fold(0.0f32, f32::max);
    if exposure == 0.0 {
        return 0.0;
    }

    // Played cards are global, including our plays. Subtract them only once.
    // Opponent public unplayed totals reveal draws, never their identities.
    let played = state
        .played_development
        .iter()
        .map(|count| u16::from(*count))
        .sum::<u16>();
    let held = own
        .development
        .iter()
        .map(|count| u16::from(*count))
        .sum::<u16>();
    let population = 25u16.saturating_sub(played + held);
    let knights = 14u16
        .saturating_sub(u16::from(state.played_development[0]) + u16::from(own.development[0]));
    let mut opponents = 0u8;
    let mut draws = 0u16;
    for (index, other) in state.players.iter().enumerate() {
        if index == player as usize {
            continue;
        }
        opponents += 1;
        draws += other
            .development
            .iter()
            .zip(other.bought_development)
            .map(|(held, bought)| u16::from(held.saturating_sub(bought)))
            .sum::<u16>();
    }
    let no_attack = (5.0f32 / 6.0).powi(i32::from(opponents))
        * no_knight_probability(population, knights, draws);
    // Marginal hand liquidity is .18 and hand utility is weighted .48.
    // development_utility is weighted .72 by its caller, hence .18*.48/.72.
    exposure * (1.0 - no_attack) * 0.12
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sea8653_fixture::{hex, state};

    const WEIGHTS: [f32; 5] = [1.0; 5];

    #[test]
    fn sea8653_open_cluster_has_small_contextual_reserve_value() {
        let state = state("D180");
        let value = held_knight_insurance(&state, 0, &WEIGHTS);
        assert!(value > 0.0 && value < 0.1);
        assert_eq!(state.players[1].development.iter().sum::<u8>(), 0);
        // Seven remains a threat even without any opposing development card:
        // clearing before our NEXT roll protects that roll's production.
        assert!((value - (4.0 * 5.0 / 36.0) * (1.0 / 6.0) * 0.12).abs() < 1e-6);
    }

    #[test]
    fn defensive_clear_is_not_penalized_by_insurance() {
        for decision in ["D194", "D217", "D228", "D245"] {
            let state = state(decision);
            assert_eq!(state.robber_hex, hex("h:0,1"));
            assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), 0.0);
        }
    }

    #[test]
    fn a_spare_and_newly_bought_knights_cover_the_following_turn() {
        let mut state = state("D180");
        let one = held_knight_insurance(&state, 0, &WEIGHTS);
        state.players[0].development[0] = 2;
        assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), one);
        state.players[0].bought_development[0] = 2;
        assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), one);
    }

    #[test]
    fn opposing_identities_do_not_leak_into_reserve_valuation() {
        let mut state = state("D180");
        state.players[1].development = [0, 2, 0, 0, 0];
        let unknown = held_knight_insurance(&state, 0, &WEIGHTS);
        state.players[1].development = [2, 0, 0, 0, 0];
        assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), unknown);
        state.players[1].development = [0; 5];
        assert!(held_knight_insurance(&state, 0, &WEIGHTS) < unknown);
    }

    #[test]
    fn public_opponent_draws_use_the_remaining_global_pool() {
        let mut state = state("D180");
        state.players[1].development = [0, 2, 0, 0, 0];
        let held = state.players[0]
            .development
            .iter()
            .map(|count| u16::from(*count))
            .sum::<u16>();
        let population = 25
            - held
            - state
                .played_development
                .iter()
                .map(|count| u16::from(*count))
                .sum::<u16>();
        let knights = 14
            - u16::from(state.players[0].development[0])
            - u16::from(state.played_development[0]);
        let non_knights = population - knights;
        let no_knight = (non_knights as f32 / population as f32)
            * ((non_knights - 1) as f32 / (population - 1) as f32);
        let expected = (4.0 * 5.0 / 36.0) * (1.0 - 5.0 / 6.0 * no_knight) * 0.12;
        assert!((held_knight_insurance(&state, 0, &WEIGHTS) - expected).abs() < 1e-6);
        state.players[0].bought_development[0] = 1;
        assert!((held_knight_insurance(&state, 0, &WEIGHTS) - expected).abs() < 1e-6);
    }

    #[test]
    fn an_empty_public_bank_has_no_next_roll_production_to_insure() {
        let mut state = state("D180");
        state.bank = [0; 5];
        state.bank_is_public = true;
        assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), 0.0);
        state.bank_is_public = false;
        assert!(held_knight_insurance(&state, 0, &WEIGHTS) > 0.0);
    }

    #[test]
    fn recorded_blocked_income_still_uses_a_defensive_knight_in_search() {
        let particles = crate::sea8653_fixture::particles_with_development("D194", [0, 2, 0, 0, 0]);
        let result =
            crate::depth::search_weighted_belief_maxn_bounded(&particles, 2, 10, 4_000).unwrap();
        assert!(
            matches!(result.chosen, Some(colonist_catan_core::Action::PlayKnight { hex: target, .. })
            if target != hex("h:0,1")),
            "blocked grain must be cleared: {:?}",
            result.chosen
        );
    }

    #[test]
    fn depletion_is_global_and_friendly_tiles_are_protected() {
        assert_eq!(no_knight_probability(5, 0, 3), 1.0);
        assert_eq!(no_knight_probability(5, 4, 2), 0.0);
        let mut state = state("D180");
        state.players[0].public_victory_points = 2;
        assert_eq!(held_knight_insurance(&state, 0, &WEIGHTS), 0.0);
        state.friendly_robber = false;
        assert!(held_knight_insurance(&state, 0, &WEIGHTS) > 0.0);
    }
}
