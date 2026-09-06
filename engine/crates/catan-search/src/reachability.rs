use colonist_catan_core::DevCard;

use crate::mcts::BeliefParticle;

/// Cheap, deliberately optimistic point-source accounting used for diagnostics.
///
/// This is an upper bound, not a plan. Building gain assumes every remaining
/// settlement can eventually be placed and every remaining city can both be
/// built and free a settlement for another placement. Award gain assumes the
/// player can eventually own any award they do not already hold. These
/// relaxations can overestimate attainable points, which is required for a
/// sound insufficiency proof.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReachabilityDiagnostic {
    pub exact_victory_points: u8,
    pub victory_target: u8,
    pub optimistic_building_gain: u8,
    pub optimistic_award_gain: u8,
    pub optimistic_without_future_development_vp: u8,
    pub minimum_future_development_vp_required: u8,
    pub future_development_vp_necessary: bool,
    pub remaining_development_vp_min: u8,
    pub remaining_development_vp_max: u8,
    pub remaining_development_vp_expected: f32,
    pub optimistic_with_min_future_development_vp: u8,
    pub optimistic_with_max_future_development_vp: u8,
    pub target_reachable_in_all_compatible_worlds: bool,
    pub target_reachable_in_some_compatible_world: bool,
}

pub fn optimistic_reachability(
    particles: &[BeliefParticle],
    actor: u8,
) -> Option<ReachabilityDiagnostic> {
    let first = particles.first()?;
    if actor >= first.state.board.num_players || actor as usize >= first.state.players.len() {
        return None;
    }

    let player = &first.state.players[actor as usize];
    let exact_victory_points = player.victory_points();
    let victory_target = first.state.victory_target;

    // Every remaining city can add one point by upgrading a settlement and can
    // also return that settlement piece for a later one-point placement.
    let optimistic_building_gain = player
        .settlements_left
        .saturating_add(player.cities_left.saturating_mul(2));
    let optimistic_award_gain = 2u8
        .saturating_mul(u8::from(!player.has_longest_road))
        .saturating_add(2u8.saturating_mul(u8::from(!player.has_largest_army)));
    let optimistic_without_future_development_vp = exact_victory_points
        .saturating_add(optimistic_building_gain)
        .saturating_add(optimistic_award_gain);
    let minimum_future_development_vp_required =
        victory_target.saturating_sub(optimistic_without_future_development_vp);

    let vp_index = DevCard::VictoryPoint.index();
    let mut remaining_development_vp_min = u8::MAX;
    let mut remaining_development_vp_max = 0u8;
    let mut weighted_total = 0.0f32;
    let mut positive_weight = 0.0f32;
    let mut saw_positive = false;
    for particle in particles {
        let weight = particle.weight.max(0.0);
        if weight <= 0.0 {
            continue;
        }
        let remaining = particle.state.development_deck[vp_index];
        remaining_development_vp_min = remaining_development_vp_min.min(remaining);
        remaining_development_vp_max = remaining_development_vp_max.max(remaining);
        weighted_total += remaining as f32 * weight;
        positive_weight += weight;
        saw_positive = true;
    }
    if !saw_positive {
        return None;
    }

    let remaining_development_vp_expected = weighted_total / positive_weight.max(f32::EPSILON);
    let optimistic_with_min_future_development_vp =
        optimistic_without_future_development_vp.saturating_add(remaining_development_vp_min);
    let optimistic_with_max_future_development_vp =
        optimistic_without_future_development_vp.saturating_add(remaining_development_vp_max);

    Some(ReachabilityDiagnostic {
        exact_victory_points,
        victory_target,
        optimistic_building_gain,
        optimistic_award_gain,
        optimistic_without_future_development_vp,
        minimum_future_development_vp_required,
        future_development_vp_necessary: minimum_future_development_vp_required > 0,
        remaining_development_vp_min,
        remaining_development_vp_max,
        remaining_development_vp_expected,
        optimistic_with_min_future_development_vp,
        optimistic_with_max_future_development_vp,
        target_reachable_in_all_compatible_worlds: optimistic_with_min_future_development_vp
            >= victory_target,
        target_reachable_in_some_compatible_world: optimistic_with_max_future_development_vp
            >= victory_target,
    })
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::{DevCard, GameState};

    use super::*;

    fn particle(state: GameState, weight: f32) -> BeliefParticle {
        BeliefParticle { state, weight }
    }

    #[test]
    fn proves_future_development_vp_needed_only_below_optimistic_non_dev_ceiling() {
        let mut state = GameState::standard(17, 2);
        state.victory_target = 15;
        state.players[0].public_victory_points = 10;
        state.players[0].settlements_left = 0;
        state.players[0].cities_left = 0;
        state.players[0].has_longest_road = true;
        state.players[0].has_largest_army = true;
        state.development_deck[DevCard::VictoryPoint.index()] = 3;

        let diagnostic = optimistic_reachability(&[particle(state, 1.0)], 0).unwrap();
        assert_eq!(diagnostic.optimistic_without_future_development_vp, 10);
        assert_eq!(diagnostic.minimum_future_development_vp_required, 5);
        assert!(diagnostic.future_development_vp_necessary);
        assert!(!diagnostic.target_reachable_in_some_compatible_world);
    }

    #[test]
    fn city_supply_counts_upgrade_point_and_returned_settlement_opportunity() {
        let mut state = GameState::standard(19, 2);
        state.victory_target = 15;
        state.players[0].public_victory_points = 2;
        state.players[0].settlements_left = 3;
        state.players[0].cities_left = 4;
        state.players[0].has_longest_road = true;
        state.players[0].has_largest_army = true;

        let diagnostic = optimistic_reachability(&[particle(state, 1.0)], 0).unwrap();
        assert_eq!(diagnostic.optimistic_building_gain, 11);
        assert_eq!(diagnostic.optimistic_without_future_development_vp, 13);
        assert_eq!(diagnostic.minimum_future_development_vp_required, 2);
    }

    #[test]
    fn aggregates_remaining_vp_range_over_compatible_worlds() {
        let mut left = GameState::standard(23, 2);
        left.victory_target = 15;
        left.players[0].public_victory_points = 8;
        left.players[0].settlements_left = 0;
        left.players[0].cities_left = 0;
        left.players[0].has_longest_road = true;
        left.players[0].has_largest_army = true;
        left.development_deck[DevCard::VictoryPoint.index()] = 1;
        let mut right = left.clone();
        right.development_deck[DevCard::VictoryPoint.index()] = 4;

        let diagnostic =
            optimistic_reachability(&[particle(left, 0.25), particle(right, 0.75)], 0).unwrap();
        assert_eq!(diagnostic.remaining_development_vp_min, 1);
        assert_eq!(diagnostic.remaining_development_vp_max, 4);
        assert!((diagnostic.remaining_development_vp_expected - 3.25).abs() < 1e-6);
    }
}
