//! Hardware parity gate for the frozen CPU Mref law and state transitions.
//! Run with: cargo run --release -p colonist-catan-search --features cuda-sim --bin cuda-sim-mref-parity
use colonist_catan_core::{Action, GameState, Phase, SplitMix64};
use colonist_catan_core::{
    DiceHistoryProvenance, PublicRollObservation, ReferenceController, StochasticBelief,
    StochasticState,
};
use colonist_catan_search::{CudaSimEngine, CudaSimPackedState};
use std::error::Error;

fn setup(state: &mut GameState) -> Result<(), Box<dyn Error>> {
    while matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        state.apply(
            &state
                .legal_actions()
                .into_iter()
                .next()
                .ok_or("empty setup")?,
        )?;
    }
    Ok(())
}

fn history(
    players: u8,
    length: usize,
    seed: u64,
) -> Result<Vec<PublicRollObservation>, Box<dyn Error>> {
    let mut controller = ReferenceController::new(players)?;
    let mut rng = SplitMix64::new(seed);
    let mut result = Vec::new();
    for ordinal in 0..length {
        let actor = (ordinal % players as usize) as u8;
        controller.prepare_roll(actor)?;
        let law = controller.fixed_distribution(actor);
        let mut target = rng.next_u64() % (1u64 << 32);
        let mut total = 0;
        for (i, mass) in law.into_iter().enumerate() {
            if target < mass {
                total = i as u8 + 2;
                break;
            }
            target -= mass;
        }
        controller.resolve_roll(actor, total)?;
        result.push(PublicRollObservation {
            ordinal: ordinal as u32,
            actor,
            total,
        });
    }
    Ok(result)
}

fn compare(
    engine: &mut CudaSimEngine,
    states: &[GameState],
    step: usize,
) -> Result<usize, Box<dyn Error>> {
    for (lane, (actual, state)) in engine
        .download_packed_states()?
        .iter()
        .zip(states)
        .enumerate()
    {
        let expected = CudaSimPackedState::new(state)?;
        if &expected != actual {
            let field = expected
                .words()
                .iter()
                .zip(actual.words())
                .position(|(a, b)| a != b)
                .unwrap();
            return Err(format!(
                "step={step} lane={lane} phase={:?} field={field} CPU={} CUDA={}",
                state.phase,
                expected.words()[field],
                actual.words()[field]
            )
            .into());
        }
    }
    let vectors = engine.dice_distributions()?;
    let mut checked = 0;
    for (lane, state) in states.iter().enumerate() {
        let expected: [u64; 11] = if state.phase == Phase::RollChance {
            std::array::from_fn(|i| {
                state.chance_weight(&Action::ResolveRoll { value: i as u8 + 2 })
            })
        } else {
            [0; 11]
        };
        if vectors[lane] != expected {
            return Err(format!(
                "step={step} lane={lane}: dice law mismatch CPU={expected:?} CUDA={:?}",
                vectors[lane]
            )
            .into());
        }
        checked += usize::from(state.phase == Phase::RollChance);
    }
    Ok(checked)
}

