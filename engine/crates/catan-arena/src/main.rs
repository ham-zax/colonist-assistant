use std::env;
use std::fs::File;
use std::io::{BufWriter, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use colonist_catan_core::{Action, GameState, NodeKind, Phase, SplitMix64};
use colonist_catan_search::{
    BeliefParticle, Mcts, SearchConfig, SearchMode, SearchReport, action_prior,
    choose_rollout_action, encode_action, encode_heterogeneous_graph, evaluate,
    pool_heterogeneous_graph, search_maxn_bounded, search_paranoid_bounded, strategic_utility,
    trade_acceptance_features,
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Engine {
    Random,
    Weighted,
    MaxN,
    AlphaBeta,
    Uct,
    Puct,
}

impl Engine {
    fn parse(value: &str) -> Option<Self> {
        match value {
            "random" => Some(Self::Random),
            "weighted" => Some(Self::Weighted),
            "maxn" => Some(Self::MaxN),
            "alphabeta" | "alpha-beta" => Some(Self::AlphaBeta),
            "uct" => Some(Self::Uct),
            "puct" | "deep" => Some(Self::Puct),
            _ => None,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Random => "random",
            Self::Weighted => "weighted",
            Self::MaxN => "maxn",
            Self::AlphaBeta => "alphabeta",
            Self::Uct => "uct",
            Self::Puct => "puct",
        }
    }
}

#[derive(Clone, Debug)]
struct Config {
    players: u8,
    blocks: u32,
    seed: u64,
    iterations: u32,
    rollout_actions: u16,
    max_turns: u16,
    candidate: Engine,
    baseline: Engine,
    lineup: Option<Vec<Engine>>,
    validate: bool,
    threads: usize,
    quiet: bool,
    json: bool,
    expert_output: Option<String>,
    trade_output: Option<String>,
    expert_stride: u32,
    expert_iterations: u32,
    expert_rollout_actions: u16,
    belief_particles: usize,
    perfect_information_search: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            players: 4,
            blocks: 8,
            seed: 20260728,
            iterations: 300,
            rollout_actions: 180,
            max_turns: 600,
            candidate: Engine::Puct,
            baseline: Engine::Weighted,
            lineup: None,
            validate: false,
            threads: thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
                .min(4),
            quiet: false,
            json: false,
            expert_output: None,
            trade_output: None,
            expert_stride: 1,
            expert_iterations: 0,
            expert_rollout_actions: 0,
            belief_particles: 24,
            perfect_information_search: false,
        }
    }
}

