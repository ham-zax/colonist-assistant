use std::env;
use std::time::Instant;

use colonist_catan_core::GameState;
use colonist_catan_search::{
    CudaSimAgentSearchConfig, CudaSimArenaConfig, CudaSimArenaGameSummary, CudaSimArenaResult,
    CudaSimEngine, CudaSimPolicyProfile, cuda_sim_board_seed,
};
use serde::Serialize;

const NEUTRAL_PROFILE: CudaSimPolicyProfile = [51; 5];

#[derive(Clone, Debug)]
struct Config {
    players: Vec<u8>,
    blocks: usize,
    board_seed: u64,
    game_seed: u64,
    search_seed: u64,
    max_turns: u32,
    max_actions: u32,
    chunk_blocks: usize,
    player_trades_enabled: bool,
    max_truncation_rate: f64,
    candidate_profile: CudaSimPolicyProfile,
    baseline_profile: CudaSimPolicyProfile,
    search: CudaSimAgentSearchConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: vec![3, 4],
            blocks: 64,
            board_seed: 31_261_000,
            game_seed: 31_262_000,
            search_seed: 31_263_000,
            max_turns: 600,
            max_actions: 20_000,
            chunk_blocks: 32,
            player_trades_enabled: true,
            max_truncation_rate: 0.01,
            candidate_profile: NEUTRAL_PROFILE,
            baseline_profile: NEUTRAL_PROFILE,
            search: CudaSimAgentSearchConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileReport {
    balanced: u8,
    expansion: u8,
    city_development: u8,
    trade_flexible: u8,
    trade_resistant: u8,
}

impl From<CudaSimPolicyProfile> for ProfileReport {
    fn from(value: CudaSimPolicyProfile) -> Self {
        Self {
            balanced: value[0],
            expansion: value[1],
            city_development: value[2],
            trade_flexible: value[3],
            trade_resistant: value[4],
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct DeviceReport {
    backend: &'static str,
    ordinal: usize,
    name: String,
    compute_capability: [i32; 2],
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchReport {
    root_samples: usize,
    rollouts_per_action: usize,
    rollout_steps: usize,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ConfigReport {
    players: Vec<u8>,
    blocks_per_player_count: usize,
    games_per_block: &'static str,
    board_seed: u64,
    game_seed: u64,
    search_seed: u64,
    max_turns: u32,
    max_actions: u32,
    chunk_blocks: usize,
    player_trades_enabled: bool,
    max_truncation_rate: f64,
    candidate_agent: &'static str,
    baseline_agent: &'static str,
    search_semantics: &'static str,
    board_matching: &'static str,
    chance_matching: &'static str,
    candidate_seat_rotation: &'static str,
    candidate_profile: ProfileReport,
    baseline_profile: ProfileReport,
    search: SearchReport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct CandidateSeatOutcome {
    seat: u8,
    terminal: bool,
    truncated: bool,
    outcome: &'static str,
    winner: Option<u8>,
    candidate_victory_points: u8,
    best_opponent_victory_points: u8,
    candidate_margin: i16,
    turn: u32,
    actions: u32,
    truncation_seat: Option<u8>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct MatchedBlockResult {
    block_id: usize,
    seed: u64,
    candidate_seat_outcomes: Vec<CandidateSeatOutcome>,
    candidate_wins: u64,
    mean_candidate_victory_points: f64,
    mean_best_opponent_victory_points: f64,
    mean_candidate_margin: f64,
    total_truncations: u64,
    candidate_seat_truncations: u64,
    opponent_seat_truncations: u64,
    unattributed_truncations: u64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PlayerResult {
    players: u8,
    blocks: usize,
    games: u64,
    terminal_games: u64,
    truncated_games: u64,
    candidate_wins: u64,
    opponent_wins: u64,
    candidate_win_rate_terminal: f64,
    candidate_win_rate_all_games: f64,
    candidate_win_rate_95ci: Option<[f64; 2]>,
    theoretical_fair_share: f64,
    candidate_games_by_seat: [u64; 4],
    candidate_wins_by_seat: [u64; 4],
    mean_candidate_victory_points: f64,
    mean_best_opponent_victory_points: f64,
    mean_victory_margin: f64,
    mean_turns: f64,
    mean_actions: f64,
    total_actions: u64,
    candidate_decisions: u64,
    root_proposals_evaluated: u64,
    elapsed_ms: f64,
    games_per_second: f64,
    complete_games_per_second: f64,
    actions_per_second: f64,
    root_proposals_per_second: f64,
    matched_blocks: Vec<MatchedBlockResult>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    kind: &'static str,
    device: DeviceReport,
    config: ConfigReport,
    results: Vec<PlayerResult>,
}

#[derive(Debug, Default)]
struct Accumulator {
    games: u64,
    terminal_games: u64,
    truncated_games: u64,
    candidate_wins: u64,
    candidate_games_by_seat: [u64; 4],
    candidate_wins_by_seat: [u64; 4],
    candidate_vp: u64,
    best_opponent_vp: u64,
    turns: u64,
    total_actions: u64,
    candidate_decisions: u64,
    root_proposals: u64,
    elapsed_seconds: f64,
    matched_blocks: Vec<MatchedBlockResult>,
}

fn matched_block_result(
    players: u8,
    block_id: usize,
    board_seed: u64,
    games: &[CudaSimArenaGameSummary],
) -> MatchedBlockResult {
    let mut candidate_wins = 0u64;
    let mut candidate_vp = 0u64;
    let mut best_opponent_vp = 0u64;
    let mut total_truncations = 0u64;
    let mut candidate_seat_truncations = 0u64;
    let mut opponent_seat_truncations = 0u64;
    let mut unattributed_truncations = 0u64;
    let mut outcomes = Vec::with_capacity(games.len());

    for (candidate, game) in games.iter().enumerate() {
        let candidate = candidate as u8;
        let candidate_points = game.game.victory_points[candidate as usize];
        let opponent_points = game
            .game
            .victory_points
            .iter()
            .copied()
            .enumerate()
            .filter(|(player, _)| *player < players as usize && *player != candidate as usize)
            .map(|(_, vp)| vp)
            .max()
            .unwrap_or(0);
        let candidate_win = game.game.terminal && game.game.winner == Some(candidate);
        candidate_wins = candidate_wins.saturating_add(u64::from(candidate_win));
        candidate_vp = candidate_vp.saturating_add(u64::from(candidate_points));
        best_opponent_vp = best_opponent_vp.saturating_add(u64::from(opponent_points));

        let truncation_seat = if game.truncated && game.game.turn > 0 {
            Some(((game.game.turn - 1) % u32::from(players)) as u8)
        } else {
            None
        };
        if game.truncated {
            total_truncations = total_truncations.saturating_add(1);
            match truncation_seat {
                Some(seat) if seat == candidate => {
                    candidate_seat_truncations = candidate_seat_truncations.saturating_add(1);
                }
                Some(_) => {
                    opponent_seat_truncations = opponent_seat_truncations.saturating_add(1);
                }
                None => {
                    unattributed_truncations = unattributed_truncations.saturating_add(1);
                }
            }
        }
        outcomes.push(CandidateSeatOutcome {
            seat: candidate,
            terminal: game.game.terminal,
            truncated: game.truncated,
            outcome: if game.truncated {
                "truncated"
            } else if candidate_win {
                "win"
            } else {
                "loss"
            },
            winner: game.game.winner,
            candidate_victory_points: candidate_points,
            best_opponent_victory_points: opponent_points,
            candidate_margin: i16::from(candidate_points) - i16::from(opponent_points),
            turn: game.game.turn,
            actions: game.actions,
            truncation_seat,
        });
    }

    let games = games.len().max(1) as f64;
    let mean_candidate_victory_points = candidate_vp as f64 / games;
    let mean_best_opponent_victory_points = best_opponent_vp as f64 / games;
    MatchedBlockResult {
        block_id,
        seed: cuda_sim_board_seed(board_seed, block_id as u64),
        candidate_seat_outcomes: outcomes,
        candidate_wins,
        mean_candidate_victory_points,
        mean_best_opponent_victory_points,
        mean_candidate_margin: mean_candidate_victory_points - mean_best_opponent_victory_points,
        total_truncations,
        candidate_seat_truncations,
        opponent_seat_truncations,
        unattributed_truncations,
    }
}

impl Accumulator {
    fn absorb(
        &mut self,
        players: u8,
        game_offset: usize,
        board_seed: u64,
        result: &CudaSimArenaResult,
        candidate_decisions: u64,
        root_proposals: u64,
        elapsed_seconds: f64,
    ) {
        self.games = self.games.saturating_add(result.games.len() as u64);
        self.terminal_games = self.terminal_games.saturating_add(result.terminal_games);
        self.truncated_games = self.truncated_games.saturating_add(result.truncated_games);
        self.total_actions = self.total_actions.saturating_add(result.total_actions);
        self.candidate_decisions = self.candidate_decisions.saturating_add(candidate_decisions);
        self.root_proposals = self.root_proposals.saturating_add(root_proposals);
        self.elapsed_seconds += elapsed_seconds;
        assert_eq!(game_offset % players as usize, 0);
        assert_eq!(result.games.len() % players as usize, 0);
        let first_block = game_offset / players as usize;
        for (local_block, games) in result.games.chunks_exact(players as usize).enumerate() {
            self.matched_blocks.push(matched_block_result(
                players,
                first_block + local_block,
                board_seed,
                games,
            ));
        }
        for (local, game) in result.games.iter().enumerate() {
            let global = game_offset + local;
            let candidate = global % players as usize;
            self.candidate_games_by_seat[candidate] =
                self.candidate_games_by_seat[candidate].saturating_add(1);
            if game.game.winner == Some(candidate as u8) {
                self.candidate_wins = self.candidate_wins.saturating_add(1);
                self.candidate_wins_by_seat[candidate] =
                    self.candidate_wins_by_seat[candidate].saturating_add(1);
            }
            self.candidate_vp = self
                .candidate_vp
                .saturating_add(game.game.victory_points[candidate] as u64);
            let opponent_best = game
                .game
                .victory_points
                .iter()
                .copied()
                .enumerate()
                .filter(|(player, _)| *player < players as usize && *player != candidate)
                .map(|(_, vp)| vp)
                .max()
                .unwrap_or(0);
            self.best_opponent_vp = self.best_opponent_vp.saturating_add(opponent_best as u64);
            self.turns = self.turns.saturating_add(game.game.turn as u64);
        }
    }

    fn into_result(self, players: u8, blocks: usize) -> PlayerResult {
        let games = self.games.max(1) as f64;
        let elapsed = self.elapsed_seconds.max(f64::EPSILON);
        let candidate_vp = self.candidate_vp as f64 / games;
        let opponent_vp = self.best_opponent_vp as f64 / games;
        PlayerResult {
            players,
            blocks,
            games: self.games,
            terminal_games: self.terminal_games,
            truncated_games: self.truncated_games,
            candidate_wins: self.candidate_wins,
            opponent_wins: self.terminal_games.saturating_sub(self.candidate_wins),
            candidate_win_rate_terminal: if self.terminal_games == 0 {
                0.0
            } else {
                self.candidate_wins as f64 / self.terminal_games as f64
            },
            candidate_win_rate_all_games: self.candidate_wins as f64 / games,
            candidate_win_rate_95ci: wilson_interval(self.candidate_wins, self.terminal_games),
            theoretical_fair_share: 1.0 / players as f64,
            candidate_games_by_seat: self.candidate_games_by_seat,
            candidate_wins_by_seat: self.candidate_wins_by_seat,
            mean_candidate_victory_points: candidate_vp,
            mean_best_opponent_victory_points: opponent_vp,
            mean_victory_margin: candidate_vp - opponent_vp,
            mean_turns: self.turns as f64 / games,
            mean_actions: self.total_actions as f64 / games,
            total_actions: self.total_actions,
            candidate_decisions: self.candidate_decisions,
            root_proposals_evaluated: self.root_proposals,
            elapsed_ms: self.elapsed_seconds * 1_000.0,
            games_per_second: self.games as f64 / elapsed,
            complete_games_per_second: self.terminal_games as f64 / elapsed,
            actions_per_second: self.total_actions as f64 / elapsed,
            root_proposals_per_second: self.root_proposals as f64 / elapsed,
            matched_blocks: self.matched_blocks,
        }
    }
}

fn parse_profile(value: &str) -> Result<CudaSimPolicyProfile, String> {
    let values = value
        .split(',')
        .map(|part| {
            part.parse::<u8>()
                .map_err(|_| format!("invalid profile component: {part}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 5 || values.iter().any(|value| *value > 102) {
        return Err("profile must contain exactly five comma-separated values in 0..=102".into());
    }
    Ok([values[0], values[1], values[2], values[3], values[4]])
}

fn parse_players(value: &str) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    for item in value.split(',') {
        let players = item
            .parse::<u8>()
            .map_err(|_| format!("invalid player count: {item}"))?;
        if !(2..=4).contains(&players) {
            return Err(format!("player count must be 2, 3, or 4; got {players}"));
        }
        if !result.contains(&players) {
            result.push(players);
        }
    }
    if result.is_empty() {
        return Err("at least one player count is required".into());
    }
    Ok(result)
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index)
        .ok_or_else(|| format!("missing value after {flag}"))?
        .clone();
    *index += 1;
    Ok(value)
}

fn parse_value<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("invalid value for {flag}: {value}"))
}

fn parse_args() -> Result<Option<Config>, String> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut config = Config::default();
    let mut index = 0usize;
    while index < args.len() {
        let flag = args[index].as_str();
        index += 1;
        match flag {
            "--help" | "-h" => return Ok(None),
            "--players" => config.players = parse_players(&next_value(&args, &mut index, flag)?)?,
            "--blocks" => config.blocks = parse_value(flag, &next_value(&args, &mut index, flag)?)?,
            "--board-seed" | "--seed" => {
                config.board_seed = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--game-seed" => {
                config.game_seed = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--search-seed" => {
                config.search_seed = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--max-turns" => {
                config.max_turns = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--max-actions" => {
                config.max_actions = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--chunk-blocks" => {
                config.chunk_blocks = parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--player-trades" => config.player_trades_enabled = true,
            "--no-player-trades" => config.player_trades_enabled = false,
            "--max-truncation-rate" => {
                config.max_truncation_rate =
                    parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--candidate-profile" => {
                config.candidate_profile = parse_profile(&next_value(&args, &mut index, flag)?)?
            }
            "--baseline-profile" => {
                config.baseline_profile = parse_profile(&next_value(&args, &mut index, flag)?)?
            }
            "--root-samples" => {
                config.search.root_samples =
                    parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--rollouts-per-action" => {
                config.search.rollouts_per_action =
                    parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            "--rollout-steps" => {
                config.search.rollout_steps =
                    parse_value(flag, &next_value(&args, &mut index, flag)?)?
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if config.blocks == 0 {
        return Err("--blocks must be greater than zero".into());
    }
    if config.chunk_blocks == 0 {
        return Err("--chunk-blocks must be greater than zero".into());
    }
    if config.search.root_samples == 0
        || config.search.rollouts_per_action == 0
        || config.search.rollout_steps == 0
    {
        return Err(
            "root samples, rollouts per action, and rollout steps must all be nonzero".into(),
        );
    }
    if !(0.0..=1.0).contains(&config.max_truncation_rate) {
        return Err("--max-truncation-rate must be in 0..=1".into());
    }
    Ok(Some(config))
}

fn print_help() {
    println!(
        "gpu-sim-agent-benchmark\n\
         \nRuns a paired GPU-resident searched-candidate vs GPU-weighted benchmark.\n\
         Each board/chance block is replayed once for every candidate seat.\n\
         This is a v9-shaped matched benchmark, not MaxN: the candidate uses sampled\n\
         GPU root-rollout search with weighted-policy continuations.\n\
         \nOptions:\n\
         \x20 --players 3,4                 Player counts (default: 3,4)\n\
         \x20 --blocks N                    Matched board/chance blocks per player count (default: 64)\n\
         \x20 --chunk-blocks N              Resident blocks per GPU chunk (default: 32)\n\
         \x20 --max-turns N                 Game turn ceiling (default: 600)\n\
         \x20 --max-actions N               Game action ceiling (default: 20000)\n\
         \x20 --root-samples N              GPU root proposals per candidate decision (default: 4)\n\
         \x20 --rollouts-per-action N       GPU rollouts per sampled root (default: 16)\n\
         \x20 --rollout-steps N             Weighted continuation steps per rollout (default: 32)\n\
         \x20 --player-trades               Enable domestic trades (default)\n\
         \x20 --no-player-trades            Disable domestic trades\n\
         \x20 --max-truncation-rate R       Refuse output above this rate (default: 0.01)\n\
         \x20 --candidate-profile a,b,c,d,e Candidate policy profile\n\
         \x20 --baseline-profile a,b,c,d,e  Weighted-opponent policy profile\n\
         \x20 --board-seed N --game-seed N --search-seed N"
    );
}

fn paired_state_chunk(
    players: u8,
    block_offset: usize,
    blocks: usize,
    board_seed: u64,
    player_trades_enabled: bool,
) -> Vec<GameState> {
    let mut states = Vec::with_capacity(blocks * players as usize);
    for local_block in 0..blocks {
        let global_block = block_offset + local_block;
        let seed = cuda_sim_board_seed(board_seed, global_block as u64);
        for _seat in 0..players {
            let mut state = GameState::standard(seed, players);
            state.player_trades_enabled = player_trades_enabled;
            states.push(state);
        }
    }
    states
}

fn wilson_interval(wins: u64, samples: u64) -> Option<[f64; 2]> {
    if samples == 0 {
        return None;
    }
    let n = samples as f64;
    let p = wins as f64 / n;
    let z = 1.959_963_984_540_054_f64;
    let z2 = z * z;
    let denominator = 1.0 + z2 / n;
    let center = (p + z2 / (2.0 * n)) / denominator;
    let radius = z * ((p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt()) / denominator;
    Some([(center - radius).max(0.0), (center + radius).min(1.0)])
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = parse_args().map_err(|error| format!("{error}\nUse --help for usage."))?
    else {
        print_help();
        return Ok(());
    };
    let mut engine = CudaSimEngine::new()?;
    let identity = engine.device_identity().clone();
    let arena = CudaSimArenaConfig {
        max_actions: config.max_actions,
        max_turns: config.max_turns,
    };
    let mut results = Vec::new();

    for players in config.players.iter().copied() {
        let mut accumulator = Accumulator::default();
        let mut block_offset = 0usize;
        while block_offset < config.blocks {
            let chunk_blocks = (config.blocks - block_offset).min(config.chunk_blocks);
            let states = paired_state_chunk(
                players,
                block_offset,
                chunk_blocks,
                config.board_seed,
                config.player_trades_enabled,
            );
            let game_offset = block_offset
                .checked_mul(players as usize)
                .ok_or("game offset overflow")?;
            let started = Instant::now();
            let searched = engine.run_searched_candidate_chunk_with_seed_blocks(
                &states,
                config.candidate_profile,
                config.baseline_profile,
                arena,
                config.search,
                config.game_seed,
                config.search_seed,
                game_offset,
                players as usize,
            )?;
            accumulator.absorb(
                players,
                game_offset,
                config.board_seed,
                &searched.arena,
                searched.candidate_decisions,
                searched.root_actions_evaluated,
                started.elapsed().as_secs_f64(),
            );
            block_offset += chunk_blocks;
        }
        let result = accumulator.into_result(players, config.blocks);
        let truncation_rate = result.truncated_games as f64 / result.games.max(1) as f64;
        if truncation_rate > config.max_truncation_rate {
            return Err(format!(
                "searched strength benchmark refused: {players}P truncated {}/{} games ({:.2}%), above {:.2}%; increase game limits or explicitly relax --max-truncation-rate",
                result.truncated_games,
                result.games,
                truncation_rate * 100.0,
                config.max_truncation_rate * 100.0,
            )
            .into());
        }
        results.push(result);
    }

    let output = Output {
        kind: "cuda-resident-searched-agent-benchmark",
        device: DeviceReport {
            backend: identity.backend,
            ordinal: identity.ordinal,
            name: identity.name,
            compute_capability: [identity.compute_capability.0, identity.compute_capability.1],
        },
        config: ConfigReport {
            players: config.players,
            blocks_per_player_count: config.blocks,
            games_per_block: "players",
            board_seed: config.board_seed,
            game_seed: config.game_seed,
            search_seed: config.search_seed,
            max_turns: config.max_turns,
            max_actions: config.max_actions,
            chunk_blocks: config.chunk_blocks,
            player_trades_enabled: config.player_trades_enabled,
            max_truncation_rate: config.max_truncation_rate,
            candidate_agent: "gpu-root-rollout-search",
            baseline_agent: "gpu-weighted",
            search_semantics: "sampled-root-actions + fixed-step gpu-weighted continuations",
            board_matching: "same board within each seat-rotation block",
            chance_matching: "same chance stream within each seat-rotation block",
            candidate_seat_rotation: "block_seat_0_through_players_minus_1",
            candidate_profile: config.candidate_profile.into(),
            baseline_profile: config.baseline_profile.into(),
            search: SearchReport {
                root_samples: config.search.root_samples,
                rollouts_per_action: config.search.rollouts_per_action,
                rollout_steps: config.search.rollout_steps,
            },
        },
        results,
    };
    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    println!();
    Ok(())
}
