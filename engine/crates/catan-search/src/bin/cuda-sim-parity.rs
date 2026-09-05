use colonist_catan_core::{Action, DevCard, GameState, Phase, SplitMix64};
use colonist_catan_search::{
    CudaSimEngine, CudaSimPackedState, cuda_sim_action_supported,
};

const LANES: usize = 64;
const STEPS: usize = 256;

fn finish_setup(state: &mut GameState) -> Result<(), Box<dyn std::error::Error>> {
    while matches!(state.phase, Phase::SetupSettlement | Phase::SetupRoad { .. }) {
        let action = state
            .legal_actions()
            .into_iter()
            .next()
            .ok_or("setup state had no legal action")?;
        state.apply(&action)?;
    }
    Ok(())
}

fn verify_end_turn_winner(engine: &mut CudaSimEngine) -> Result<(), Box<dyn std::error::Error>> {
    let mut state = GameState::randomized_base_v1(69_999, 3);
    state.player_trades_enabled = false;
    finish_setup(&mut state)?;
    state.phase = Phase::Main;
    state.current_player = 0;
    state.victory_target = 7;
    let victory = DevCard::VictoryPoint.index();
    state.development_deck[victory] = 0;
    state.players[1].development[victory] = 5;
    state.validate()?;

    engine.upload_states(std::slice::from_ref(&state))?;
    engine.apply_actions(&[Action::EndTurn])?;
    state.apply(&Action::EndTurn)?;
    if state.phase != Phase::Finished || state.current_player != 1 {
        return Err("CPU EndTurn winner oracle did not enter Finished for the newly current player".into());
    }
    let expected = CudaSimPackedState::new(&state)?;
    let actual = engine.download_packed_states()?.remove(0);
    if expected != actual {
        let first = expected
            .words()
            .iter()
            .zip(actual.words())
            .position(|(left, right)| left != right)
            .unwrap_or(usize::MAX);
        return Err(format!(
            "GPU EndTurn winner mismatch at field {first}: cpu={} gpu={}",
            expected.words().get(first).copied().unwrap_or_default(),
            actual.words().get(first).copied().unwrap_or_default(),
        )
        .into());
    }
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut states = (0..LANES)
        .map(|lane| {
            let players = if lane % 2 == 0 { 3 } else { 4 };
            let mut state = GameState::randomized_base_v1(70_000 + lane as u64 * 17, players);
            state.player_trades_enabled = false;
            state
        })
        .collect::<Vec<_>>();

    let mut engine = CudaSimEngine::new()?;
    let identity = engine.device_identity().clone();
    verify_end_turn_winner(&mut engine)?;
    engine.upload_states(&states)?;
    let mut rng = SplitMix64::new(0x05ee_dcaf_ed15_ca11_u64);
    let mut transitions = 0usize;

    for step in 0..STEPS {
        let mut actions = Vec::with_capacity(states.len());
        for (lane, state) in states.iter().enumerate() {
            let supported = state
                .legal_actions()
                .into_iter()
                .filter(cuda_sim_action_supported)
                .collect::<Vec<_>>();
            if supported.is_empty() {
                return Err(format!(
                    "no supported GPU transition at step {step}, lane {lane}, phase {:?}",
                    state.phase
                )
                .into());
            }
            let index = rng.range(supported.len());
            actions.push(supported[index].clone());
        }

        engine.apply_actions(&actions)?;
        for (state, action) in states.iter_mut().zip(&actions) {
            state.apply(action)?;
        }
        transitions += states.len();

        let gpu = engine.download_packed_states()?;
        for (lane, (cpu, gpu)) in states.iter().zip(gpu).enumerate() {
            let expected = CudaSimPackedState::new(cpu)?;
            if expected != gpu {
                let first = expected
                    .words()
                    .iter()
                    .zip(gpu.words())
                    .position(|(left, right)| left != right)
                    .unwrap_or(usize::MAX);
                return Err(format!(
                    "GPU resident-state mismatch at step {step}, lane {lane}, action {:?}, phase {:?}, field {first}: cpu={} gpu={}",
                    actions[lane],
                    cpu.phase,
                    expected.words().get(first).copied().unwrap_or_default(),
                    gpu.words().get(first).copied().unwrap_or_default(),
                )
                .into());
            }
        }
    }

    println!(
        "{{\"kind\":\"cuda-resident-sim-parity\",\"parity\":true,\"lanes\":{LANES},\"steps\":{STEPS},\"transitions\":{transitions},\"backend\":\"{}\",\"gpu\":\"{}\",\"computeCapability\":[{},{}]}}",
        identity.backend,
        identity.name.replace('"', "\\\""),
        identity.compute_capability.0,
        identity.compute_capability.1,
    );
    Ok(())
}
