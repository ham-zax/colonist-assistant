use std::env;
use std::fs;
use std::time::Instant;

use colonist_catan_core::{Action, GameState, Phase, SplitMix64};
use colonist_catan_search::{CudaSimEngine, CudaSimPackedState, CudaSimRootSearchResult};

const STATE_WORDS: usize = 404;
const ACTION_WORDS: usize = 12;
const ROOT_LIMIT: usize = 8;
const DEFAULT_ROLLOUTS_PER_ROOT: usize = 4096;
const DEFAULT_REPETITIONS: usize = 3;
const SEED: u64 = 0x1234_5678_9abc_def0;

const HEX_COUNT: usize = 19;
const VERTEX_COUNT: usize = 54;
const EDGE_COUNT: usize = 72;
const MAX_VERTEX_ADJACENCY: usize = 3;
const TOPO_VERTEX_HEX_COUNTS: usize = 0;
const TOPO_VERTEX_HEXES: usize = TOPO_VERTEX_HEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_VERTEX_COUNTS: usize = TOPO_VERTEX_HEXES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_VERTICES: usize = TOPO_VERTEX_VERTEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_EDGE_COUNTS: usize = TOPO_VERTEX_VERTICES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_EDGES: usize = TOPO_VERTEX_EDGE_COUNTS + VERTEX_COUNT;
const TOPO_EDGE_VERTICES: usize = TOPO_VERTEX_EDGES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPOLOGY_WORDS: usize = TOPO_EDGE_VERTICES + EDGE_COUNT * 2;

const ACTION_TAG: usize = 0;
const ACTION_ARG0: usize = 1;
const ACTION_ROLL: u32 = 2;
const ACTION_BUILD_ROAD: u32 = 7;
const ACTION_BUILD_SETTLEMENT: u32 = 8;
const ACTION_BUILD_CITY: u32 = 9;
const ACTION_BUY_DEVELOPMENT: u32 = 10;
const ACTION_PLAY_KNIGHT: u32 = 12;
const ACTION_PLAY_ROAD_BUILDING: u32 = 13;
const ACTION_PLAY_YEAR_OF_PLENTY: u32 = 14;
const ACTION_PLAY_MONOPOLY: u32 = 15;
const ACTION_MARITIME_TRADE: u32 = 16;
const ACTION_END_TURN: u32 = 17;

#[derive(Debug)]
struct BenchResult {
    steps: usize,
    lane_count: usize,
    median_ms: f64,
    min_ms: f64,
    max_ms: f64,
    rollout_steps_per_second: f64,
    result: CudaSimRootSearchResult,
}

fn arg_value(name: &str) -> Option<String> {
    let prefix = format!("--{name}=");
    env::args().find_map(|arg| arg.strip_prefix(&prefix).map(ToOwned::to_owned))
}

fn json_escape(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
}

fn vec_json(values: &[u32]) -> String {
    let mut out = String::with_capacity(values.len() * 4);
    out.push('[');
    for (index, value) in values.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        out.push_str(&value.to_string());
    }
    out.push(']');
    out
}

fn root_supported(action: &Action) -> bool {
    matches!(
        action,
        Action::Roll
            | Action::BuildRoad { .. }
            | Action::BuildSettlement { .. }
            | Action::BuildCity { .. }
            | Action::BuyDevelopment
            | Action::PlayKnight { .. }
            | Action::PlayRoadBuilding { .. }
            | Action::PlayYearOfPlenty { .. }
            | Action::PlayMonopoly { .. }
            | Action::MaritimeTrade { .. }
            | Action::EndTurn
    )
}

fn representative_state() -> Result<(GameState, Vec<Action>, u64, usize), String> {
    for attempt in 0..256u64 {
        let board_seed = 91_000 + attempt * 37;
        let mut state = GameState::standard(board_seed, 4);
        state.player_trades_enabled = false;
        let mut rng = SplitMix64::new(42_000 + attempt * 101);
        for step in 0..240usize {
            if state.is_terminal() {
                break;
            }
            if matches!(state.phase, Phase::Main) {
                let legal = state
                    .legal_actions()
                    .into_iter()
                    .filter(root_supported)
                    .collect::<Vec<_>>();
                let road_roots = legal
                    .iter()
                    .filter(|action| matches!(action, Action::BuildRoad { .. }))
                    .count();
                if legal.len() >= ROOT_LIMIT && road_roots >= 2 {
                    return Ok((state, legal.into_iter().take(ROOT_LIMIT).collect(), board_seed, step));
                }
            }
            let legal = state.legal_actions();
            if legal.is_empty() {
                break;
            }
            let action = legal[rng.range(legal.len())].clone();
            state.apply(&action).map_err(|error| error.to_string())?;
        }
    }
    Err("could not construct a deterministic representative midgame state".into())
}

