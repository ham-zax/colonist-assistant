use colonist_catan_core::{GameState, SplitMix64};
use colonist_catan_search::{
    CudaSimEngine, CudaSimPackedState, cuda_sim_action_supported,
};

const LANES: usize = 64;
const STEPS: usize = 256;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut states = (0..LANES)
        .map(|lane| {
            let players = if lane % 2 == 0 { 3 } else { 4 };
            let mut state = GameState::standard(70_000 + lane as u64 * 17, players);
            state.player_trades_enabled = false;
            state
        })
        .collect::<Vec<_>>();

    let mut engine = CudaSimEngine::new()?;
    let identity = engine.device_identity().clone();
    engine.upload_states(&states)?;
    let mut rng = SplitMix64::new(0x5eed_cafe_d15c_a11u64);
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
