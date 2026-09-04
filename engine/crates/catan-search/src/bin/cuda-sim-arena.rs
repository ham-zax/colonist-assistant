use colonist_catan_core::GameState;
use colonist_catan_search::{CudaSimArenaConfig, CudaSimEngine};

const GAMES: usize = 64;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let states = (0..GAMES)
        .map(|game| {
            let players = if game % 2 == 0 { 3 } else { 4 };
            let mut state = GameState::randomized_base_v1(120_000 + game as u64 * 101, players);
            state.player_trades_enabled = false;
            state
        })
        .collect::<Vec<_>>();

    let config = CudaSimArenaConfig {
        max_actions: 4_096,
        max_turns: 160,
    };
    let seed = 0x7d31_cafe_1234_9876;
    let mut engine = CudaSimEngine::new()?;
    engine.upload_states(&states)?;
    let result = engine.run_arena_games(config, seed)?;
    let packed = engine.download_packed_states()?;

    for (lane, (game, state)) in result.games.iter().zip(packed.iter()).enumerate() {
        let packed_summary = state.summary();
        if game.game != packed_summary {
            return Err(format!(
                "arena summary mismatch lane={lane}: compact={:?} packed={packed_summary:?}",
                game.game,
            )
            .into());
        }
        if game.actions > 4_096 {
            return Err(format!("arena action bound exceeded lane={lane}: {}", game.actions).into());
        }
        if game.truncated != (!game.game.terminal && (game.game.turn >= 160 || game.actions >= 4_096)) {
            return Err(format!("arena truncation mismatch lane={lane}: {game:?}").into());
        }
    }

    let campaign = engine.run_arena_campaign(&states, config, seed, 7)?;
    if campaign != result {
        return Err("arena campaign changed when resident chunk size changed".into());
    }
    let neutral = [51u8; 5];
    let profiled = engine.run_rotating_profile_campaign(
        &states,
        neutral,
        neutral,
        config,
        seed,
        11,
    )?;
    if profiled != result {
        return Err("neutral rotating profile assignment changed the campaign".into());
    }

    let wins = result.wins;
    println!(
        "{{\"kind\":\"cuda-resident-arena\",\"games\":{},\"terminalGames\":{},\"truncatedGames\":{},\"totalActions\":{},\"chunkParity\":true,\"profileParity\":true,\"wins\":[{},{},{},{}],\"gpu\":\"{}\"}}",
        result.games.len(),
        result.terminal_games,
        result.truncated_games,
        result.total_actions,
        wins[0],
        wins[1],
        wins[2],
        wins[3],
        engine.device_identity().name,
    );
    Ok(())
}
