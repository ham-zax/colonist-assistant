use colonist_catan_core::{GameState, Phase};
use colonist_catan_search::{CudaSimEngine, CudaSimPackedState};

const LANES: usize = 64;
const MAX_STEPS: usize = 512;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut states = (0..LANES)
        .map(|lane| {
            let players = if lane % 2 == 0 { 3 } else { 4 };
            let mut state = GameState::randomized_base_v1(90_000 + lane as u64 * 29, players);
            state.player_trades_enabled = false;
            state
        })
        .collect::<Vec<_>>();

    let mut engine = CudaSimEngine::new()?;
    let identity = engine.device_identity().clone();
    engine.upload_states(&states)?;
    engine.seed_rollout_rng(0x1234_5678_9abc_def0)?;

    let mut transitions = 0usize;
    let mut steps = 0usize;
    for step in 0..MAX_STEPS {
        if states.iter().any(|state| state.phase == Phase::Finished) {
            break;
        }

        engine.generate_rollout_actions()?;
        let actions = engine.download_generated_actions()?;
        for (lane, (state, action)) in states.iter().zip(&actions).enumerate() {
            let legal = state.legal_actions();
            if !legal.contains(action) {
                return Err(format!(
                    "GPU generated illegal action at step {step}, lane {lane}, phase {:?}: {:?}; legal_count={} legal_sample={:?}",
                    state.phase,
                    action,
                    legal.len(),
                    legal.iter().take(12).collect::<Vec<_>>(),
                )
                .into());
            }
        }

        engine.apply_generated_actions()?;
        for (state, action) in states.iter_mut().zip(&actions) {
            state.apply(action)?;
        }
        transitions += states.len();
        steps = step + 1;

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
                    "GPU generated transition mismatch at step {step}, lane {lane}, action {:?}, phase {:?}, field {first}: cpu={} gpu={}",
                    actions[lane],
                    cpu.phase,
                    expected.words().get(first).copied().unwrap_or_default(),
                    gpu.words().get(first).copied().unwrap_or_default(),
                )
                .into());
            }
        }
    }

    let terminal = states
        .iter()
        .filter(|state| state.phase == Phase::Finished)
        .count();
    println!(
        "{{\"kind\":\"cuda-resident-generated-parity\",\"parity\":true,\"lanes\":{LANES},\"steps\":{steps},\"transitions\":{transitions},\"terminalLanes\":{terminal},\"backend\":\"{}\",\"gpu\":\"{}\"}}",
        identity.backend,
        identity.name.replace('"', "\\\""),
    );
    Ok(())
}