fn parse_config() -> Config {
    let mut config = Config::default();
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0;
    while index < arguments.len() {
        let value = arguments.get(index + 1).map(String::as_str);
        match arguments[index].as_str() {
            "--players" => config.players = value.and_then(|v| v.parse().ok()).unwrap_or(4),
            "--blocks" => config.blocks = value.and_then(|v| v.parse().ok()).unwrap_or(8),
            "--seed" => config.seed = value.and_then(|v| v.parse().ok()).unwrap_or(20260728),
            "--iterations" => config.iterations = value.and_then(|v| v.parse().ok()).unwrap_or(300),
            "--rollout-actions" => {
                config.rollout_actions = value.and_then(|v| v.parse().ok()).unwrap_or(180)
            }
            "--max-turns" => config.max_turns = value.and_then(|v| v.parse().ok()).unwrap_or(600),
            "--expert-output" => config.expert_output = value.map(str::to_string),
            "--trade-output" => config.trade_output = value.map(str::to_string),
            "--expert-stride" => {
                config.expert_stride = value.and_then(|v| v.parse().ok()).unwrap_or(1).max(1)
            }
            "--expert-iterations" => {
                config.expert_iterations = value.and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            "--expert-rollout-actions" => {
                config.expert_rollout_actions = value.and_then(|v| v.parse().ok()).unwrap_or(0)
            }
            "--belief-particles" => {
                config.belief_particles = value
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(24)
                    .clamp(1, 256)
            }
            "--threads" => config.threads = value.and_then(|v| v.parse().ok()).unwrap_or(1).max(1),
            "--candidate" => {
                config.candidate = value.and_then(Engine::parse).unwrap_or(Engine::Puct)
            }
            "--baseline" => {
                config.baseline = value.and_then(Engine::parse).unwrap_or(Engine::Weighted)
            }
            "--lineup" => {
                config.lineup = value.map(|lineup| {
                    lineup
                        .split(',')
                        .map(|engine| {
                            Engine::parse(engine.trim()).unwrap_or_else(|| {
                                eprintln!("unknown engine in --lineup: {engine}");
                                std::process::exit(2);
                            })
                        })
                        .collect()
                });
            }
            "--validate" => {
                config.validate = true;
                index += 1;
                continue;
            }
            "--quiet" => {
                config.quiet = true;
                index += 1;
                continue;
            }
            "--json" => {
                config.json = true;
                config.quiet = true;
                index += 1;
                continue;
            }
            "--perfect-information" => {
                config.perfect_information_search = true;
                index += 1;
                continue;
            }
            "--help" | "-h" => {
                println!(
                    "colonist-arena [--players 2|3|4] [--blocks N] [--seed N] \\
                     [--candidate random|weighted|maxn|alphabeta|uct|puct] [--baseline ...] \\
                     [--lineup puct,puct,maxn,maxn] \\
                     [--iterations N] [--rollout-actions N] [--max-turns N] \\
                     [--belief-particles N] [--perfect-information] \\
                     [--expert-output samples.jsonl] [--trade-output trades.jsonl] \\
                     [--expert-stride N] [--expert-iterations N] \\
                     [--expert-rollout-actions N] \\
                     [--threads N] [--validate] [--quiet] [--json]"
                );
                std::process::exit(0);
            }
            unknown => {
                eprintln!("unknown argument: {unknown}");
                std::process::exit(2);
            }
        }
        index += 2;
    }
    if !(2..=4).contains(&config.players) {
        eprintln!("--players must be 2, 3, or 4");
        std::process::exit(2);
    }
    if config.blocks == 0 {
        eprintln!("--blocks must be at least 1");
        std::process::exit(2);
    }
    if config
        .lineup
        .as_ref()
        .is_some_and(|lineup| lineup.len() != config.players as usize)
    {
        eprintln!("--lineup must contain exactly --players engines");
        std::process::exit(2);
    }
    config
}

struct EngineChoice {
    action: Action,
    search: Option<SearchReport>,
}

/// Deterministic observer-consistent determinizations for arena play.
///
/// The acting player retains its exact hand. Opponent resource identities are
/// redistributed while preserving each public hand size, the exact public bank,
/// and global resource conservation. Opponent development identities and the
/// deck are sampled jointly while preserving every public card count.
fn belief_particles(
    state: &GameState,
    observer: u8,
    count: usize,
    seed: u64,
) -> Vec<BeliefParticle> {
    if count <= 1 {
        return vec![BeliefParticle {
            state: state.clone(),
            weight: 1.0,
        }];
    }
    let opponents = (0..state.board.num_players)
        .filter(|player| *player != observer)
        .collect::<Vec<_>>();
    let resource_totals = opponents
        .iter()
        .map(|player| state.players[*player as usize].resource_total())
        .collect::<Vec<_>>();
    let development_totals = opponents
        .iter()
        .map(|player| {
            state.players[*player as usize]
                .development
                .iter()
                .copied()
                .sum::<u8>()
        })
        .collect::<Vec<_>>();
    let bought_totals = opponents
        .iter()
        .map(|player| {
            state.players[*player as usize]
                .bought_development
                .iter()
                .copied()
                .sum::<u8>()
        })
        .collect::<Vec<_>>();
    let resource_pool = opponents
        .iter()
        .flat_map(|player| {
            state.players[*player as usize]
                .resources
                .iter()
                .enumerate()
                .flat_map(|(resource, count)| std::iter::repeat_n(resource as u8, *count as usize))
        })
        .collect::<Vec<_>>();
    let mut development_pool = opponents
        .iter()
        .flat_map(|player| {
            state.players[*player as usize]
                .development
                .iter()
                .enumerate()
                .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize))
        })
        .collect::<Vec<_>>();
    development_pool.extend(
        state
            .development_deck
            .iter()
            .enumerate()
            .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize)),
    );

    (0..count)
        .map(|sample| {
            let mut rng =
                SplitMix64::new(seed ^ (sample as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15));
            let mut particle = state.clone();
            let mut resources = resource_pool.clone();
            rng.shuffle(&mut resources);
            let mut cursor = 0usize;
            for (opponent_index, player) in opponents.iter().enumerate() {
                particle.players[*player as usize].resources = [0; 5];
                for _ in 0..resource_totals[opponent_index] {
                    let resource = resources[cursor] as usize;
                    particle.players[*player as usize].resources[resource] += 1;
                    cursor += 1;
                }
            }

            let mut development = development_pool.clone();
            rng.shuffle(&mut development);
            let mut development_cursor = 0usize;
            for (opponent_index, player) in opponents.iter().enumerate() {
                let player_state = &mut particle.players[*player as usize];
                player_state.development = [0; 5];
                player_state.bought_development = [0; 5];
                for _ in 0..development_totals[opponent_index] {
                    let card = development[development_cursor] as usize;
                    player_state.development[card] += 1;
                    development_cursor += 1;
                }
                let mut held_cards = player_state
                    .development
                    .iter()
                    .enumerate()
                    .flat_map(|(card, count)| std::iter::repeat_n(card as u8, *count as usize))
                    .collect::<Vec<_>>();
                rng.shuffle(&mut held_cards);
                for card in held_cards
                    .into_iter()
                    .take(bought_totals[opponent_index] as usize)
                {
                    player_state.bought_development[card as usize] += 1;
                }
            }
            particle.development_deck = [0; 5];
            for card in development.into_iter().skip(development_cursor) {
                particle.development_deck[card as usize] += 1;
            }
            debug_assert!(particle.validate().is_ok());
            BeliefParticle {
                state: particle,
                weight: 1.0 / count as f32,
            }
        })
        .collect()
}