fn main() -> Result<(), Box<dyn Error>> {
    let started = std::time::Instant::now();
    eprintln!("Initializing CUDA module");
    let mut engine = CudaSimEngine::new()?;
    eprintln!("CUDA initialized in {:?}", started.elapsed());
    let mut states = Vec::new();
    let mut multi = 0;
    for lane in 0..36 {
        let players = 2 + (lane % 3) as u8;
        let mut state = GameState::randomized_base_v1(810_000 + lane as u64, players);
        state.player_trades_enabled = false;
        state.victory_target = 15;
        state.card_discard_limit = 9;
        state.friendly_robber = true;
        setup(&mut state)?;
        if lane >= 6 {
            let length = [0, 1, 23, 24, 25, 48][lane % 6];
            let observed = history(players, length, 919 + lane as u64)?;
            let (observed, provenance) = if lane >= 24 {
                // One observed suffix anchor leaves a genuinely uncertain public posterior.
                let prefix = 8 + lane as u32;
                (
                    vec![PublicRollObservation {
                        ordinal: prefix,
                        actor: (prefix % players as u32) as u8,
                        total: 8,
                    }],
                    DiceHistoryProvenance::GapFreeSuffix {
                        missing_prefix_rolls: Some(prefix),
                    },
                )
            } else {
                (
                    observed,
                    DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
                )
            };
            let belief = StochasticBelief::from_public_history(
                players,
                &observed,
                &provenance,
                823 + lane as u64,
            )?;
            multi += usize::from(belief.particle_count() > 1);
            state.current_player = observed.last().map_or(0, |roll| (roll.actor + 1) % players);
            state.stochastic = StochasticState::reference(belief);
        }
        states.push(state);
    }
    assert!(
        multi > 0,
        "partial-history posterior fixtures must have multiple particles"
    );
    engine.upload_states(&states)?;
    engine.seed_rollout_rng(0x7abb_c001)?;
    eprintln!(
        "Checking {} lanes, including {multi} multi-particle posteriors",
        states.len()
    );
    let mut vectors = compare(&mut engine, &states, 0)?;
    let mut transitions = 0;
    // GPU policy supplies actions; CPU legality and full packed descendant state
    // independently check every action. The chance law is compared bit-for-bit.
    for step in 1..=320 {
        engine.generate_rollout_actions()?;
        let actions = engine.download_generated_actions()?;
        for (lane, (state, action)) in states.iter_mut().zip(&actions).enumerate() {
            if state.phase == Phase::Finished {
                continue;
            }
            if !state.legal_actions().contains(action) {
                return Err(format!(
                    "illegal GPU action at step={step} lane={lane}: {action:?} in {:?}",
                    state.phase
                )
                .into());
            }
            state.apply(action)?;
            transitions += 1;
        }
        engine.apply_generated_actions()?;
        vectors += compare(&mut engine, &states, step)?;
    }
    // An explicitly impossible reference outcome must not mutate game state.
    let mut impossible = GameState::randomized_base_v1(991_233, 2);
    setup(&mut impossible)?;
    impossible.current_player = 1;
    let repeated = (0..3)
        .map(|ordinal| PublicRollObservation {
            ordinal,
            actor: (ordinal % 2) as u8,
            total: 8,
        })
        .collect::<Vec<_>>();
    impossible.stochastic = StochasticState::reference(StochasticBelief::from_public_history(
        2,
        &repeated,
        &DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
        0,
    )?);
    impossible.apply(&Action::Roll)?;
    assert_eq!(
        impossible.chance_weight(&Action::ResolveRoll { value: 8 }),
        0
    );
    engine.upload_states(std::slice::from_ref(&impossible))?;
    assert!(
        engine
            .apply_actions(&[Action::ResolveRoll { value: 8 }])
            .is_err()
    );
    assert_eq!(
        engine.download_packed_states()?[0],
        CudaSimPackedState::new(&impossible)?
    );

    // Actual strategic root expansion/rollout/reduction, not just transition kernels.
    let mut root = GameState::randomized_base_v1(991_234, 2);
    setup(&mut root)?;
    root.player_trades_enabled = false;
    root.victory_target = 15;
    root.card_discard_limit = 9;
    root.friendly_robber = true;
    root.stochastic = StochasticState::reference(StochasticBelief::from_public_history(
        2,
        &[],
        &DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
        0,
    )?);
    engine.upload_states(&[root])?;
    let result = engine.search_root_actions(&[vec![Action::Roll]], 32, 96, 442_891)?;
    assert_eq!(result.rows[0][0].samples, 32);
    assert_eq!(result.rows[0][0].errors, 0);
    println!(
        "Mref parity PASS: {transitions} complete-state transitions; {vectors} exact dice vectors; {multi} multi-particle lanes; 32 x 96-step strategic rollouts on {}",
        engine.device_identity().name
    );
    Ok(())
}
