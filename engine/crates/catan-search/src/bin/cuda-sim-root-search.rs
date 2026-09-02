use colonist_catan_core::{GameState, SplitMix64};
use colonist_catan_search::CudaSimEngine;

const LANES: usize = 8;
const ROOT_LIMIT: usize = 6;
const ROLLOUTS: usize = 8;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut states = Vec::with_capacity(LANES);
    for lane in 0..LANES {
        let players = if lane % 2 == 0 { 3 } else { 4 };
        let mut state = GameState::standard(91_000 + lane as u64 * 37, players);
        state.player_trades_enabled = false;
        let mut rng = SplitMix64::new(42_000 + lane as u64);
        for _ in 0..40 {
            if state.is_terminal() {
                break;
            }
            let legal = state.legal_actions();
            let action = legal[rng.range(legal.len())].clone();
            state.apply(&action)?;
        }
        states.push(state);
    }

    let roots = states
        .iter()
        .map(|state| state.legal_actions().into_iter().take(ROOT_LIMIT).collect::<Vec<_>>())
        .collect::<Vec<_>>();

    let mut engine = CudaSimEngine::new()?;
    engine.upload_states(&states)?;
    let zero_step = engine.search_root_actions(&roots, ROLLOUTS, 0, 0xabcdef01)?;

    for (lane, (state, row)) in states.iter().zip(zero_step.rows.iter()).enumerate() {
        for (root, stats) in row.iter().enumerate() {
            let mut expected = state.clone();
            expected.apply(&stats.action)?;
            let actor = state.actor() as usize;
            let actor_vp = expected.players[actor].victory_points() as f32;
            let best_opponent = expected
                .players
                .iter()
                .enumerate()
                .filter(|(player, _)| *player != actor)
                .map(|(_, player)| player.victory_points())
                .max()
                .unwrap_or(0) as f32;
            if stats.samples != ROLLOUTS as u32
                || stats.errors != 0
                || stats.mean_turn != expected.turn as f32
                || stats.mean_victory_points != actor_vp
                || stats.mean_best_opponent_victory_points != best_opponent
            {
                return Err(format!(
                    "zero-step root mismatch lane={lane} root={root} action={:?} stats={stats:?} expected_turn={} expected_actor_vp={actor_vp} expected_opp_vp={best_opponent}",
                    stats.action,
                    expected.turn,
                )
                .into());
            }
        }
    }

    let chunked = engine.search_root_actions(&roots, 4_097, 0, 0x55aa_aa55_1234_5678)?;
    for (lane, (state, row)) in states.iter().zip(chunked.rows.iter()).enumerate() {
        for (root, stats) in row.iter().enumerate() {
            let mut expected = state.clone();
            expected.apply(&stats.action)?;
            let actor = state.actor() as usize;
            let actor_vp = expected.players[actor].victory_points() as f32;
            if stats.samples != 4_097
                || stats.errors != 0
                || stats.mean_turn != expected.turn as f32
                || stats.mean_victory_points != actor_vp
            {
                return Err(format!(
                    "chunked root mismatch lane={lane} root={root} action={:?} stats={stats:?}",
                    stats.action,
                )
                .into());
            }
        }
    }

    let rollout = engine.search_root_actions(&roots, 16, 32, 0x1234_5678_9abc_def0)?;
    for (lane, row) in rollout.rows.iter().enumerate() {
        for (root, stats) in row.iter().enumerate() {
            if stats.samples != 16 || stats.errors != 0 {
                return Err(format!(
                    "rollout root failed lane={lane} root={root} action={:?} stats={stats:?}",
                    stats.action,
                )
                .into());
            }
        }
    }

    println!(
        "{{\"kind\":\"cuda-resident-root-search\",\"parity\":true,\"baseStates\":{},\"roots\":{},\"zeroStepRollouts\":{},\"chunkedRollouts\":4097,\"searchRollouts\":16,\"searchSteps\":32,\"gpu\":\"{}\"}}",
        states.len(),
        roots.iter().map(Vec::len).sum::<usize>(),
        ROLLOUTS,
        engine.device_identity().name,
    );
    Ok(())
}