fn choose_action(
    engine: Engine,
    state: &GameState,
    rng: &mut SplitMix64,
    config: &Config,
    persistent_searches: &mut [Option<Mcts>],
) -> EngineChoice {
    let actions = state.legal_actions();
    let action = match engine {
        Engine::Random => actions[rng.range(actions.len())].clone(),
        Engine::Weighted => choose_rollout_action(state, &actions, rng),
        Engine::MaxN => {
            search_maxn_bounded(state, 3, 12, (config.iterations * 160).clamp(4_000, 80_000))
                .chosen
                .unwrap_or_else(|| actions[0].clone())
        }
        Engine::AlphaBeta => search_paranoid_bounded(
            state,
            state.actor(),
            3,
            12,
            (config.iterations * 160).clamp(4_000, 80_000),
        )
        .chosen
        .unwrap_or_else(|| actions[0].clone()),
        Engine::Uct | Engine::Puct => {
            let actor = state.actor() as usize;
            let search = persistent_searches[actor]
                .get_or_insert_with(|| Mcts::new(SearchConfig::default(), state));
            let report = run_mcts_search(
                state,
                rng.next_u64(),
                config,
                search,
                config.iterations,
                config.rollout_actions,
                if engine == Engine::Uct {
                    SearchMode::Uct
                } else {
                    SearchMode::Puct
                },
            );
            let action = report.chosen.clone().unwrap_or_else(|| actions[0].clone());
            return EngineChoice {
                action,
                search: Some(report),
            };
        }
    };
    EngineChoice {
        action,
        search: None,
    }
}

fn run_mcts_search(
    state: &GameState,
    seed: u64,
    config: &Config,
    search: &mut Mcts,
    iterations: u32,
    rollout_actions: u16,
    mode: SearchMode,
) -> SearchReport {
    let search_config = SearchConfig {
        iterations,
        max_nodes: (iterations as usize * 40).max(5_000),
        rollout_actions,
        tactical_depth: 14,
        tactical_nodes: 4_000,
        seed,
        mode,
        ..SearchConfig::default()
    };
    search.reconfigure(search_config);
    if config.perfect_information_search {
        search.search(state)
    } else {
        let particles = belief_particles(
            state,
            state.actor(),
            config.belief_particles,
            state.observation_hash(state.actor()) ^ config.seed,
        );
        search
            .search_weighted_belief(&particles)
            .expect("arena belief particles share one public observation")
    }
}

