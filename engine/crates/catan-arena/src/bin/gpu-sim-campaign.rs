use std::env;
use std::time::Instant;

use colonist_catan_core::{GameState, SyntheticBoardGenerator};
use colonist_catan_search::{
    CudaSimArenaConfig, CudaSimArenaResult, CudaSimEngine, CudaSimPolicyProfile,
    cuda_sim_board_seed,
};
use serde::Serialize;

const NEUTRAL_PROFILE: CudaSimPolicyProfile = [51; 5];

#[derive(Clone, Debug)]
struct Config {
    players: Vec<u8>,
    games: usize,
    board_seed: u64,
    simulation_seed: u64,
    max_turns: u32,
    max_actions: u32,
    chunk_games: usize,
    grid_step: Option<u8>,
    player_trades_enabled: bool,
    paired_seat_blocks: bool,
    max_truncation_rate: f64,
    baseline_profile: CudaSimPolicyProfile,
    candidate_profiles: Vec<CudaSimPolicyProfile>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: vec![3, 4],
            games: 256,
            board_seed: 20_261_000,
            simulation_seed: 20_262_000,
            max_turns: 600,
            max_actions: 20_000,
            chunk_games: 4_096,
            grid_step: None,
            player_trades_enabled: false,
            paired_seat_blocks: false,
            max_truncation_rate: 0.01,
            baseline_profile: NEUTRAL_PROFILE,
            candidate_profiles: Vec::new(),
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
struct CampaignConfigReport {
    players: Vec<u8>,
    games_per_trial: usize,
    board_seed: u64,
    simulation_seed: u64,
    max_turns: u32,
    max_actions: u32,
    chunk_games: usize,
    grid_step: Option<u8>,
    candidate_profile_count: usize,
    player_trades_enabled: bool,
    paired_seat_blocks: bool,
    max_truncation_rate: f64,
    candidate_seat_rotation: &'static str,
    seed_matching: &'static str,
    baseline_profile: ProfileReport,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TrialReport {
    players: u8,
    board_generator: &'static str,
    candidate_profile: ProfileReport,
    games: u64,
    terminal_games: u64,
    truncated_games: u64,
    candidate_wins: u64,
    opponent_wins: u64,
    candidate_win_rate_terminal: f64,
    candidate_win_rate_all_games: f64,
    candidate_win_rate_95ci: Option<[f64; 2]>,
    candidate_games_by_seat: [u64; 4],
    candidate_wins_by_seat: [u64; 4],
    mean_candidate_victory_points: f64,
    mean_best_opponent_victory_points: f64,
    mean_victory_margin: f64,
    mean_turns: f64,
    mean_actions: f64,
    total_actions: u64,
    elapsed_ms: f64,
    games_per_second: f64,
    actions_per_second: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProfileRankingReport {
    rank: usize,
    players: u8,
    candidate_profile: ProfileReport,
    games: u64,
    terminal_games: u64,
    candidate_wins: u64,
    candidate_win_rate_terminal: f64,
    candidate_win_rate_95ci: Option<[f64; 2]>,
    mean_victory_margin: f64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct Output {
    kind: &'static str,
    device: DeviceReport,
    config: CampaignConfigReport,
    trials: Vec<TrialReport>,
    ranking: Vec<ProfileRankingReport>,
}

#[derive(Clone, Debug, Default)]
struct TrialAccumulator {
    games: u64,
    terminal_games: u64,
    truncated_games: u64,
    candidate_wins: u64,
    candidate_games_by_seat: [u64; 4],
    candidate_wins_by_seat: [u64; 4],
    candidate_victory_points: u64,
    best_opponent_victory_points: u64,
    turns: u64,
    total_actions: u64,
    elapsed_seconds: f64,
}

impl TrialAccumulator {
    fn absorb(
        &mut self,
        players: u8,
        global_game_offset: usize,
        result: &CudaSimArenaResult,
        elapsed_seconds: f64,
    ) {
        self.games = self.games.saturating_add(result.games.len() as u64);
        self.terminal_games = self.terminal_games.saturating_add(result.terminal_games);
        self.truncated_games = self.truncated_games.saturating_add(result.truncated_games);
        self.total_actions = self.total_actions.saturating_add(result.total_actions);
        self.elapsed_seconds += elapsed_seconds;

        for (local_index, game) in result.games.iter().enumerate() {
            let global_index = global_game_offset + local_index;
            let candidate = global_index % players as usize;
            self.candidate_games_by_seat[candidate] =
                self.candidate_games_by_seat[candidate].saturating_add(1);
            if game.game.winner == Some(candidate as u8) {
                self.candidate_wins = self.candidate_wins.saturating_add(1);
                self.candidate_wins_by_seat[candidate] =
                    self.candidate_wins_by_seat[candidate].saturating_add(1);
            }
            self.candidate_victory_points = self
                .candidate_victory_points
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
            self.best_opponent_victory_points = self
                .best_opponent_victory_points
                .saturating_add(opponent_best as u64);
            self.turns = self.turns.saturating_add(game.game.turn as u64);
        }
    }

    fn into_report(self, players: u8, candidate_profile: CudaSimPolicyProfile) -> TrialReport {
        let games = self.games.max(1) as f64;
        let opponent_wins = self.terminal_games.saturating_sub(self.candidate_wins);
        let mean_candidate_vp = self.candidate_victory_points as f64 / games;
        let mean_best_opponent_vp = self.best_opponent_victory_points as f64 / games;
        TrialReport {
            players,
            board_generator: strength_board_generator(players).serialized_id(),
            candidate_profile: candidate_profile.into(),
            games: self.games,
            terminal_games: self.terminal_games,
            truncated_games: self.truncated_games,
            candidate_wins: self.candidate_wins,
            opponent_wins,
            candidate_win_rate_terminal: if self.terminal_games == 0 {
                0.0
            } else {
                self.candidate_wins as f64 / self.terminal_games as f64
            },
            candidate_win_rate_all_games: self.candidate_wins as f64 / games,
            candidate_win_rate_95ci: wilson_interval(self.candidate_wins, self.terminal_games),
            candidate_games_by_seat: self.candidate_games_by_seat,
            candidate_wins_by_seat: self.candidate_wins_by_seat,
            mean_candidate_victory_points: mean_candidate_vp,
            mean_best_opponent_victory_points: mean_best_opponent_vp,
            mean_victory_margin: mean_candidate_vp - mean_best_opponent_vp,
            mean_turns: self.turns as f64 / games,
            mean_actions: self.total_actions as f64 / games,
            total_actions: self.total_actions,
            elapsed_ms: self.elapsed_seconds * 1_000.0,
            games_per_second: self.games as f64 / self.elapsed_seconds.max(f64::EPSILON),
            actions_per_second: self.total_actions as f64 / self.elapsed_seconds.max(f64::EPSILON),
        }
    }
}

fn parse_profile(value: &str) -> Result<CudaSimPolicyProfile, String> {
    let values = value
        .split(',')
        .map(|part| {
            part.parse::<u8>()
                .map_err(|_| format!("invalid policy-profile component: {part}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    if values.len() != 5 {
        return Err(format!(
            "policy profile must have exactly 5 comma-separated values, got {}",
            values.len()
        ));
    }
    if let Some(value) = values.iter().copied().find(|value| *value > 102) {
        return Err(format!(
            "policy-profile values must be in 0..=102; got {value}"
        ));
    }
    Ok([values[0], values[1], values[2], values[3], values[4]])
}

fn grid_values(step: u8) -> Result<Vec<u8>, String> {
    if step == 0 {
        return Err("--grid-step must be greater than zero".into());
    }
    let mut result = Vec::new();
    let mut value = 0u16;
    loop {
        result.push(value.min(102) as u8);
        if value >= 102 {
            break;
        }
        value = (value + step as u16).min(102);
    }
    Ok(result)
}

fn append_grid_profiles(config: &mut Config) -> Result<(), String> {
    let Some(step) = config.grid_step else {
        return Ok(());
    };
    let values = grid_values(step)?;
    let dimensions = if config.player_trades_enabled { 5 } else { 3 };
    let grid_size = values.len().saturating_pow(dimensions);
    if grid_size > 4_096 {
        return Err(format!(
            "--grid-step {step} expands to {grid_size} profiles in {dimensions} dimensions; choose a larger step (maximum 4096 profiles per invocation)"
        ));
    }
    for balanced in values.iter().copied() {
        for expansion in values.iter().copied() {
            for city_development in values.iter().copied() {
                if config.player_trades_enabled {
                    for trade_flexible in values.iter().copied() {
                        for trade_resistant in values.iter().copied() {
                            config.candidate_profiles.push([
                                balanced,
                                expansion,
                                city_development,
                                trade_flexible,
                                trade_resistant,
                            ]);
                        }
                    }
                } else {
                    config.candidate_profiles.push([
                        balanced,
                        expansion,
                        city_development,
                        config.baseline_profile[3],
                        config.baseline_profile[4],
                    ]);
                }
            }
        }
    }
    Ok(())
}

fn parse_players(value: &str) -> Result<Vec<u8>, String> {
    let mut players = Vec::new();
    for item in value.split(',') {
        let count = item
            .parse::<u8>()
            .map_err(|_| format!("invalid player count: {item}"))?;
        if !(2..=4).contains(&count) {
            return Err(format!("player count must be 2, 3, or 4; got {count}"));
        }
        if !players.contains(&count) {
            players.push(count);
        }
    }
    if players.is_empty() {
        return Err("at least one player count is required".into());
    }
    Ok(players)
}

fn parse_value<T: std::str::FromStr>(flag: &str, value: &str) -> Result<T, String> {
    value
        .parse::<T>()
        .map_err(|_| format!("invalid value for {flag}: {value}"))
}

fn next_value(args: &[String], index: &mut usize, flag: &str) -> Result<String, String> {
    let value = args
        .get(*index)
        .ok_or_else(|| format!("missing value after {flag}"))?
        .clone();
    *index += 1;
    Ok(value)
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
            "--players" => {
                config.players = parse_players(&next_value(&args, &mut index, flag)?)?;
            }
            "--games" => {
                config.games = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--board-seed" | "--seed" => {
                config.board_seed = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--simulation-seed" => {
                config.simulation_seed = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--max-turns" => {
                config.max_turns = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--max-actions" => {
                config.max_actions = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--chunk-games" => {
                config.chunk_games = parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--grid-step" => {
                config.grid_step = Some(parse_value(flag, &next_value(&args, &mut index, flag)?)?);
            }
            "--player-trades" => {
                config.player_trades_enabled = true;
            }
            "--paired-seat-blocks" => {
                config.paired_seat_blocks = true;
            }
            "--max-truncation-rate" => {
                config.max_truncation_rate =
                    parse_value(flag, &next_value(&args, &mut index, flag)?)?;
            }
            "--baseline-profile" => {
                config.baseline_profile = parse_profile(&next_value(&args, &mut index, flag)?)?;
            }
            "--candidate-profile" => {
                config
                    .candidate_profiles
                    .push(parse_profile(&next_value(&args, &mut index, flag)?)?);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    if config.games == 0 {
        return Err("--games must be greater than zero".into());
    }
    if config.chunk_games == 0 {
        return Err("--chunk-games must be greater than zero".into());
    }
    if !(0.0..=1.0).contains(&config.max_truncation_rate) {
        return Err("--max-truncation-rate must be in 0..=1".into());
    }
    if config.paired_seat_blocks {
        for players in config.players.iter().copied() {
            if config.games % players as usize != 0 {
                return Err(format!(
                    "--paired-seat-blocks requires --games to be divisible by every player count; {} games is not divisible by {players}",
                    config.games
                ));
            }
        }
    }
    append_grid_profiles(&mut config)?;
    if config.candidate_profiles.is_empty() {
        config.candidate_profiles.push(NEUTRAL_PROFILE);
    }
    let mut unique_profiles = Vec::with_capacity(config.candidate_profiles.len());
    for profile in config.candidate_profiles.drain(..) {
        if !unique_profiles.contains(&profile) {
            unique_profiles.push(profile);
        }
    }
    config.candidate_profiles = unique_profiles;
    Ok(Some(config))
}

fn print_help() {
    println!(
        "gpu-sim-campaign\n\
         \nRuns matched strength campaigns entirely on the GPU after initial state creation.\n\
         \nOptions:\n\
         \x20 --players 3,4                 Player counts to evaluate (default: 3,4)\n\
         \x20 --games N                     Games per player-count/profile trial (default: 256)\n\
         \x20 --board-seed N                Base board seed (default: 20261000)\n\
         \x20 --simulation-seed N           Base GPU RNG seed (default: 20262000)\n\
         \x20 --max-turns N                 Per-game turn ceiling (default: 600)\n\
         \x20 --max-actions N               Per-game action ceiling (default: 20000)\n\
         \x20 --chunk-games N               Max resident games per GPU chunk (default: 4096)\n\
         \x20 --grid-step N                  Grid 3 policy dimensions, or all 5 with --player-trades\n\
         \x20 --player-trades                Enable player-to-player trade simulation\n\
         \x20 --paired-seat-blocks           Reuse board/chance seed across each seat rotation block\n\
         \x20 --max-truncation-rate R        Reject strength rankings above this rate (default: 0.01)\n\
         \x20 --baseline-profile a,b,c,d,e  Baseline policy profile (0..102 each)\n\
         \x20 --candidate-profile a,b,c,d,e Candidate profile; repeat for an in-process sweep\n\
         \nProfile fields: balanced, expansion, city/development, trade-flexible, trade-resistant."
    );
}

fn strength_board_generator(players: u8) -> SyntheticBoardGenerator {
    if players == 4 {
        SyntheticBoardGenerator::Classic4pV1
    } else {
        SyntheticBoardGenerator::LegacyRandomizedV1
    }
}

fn base_state_chunk(
    players: u8,
    global_game_offset: usize,
    games: usize,
    seed: u64,
    player_trades_enabled: bool,
    paired_seat_blocks: bool,
) -> Vec<GameState> {
    (0..games)
        .map(|local_game| {
            let global_game = global_game_offset + local_game;
            let board_index = if paired_seat_blocks {
                global_game / players as usize
            } else {
                global_game
            };
            let board_seed = cuda_sim_board_seed(seed, board_index as u64);
            let mut state =
                GameState::from_generator(strength_board_generator(players), board_seed, players)
                    .expect("strength benchmark generator must support configured player count");
            state.player_trades_enabled = player_trades_enabled;
            state
        })
        .collect()
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

fn ensure_strength_truncation_bound(
    trials: &[TrialReport],
    max_truncation_rate: f64,
) -> Result<(), String> {
    for trial in trials {
        if trial.games == 0 {
            continue;
        }
        let rate = trial.truncated_games as f64 / trial.games as f64;
        if rate > max_truncation_rate {
            return Err(format!(
                "strength ranking refused: {}-player profile {:?} truncated {}/{} games ({:.2}%), above --max-truncation-rate {:.2}%; raise --max-turns/--max-actions or explicitly relax the threshold",
                trial.players,
                trial.candidate_profile,
                trial.truncated_games,
                trial.games,
                rate * 100.0,
                max_truncation_rate * 100.0,
            ));
        }
    }
    Ok(())
}

fn build_ranking(
    trials: &[TrialReport],
    profiles: &[CudaSimPolicyProfile],
    player_counts: &[u8],
) -> Vec<ProfileRankingReport> {
    let mut ranking = Vec::new();
    for players in player_counts.iter().copied() {
        let mut rows = profiles
            .iter()
            .copied()
            .map(|profile| {
                let profile_report: ProfileReport = profile.into();
                let matching = trials
                    .iter()
                    .filter(|trial| {
                        trial.players == players && trial.candidate_profile == profile_report
                    })
                    .collect::<Vec<_>>();
                let games = matching.iter().map(|trial| trial.games).sum::<u64>();
                let terminal_games = matching
                    .iter()
                    .map(|trial| trial.terminal_games)
                    .sum::<u64>();
                let candidate_wins = matching
                    .iter()
                    .map(|trial| trial.candidate_wins)
                    .sum::<u64>();
                let weighted_margin = matching
                    .iter()
                    .map(|trial| trial.mean_victory_margin * trial.games as f64)
                    .sum::<f64>();
                ProfileRankingReport {
                    rank: 0,
                    players,
                    candidate_profile: profile_report,
                    games,
                    terminal_games,
                    candidate_wins,
                    candidate_win_rate_terminal: if terminal_games == 0 {
                        0.0
                    } else {
                        candidate_wins as f64 / terminal_games as f64
                    },
                    candidate_win_rate_95ci: wilson_interval(candidate_wins, terminal_games),
                    mean_victory_margin: if games == 0 {
                        0.0
                    } else {
                        weighted_margin / games as f64
                    },
                }
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .candidate_win_rate_terminal
                .total_cmp(&left.candidate_win_rate_terminal)
                .then_with(|| {
                    right
                        .mean_victory_margin
                        .total_cmp(&left.mean_victory_margin)
                })
                .then_with(|| right.terminal_games.cmp(&left.terminal_games))
        });
        for (index, row) in rows.iter_mut().enumerate() {
            row.rank = index + 1;
        }
        ranking.extend(rows);
    }
    ranking
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(config) = parse_args().map_err(|error| format!("{error}\nUse --help for usage."))?
    else {
        print_help();
        return Ok(());
    };

    let arena_config = CudaSimArenaConfig {
        max_actions: config.max_actions,
        max_turns: config.max_turns,
    };
    let mut engine = CudaSimEngine::new()?;
    let identity = engine.device_identity().clone();
    let mut trials = Vec::new();

    for players in config.players.iter().copied() {
        let mut accumulators = vec![TrialAccumulator::default(); config.candidate_profiles.len()];
        let mut game_offset = 0usize;
        while game_offset < config.games {
            let chunk_games = (config.games - game_offset).min(config.chunk_games);
            let states = base_state_chunk(
                players,
                game_offset,
                chunk_games,
                config.board_seed,
                config.player_trades_enabled,
                config.paired_seat_blocks,
            );
            for (profile_index, candidate_profile) in
                config.candidate_profiles.iter().copied().enumerate()
            {
                let started = Instant::now();
                let seed_block_games = if config.paired_seat_blocks {
                    players as usize
                } else {
                    1
                };
                let result = engine.run_rotating_profile_chunk_with_seed_blocks(
                    &states,
                    candidate_profile,
                    config.baseline_profile,
                    arena_config,
                    config.simulation_seed,
                    game_offset,
                    seed_block_games,
                )?;
                accumulators[profile_index].absorb(
                    players,
                    game_offset,
                    &result,
                    started.elapsed().as_secs_f64(),
                );
            }
            game_offset += chunk_games;
        }
        trials.extend(
            accumulators
                .into_iter()
                .zip(config.candidate_profiles.iter().copied())
                .map(|(accumulator, profile)| accumulator.into_report(players, profile)),
        );
    }

    ensure_strength_truncation_bound(&trials, config.max_truncation_rate)?;
    let ranking = build_ranking(&trials, &config.candidate_profiles, &config.players);
    let output = Output {
        kind: "cuda-resident-strength-campaign",
        device: DeviceReport {
            backend: identity.backend,
            ordinal: identity.ordinal,
            name: identity.name,
            compute_capability: [identity.compute_capability.0, identity.compute_capability.1],
        },
        config: CampaignConfigReport {
            players: config.players,
            games_per_trial: config.games,
            board_seed: config.board_seed,
            simulation_seed: config.simulation_seed,
            max_turns: config.max_turns,
            max_actions: config.max_actions,
            chunk_games: config.chunk_games,
            grid_step: config.grid_step,
            candidate_profile_count: config.candidate_profiles.len(),
            player_trades_enabled: config.player_trades_enabled,
            paired_seat_blocks: config.paired_seat_blocks,
            max_truncation_rate: config.max_truncation_rate,
            candidate_seat_rotation: "global_game_index_mod_players",
            seed_matching: if config.paired_seat_blocks {
                "board_and_chance_rng_by_seat_rotation_block;policy_rng_by_global_game"
            } else {
                "board_policy_and_chance_rng_by_global_game"
            },
            baseline_profile: config.baseline_profile.into(),
        },
        trials,
        ranking,
    };

    serde_json::to_writer(std::io::stdout().lock(), &output)?;
    println!();
    Ok(())
}
