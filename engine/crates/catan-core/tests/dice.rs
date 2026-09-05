use colonist_catan_core::{
    Action, BeliefPolicy, DiceHistoryProvenance, GameState, M0_FAIR_IID_2D6_V1,
    MREF_COLONIST_LINKED_2024_V1, MissingRollGap, PUBLIC_HISTORY_BELIEF_V1, Phase,
    PublicRollObservation, ReferenceController, SplitMix64, StochasticBelief, StochasticState,
};

const M0_WEIGHTS: [u64; 11] = [1, 2, 3, 4, 5, 6, 5, 4, 3, 2, 1];

fn pre_roll_state() -> GameState {
    let mut state = GameState::standard(91, 2);
    state.phase = Phase::PreRoll;
    state.current_player = 0;
    state.turn = 1;
    state
}

fn exact_belief(history: &[(u8, u8)]) -> StochasticBelief {
    let observations = history
        .iter()
        .enumerate()
        .map(|(ordinal, (actor, total))| PublicRollObservation {
            ordinal: ordinal as u32,
            actor: *actor,
            total: *total,
        })
        .collect::<Vec<_>>();
    StochasticBelief::from_public_history(
        2,
        &observations,
        &DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
        0x1234_5678_9abc_def0,
    )
    .unwrap()
}

fn mref_state(history: &[(u8, u8)]) -> GameState {
    let mut state = pre_roll_state();
    state.stochastic = StochasticState::reference(exact_belief(history));
    state
}

fn legacy_m0_sample(rng: &mut SplitMix64) -> u8 {
    let mut target = rng.range(36);
    for (offset, weight) in M0_WEIGHTS.into_iter().enumerate() {
        if target < weight as usize {
            return offset as u8 + 2;
        }
        target -= weight as usize;
    }
    unreachable!()
}

#[test]
fn stochastic_ids_are_frozen() {
    assert_eq!(M0_FAIR_IID_2D6_V1, "m0-fair-iid-2d6-v1");
    assert_eq!(MREF_COLONIST_LINKED_2024_V1, "mref-colonist-linked-2024-v1");
    assert_eq!(PUBLIC_HISTORY_BELIEF_V1, "public-history-belief-v1");
    assert_eq!(
        BeliefPolicy::PublicHistoryV1.as_str(),
        PUBLIC_HISTORY_BELIEF_V1
    );
}

#[test]
fn m0_distribution_is_the_legacy_36_combination_distribution() {
    let mut state = pre_roll_state();
    state.apply(&Action::Roll).unwrap();
    let distribution = state.chance_distribution();
    assert_eq!(
        distribution
            .iter()
            .map(|(action, weight)| match action {
                Action::ResolveRoll { value } => (*value, *weight),
                other => panic!("unexpected chance action {other:?}"),
            })
            .collect::<Vec<_>>(),
        (2u8..=12).zip(M0_WEIGHTS).collect::<Vec<_>>()
    );
    assert_eq!(
        distribution.iter().map(|(_, weight)| *weight).sum::<u64>(),
        36
    );
}