#[derive(Clone, Debug, Default)]
struct GameMetrics {
    roads: [u32; 4],
    settlements: [u32; 4],
    cities: [u32; 4],
    development_bought: [u32; 4],
    dead_roads: [u32; 4],
    offers: [u32; 4],
    accepts: [u32; 4],
    counters: [u32; 4],
    end_turns_over_seven: [u32; 4],
    cards_lost_to_sevens: [u32; 4],
    robber_blocked_production: [u32; 4],
    decision_count: [u32; 4],
    decision_time: [Duration; 4],
    trade_value_sum: [f32; 4],
    calibration_brier_sum: [f32; 4],
    calibration_log_loss_sum: [f32; 4],
    calibration_count: [u32; 4],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpertActionSample {
    key: String,
    features: Vec<f32>,
    visits: u32,
    policy: f32,
    search_value: [f32; 4],
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct ExpertSample {
    schema_version: u8,
    state_hash: String,
    board_seed: u64,
    chance_seed: u64,
    turn: u16,
    phase: String,
    actor: u8,
    actor_victory_points: u8,
    players: u8,
    engine: String,
    state_features: Vec<f32>,
    actions: Vec<ExpertActionSample>,
    root_search_value: [f32; 4],
    winner: u8,
    outcome: [f32; 4],
    terminal: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TradeSample {
    schema_version: u8,
    state_hash: String,
    board_seed: u64,
    chance_seed: u64,
    turn: u16,
    actor: u8,
    engine: String,
    features: Vec<f32>,
    accepted: bool,
    countered: bool,
}

#[derive(Clone, Debug)]
struct GameResult {
    winner: u8,
    turns: u16,
    actions: u32,
    cutoff: bool,
    points: [u8; 4],
    ranks: [f32; 4],
    metrics: GameMetrics,
    expert_samples: Vec<ExpertSample>,
    trade_samples: Vec<TradeSample>,
}

#[derive(Clone, Debug)]
struct ArenaResult {
    block: u32,
    seat: u8,
    engines: Vec<Engine>,
    game: GameResult,
}

#[derive(Default)]
struct CandidateMetrics {
    seats: u32,
    points: u64,
    ranks: f64,
    roads: u64,
    settlements: u64,
    cities: u64,
    development_bought: u64,
    dead_roads: u64,
    offers: u64,
    accepts: u64,
    counters: u64,
    end_turns_over_seven: u64,
    cards_lost_to_sevens: u64,
    robber_blocked_production: u64,
    decisions: u64,
    decision_nanos: u128,
    trade_value_sum: f64,
    calibration_brier_sum: f64,
    calibration_log_loss_sum: f64,
    calibration_count: u64,
}

fn play_game(board_seed: u64, chance_seed: u64, engines: &[Engine], config: &Config) -> GameResult {
    let mut state = GameState::standard(board_seed, config.players);
    // Rotate explicit bounded-rational styles independently of engine family.
    // Paired seat rotations therefore test road-heavy, development-heavy,
    // trade-happy, and trade-resistant behavior instead of overfitting every
    // search policy to one perfectly symmetric opponent model.
    const POLICY_PROFILES: [[u8; 5]; 6] = [
        [51, 51, 51, 51, 51],
        [45, 92, 28, 38, 45],
        [45, 30, 92, 35, 45],
        [45, 45, 45, 96, 25],
        [45, 45, 45, 15, 96],
        [70, 55, 55, 55, 55],
    ];
    for player in 0..config.players as usize {
        let profile = ((board_seed ^ (player as u64 + 1).wrapping_mul(0x9e37_79b9_7f4a_7c15))
            % POLICY_PROFILES.len() as u64) as usize;
        state.players[player].policy_profile = POLICY_PROFILES[profile];
    }
    let mut chance_rng = SplitMix64::new(chance_seed);
    let mut policy_rngs = (0..config.players)
        .map(|player| SplitMix64::new(chance_seed ^ ((player as u64 + 1) * 0x9e37_79b9)))
        .collect::<Vec<_>>();
    let mut actions = 0u32;
    let mut metrics = GameMetrics::default();
    let mut calibration = Vec::<(u8, f32)>::new();
    let mut expert_samples = Vec::<ExpertSample>::new();
    let mut trade_samples = Vec::<TradeSample>::new();
    let mut persistent_searches = (0..config.players)
        .map(|_| None)
        .collect::<Vec<Option<Mcts>>>();
    let mut expert_searches = (0..config.players)
        .map(|_| None)
        .collect::<Vec<Option<Mcts>>>();
    while !state.is_terminal() && state.turn <= config.max_turns {
        let action = if state.node_kind() == NodeKind::Chance {
            state
                .sample_chance(&mut chance_rng)
                .expect("chance state must have an outcome")
        } else {
            let actor = state.actor() as usize;
            let started = Instant::now();
            let choice = choose_action(
                engines[actor],
                &state,
                &mut policy_rngs[actor],
                config,
                &mut persistent_searches,
            );
            metrics.decision_time[actor] += started.elapsed();
            metrics.decision_count[actor] += 1;
            let should_record_expert = config.expert_output.is_some()
                && metrics.decision_count[actor] % config.expert_stride == 0;
            let teacher_report = if should_record_expert && config.expert_iterations > 0 {
                let search = expert_searches[actor]
                    .get_or_insert_with(|| Mcts::new(SearchConfig::default(), &state));
                Some(run_mcts_search(
                    &state,
                    state.observation_hash(actor as u8) ^ config.seed ^ 0x4558_5045_5254_5055,
                    config,
                    search,
                    config.expert_iterations,
                    config.expert_rollout_actions.max(config.rollout_actions),
                    SearchMode::Puct,
                ))
            } else {
                None
            };
            if state.phase == Phase::PreRoll {
                calibration.push((
                    actor as u8,
                    choice.search.as_ref().map_or_else(
                        || evaluate(&state)[actor],
                        |report| report.root_value[actor],
                    ),
                ));
            }
            if should_record_expert
                && let Some(report) = teacher_report.as_ref().or(choice.search.as_ref())
            {
                let graph = encode_heterogeneous_graph(&state, actor as u8, false);
                let state_features = pool_heterogeneous_graph(&graph, actor as u8).to_vec();
                let visit_total = report
                    .actions
                    .iter()
                    .map(|sample| sample.visits)
                    .sum::<u32>()
                    .max(1);
                let actions = report
                    .actions
                    .iter()
                    .map(|sample| ExpertActionSample {
                        key: format!("{:?}", sample.action),
                        features: encode_action(&state, &sample.action).to_vec(),
                        visits: sample.visits,
                        policy: sample.visits as f32 / visit_total as f32,
                        search_value: sample.value,
                    })
                    .collect();
                expert_samples.push(ExpertSample {
                    schema_version: 1,
                    state_hash: format!("{:016x}", state.observation_hash(actor as u8),),
                    board_seed,
                    chance_seed,
                    turn: state.turn,
                    phase: format!("{:?}", state.phase),
                    actor: actor as u8,
                    actor_victory_points: state.players[actor].victory_points(),
                    players: config.players,
                    engine: if teacher_report.is_some() {
                        "puct-teacher".to_string()
                    } else {
                        engines[actor].as_str().to_string()
                    },
                    state_features,
                    actions,
                    root_search_value: report.root_value,
                    winner: u8::MAX,
                    outcome: [0.0; 4],
                    terminal: false,
                });
            }
            if config.trade_output.is_some()
                && matches!(
                    choice.action,
                    Action::RespondTrade { .. } | Action::CounterTrade { .. }
                )
                && let Some(features) = trade_acceptance_features(&state, actor as u8)
            {
                trade_samples.push(TradeSample {
                    schema_version: 1,
                    state_hash: format!("{:016x}", state.observation_hash(actor as u8)),
                    board_seed,
                    chance_seed,
                    turn: state.turn,
                    actor: actor as u8,
                    engine: engines[actor].as_str().to_string(),
                    features: features.to_vec(),
                    accepted: matches!(choice.action, Action::RespondTrade { accept: true }),
                    countered: matches!(choice.action, Action::CounterTrade { .. }),
                });
            }
            choice.action
        };
        let actor = state.actor() as usize;
        match &action {
            Action::PlaceRoad { .. } | Action::BuildRoad { .. } => {
                metrics.roads[actor] += 1;
                if matches!(action, Action::BuildRoad { .. })
                    && action_prior(&state, &action, actor as u8) <= 0.20
                {
                    metrics.dead_roads[actor] += 1;
                }
            }
            Action::PlaceSettlement { .. } | Action::BuildSettlement { .. } => {
                metrics.settlements[actor] += 1;
            }
            Action::BuildCity { .. } => metrics.cities[actor] += 1,
            Action::BuyDevelopment => metrics.development_bought[actor] += 1,
            Action::OfferTrade { .. } => metrics.offers[actor] += 1,
            Action::RespondTrade { accept: true } => metrics.accepts[actor] += 1,
            Action::CounterTrade { .. } => metrics.counters[actor] += 1,
            Action::EndTurn if state.players[actor].resource_total() > 7 => {
                metrics.end_turns_over_seven[actor] += 1;
            }
            Action::Discard { cards } => {
                metrics.cards_lost_to_sevens[actor] +=
                    cards.iter().copied().map(u32::from).sum::<u32>();
            }
            Action::ResolveRoll { value } if *value != 7 => {
                let robber = state.robber_hex as usize;
                if state.board.hexes[robber].number == *value {
                    for (vertex, building) in state.buildings.iter().enumerate() {
                        let Some(building) = building else {
                            continue;
                        };
                        if state.board.vertices[vertex]
                            .adjacent_hexes
                            .contains(&(robber as u8))
                        {
                            metrics.robber_blocked_production[building.player() as usize] +=
                                building.production_multiplier() as u32;
                        }
                    }
                }
            }
            _ => {}
        }
        let trade_actor = if matches!(action, Action::ConfirmTrade { .. }) {
            Some(actor as u8)
        } else {
            None
        };
        let trade_before = trade_actor.map(|player| strategic_utility(&state, player));
        state.apply(&action).unwrap_or_else(|error| {
            panic!(
                "illegal arena action {action:?} at hash {:016x}: {error}",
                state.state_hash()
            )
        });
        if let (Some(player), Some(before)) = (trade_actor, trade_before) {
            metrics.trade_value_sum[player as usize] += strategic_utility(&state, player) - before;
        }
        actions += 1;
        if config.validate {
            state.validate().unwrap_or_else(|error| {
                panic!(
                    "invariant failed after {action:?} at hash {:016x}: {error}",
                    state.state_hash()
                )
            });
        }
    }
    let terminal_winner = state.winner();
    let winner = terminal_winner.unwrap_or_else(|| {
        let values = evaluate(&state);
        values
            .iter()
            .take(config.players as usize)
            .enumerate()
            .max_by(|(_, a), (_, b)| a.total_cmp(b))
            .map(|(player, _)| player as u8)
            .unwrap_or(0)
    });
    for (player, prediction) in calibration {
        let outcome = f32::from(player == winner);
        let clipped = prediction.clamp(1e-6, 1.0 - 1e-6);
        metrics.calibration_brier_sum[player as usize] += (prediction - outcome).powi(2);
        metrics.calibration_log_loss_sum[player as usize] +=
            -(outcome * clipped.ln() + (1.0 - outcome) * (1.0 - clipped).ln());
        metrics.calibration_count[player as usize] += 1;
    }
    for sample in &mut expert_samples {
        sample.winner = winner;
        sample.terminal = terminal_winner.is_some();
        if terminal_winner.is_some() {
            sample.outcome[winner as usize] = 1.0;
        }
    }
    let mut points = [0u8; 4];
    let mut ranks = [0.0f32; 4];
    for (player, point) in points.iter_mut().enumerate().take(config.players as usize) {
        *point = state.players[player].victory_points();
    }
    for player in 0..config.players as usize {
        let better = points
            .iter()
            .take(config.players as usize)
            .filter(|candidate_points| **candidate_points > points[player])
            .count();
        let tied = points
            .iter()
            .take(config.players as usize)
            .filter(|candidate_points| **candidate_points == points[player])
            .count();
        ranks[player] = 1.0 + better as f32 + (tied.saturating_sub(1)) as f32 / 2.0;
    }
    GameResult {
        winner,
        turns: state.turn,
        actions,
        cutoff: terminal_winner.is_none(),
        points,
        ranks,
        metrics,
        expert_samples,
        trade_samples,
    }
}

fn percentile(sorted: &[f32], probability: f32) -> f32 {
    let index = ((sorted.len() - 1) as f32 * probability).round() as usize;
    sorted[index]
}

fn bootstrap_interval(block_scores: &[f32], seed: u64) -> (f32, f32) {
    let mut rng = SplitMix64::new(seed);
    let mut samples = Vec::with_capacity(2_000);
    for _ in 0..2_000 {
        let mean = (0..block_scores.len())
            .map(|_| block_scores[rng.range(block_scores.len())])
            .sum::<f32>()
            / block_scores.len() as f32;
        samples.push(mean);
    }
    samples.sort_by(f32::total_cmp);
    (percentile(&samples, 0.025), percentile(&samples, 0.975))
}

fn main() {
    let config = parse_config();
    let started = Instant::now();
    let mut total_games = 0u32;
    let mut candidate_wins = 0u32;
    let mut cutoffs = 0u32;
    let mut turns = 0u64;
    let mut actions = 0u64;
    let mut block_scores = Vec::with_capacity(config.blocks as usize);
    let mut candidate_metrics = CandidateMetrics::default();

    if !config.json {
        println!(
            "arena candidate={:?} baseline={:?} lineup={:?} players={} blocks={} iterations={} threads={} seed={}",
            config.candidate,
            config.baseline,
            config.lineup,
            config.players,
            config.blocks,
            config.iterations,
            config.threads,
            config.seed
        );
    }
    let jobs = Arc::new(
        (0..config.blocks)
            .flat_map(|block| (0..config.players).map(move |seat| (block, seat)))
            .collect::<Vec<_>>(),
    );
    let next_job = Arc::new(AtomicUsize::new(0));
    let shared_config = Arc::new(config.clone());
    let (sender, receiver) = mpsc::channel::<ArenaResult>();
    let worker_count = config.threads.min(jobs.len().max(1));
    let mut workers = Vec::with_capacity(worker_count);
    for _ in 0..worker_count {
        let jobs = Arc::clone(&jobs);
        let next_job = Arc::clone(&next_job);
        let config = Arc::clone(&shared_config);
        let sender = sender.clone();
        workers.push(thread::spawn(move || {
            loop {
                let index = next_job.fetch_add(1, Ordering::Relaxed);
                let Some(&(block, seat)) = jobs.get(index) else {
                    break;
                };
                let board_seed = config.seed.wrapping_add(block as u64 * 2);
                let chance_seed = config.seed.wrapping_add(block as u64 * 2 + 1);
                let engines = if let Some(lineup) = &config.lineup {
                    (0..config.players as usize)
                        .map(|player| lineup[(player + seat as usize) % lineup.len()])
                        .collect::<Vec<_>>()
                } else {
                    let mut engines = vec![config.baseline; config.players as usize];
                    engines[seat as usize] = config.candidate;
                    engines
                };
                let result = play_game(board_seed, chance_seed, &engines, &config);
                if sender
                    .send(ArenaResult {
                        block,
                        seat,
                        engines,
                        game: result,
                    })
                    .is_err()
                {
                    break;
                }
            }
        }));
    }
    drop(sender);
    let mut results = receiver.into_iter().collect::<Vec<_>>();
    for worker in workers {
        worker.join().expect("arena worker panicked");
    }
    results.sort_by_key(|result| (result.block, result.seat));
    let mut block_wins = vec![0u32; config.blocks as usize];
    let mut engine_wins = [0u32; 6];
    let mut expert_writer = config.expert_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create expert data {path}: {error}")),
        )
    });
    let mut expert_sample_count = 0u64;
    let mut trade_writer = config.trade_output.as_ref().map(|path| {
        BufWriter::new(
            File::create(path)
                .unwrap_or_else(|error| panic!("failed to create trade data {path}: {error}")),
        )
    });
    let mut trade_sample_count = 0u64;
    for result in results {
        if let Some(writer) = &mut expert_writer {
            for sample in &result.game.expert_samples {
                serde_json::to_writer(&mut *writer, sample).expect("expert sample must serialize");
                writer
                    .write_all(b"\n")
                    .expect("expert data must be writable");
                expert_sample_count += 1;
            }
        }
        if let Some(writer) = &mut trade_writer {
            for sample in &result.game.trade_samples {
                serde_json::to_writer(&mut *writer, sample).expect("trade sample must serialize");
                writer
                    .write_all(b"\n")
                    .expect("trade data must be writable");
                trade_sample_count += 1;
            }
        }
        let winner_engine = result.engines[result.game.winner as usize];
        let won = winner_engine == config.candidate;
        block_wins[result.block as usize] += u32::from(won);
        candidate_wins += u32::from(won);
        engine_wins[winner_engine as usize] += 1;
        for (player, engine) in result.engines.iter().enumerate() {
            if *engine != config.candidate {
                continue;
            }
            let metrics = &result.game.metrics;
            candidate_metrics.seats += 1;
            candidate_metrics.points += result.game.points[player] as u64;
            candidate_metrics.ranks += result.game.ranks[player] as f64;
            candidate_metrics.roads += metrics.roads[player] as u64;
            candidate_metrics.settlements += metrics.settlements[player] as u64;
            candidate_metrics.cities += metrics.cities[player] as u64;
            candidate_metrics.development_bought += metrics.development_bought[player] as u64;
            candidate_metrics.dead_roads += metrics.dead_roads[player] as u64;
            candidate_metrics.offers += metrics.offers[player] as u64;
            candidate_metrics.accepts += metrics.accepts[player] as u64;
            candidate_metrics.counters += metrics.counters[player] as u64;
            candidate_metrics.end_turns_over_seven += metrics.end_turns_over_seven[player] as u64;
            candidate_metrics.cards_lost_to_sevens += metrics.cards_lost_to_sevens[player] as u64;
            candidate_metrics.robber_blocked_production +=
                metrics.robber_blocked_production[player] as u64;
            candidate_metrics.decisions += metrics.decision_count[player] as u64;
            candidate_metrics.decision_nanos += metrics.decision_time[player].as_nanos();
            candidate_metrics.trade_value_sum += metrics.trade_value_sum[player] as f64;
            candidate_metrics.calibration_brier_sum += metrics.calibration_brier_sum[player] as f64;
            candidate_metrics.calibration_log_loss_sum +=
                metrics.calibration_log_loss_sum[player] as f64;
            candidate_metrics.calibration_count += metrics.calibration_count[player] as u64;
        }
        cutoffs += u32::from(result.game.cutoff);
        turns += result.game.turns as u64;
        actions += result.game.actions as u64;
        total_games += 1;
        if !config.quiet {
            println!(
                "game block={block} rotation={seat} winner={} winner_engine={} candidate_win={} turns={} actions={} cutoff={}",
                result.game.winner,
                winner_engine.as_str(),
                won,
                result.game.turns,
                result.game.actions,
                result.game.cutoff,
                block = result.block,
                seat = result.seat,
            );
        }
    }
    if let Some(writer) = &mut expert_writer {
        writer.flush().expect("expert data must flush");
    }
    if let Some(writer) = &mut trade_writer {
        writer.flush().expect("trade data must flush");
    }
    block_scores.extend(
        block_wins
            .into_iter()
            .map(|wins| wins as f32 / config.players as f32),
    );

