//! Hardware parity for the CPU-frozen Mref contract and the CUDA rollout boundary.
use super::*;
use colonist_catan_core::{
    DiceHistoryProvenance, PublicRollObservation, SplitMix64, StochasticBelief,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;

fn finish_setup(state: &mut GameState) {
    while matches!(
        state.phase,
        Phase::SetupSettlement | Phase::SetupRoad { .. }
    ) {
        state.apply(&state.legal_actions()[0].clone()).unwrap();
    }
}

fn complete_state(players: u8, prefix: usize) -> Result<GameState> {
    let mut controller = ReferenceController::new(players)?;
    let mut rng = SplitMix64::new(4101 + prefix as u64);
    let mut observations = Vec::new();
    for ordinal in 0..prefix {
        let actor = (ordinal % players as usize) as u8;
        controller.prepare_roll(actor)?;
        let probabilities = controller.fixed_distribution(actor);
        let mut ticket = rng.next_u64() & 0xffff_ffff;
        let mut total = 2;
        for (index, weight) in probabilities.iter().enumerate() {
            if ticket < *weight {
                total = index as u8 + 2;
                break;
            }
            ticket -= weight;
        }
        // Exercise seven initialization/imbalance/streaks rather than relying
        // solely on their frequency in random histories.
        if ordinal % 4 != 0 && probabilities[5] != 0 {
            total = 7;
        }
        controller.resolve_roll(actor, total)?;
        observations.push(PublicRollObservation {
            ordinal: ordinal as u32,
            actor,
            total,
        });
    }
    let mut state = GameState::standard(72001, players);
    finish_setup(&mut state);
    state.bank_is_public = true;
    state.player_trades_enabled = false;
    state.domestic_trade_disabled = 1;
    state.victory_target = 15;
    state.card_discard_limit = 9;
    state.friendly_robber = true;
    state.current_player = (prefix % players as usize) as u8;
    state.stochastic = StochasticState::reference(StochasticBelief::from_public_history(
        players,
        &observations,
        &DiceHistoryProvenance::CompleteFromFirstGameplayRoll,
        0,
    )?);
    Ok(state)
}

fn distributions(engine: &CudaSimEngine) -> Result<Vec<[u64; 11]>> {
    let count = engine.resident_states;
    let count_u32 = u32::try_from(count)?;
    let kernel = engine
        ._module
        .load_function("reference_dice_distribution_kernel")?;
    let mut output = engine.stream.alloc_zeros::<u64>(count * 11)?;
    let mut args = engine.stream.launch_builder(&kernel);
    args.arg(&engine.state_device);
    args.arg(&mut output);
    args.arg(&count_u32);
    args.arg(&count_u32);
    unsafe { args.launch(LaunchConfig::for_num_elems(count_u32))? };
    let words = engine.stream.clone_dtoh(&output)?;
    engine.stream.synchronize()?;
    Ok((0..count)
        .map(|lane| std::array::from_fn(|i| words[i * count + lane]))
        .collect())
}

#[test]
fn mref_exact_probabilities_and_stateful_transitions_match_cpu() -> Result {
    let prefixes = [0, 1, 5, 12, 23, 24, 25, 47, 48, 73, 120];
    let mut states = Vec::new();
    for players in 2..=4 {
        for prefix in prefixes {
            states.push(complete_state(players, prefix)?);
        }
    }
    let mut gpu = CudaSimEngine::new()?;
    gpu.upload_states(&states)?;
    gpu.seed_rollout_rng(54019)?;
    for state in &mut states {
        state.apply(&Action::Roll)?;
    }
    gpu.apply_actions(&vec![Action::Roll; states.len()])?;
    for (state, law) in states.iter().zip(distributions(&gpu)?) {
        assert_eq!(
            law,
            state
                .stochastic
                .reference_distribution(state.current_player)
                .unwrap()
        );
        assert_eq!(law.iter().sum::<u64>(), FIXED_BELIEF_MASS);
    }
    // Device-generated chance actions are applied to the CPU oracle. Full
    // packed equality checks controller progression, rules, and bookkeeping.
    for step in 0..192 {
        gpu.generate_rollout_actions()?;
        let actions = gpu.download_generated_actions()?;
        for (lane, (state, action)) in states.iter_mut().zip(&actions).enumerate() {
            assert!(
                state.legal_actions().contains(action),
                "step {step} lane {lane}: {action:?}"
            );
            state.apply(action)?;
        }
        gpu.apply_actions(&actions)?;
        let actual = gpu.download_packed_states()?;
        for (lane, (state, packed)) in states.iter().zip(actual).enumerate() {
            let expected = CudaSimPackedState::new(state)?;
            assert_eq!(expected, packed, "step {step} lane {lane}");
        }
        if step % 8 == 0 {
            for (state, law) in states.iter().zip(distributions(&gpu)?) {
                if state.phase == Phase::RollChance {
                    assert_eq!(
                        law,
                        state
                            .stochastic
                            .reference_distribution(state.current_player)
                            .unwrap()
                    );
                }
            }
        }
    }
    eprintln!(
        "Mref parity: {} lanes, {} exact transitions, 2/3/4 players",
        states.len(),
        states.len() * 192
    );
    Ok(())
}

fn partial_state() -> Result<GameState> {
    let mut state = complete_state(2, 0)?;
    state.current_player = 1;
    state.stochastic = StochasticState::reference(StochasticBelief::from_public_history(
        2,
        &[PublicRollObservation {
            ordinal: 8,
            actor: 0,
            total: 8,
        }],
        &DiceHistoryProvenance::GapFreeSuffix {
            missing_prefix_rolls: Some(8),
        },
        4321,
    )?);
    assert!(state.stochastic.particle_count() > 1);
    Ok(state)
}

#[test]
fn mref_partial_posterior_is_shared_and_sampled_per_lane() -> Result {
    let state = partial_state()?;
    let particles = state.stochastic.reference_belief().unwrap().particles();
    let mut gpu = CudaSimEngine::new()?;
    let count = 4096;
    gpu.upload_states(&vec![state.clone(); count])?;
    assert_eq!(
        gpu.dice_pool_device.len(),
        particles.len() * DICE_PARTICLE_WORDS
    );
    let seed = 47831;
    gpu.seed_rollout_rng(seed)?;
    let packed = gpu.download_packed_states()?;
    let mut hits = vec![0usize; particles.len()];
    for (lane, actual) in packed.iter().enumerate() {
        let lane_seed = cuda_sim_game_chance_rng_seed(seed, lane as u64);
        let mut rng = SplitMix64::new(mix_stream_seed(lane_seed, 0, 0x6d72_6566_6469_6365));
        let mut ticket = rng.next_u64() & 0xffff_ffff;
        let index = particles
            .iter()
            .position(|particle| {
                if ticket < particle.mass {
                    true
                } else {
                    ticket -= particle.mass;
                    false
                }
            })
            .unwrap();
        hits[index] += 1;
        assert_eq!(
            &actual.words[STATE_DICE_CONTROLLER..],
            &pack_reference_controller(&particles[index].controller)
        );
        assert_eq!(actual.words[STATE_DICE_POOL_COUNT], 0);
    }
    assert!(hits.iter().filter(|count| **count > 0).count() > 1);
    // Every sampled controller must keep evolving legally after the root.
    gpu.run_rollout_steps(96)?;
    let summaries = gpu.download_game_summaries()?;
    assert_eq!(summaries.len(), count);
    eprintln!(
        "Mref posterior: {} controllers shared across {count} lanes",
        particles.len()
    );
    Ok(())
}

#[test]
fn mref_partial_controller_laws_match_cpu_without_private_world_inputs() -> Result {
    let state = partial_state()?;
    let belief = state
        .stochastic
        .reference_belief()
        .unwrap()
        .prepare_roll(1)?;
    let particles = belief.particles();
    let mut gpu = CudaSimEngine::new()?;
    let mut inert = state.clone();
    inert.stochastic = StochasticState::m0();
    inert.phase = Phase::RollChance;
    gpu.upload_states(&vec![inert; particles.len() * 2])?;
    let count = particles.len() * 2;
    for (index, particle) in particles.iter().enumerate() {
        for lane in [index, index + particles.len()] {
            gpu.state_host[STATE_DICE_MODEL * count + lane] = 1;
            for (field, word) in pack_reference_controller(&particle.controller)
                .into_iter()
                .enumerate()
            {
                gpu.state_host[(STATE_DICE_CONTROLLER + field) * count + lane] = word;
            }
            // Change only a hidden hand in the second copy of each controller.
            gpu.state_host[(STATE_PLAYERS + 4) * count + lane] =
                if lane < particles.len() { 0 } else { 7 };
        }
    }
    gpu.stream
        .memcpy_htod(&gpu.state_host, &mut gpu.state_device)?;
    let laws = distributions(&gpu)?;
    let mut numerator = [0u128; 11];
    for (index, particle) in particles.iter().enumerate() {
        assert_eq!(laws[index], particle.controller.fixed_distribution(1));
        assert_eq!(laws[index], laws[index + particles.len()]);
        for (total, value) in numerator.iter_mut().enumerate() {
            *value += particle.mass as u128 * laws[index][total] as u128;
        }
    }
    let mut combined = numerator.map(|value| (value >> 32) as u64);
    let mut order = (0..11).collect::<Vec<_>>();
    order.sort_by_key(|i| (std::cmp::Reverse(numerator[*i] & 0xffff_ffff), *i));
    let remaining = FIXED_BELIEF_MASS - combined.iter().sum::<u64>();
    for i in order.into_iter().take(remaining as usize) {
        combined[i] += 1;
    }
    assert_eq!(combined, belief.distribution(1));
    Ok(())
}

#[test]
fn mref_root_expansion_and_cancellation_preserve_public_base() -> Result {
    let state = partial_state()?;
    let mut gpu = CudaSimEngine::new()?;
    gpu.upload_states(&[state])?;
    let roots = vec![vec![Action::Roll]];
    assert!(matches!(
        gpu.search_root_actions_controlled(&roots, 64, 48, 3, || true),
        Err(CudaSimError::Cancelled)
    ));
    let first = gpu.search_root_actions(&roots, 512, 48, 3)?;
    let second = gpu.search_root_actions(&roots, 512, 48, 3)?;
    assert_eq!(first.rows[0][0].samples, 512);
    assert_eq!(first.rows[0][0].errors, 0);
    assert_eq!(
        first.rows[0][0].mean_victory_points,
        second.rows[0][0].mean_victory_points
    );
    assert_eq!(
        first.rows[0][0].mean_strategic_margin,
        second.rows[0][0].mean_strategic_margin
    );
    assert!(
        gpu.pending_dice,
        "search must not materialize the public base once for all roots"
    );
    Ok(())
}

#[test]
fn m0_cuda_dice_random_stream_is_unchanged() -> Result {
    let mut state = complete_state(2, 0)?;
    state.stochastic = StochasticState::m0();
    state.apply(&Action::Roll)?;
    let mut gpu = CudaSimEngine::new()?;
    let count = 1024;
    let seed = 54329;
    gpu.upload_states(&vec![state; count])?;
    gpu.seed_rollout_rng(seed)?;
    gpu.generate_rollout_actions()?;
    for (lane, action) in gpu.download_generated_actions()?.into_iter().enumerate() {
        let mut rng = SplitMix64::new(cuda_sim_game_chance_rng_seed(seed, lane as u64));
        assert_eq!(
            action,
            Action::ResolveRoll {
                value: rng.roll_2d6()
            }
        );
    }
    Ok(())
}
