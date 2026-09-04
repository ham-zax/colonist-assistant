use colonist_catan_core::{GameState, SyntheticBoardGenerator};
use colonist_catan_search::{
    CudaSimAgentSearchConfig, CudaSimArenaConfig, CudaSimArenaResult, CudaSimEngine,
    CudaSimSearchedArenaResult, cuda_sim_board_seed,
};

const GAME_SEED: u64 = 0x91d5_1123_a77c_4401;
const SEARCH_SEED: u64 = 0x441a_70c3_9e20_8817;
const BOARD_SEED: u64 = 0x7fa4_61b2_d0e1_3009;
const PROFILE: [u8; 5] = [51; 5];

fn board_generator(players: u8) -> SyntheticBoardGenerator {
    if players == 4 {
        SyntheticBoardGenerator::Classic4pV1
    } else {
        SyntheticBoardGenerator::LegacyRandomizedV1
    }
}

fn paired_states(players: u8, blocks: usize) -> Vec<GameState> {
    let mut states = Vec::with_capacity(blocks * players as usize);
    for block in 0..blocks {
        let board_seed = cuda_sim_board_seed(BOARD_SEED, block as u64);
        for _seat in 0..players {
            let mut state = GameState::from_generator(board_generator(players), board_seed, players)
                .expect("searched arena generator must support configured player count");
            state.player_trades_enabled = true;
            states.push(state);
        }
    }
    states
}

fn append_arena(destination: &mut CudaSimArenaResult, source: CudaSimArenaResult) {
    destination.games.extend(source.games);
    for player in 0..4 {
        destination.wins[player] = destination.wins[player].saturating_add(source.wins[player]);
    }
    destination.terminal_games = destination
        .terminal_games
        .saturating_add(source.terminal_games);
    destination.truncated_games = destination
        .truncated_games
        .saturating_add(source.truncated_games);
    destination.total_actions = destination
        .total_actions
        .saturating_add(source.total_actions);
}

fn chunked(
    engine: &mut CudaSimEngine,
    states: &[GameState],
    players: u8,
    arena: CudaSimArenaConfig,
    search: CudaSimAgentSearchConfig,
) -> Result<CudaSimSearchedArenaResult, Box<dyn std::error::Error>> {
    let mut aggregate = CudaSimSearchedArenaResult {
        arena: CudaSimArenaResult {
            games: Vec::new(),
            wins: [0; 4],
            terminal_games: 0,
            truncated_games: 0,
            total_actions: 0,
        },
        candidate_decisions: 0,
        root_actions_evaluated: 0,
    };
    let chunk_games = players as usize;
    for (chunk_index, chunk) in states.chunks(chunk_games).enumerate() {
        let game_offset = chunk_index * chunk_games;
        let result = engine.run_searched_candidate_chunk_with_seed_blocks(
            chunk,
            PROFILE,
            PROFILE,
            arena,
            search,
            GAME_SEED,
            SEARCH_SEED,
            game_offset,
            players as usize,
        )?;
        append_arena(&mut aggregate.arena, result.arena);
        aggregate.candidate_decisions = aggregate
            .candidate_decisions
            .saturating_add(result.candidate_decisions);
        aggregate.root_actions_evaluated = aggregate
            .root_actions_evaluated
            .saturating_add(result.root_actions_evaluated);
    }
    Ok(aggregate)
}

fn verify_players(
    engine: &mut CudaSimEngine,
    players: u8,
    blocks: usize,
) -> Result<(usize, u64, u64), Box<dyn std::error::Error>> {
    let states = paired_states(players, blocks);
    let arena = CudaSimArenaConfig {
        max_actions: 1_024,
        max_turns: 40,
    };
    let search = CudaSimAgentSearchConfig {
        root_samples: 4,
        rollouts_per_action: 4,
        rollout_steps: 8,
    };
    let monolithic = engine.run_searched_candidate_chunk_with_seed_blocks(
        &states,
        PROFILE,
        PROFILE,
        arena,
        search,
        GAME_SEED,
        SEARCH_SEED,
        0,
        players as usize,
    )?;
    let block_chunked = chunked(engine, &states, players, arena, search)?;
    if monolithic != block_chunked {
        return Err(format!(
            "searched arena changed with block-aligned chunking for {players}P: monolithic={monolithic:?} chunked={block_chunked:?}"
        )
        .into());
    }
    if monolithic.candidate_decisions == 0 || monolithic.root_actions_evaluated == 0 {
        return Err(
            format!("searched arena did not execute candidate search for {players}P").into(),
        );
    }
    if monolithic.arena.games.len() != states.len() {
        return Err(format!("searched arena lost games for {players}P").into());
    }
    for (lane, game) in monolithic.arena.games.iter().enumerate() {
        if game.actions > arena.max_actions {
            return Err(
                format!("searched arena action bound exceeded lane={lane} {players}P").into(),
            );
        }
        let expected_truncated = !game.game.terminal
            && (game.game.turn >= arena.max_turns || game.actions >= arena.max_actions);
        if game.truncated != expected_truncated {
            return Err(
                format!("searched arena truncation mismatch lane={lane} {players}P").into(),
            );
        }
    }
    Ok((
        monolithic.arena.games.len(),
        monolithic.candidate_decisions,
        monolithic.root_actions_evaluated,
    ))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut engine = CudaSimEngine::new()?;
    let p3 = verify_players(&mut engine, 3, 2)?;
    let p4 = verify_players(&mut engine, 4, 2)?;
    println!(
        "{{\"kind\":\"cuda-resident-searched-arena\",\"parity\":true,\"p3Games\":{},\"p3CandidateDecisions\":{},\"p3RootProposals\":{},\"p4Games\":{},\"p4CandidateDecisions\":{},\"p4RootProposals\":{},\"chunkParity\":true,\"gpu\":\"{}\"}}",
        p3.0,
        p3.1,
        p3.2,
        p4.0,
        p4.1,
        p4.2,
        engine.device_identity().name,
    );
    Ok(())
}