    let win_share = candidate_wins as f32 / total_games as f32;
    let candidate_seats = config
        .lineup
        .as_ref()
        .map_or(1, |lineup| {
            lineup
                .iter()
                .filter(|engine| **engine == config.candidate)
                .count()
        })
        .max(1);
    let fair_share = candidate_seats as f32 / config.players as f32;
    let (lower, upper) = bootstrap_interval(&block_scores, config.seed ^ 0xb007_57a9);
    let elapsed = started.elapsed();
    let mean_turns = turns as f64 / total_games as f64;
    let mean_actions = actions as f64 / total_games as f64;
    let games_per_second = total_games as f64 / elapsed.as_secs_f64();
    if config.json {
        println!(
            concat!(
                "{{",
                "\"schemaVersion\":1,",
                "\"simulator\":\"colonist-native\",",
                "\"candidate\":\"{}\",",
                "\"baseline\":\"{}\",",
                "\"lineup\":{},",
                "\"players\":{},",
                "\"blocks\":{},",
                "\"games\":{},",
                "\"candidateWins\":{},",
                "\"engineWins\":{{\"random\":{},\"weighted\":{},\"maxn\":{},\"alphabeta\":{},\"uct\":{},\"puct\":{}}},",
                "\"winShare\":{:.8},",
                "\"fairShare\":{:.8},",
                "\"fairShareDelta\":{:.8},",
                "\"blockedCi95\":{{\"lower\":{:.8},\"upper\":{:.8}}},",
                "\"meanTurns\":{:.4},",
                "\"meanActions\":{:.4},",
                "\"candidateMetrics\":{{",
                "\"seatSamples\":{},",
                "\"meanRank\":{:.6},",
                "\"meanVictoryPoints\":{:.6},",
                "\"meanRoads\":{:.6},",
                "\"meanSettlements\":{:.6},",
                "\"meanCities\":{:.6},",
                "\"meanDevelopmentCardsBought\":{:.6},",
                "\"meanDeadRoads\":{:.6},",
                "\"meanDomesticOffers\":{:.6},",
                "\"tradeAcceptanceRate\":{:.6},",
                "\"meanCounters\":{:.6},",
                "\"meanEndTurnsAboveSeven\":{:.6},",
                "\"meanCardsLostToSevens\":{:.6},",
                "\"meanRobberBlockedProduction\":{:.6},",
                "\"meanTradeValue\":{:.6},",
                "\"meanDecisionLatencyMs\":{:.6},",
                "\"calibrationBrier\":{:.8},",
                "\"calibrationLogLoss\":{:.8}",
                "}},",
                "\"expertSamples\":{},",
                "\"tradeSamples\":{},",
                "\"cutoffs\":{},",
                "\"elapsedMs\":{},",
                "\"gamesPerSecond\":{:.6},",
                "\"seed\":{},",
                "\"iterations\":{},",
                "\"rolloutActions\":{},",
                "\"maxTurns\":{},",
                "\"beliefParticles\":{},",
                "\"perfectInformationSearch\":{},",
                "\"threads\":{},",
                "\"validate\":{}",
                "}}"
            ),
            config.candidate.as_str(),
            config.baseline.as_str(),
            config.lineup.as_ref().map_or_else(
                || "null".to_string(),
                |lineup| format!(
                    "[{}]",
                    lineup
                        .iter()
                        .map(|engine| format!("\"{}\"", engine.as_str()))
                        .collect::<Vec<_>>()
                        .join(",")
                ),
            ),
            config.players,
            config.blocks,
            total_games,
            candidate_wins,
            engine_wins[Engine::Random as usize],
            engine_wins[Engine::Weighted as usize],
            engine_wins[Engine::MaxN as usize],
            engine_wins[Engine::AlphaBeta as usize],
            engine_wins[Engine::Uct as usize],
            engine_wins[Engine::Puct as usize],
            win_share,
            fair_share,
            win_share - fair_share,
            lower,
            upper,
            mean_turns,
            mean_actions,
            candidate_metrics.seats,
            candidate_metrics.ranks / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.points as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.roads as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.settlements as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.cities as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.development_bought as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.dead_roads as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.offers as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.accepts as f64 / candidate_metrics.offers.max(1) as f64,
            candidate_metrics.counters as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.end_turns_over_seven as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.cards_lost_to_sevens as f64 / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.robber_blocked_production as f64
                / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.trade_value_sum / candidate_metrics.seats.max(1) as f64,
            candidate_metrics.decision_nanos as f64
                / candidate_metrics.decisions.max(1) as f64
                / 1_000_000.0,
            candidate_metrics.calibration_brier_sum
                / candidate_metrics.calibration_count.max(1) as f64,
            candidate_metrics.calibration_log_loss_sum
                / candidate_metrics.calibration_count.max(1) as f64,
            expert_sample_count,
            trade_sample_count,
            cutoffs,
            elapsed.as_millis(),
            games_per_second,
            config.seed,
            config.iterations,
            config.rollout_actions,
            config.max_turns,
            config.belief_particles,
            config.perfect_information_search,
            config.threads,
            config.validate,
        );
    } else {
        println!(
            "summary games={total_games} candidate_wins={candidate_wins} win_share={win_share:.4} blocked_ci95=[{lower:.4},{upper:.4}] mean_turns={mean_turns:.1} mean_actions={mean_actions:.1} cutoffs={cutoffs} elapsed_ms={} games_per_second={games_per_second:.3}",
            elapsed.as_millis(),
        );
    }
}