#[test]
fn m0_sampling_preserves_the_legacy_seed_sequence() {
    let mut state = pre_roll_state();
    state.apply(&Action::Roll).unwrap();
    let mut expected_rng = SplitMix64::new(0xfeed_beef);
    let mut actual_rng = SplitMix64::new(0xfeed_beef);
    let expected = (0..128)
        .map(|_| legacy_m0_sample(&mut expected_rng))
        .collect::<Vec<_>>();
    let actual = (0..128)
        .map(|_| match state.sample_chance(&mut actual_rng).unwrap() {
            Action::ResolveRoll { value } => value,
            other => panic!("unexpected chance action {other:?}"),
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

#[test]
fn reference_controller_starts_with_the_36_total_combination_deck() {
    let controller = ReferenceController::new(2).unwrap();
    assert_eq!(
        controller.remaining_counts(),
        M0_WEIGHTS.map(|value| value as u8)
    );
    assert_eq!(controller.cards_left(), 36);
    assert_eq!(controller.recent_totals(), Vec::<u8>::new());
}

#[test]
fn reference_controller_reshuffles_before_a_draw_when_fewer_than_13_cards_remain() {
    let mut controller = ReferenceController::new(2).unwrap();
    for _ in 0..24 {
        let actor = 0;
        controller.prepare_roll(actor).unwrap();
        let value = (2..=12)
            .find(|value| controller.outcome_weight(actor, *value) > 0)
            .unwrap();
        controller.resolve_roll(actor, value).unwrap();
    }
    assert_eq!(controller.cards_left(), 12);
    controller.prepare_roll(1).unwrap();
    assert_eq!(controller.cards_left(), 36);
    assert_eq!(
        controller.remaining_counts(),
        M0_WEIGHTS.map(|value| value as u8)
    );
}

#[test]
fn reference_controller_keeps_only_the_last_five_totals() {
    let mut controller = ReferenceController::new(2).unwrap();
    for (actor, value) in [(0, 4), (1, 5), (0, 6), (1, 7), (0, 9), (1, 10)] {
        controller.prepare_roll(actor).unwrap();
        controller.resolve_roll(actor, value).unwrap();
    }
    assert_eq!(controller.recent_totals(), vec![5, 6, 7, 9, 10]);
}

#[test]
fn repeated_total_suppression_reaches_zero_after_three_recent_occurrences() {
    let mut controller = ReferenceController::new(2).unwrap();
    let actor = 0;
    let mut weights = Vec::new();
    for _ in 0..3 {
        controller.prepare_roll(actor).unwrap();
        weights.push(controller.outcome_weight(actor, 8));
        controller.resolve_roll(actor, 8).unwrap();
    }
    controller.prepare_roll(actor).unwrap();
    weights.push(controller.outcome_weight(actor, 8));
    assert!(weights[0] > weights[1]);
    assert!(weights[1] > weights[2]);
    assert_eq!(weights[3], 0);
}

#[test]
fn actor_is_initialized_before_seven_weighting() {
    let mut controller = ReferenceController::new(2).unwrap();
    assert_eq!(controller.initialized_player_mask(), 0);
    controller.prepare_roll(1).unwrap();
    assert_eq!(controller.initialized_player_mask(), 0b10);
    assert!(controller.outcome_weight(1, 7) > 0);
}

#[test]
fn seven_imbalance_favors_the_actor_with_fewer_sevens() {
    let mut controller = ReferenceController::new(2).unwrap();
    controller.prepare_roll(0).unwrap();
    controller.resolve_roll(0, 7).unwrap();
    controller.prepare_roll(1).unwrap();
    controller.resolve_roll(1, 6).unwrap();
    controller.prepare_roll(0).unwrap();
    controller.resolve_roll(0, 7).unwrap();

    let mut actor_zero = controller.clone();
    actor_zero.prepare_roll(0).unwrap();
    let mut actor_one = controller;
    actor_one.prepare_roll(1).unwrap();
    assert!(
        actor_one.seven_adjustment_parts(1).0 * actor_zero.seven_adjustment_parts(0).1
            > actor_zero.seven_adjustment_parts(0).0 * actor_one.seven_adjustment_parts(1).1
    );
}

#[test]
fn seven_event_streak_penalizes_owner_and_favors_other_actor() {
    let mut controller = ReferenceController::new(2).unwrap();
    controller.prepare_roll(0).unwrap();
    controller.resolve_roll(0, 7).unwrap();
    controller.prepare_roll(0).unwrap();
    controller.resolve_roll(0, 7).unwrap();

    let mut owner = controller.clone();
    owner.prepare_roll(0).unwrap();
    let mut other = controller;
    other.prepare_roll(1).unwrap();
    let owner_adjustment = owner.seven_adjustment_parts(0);
    let other_adjustment = other.seven_adjustment_parts(1);
    assert!(other_adjustment.0 * owner_adjustment.1 > owner_adjustment.0 * other_adjustment.1);
    assert_eq!(owner.seven_streak_owner(), Some(0));
    assert_eq!(owner.seven_streak_count(), 2);
}

#[test]
fn seven_adjustment_is_clamped_to_zero_through_two() {
    let mut controller = ReferenceController::new(2).unwrap();
    for (actor, value) in [(0, 7), (1, 6), (0, 7)] {
        controller.prepare_roll(actor).unwrap();
        controller.resolve_roll(actor, value).unwrap();
    }
    let mut owner = controller.clone();
    owner.prepare_roll(0).unwrap();
    let mut other = controller;
    other.prepare_roll(1).unwrap();
    let owner_adjustment = owner.seven_adjustment_parts(0);
    let other_adjustment = other.seven_adjustment_parts(1);
    assert_eq!(owner_adjustment.0, 0);
    assert_eq!(other_adjustment.0, 2 * other_adjustment.1);
}

#[test]
fn complete_history_replays_to_one_exact_controller_particle() {
    let history = [(0, 8), (1, 6), (0, 7), (1, 10), (0, 5)];
    let belief = exact_belief(&history);
    assert_eq!(belief.particle_count(), 1);

    let mut manual = ReferenceController::new(2).unwrap();
    for (actor, total) in history {
        manual.prepare_roll(actor).unwrap();
        manual.resolve_roll(actor, total).unwrap();
    }
    assert_eq!(belief.particles()[0].controller, manual);
}

#[test]
fn partial_history_posterior_is_deterministic_without_resampling() {
    let history = [
        PublicRollObservation {
            ordinal: 5,
            actor: 1,
            total: 6,
        },
        PublicRollObservation {
            ordinal: 6,
            actor: 0,
            total: 9,
        },
    ];
    let provenance = DiceHistoryProvenance::GapFreeSuffix {
        missing_prefix_rolls: Some(5),
    };
    let first = StochasticBelief::from_public_history(2, &history, &provenance, 99).unwrap();
    let second = StochasticBelief::from_public_history(2, &history, &provenance, 99).unwrap();
    assert!(first.particle_count() <= 64);
    assert!(first.particle_count() > 0);
    assert_eq!(first, second);
    assert_eq!(first.digest(), second.digest());
    assert_eq!(first.total_mass(), 1u64 << 32);
}

#[test]
fn known_length_internal_gap_posterior_is_deterministic() {
    let history = [
        PublicRollObservation {
            ordinal: 0,
            actor: 0,
            total: 8,
        },
        PublicRollObservation {
            ordinal: 3,
            actor: 1,
            total: 6,
        },
    ];
    let provenance = DiceHistoryProvenance::Gapped {
        missing_prefix_rolls: Some(0),
        gaps: vec![MissingRollGap {
            after_ordinal: 0,
            missing_rolls: Some(2),
        }],
    };
    let first = StochasticBelief::from_public_history(2, &history, &provenance, 123).unwrap();
    let second = StochasticBelief::from_public_history(2, &history, &provenance, 123).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.total_mass(), 1u64 << 32);
}

#[test]
fn unknown_gap_length_and_unknown_provenance_fail_closed() {
    let history = [
        PublicRollObservation {
            ordinal: 0,
            actor: 0,
            total: 8,
        },
        PublicRollObservation {
            ordinal: 2,
            actor: 0,
            total: 6,
        },
    ];
    let unknown_gap = DiceHistoryProvenance::Gapped {
        missing_prefix_rolls: Some(0),
        gaps: vec![MissingRollGap {
            after_ordinal: 0,
            missing_rolls: None,
        }],
    };
    let gap_error =
        StochasticBelief::from_public_history(2, &history, &unknown_gap, 123).unwrap_err();
    assert!(gap_error.to_string().contains("unknown roll length"));

    let provenance_error = StochasticBelief::from_public_history(
        2,
        &history[..1],
        &DiceHistoryProvenance::Unknown,
        123,
    )
    .unwrap_err();
    assert!(
        provenance_error
            .to_string()
            .contains("provenance is unknown")
    );
}

#[test]
fn impossible_complete_history_fails_closed() {
    let history = (0..4)
        .map(|ordinal| PublicRollObservation {
            ordinal,
            actor: 0,
            total: 8,
        })
        .collect::<Vec<_>>();
    let error = StochasticBelief::from_public_history(
        2,
        &history,
        &DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
        1,
    )
    .unwrap_err();
    assert!(error.to_string().contains("inconsistent"));
}

#[test]
fn reference_distribution_and_sampling_use_the_same_weights() {
    let mut state = mref_state(&[(0, 8), (1, 6), (0, 7)]);
    state.apply(&Action::Roll).unwrap();
    let distribution = state.chance_distribution();
    for (action, weight) in &distribution {
        assert_eq!(*weight, state.chance_weight(action));
    }
    let positive = distribution
        .iter()
        .filter(|(_, weight)| *weight > 0)
        .map(|(action, _)| action.clone())
        .collect::<Vec<_>>();
    let mut rng = SplitMix64::new(44);
    for _ in 0..128 {
        assert!(positive.contains(&state.sample_chance(&mut rng).unwrap()));
    }
}

#[test]
fn reference_belief_persists_and_evolves_across_multiple_simulated_rolls() {
    let mut state = mref_state(&[(0, 8), (1, 6)]);
    let before = state.stochastic.digest();
    state.apply(&Action::Roll).unwrap();
    let value = state
        .chance_distribution()
        .into_iter()
        .find_map(|(action, weight)| match action {
            Action::ResolveRoll { value } if weight > 0 && value != 7 => Some(value),
            _ => None,
        })
        .unwrap();
    state.apply(&Action::ResolveRoll { value }).unwrap();
    let after_first = state.stochastic.digest();
    assert_ne!(before, after_first);
    state.apply(&Action::EndTurn).unwrap();
    state.apply(&Action::Roll).unwrap();
    let next = state
        .chance_distribution()
        .into_iter()
        .find_map(|(action, weight)| match action {
            Action::ResolveRoll { value } if weight > 0 && value != 7 => Some(value),
            _ => None,
        })
        .unwrap();
    state.apply(&Action::ResolveRoll { value: next }).unwrap();
    assert_ne!(after_first, state.stochastic.digest());
}

#[test]
fn mref_state_hash_splits_behaviorally_distinct_beliefs() {
    let left = mref_state(&[(0, 8)]);
    let right = mref_state(&[(0, 6)]);
    assert_ne!(left.state_hash(), right.state_hash());
}

#[test]
fn explicit_m0_metadata_is_hash_compatible_with_the_default_state() {
    let default_state = pre_roll_state();
    let mut explicit = default_state.clone();
    explicit.stochastic = StochasticState::m0();
    assert_eq!(default_state.state_hash(), explicit.state_hash());
    assert_eq!(
        default_state.stochastic.model().as_str(),
        M0_FAIR_IID_2D6_V1
    );
}
