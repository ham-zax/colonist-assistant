use colonist_catan_core::GameState;
use colonist_catan_search::CudaSimEngine;

const LANES: usize = 128;
const STEPS: usize = 256;
const SEED: u64 = 0x6a09_e667_f3bc_c909;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let states = (0..LANES)
        .map(|lane| {
            let players = if lane % 2 == 0 { 3 } else { 4 };
            let mut state = GameState::randomized_base_v1(120_000 + lane as u64 * 31, players);
            state.player_trades_enabled = false;
            state
        })
        .collect::<Vec<_>>();

    let mut bulk = CudaSimEngine::new()?;
    bulk.upload_states(&states)?;
    bulk.seed_rollout_rng(SEED)?;

    let mut stepped = CudaSimEngine::new()?;
    stepped.upload_states(&states)?;
    stepped.seed_rollout_rng(SEED)?;

    bulk.run_rollout_steps(STEPS)?;
    for _ in 0..STEPS {
        stepped.generate_rollout_actions()?;
        stepped.apply_generated_actions()?;
    }

    let bulk_states = bulk.download_packed_states()?;
    let stepped_states = stepped.download_packed_states()?;
    if bulk_states != stepped_states {
        let lane = bulk_states
            .iter()
            .zip(&stepped_states)
            .position(|(left, right)| left != right)
            .unwrap_or(usize::MAX);
        return Err(format!("bulk CUDA rollout diverged from stepped CUDA rollout at lane {lane}").into());
    }

    println!(
        "{{\"kind\":\"cuda-resident-bulk-parity\",\"parity\":true,\"lanes\":{LANES},\"steps\":{STEPS},\"transitions\":{}}}",
        LANES * STEPS,
    );
    Ok(())
}