#[cfg(test)]
mod tests {
    use colonist_catan_core::Phase;

    use super::{Config, Engine, belief_particles, play_game};

    #[test]
    fn random_arena_game_is_reproducible() {
        let config = Config {
            players: 3,
            max_turns: 40,
            ..Config::default()
        };
        let engines = [Engine::Random; 3];
        let first = play_game(1, 2, &engines, &config);
        let second = play_game(1, 2, &engines, &config);
        assert_eq!(first.winner, second.winner);
        assert_eq!(first.turns, second.turns);
        assert_eq!(first.actions, second.actions);
    }

    #[test]
    fn priors_rank_generated_actions_without_panicking() {
        let state = colonist_catan_core::GameState::standard(4, 4);
        for action in state.legal_actions() {
            assert!(
                colonist_catan_search::action_prior(&state, &action, state.actor()).is_finite()
            );
        }
    }

    #[test]
    fn arena_beliefs_preserve_the_observers_information_set_and_conservation() {
        let mut state = colonist_catan_core::GameState::standard(17, 4);
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
        let observer = state.actor();
        let exact_hand = state.players[observer as usize].resources;
        let observation = state.observation_hash(observer);
        let particles = belief_particles(&state, observer, 24, 991);
        assert_eq!(particles.len(), 24);
        for particle in particles {
            assert_eq!(
                particle.state.players[observer as usize].resources,
                exact_hand,
            );
            assert_eq!(particle.state.observation_hash(observer), observation,);
            particle.state.validate().unwrap();
        }
    }
}