fn topology_words(state: &GameState) -> Result<Vec<u32>, String> {
    let board = state.board.as_ref();
    if board.hexes.len() != HEX_COUNT
        || board.vertices.len() != VERTEX_COUNT
        || board.edges.len() != EDGE_COUNT
    {
        return Err("unexpected board topology".into());
    }
    let mut words = vec![0u32; TOPOLOGY_WORDS];
    for (vertex, data) in board.vertices.iter().enumerate() {
        if data.adjacent_hexes.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_vertices.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_edges.len() > MAX_VERTEX_ADJACENCY
        {
            return Err("vertex adjacency exceeds standard board limit".into());
        }
        words[TOPO_VERTEX_HEX_COUNTS + vertex] = data.adjacent_hexes.len() as u32;
        words[TOPO_VERTEX_VERTEX_COUNTS + vertex] = data.adjacent_vertices.len() as u32;
        words[TOPO_VERTEX_EDGE_COUNTS + vertex] = data.adjacent_edges.len() as u32;
        for (slot, hex) in data.adjacent_hexes.iter().enumerate() {
            words[TOPO_VERTEX_HEXES + vertex * MAX_VERTEX_ADJACENCY + slot] = *hex as u32;
        }
        for (slot, adjacent) in data.adjacent_vertices.iter().enumerate() {
            words[TOPO_VERTEX_VERTICES + vertex * MAX_VERTEX_ADJACENCY + slot] = *adjacent as u32;
        }
        for (slot, edge) in data.adjacent_edges.iter().enumerate() {
            words[TOPO_VERTEX_EDGES + vertex * MAX_VERTEX_ADJACENCY + slot] = *edge as u32;
        }
    }
    for (edge, data) in board.edges.iter().enumerate() {
        words[TOPO_EDGE_VERTICES + edge * 2] = data.vertices[0] as u32;
        words[TOPO_EDGE_VERTICES + edge * 2 + 1] = data.vertices[1] as u32;
    }
    Ok(words)
}

fn pack_action(action: &Action) -> Result<[u32; ACTION_WORDS], String> {
    let mut words = [0u32; ACTION_WORDS];
    match action {
        Action::Roll => words[ACTION_TAG] = ACTION_ROLL,
        Action::BuildRoad { edge } => {
            words[ACTION_TAG] = ACTION_BUILD_ROAD;
            words[ACTION_ARG0] = *edge as u32;
        }
        Action::BuildSettlement { vertex } => {
            words[ACTION_TAG] = ACTION_BUILD_SETTLEMENT;
            words[ACTION_ARG0] = *vertex as u32;
        }
        Action::BuildCity { vertex } => {
            words[ACTION_TAG] = ACTION_BUILD_CITY;
            words[ACTION_ARG0] = *vertex as u32;
        }
        Action::BuyDevelopment => words[ACTION_TAG] = ACTION_BUY_DEVELOPMENT,
        Action::PlayKnight { hex, victim } => {
            words[ACTION_TAG] = ACTION_PLAY_KNIGHT;
            words[ACTION_ARG0] = *hex as u32;
            words[ACTION_ARG0 + 1] = victim.map(|player| player as u32 + 1).unwrap_or(0);
        }
        Action::PlayRoadBuilding { first, second } => {
            words[ACTION_TAG] = ACTION_PLAY_ROAD_BUILDING;
            words[ACTION_ARG0] = *first as u32;
            words[ACTION_ARG0 + 1] = second.map(|edge| edge as u32 + 1).unwrap_or(0);
        }
        Action::PlayYearOfPlenty { first, second } => {
            words[ACTION_TAG] = ACTION_PLAY_YEAR_OF_PLENTY;
            words[ACTION_ARG0] = first.index() as u32;
            words[ACTION_ARG0 + 1] = second.index() as u32;
        }
        Action::PlayMonopoly { resource } => {
            words[ACTION_TAG] = ACTION_PLAY_MONOPOLY;
            words[ACTION_ARG0] = resource.index() as u32;
        }
        Action::MaritimeTrade {
            give,
            receive,
            ratio,
        } => {
            words[ACTION_TAG] = ACTION_MARITIME_TRADE;
            words[ACTION_ARG0] = give.index() as u32;
            words[ACTION_ARG0 + 1] = receive.index() as u32;
            words[ACTION_ARG0 + 2] = *ratio as u32;
        }
        Action::EndTurn => words[ACTION_TAG] = ACTION_END_TURN,
        other => return Err(format!("unsupported feasibility root action: {other:?}")),
    }
    Ok(words)
}

