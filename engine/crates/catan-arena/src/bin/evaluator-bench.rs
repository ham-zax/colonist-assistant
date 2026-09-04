use std::hint::black_box;
use std::time::Instant;

use colonist_catan_core::{
    Action, GameState, NodeKind, Phase, SplitMix64, SyntheticBoardGenerator,
};
use colonist_catan_search::{
    encode_heterogeneous_graph, evaluate, evaluate_profiled, expansion_option_value,
    expected_discard_loss, largest_army_outlook, longest_road_outlook, marginal_development_value,
    pool_heterogeneous_graph, production_pips, strategic_utility,
};
use serde::Serialize;

#[derive(Clone, Copy)]
struct Options {
    players: u8,
    states: usize,
    repeats: u32,
    seed: u64,
    player_trades_enabled: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            players: 4,
            states: 256,
            repeats: 32,
            seed: 108_100_001,
            player_trades_enabled: false,
        }
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Measurement {
    name: &'static str,
    states: usize,
    repeats: u32,
    elapsed_ms: f64,
    states_per_second: f64,
    microseconds_per_state: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct EvaluatorBreakdown {
    route_map_microseconds_per_state: f64,
    arrival_score_microseconds_per_state: f64,
    remaining_utility_microseconds_per_state: f64,
    route_map_share: f64,
    arrival_score_share: f64,
    remaining_utility_share: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Report {
    schema_version: u8,
    kind: &'static str,
    players: u8,
    sampled_states: usize,
    repeats: u32,
    board_generator: &'static str,
    seed: u64,
    player_trades_enabled: bool,
    measurements: Vec<Measurement>,
    evaluator_breakdown: EvaluatorBreakdown,
    evaluator_share_of_node_primitive: f64,
    infinite_evaluator_speedup_ceiling: f64,
}

fn parse_options() -> Options {
    let mut options = Options::default();
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--players" => {
                index += 1;
                options.players = args
                    .get(index)
                    .expect("--players requires a value")
                    .parse()
                    .expect("--players must be an integer");
            }
            "--states" => {
                index += 1;
                options.states = args
                    .get(index)
                    .expect("--states requires a value")
                    .parse()
                    .expect("--states must be an integer");
            }
            "--repeats" => {
                index += 1;
                options.repeats = args
                    .get(index)
                    .expect("--repeats requires a value")
                    .parse()
                    .expect("--repeats must be an integer");
            }
            "--seed" => {
                index += 1;
                options.seed = args
                    .get(index)
                    .expect("--seed requires a value")
                    .parse()
                    .expect("--seed must be an integer");
            }
            "--player-trades" => options.player_trades_enabled = true,
            "--no-player-trades" => options.player_trades_enabled = false,
            "--help" | "-h" => {
                println!(
                    "Usage: evaluator-bench [--players 3|4] [--states N] [--repeats N] [--seed N] [--player-trades|--no-player-trades]"
                );
                std::process::exit(0);
            }
            other => panic!("unknown argument: {other}"),
        }
        index += 1;
    }
    assert!(
        (2..=4).contains(&options.players),
        "--players must be 2..=4"
    );
    assert!(options.states > 0, "--states must be positive");
    assert!(options.repeats > 0, "--repeats must be positive");
    options
}

fn choose_action(state: &GameState, actions: &[Action], rng: &mut SplitMix64) -> usize {
    if matches!(state.node_kind(), NodeKind::Chance) {
        let total = actions
            .iter()
            .map(|action| u64::from(state.chance_weight(action)))
            .sum::<u64>();
        if total > 0 {
            let mut target = rng.next_u64() % total;
            for (index, action) in actions.iter().enumerate() {
                let weight = u64::from(state.chance_weight(action));
                if target < weight {
                    return index;
                }
                target -= weight;
            }
        }
    }
    rng.range(actions.len())
}

fn benchmark_board_generator(players: u8) -> SyntheticBoardGenerator {
    if players == 4 {
        SyntheticBoardGenerator::Classic4pV1
    } else {
        SyntheticBoardGenerator::LegacyRandomizedV1
    }
}

fn sample_states(options: Options) -> Vec<GameState> {
    let mut sampled = Vec::with_capacity(options.states);
    let mut game_index = 0u64;
    while sampled.len() < options.states {
        let game_seed = options
            .seed
            .wrapping_add(game_index.wrapping_mul(0x9e37_79b9));
        let mut state = GameState::from_generator(
            benchmark_board_generator(options.players),
            game_seed,
            options.players,
        )
        .expect("benchmark generator must support configured player count");
        state.player_trades_enabled = options.player_trades_enabled;
        let mut rng = SplitMix64::new(game_seed ^ 0xd1b5_4a32_d192_ed03);
        let mut steps = 0usize;
        while sampled.len() < options.states && !state.is_terminal() && steps < 2_000 {
            let actions = state.legal_actions();
            if actions.is_empty() {
                break;
            }
            let action = actions[choose_action(&state, &actions, &mut rng)].clone();
            state
                .apply(&action)
                .expect("sampled legal action must transition");
            steps += 1;
            if !matches!(
                state.phase,
                Phase::SetupSettlement | Phase::SetupRoad { .. }
            ) && steps.is_multiple_of(3)
            {
                sampled.push(state.clone());
            }
        }
        game_index += 1;
        assert!(
            game_index < 10_000,
            "could not generate enough benchmark states"
        );
    }
    sampled
}