fn root_words(actions: &[Action]) -> Result<Vec<u32>, String> {
    let mut words = vec![0u32; ACTION_WORDS * actions.len()];
    for (root, action) in actions.iter().enumerate() {
        let packed = pack_action(action)?;
        for (field, value) in packed.into_iter().enumerate() {
            words[field * actions.len() + root] = value;
        }
    }
    Ok(words)
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    sorted[sorted.len() / 2]
}

fn benchmark(
    engine: &mut CudaSimEngine,
    roots: &[Action],
    rollouts_per_root: usize,
    steps: usize,
    repetitions: usize,
) -> Result<BenchResult, String> {
    let rows = vec![roots.to_vec()];
    let lane_count = roots.len() * rollouts_per_root;
    let mut times = Vec::with_capacity(repetitions);
    let mut result = None;
    for repetition in 0..repetitions {
        let started = Instant::now();
        let next = engine
            .search_root_actions(
                &rows,
                rollouts_per_root,
                steps,
                SEED,
            )
            .map_err(|error| error.to_string())?;
        let elapsed_ms = started.elapsed().as_secs_f64() * 1000.0;
        times.push(elapsed_ms);
        if repetition + 1 == repetitions {
            result = Some(next);
        }
    }
    let median_ms = median(&times);
    let lane_steps = lane_count.saturating_mul(steps) as f64;
    Ok(BenchResult {
        steps,
        lane_count,
        median_ms,
        min_ms: times.iter().copied().fold(f64::INFINITY, f64::min),
        max_ms: times.iter().copied().fold(0.0, f64::max),
        rollout_steps_per_second: lane_steps / (median_ms / 1000.0),
        result: result.expect("at least one repetition"),
    })
}

fn root_stats_json(result: &CudaSimRootSearchResult, roots: &[Action]) -> String {
    let row = &result.rows[0];
    let mut out = String::from("[");
    for (index, stats) in row.iter().enumerate() {
        if index != 0 {
            out.push(',');
        }
        let label = json_escape(&format!("{:?}", roots[index]));
        out.push_str(&format!(
            "{{\"root\":{},\"label\":\"{}\",\"samples\":{},\"errors\":{},\"terminalSamples\":{},\"wins\":{},\"meanTurn\":{:.6},\"meanVictoryPoints\":{:.6},\"meanBestOpponentVictoryPoints\":{:.6},\"meanVictoryMargin\":{:.6}}}",
            index,
            label,
            stats.samples,
            stats.errors,
            stats.terminal_samples,
            stats.wins,
            stats.mean_turn,
            stats.mean_victory_points,
            stats.mean_best_opponent_victory_points,
            stats.mean_victory_margin(),
        ));
    }
    out.push(']');
    out
}

fn bench_json(result: &BenchResult, roots: &[Action]) -> String {
    format!(
        "{{\"steps\":{},\"laneCount\":{},\"medianMs\":{:.6},\"minMs\":{:.6},\"maxMs\":{:.6},\"rolloutStepsPerSecond\":{:.3},\"roots\":{}}}",
        result.steps,
        result.lane_count,
        result.median_ms,
        result.min_ms,
        result.max_ms,
        result.rollout_steps_per_second,
        root_stats_json(&result.result, roots),
    )
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = arg_value("output").unwrap_or_else(|| "experiments/webgpu-rollout/case.json".into());
    let rollouts_per_root = arg_value("rollouts")
        .as_deref()
        .unwrap_or("4096")
        .parse::<usize>()
        .unwrap_or(DEFAULT_ROLLOUTS_PER_ROOT);
    let repetitions = arg_value("repetitions")
        .as_deref()
        .unwrap_or("3")
        .parse::<usize>()
        .unwrap_or(DEFAULT_REPETITIONS)
        .max(1);

    let (state, roots, board_seed, generated_actions) = representative_state()?;
    if roots.len() != ROOT_LIMIT {
        return Err(format!("expected {ROOT_LIMIT} roots, got {}", roots.len()).into());
    }
    if state.player_trades_enabled {
        return Err("representative state unexpectedly enables player trades".into());
    }
    let packed = CudaSimPackedState::new(&state)?;
    let base_state_words = packed.words().to_vec();
    if base_state_words.len() != STATE_WORDS {
        return Err("packed state size drifted".into());
    }
    let topology = topology_words(&state)?;
    let root_action_words = root_words(&roots)?;

    let init_started = Instant::now();
    let mut engine = CudaSimEngine::new()?;
    let init_ms = init_started.elapsed().as_secs_f64() * 1000.0;
    let upload_started = Instant::now();
    engine.upload_states(std::slice::from_ref(&state))?;
    let upload_ms = upload_started.elapsed().as_secs_f64() * 1000.0;
    let warm_started = Instant::now();
    let warmup = engine.search_root_actions(&[roots.clone()], 64, 16, SEED)?;
    let warmup_ms = warm_started.elapsed().as_secs_f64() * 1000.0;
    if warmup.rows[0].iter().any(|stats| stats.errors != 0) {
        return Err("CUDA warmup produced rollout errors".into());
    }

    let cuda0 = benchmark(&mut engine, &roots, rollouts_per_root, 0, repetitions)?;
    let cuda48 = benchmark(&mut engine, &roots, rollouts_per_root, 48, repetitions)?;
    let cuda96 = benchmark(&mut engine, &roots, rollouts_per_root, 96, repetitions)?;
    let labels = roots
        .iter()
        .map(|action| format!("\"{}\"", json_escape(&format!("{action:?}"))))
        .collect::<Vec<_>>()
        .join(",");

    let seed_lo = SEED as u32;
    let seed_hi = (SEED >> 32) as u32;
    let mut report = String::from("{\n");
    report.push_str("  \"schema\": \"colonist-webgpu-rollout-feasibility/1\",\n");
    report.push_str(&format!(
        "  \"generator\": {{\"boardSeed\": {board_seed}, \"generatedActions\": {generated_actions}, \"players\": 4, \"playerTradesEnabled\": false}},\n"
    ));
    report.push_str(&format!(
        "  \"layout\": {{\"stateWords\": {STATE_WORDS}, \"actionWords\": {ACTION_WORDS}, \"topologyWords\": {TOPOLOGY_WORDS}, \"laneWords\": 421, \"stateBytesPerLane\": {}, \"laneBytesPerLane\": 1684, \"storageBuffers\": 5}},\n",
        STATE_WORDS * 4
    ));
    report.push_str(&format!(
        "  \"seed\": {{\"lo\": {seed_lo}, \"hi\": {seed_hi}, \"hex\": \"0x123456789abcdef0\"}},\n"
    ));
    report.push_str(&format!("  \"baseStateWords\": {},\n", vec_json(&base_state_words)));
    report.push_str(&format!("  \"topologyWordsData\": {},\n", vec_json(&topology)));
    report.push_str(&format!("  \"rootActionWords\": {},\n", vec_json(&root_action_words)));
    report.push_str("  \"rootBaseIndices\": [0,0,0,0,0,0,0,0],\n");
    report.push_str(&format!("  \"rootLabels\": [{labels}],\n"));
    report.push_str("  \"cuda\": {\n");
    report.push_str(&format!(
        "    \"device\": \"{}\",\n",
        json_escape(&engine.device_identity().name)
    ));
    report.push_str(&format!("    \"initMs\": {init_ms:.6},\n"));
    report.push_str(&format!("    \"uploadMs\": {upload_ms:.6},\n"));
    report.push_str(&format!("    \"warmupMs\": {warmup_ms:.6},\n"));
    report.push_str(&format!("    \"rolloutsPerRoot\": {rollouts_per_root},\n"));
    report.push_str(&format!("    \"repetitions\": {repetitions},\n"));
    report.push_str(&format!("    \"h0ExpandReduceUpperBound\": {},\n", bench_json(&cuda0, &roots)));
    report.push_str(&format!("    \"h48\": {},\n", bench_json(&cuda48, &roots)));
    report.push_str(&format!("    \"h96\": {}\n", bench_json(&cuda96, &roots)));
    report.push_str("  }\n}\n");
    fs::write(&output, report)?;
    println!(
        "wrote {output}: roots={} lanes={} cuda48={:.3}ms cuda96={:.3}ms gpu={}",
        roots.len(),
        roots.len() * rollouts_per_root,
        cuda48.median_ms,
        cuda96.median_ms,
        engine.device_identity().name,
    );
    Ok(())
}