fn measure<F>(
    name: &'static str,
    states: &[GameState],
    repeats: u32,
    mut operation: F,
) -> Measurement
where
    F: FnMut(&GameState),
{
    for state in states.iter().take(8) {
        operation(black_box(state));
    }
    let start = Instant::now();
    for _ in 0..repeats {
        for state in states {
            operation(black_box(state));
        }
    }
    let elapsed = start.elapsed();
    let state_calls = states.len() as f64 * f64::from(repeats);
    let elapsed_seconds = elapsed.as_secs_f64().max(f64::EPSILON);
    Measurement {
        name,
        states: states.len(),
        repeats,
        elapsed_ms: elapsed_seconds * 1_000.0,
        states_per_second: state_calls / elapsed_seconds,
        microseconds_per_state: elapsed_seconds * 1_000_000.0 / state_calls,
    }
}

fn all_players<F>(state: &GameState, mut operation: F)
where
    F: FnMut(&GameState, u8),
{
    for player in 0..state.board.num_players {
        operation(state, player);
    }
}

fn evaluator_breakdown(states: &[GameState], repeats: u32) -> EvaluatorBreakdown {
    let profile_repeats = repeats.div_ceil(4).max(1);
    let mut route_nanos = 0u128;
    let mut arrival_nanos = 0u128;
    let mut utility_nanos = 0u128;
    for _ in 0..profile_repeats {
        for state in states {
            let (value, profile) = evaluate_profiled(black_box(state));
            black_box(value);
            route_nanos += u128::from(profile.route_map_nanos);
            arrival_nanos += u128::from(profile.arrival_score_nanos);
            utility_nanos += u128::from(profile.remaining_utility_nanos);
        }
    }
    let calls = states.len() as f64 * f64::from(profile_repeats);
    let route_us = route_nanos as f64 / calls / 1_000.0;
    let arrival_us = arrival_nanos as f64 / calls / 1_000.0;
    let utility_us = utility_nanos as f64 / calls / 1_000.0;
    let total = (route_us + arrival_us + utility_us).max(f64::EPSILON);
    EvaluatorBreakdown {
        route_map_microseconds_per_state: route_us,
        arrival_score_microseconds_per_state: arrival_us,
        remaining_utility_microseconds_per_state: utility_us,
        route_map_share: route_us / total,
        arrival_score_share: arrival_us / total,
        remaining_utility_share: utility_us / total,
    }
}

fn main() {
    let options = parse_options();
    let states = sample_states(options);
    let mut measurements = Vec::new();

    measurements.push(measure("evaluate", &states, options.repeats, |state| {
        black_box(evaluate(state));
    }));
    measurements.push(measure(
        "strategic-utility-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(strategic_utility(state, player));
            });
        },
    ));
    measurements.push(measure(
        "expansion-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(expansion_option_value(state, player));
            });
        },
    ));
    measurements.push(measure(
        "longest-road-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(longest_road_outlook(state, player));
            });
        },
    ));
    measurements.push(measure(
        "largest-army-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(largest_army_outlook(state, player));
            });
        },
    ));
    measurements.push(measure(
        "discard-loss-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(expected_discard_loss(state, player));
            });
        },
    ));
    measurements.push(measure(
        "production-pips-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(production_pips(state, player));
            });
        },
    ));
    measurements.push(measure(
        "marginal-development-all-players",
        &states,
        options.repeats,
        |state| {
            all_players(state, |state, player| {
                black_box(marginal_development_value(state, player));
            });
        },
    ));
    measurements.push(measure(
        "graph-feature-encode-pool",
        &states,
        options.repeats,
        |state| {
            let observer = state.actor();
            let graph = encode_heterogeneous_graph(state, observer, false);
            black_box(pool_heterogeneous_graph(&graph, observer));
        },
    ));
    measurements.push(measure(
        "node-without-evaluate",
        &states,
        options.repeats,
        |state| {
            let actions = state.legal_actions();
            if let Some(action) = actions.first() {
                let mut next = state.clone();
                black_box(next.apply(action).is_ok());
                black_box(next.state_hash());
            } else {
                black_box(state.state_hash());
            }
        },
    ));
    measurements.push(measure(
        "node-with-evaluate",
        &states,
        options.repeats,
        |state| {
            let actions = state.legal_actions();
            if let Some(action) = actions.first() {
                let mut next = state.clone();
                if next.apply(action).is_ok() {
                    black_box(evaluate(&next));
                } else {
                    black_box(evaluate(state));
                }
            } else {
                black_box(evaluate(state));
            }
        },
    ));

    let base = measurements
        .iter()
        .find(|measurement| measurement.name == "node-without-evaluate")
        .expect("node base measurement");
    let full = measurements
        .iter()
        .find(|measurement| measurement.name == "node-with-evaluate")
        .expect("node full measurement");
    let evaluator_time = (full.microseconds_per_state - base.microseconds_per_state).max(0.0);
    let evaluator_share = if full.microseconds_per_state > 0.0 {
        evaluator_time / full.microseconds_per_state
    } else {
        0.0
    };
    let infinite_speedup = if base.microseconds_per_state > 0.0 {
        full.microseconds_per_state / base.microseconds_per_state
    } else {
        f64::INFINITY
    };

    let evaluator_breakdown = evaluator_breakdown(&states, options.repeats);
    let report = Report {
        schema_version: 1,
        kind: "colonist-exact-evaluator-feasibility",
        players: options.players,
        sampled_states: states.len(),
        repeats: options.repeats,
        board_generator: benchmark_board_generator(options.players).serialized_id(),
        seed: options.seed,
        player_trades_enabled: options.player_trades_enabled,
        measurements,
        evaluator_breakdown,
        evaluator_share_of_node_primitive: evaluator_share,
        infinite_evaluator_speedup_ceiling: infinite_speedup,
    };
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize benchmark report")
    );
}
