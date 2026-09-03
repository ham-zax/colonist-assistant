use std::env;
use std::path::PathBuf;

use colonist_catan_arena::tactical_corpus::{
    build_state, default_corpus_path, load_tactical_corpus, TacticalScenario,
};
use colonist_catan_core::{
    Building, GameState, NodeKind, Phase, PlayerState, SplitMix64, TradeOffer,
};
use colonist_catan_search::{
    search_maxn_bounded, search_maxn_hostility_stress_bounded, DepthActionValue, DepthSearchResult,
};
use serde::{Deserialize, Serialize};

const HOSTILITY_GRID: [f32; 5] = [0.0, 0.25, 0.50, 0.75, 1.0];

#[derive(Clone, Debug)]
struct Args {
    corpus_path: PathBuf,
    scenario_ids: Vec<String>,
    takeover_snapshot: Option<PathBuf>,
    takeover_snapshot_id: Option<String>,
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
    output: Option<PathBuf>,
}

impl Default for Args {
    fn default() -> Self {
        Self {
            corpus_path: default_corpus_path(),
            scenario_ids: Vec::new(),
            takeover_snapshot: None,
            takeover_snapshot_id: None,
            depth: 3,
            branch_cap: 12,
            maximum_nodes: 48_000,
            output: None,
        }
    }
}

fn parse_args() -> Args {
    let mut parsed = Args::default();
    let args = env::args().skip(1).collect::<Vec<_>>();
    let mut index = 0usize;
    while index < args.len() {
        match args[index].as_str() {
            "--corpus" => {
                index += 1;
                parsed.corpus_path = PathBuf::from(
                    args.get(index)
                        .unwrap_or_else(|| panic!("--corpus requires a path")),
                );
            }
            "--scenario" => {
                index += 1;
                parsed.scenario_ids.push(
                    args.get(index)
                        .unwrap_or_else(|| panic!("--scenario requires an id"))
                        .clone(),
                );
            }
            "--takeover-snapshot" => {
                index += 1;
                parsed.takeover_snapshot = Some(PathBuf::from(
                    args.get(index)
                        .unwrap_or_else(|| panic!("--takeover-snapshot requires a JSONL path")),
                ));
            }
            "--snapshot-id" => {
                index += 1;
                parsed.takeover_snapshot_id = Some(
                    args.get(index)
                        .unwrap_or_else(|| panic!("--snapshot-id requires an id"))
                        .clone(),
                );
            }
            "--depth" => {
                index += 1;
                parsed.depth = args
                    .get(index)
                    .unwrap_or_else(|| panic!("--depth requires a value"))
                    .parse::<u8>()
                    .unwrap_or_else(|error| panic!("invalid --depth: {error}"));
            }
            "--branch-cap" => {
                index += 1;
                parsed.branch_cap = args
                    .get(index)
                    .unwrap_or_else(|| panic!("--branch-cap requires a value"))
                    .parse::<usize>()
                    .unwrap_or_else(|error| panic!("invalid --branch-cap: {error}"));
            }
            "--maximum-nodes" => {
                index += 1;
                parsed.maximum_nodes = args
                    .get(index)
                    .unwrap_or_else(|| panic!("--maximum-nodes requires a value"))
                    .parse::<u32>()
                    .unwrap_or_else(|error| panic!("invalid --maximum-nodes: {error}"));
            }
            "--output" => {
                index += 1;
                parsed.output = Some(PathBuf::from(
                    args.get(index)
                        .unwrap_or_else(|| panic!("--output requires a path")),
                ));
            }
            "--help" | "-h" => {
                println!(
                    "hostility-stress-benchmark\n\
                     \nRuns diagnostic-only MaxN hostility stress on the checked-in tactical corpus or\n\
                     one existing frozen takeover snapshot advanced to its first real root fork.\n\
                     Opponent utility: (1-h)*V_i + h*(1-V_root). Root utility and leaf\n\
                     evaluation remain ordinary MaxN. h is a controlled counterfactual, not\n\
                     an inferred hostility probability.\n\
                     \nOptions:\n\
                       --corpus PATH          Tactical corpus JSON (default: checked-in corpus)\n\
                       --scenario ID          Evaluate only this tactical scenario; repeatable\n\
                       --takeover-snapshot P  Existing frozen takeover JSONL instead of corpus\n\
                       --snapshot-id ID       Select exactly one snapshot row\n\
                       --depth N              MaxN depth (default: 3)\n\
                       --branch-cap N         Root/decision branch cap (default: 12)\n\
                       --maximum-nodes N      Node budget per search (default: 48000)\n\
                       --output PATH          Write pretty JSON instead of stdout\n"
                );
                std::process::exit(0);
            }
            other => panic!("unknown hostility stress option: {other}"),
        }
        index += 1;
    }
    if parsed.takeover_snapshot.is_some() && !parsed.scenario_ids.is_empty() {
        panic!("--takeover-snapshot cannot be combined with --scenario");
    }
    if parsed.takeover_snapshot.is_none() && parsed.takeover_snapshot_id.is_some() {
        panic!("--snapshot-id requires --takeover-snapshot");
    }
    parsed.branch_cap = parsed.branch_cap.max(1);
    parsed.maximum_nodes = parsed.maximum_nodes.max(1);
    parsed
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum PhaseSnapshot {
    SetupSettlement,
    SetupRoad { settlement: u8 },
    PreRoll,
    RollChance,
    Discard,
    MoveRobber,
    ResolveSteal { victim: u8 },
    Main,
    DevelopmentChance,
    TradeResponses,
    Finished,
}

impl From<PhaseSnapshot> for Phase {
    fn from(value: PhaseSnapshot) -> Self {
        match value {
            PhaseSnapshot::SetupSettlement => Self::SetupSettlement,
            PhaseSnapshot::SetupRoad { settlement } => Self::SetupRoad { settlement },
            PhaseSnapshot::PreRoll => Self::PreRoll,
            PhaseSnapshot::RollChance => Self::RollChance,
            PhaseSnapshot::Discard => Self::Discard,
            PhaseSnapshot::MoveRobber => Self::MoveRobber,
            PhaseSnapshot::ResolveSteal { victim } => Self::ResolveSteal { victim },
            PhaseSnapshot::Main => Self::Main,
            PhaseSnapshot::DevelopmentChance => Self::DevelopmentChance,
            PhaseSnapshot::TradeResponses => Self::TradeResponses,
            PhaseSnapshot::Finished => Self::Finished,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum BuildingSnapshot {
    Settlement { player: u8 },
    City { player: u8 },
}

impl From<BuildingSnapshot> for Building {
    fn from(value: BuildingSnapshot) -> Self {
        match value {
            BuildingSnapshot::Settlement { player } => Self::Settlement(player),
            BuildingSnapshot::City { player } => Self::City(player),
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PlayerStateSnapshot {
    resources: [u8; 5],
    development: [u8; 5],
    bought_development: [u8; 5],
    public_victory_points: u8,
    played_knights: u8,
    roads_left: u8,
    settlements_left: u8,
    cities_left: u8,
    has_longest_road: bool,
    has_largest_army: bool,
    played_development_this_turn: bool,
    policy_profile: [u8; 5],
}

impl From<PlayerStateSnapshot> for PlayerState {
    fn from(value: PlayerStateSnapshot) -> Self {
        Self {
            resources: value.resources,
            development: value.development,
            bought_development: value.bought_development,
            public_victory_points: value.public_victory_points,
            played_knights: value.played_knights,
            roads_left: value.roads_left,
            settlements_left: value.settlements_left,
            cities_left: value.cities_left,
            has_longest_road: value.has_longest_road,
            has_largest_army: value.has_largest_army,
            played_development_this_turn: value.played_development_this_turn,
            policy_profile: value.policy_profile,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TradeOfferSnapshot {
    creator: u8,
    recipients: u8,
    give: [u8; 5],
    receive: [u8; 5],
    accepted: u8,
    rejected: u8,
}

impl From<TradeOfferSnapshot> for TradeOffer {
    fn from(value: TradeOfferSnapshot) -> Self {
        Self {
            creator: value.creator,
            recipients: value.recipients,
            give: value.give,
            receive: value.receive,
            accepted: value.accepted,
            rejected: value.rejected,
        }
    }
}

fn default_player_trades_enabled() -> bool {
    true
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GameStateSnapshot {
    players: Vec<PlayerStateSnapshot>,
    buildings: Vec<Option<BuildingSnapshot>>,
    roads: Vec<Option<u8>>,
    bank: [u8; 5],
    bank_is_public: bool,
    development_deck: [u8; 5],
    played_development: [u8; 5],
    robber_hex: u8,
    current_player: u8,
    phase: PhaseSnapshot,
    turn: u16,
    last_roll: u8,
    victory_target: u8,
    card_discard_limit: u8,
    friendly_robber: bool,
    setup_step: u8,
    discard_remaining: [u8; 4],
    discard_cursor: u8,
    robber_return_phase: PhaseSnapshot,
    free_roads: u8,
    domestic_trade_used: bool,
    domestic_trade_count: u8,
    #[serde(default = "default_player_trades_enabled")]
    player_trades_enabled: bool,
    #[serde(default)]
    domestic_trade_embargoes: u16,
    last_rejected_trade: Option<TradeOfferSnapshot>,
    trade: Option<TradeOfferSnapshot>,
    trade_cursor: u8,
    trade_negotiation_round: u8,
    longest_road_holder: Option<u8>,
    largest_army_holder: Option<u8>,
}

impl GameStateSnapshot {
    fn restore(self, board_seed: u64, players: u8) -> GameState {
        let mut state = GameState::standard(board_seed, players);
        state.players = self.players.into_iter().map(PlayerState::from).collect();
        state.buildings = self
            .buildings
            .into_iter()
            .map(|building| building.map(Building::from))
            .collect();
        state.roads = self.roads;
        state.bank = self.bank;
        state.bank_is_public = self.bank_is_public;
        state.development_deck = self.development_deck;
        state.played_development = self.played_development;
        state.robber_hex = self.robber_hex;
        state.current_player = self.current_player;
        state.phase = self.phase.into();
        state.turn = self.turn;
        state.last_roll = self.last_roll;
        state.victory_target = self.victory_target;
        state.card_discard_limit = self.card_discard_limit;
        state.friendly_robber = self.friendly_robber;
        state.setup_step = self.setup_step;
        state.discard_remaining = self.discard_remaining;
        state.discard_cursor = self.discard_cursor;
        state.robber_return_phase = self.robber_return_phase.into();
        state.free_roads = self.free_roads;
        state.domestic_trade_used = self.domestic_trade_used;
        state.domestic_trade_count = self.domestic_trade_count;
        state.player_trades_enabled = self.player_trades_enabled;
        state.domestic_trade_embargoes = self.domestic_trade_embargoes;
        state.last_rejected_trade = self.last_rejected_trade.map(TradeOffer::from);
        state.trade = self.trade.map(TradeOffer::from);
        state.trade_cursor = self.trade_cursor;
        state.trade_negotiation_round = self.trade_negotiation_round;
        state.longest_road_holder = self.longest_road_holder;
        state.largest_army_holder = self.largest_army_holder;
        state
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct TakeoverSnapshot {
    snapshot_id: String,
    board_seed: u64,
    players: u8,
    state_hash: String,
    chance_rng_state: u64,
    target_seat: u8,
    game_state: GameStateSnapshot,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SearchConfigReport {
    depth: u8,
    branch_cap: usize,
    maximum_nodes: u32,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AlternativeReport {
    action: String,
    root_value: f32,
    value: [f32; 4],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct StressRunReport {
    h: f32,
    selected_root: Option<String>,
    root_value: f32,
    value: [f32; 4],
    changed_from_h0: bool,
    nodes: u32,
    reached_depth: u8,
    deadline_reached: bool,
    alternatives: Vec<AlternativeReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ScenarioReport {
    id: String,
    family: String,
    description: String,
    source_state_hash: Option<String>,
    pre_root_actions: Vec<String>,
    state_hash: String,
    root_player: u8,
    declared_candidate_roots: Vec<String>,
    declared_expected_root: String,
    declared_negative_control_root: Option<String>,
    scenario_is_negative_control: bool,
    ordinary_selected_root: Option<String>,
    ordinary_root_value: f32,
    ordinary_value: [f32; 4],
    h0_equivalent: bool,
    first_switch_h: Option<f32>,
    runs: Vec<StressRunReport>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BenchmarkReport {
    kind: &'static str,
    input_kind: &'static str,
    input_path: String,
    corpus_schema_version: Option<u32>,
    hostility_formula: &'static str,
    stress_parameter_semantics: &'static str,
    hostility_grid: [f32; 5],
    search: SearchConfigReport,
    selected_scenarios: Vec<String>,
    all_h0_equivalent: bool,
    switched_scenarios: usize,
    stable_scenarios: usize,
    scenarios: Vec<ScenarioReport>,
}

fn action_values_equal(left: &DepthActionValue, right: &DepthActionValue) -> bool {
    left.action == right.action
        && left.value == right.value
        && left.legal_weight == right.legal_weight
        && left.lower_confidence_value == right.lower_confidence_value
}

fn searches_exactly_equal(left: &DepthSearchResult, right: &DepthSearchResult) -> bool {
    left.chosen == right.chosen
        && left.value == right.value
        && left.nodes == right.nodes
        && left.cutoffs == right.cutoffs
        && left.depth == right.depth
        && left.deadline_reached == right.deadline_reached
        && left.actions.len() == right.actions.len()
        && left
            .actions
            .iter()
            .zip(&right.actions)
            .all(|(left, right)| action_values_equal(left, right))
}

fn alternative_reports(actions: &[DepthActionValue], root: usize) -> Vec<AlternativeReport> {
    actions
        .iter()
        .map(|entry| AlternativeReport {
            action: format!("{:?}", entry.action),
            root_value: entry.value[root],
            value: entry.value,
        })
        .collect()
}

struct CaseDefinition {
    id: String,
    family: String,
    description: String,
    state: GameState,
    source_state_hash: Option<String>,
    pre_root_actions: Vec<String>,
    declared_candidate_roots: Vec<String>,
    declared_expected_root: String,
    declared_negative_control_root: Option<String>,
    scenario_is_negative_control: bool,
}

fn run_case(case: CaseDefinition, args: &Args) -> ScenarioReport {
    let root = case.state.actor() as usize;
    let ordinary = search_maxn_bounded(
        &case.state,
        args.depth,
        args.branch_cap,
        args.maximum_nodes,
    );
    let h0 = search_maxn_hostility_stress_bounded(
        &case.state,
        0.0,
        args.depth,
        args.branch_cap,
        args.maximum_nodes,
    )
    .unwrap_or_else(|error| panic!("h=0 stress failed for {}: {error}", case.id));
    let h0_equivalent = searches_exactly_equal(&ordinary, &h0);

    let mut runs = Vec::with_capacity(HOSTILITY_GRID.len());
    let mut first_switch_h = None;
    for h in HOSTILITY_GRID {
        let result = if h == 0.0 {
            h0.clone()
        } else {
            search_maxn_hostility_stress_bounded(
                &case.state,
                h,
                args.depth,
                args.branch_cap,
                args.maximum_nodes,
            )
            .unwrap_or_else(|error| panic!("h={h} stress failed for {}: {error}", case.id))
        };
        let changed_from_h0 = result.chosen != ordinary.chosen;
        if h > 0.0 && changed_from_h0 && first_switch_h.is_none() {
            first_switch_h = Some(h);
        }
        runs.push(StressRunReport {
            h,
            selected_root: result.chosen.as_ref().map(|action| format!("{action:?}")),
            root_value: result.value[root],
            value: result.value,
            changed_from_h0,
            nodes: result.nodes,
            reached_depth: result.depth,
            deadline_reached: result.deadline_reached,
            alternatives: alternative_reports(&result.actions, root),
        });
    }

    ScenarioReport {
        id: case.id,
        family: case.family,
        description: case.description,
        source_state_hash: case.source_state_hash,
        pre_root_actions: case.pre_root_actions,
        state_hash: format!("{:016x}", case.state.state_hash()),
        root_player: root as u8,
        declared_candidate_roots: case.declared_candidate_roots,
        declared_expected_root: case.declared_expected_root,
        declared_negative_control_root: case.declared_negative_control_root,
        scenario_is_negative_control: case.scenario_is_negative_control,
        ordinary_selected_root: ordinary.chosen.as_ref().map(|action| format!("{action:?}")),
        ordinary_root_value: ordinary.value[root],
        ordinary_value: ordinary.value,
        h0_equivalent,
        first_switch_h,
        runs,
    }
}

fn run_scenario(scenario: &TacticalScenario, args: &Args) -> ScenarioReport {
    let state = build_state(&scenario.state)
        .unwrap_or_else(|error| panic!("invalid tactical scenario {}: {error}", scenario.id));
    run_case(
        CaseDefinition {
            id: scenario.id.clone(),
            family: scenario.family.clone(),
            description: scenario.description.clone(),
            state,
            source_state_hash: None,
            pre_root_actions: Vec::new(),
            declared_candidate_roots: scenario
                .candidate_roots
                .iter()
                .map(|root| format!("{:?}", root.to_action()))
                .collect(),
            declared_expected_root: format!("{:?}", scenario.expected_best_root.to_action()),
            declared_negative_control_root: scenario
                .negative_control_root
                .as_ref()
                .map(|root| format!("{:?}", root.to_action())),
            scenario_is_negative_control: scenario.is_negative_control,
        },
        args,
    )
}

fn load_takeover_snapshot(path: &PathBuf, selected_id: Option<&str>) -> TakeoverSnapshot {
    let contents = std::fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
    let mut selected = None;
    for (line_index, line) in contents.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let snapshot: TakeoverSnapshot = serde_json::from_str(line).unwrap_or_else(|error| {
            panic!(
                "invalid takeover snapshot {} line {}: {error}",
                path.display(),
                line_index + 1
            )
        });
        if selected_id.is_some_and(|id| id != snapshot.snapshot_id) {
            continue;
        }
        if selected.is_some() {
            panic!("takeover snapshot selection must resolve to exactly one row");
        }
        selected = Some(snapshot);
    }
    selected.unwrap_or_else(|| panic!("takeover snapshot selection matched no rows"))
}

fn advance_to_first_takeover_fork(
    mut state: GameState,
    chance_rng_state: u64,
    target_seat: u8,
) -> (GameState, Vec<String>) {
    let mut chance_rng = SplitMix64::from_state(chance_rng_state);
    let mut pre_root_actions = Vec::new();
    for _ in 0..16 {
        if state.is_terminal() {
            panic!("takeover snapshot terminated before the first strategic root fork");
        }
        match state.node_kind() {
            NodeKind::Chance => {
                let action = state
                    .sample_chance(&mut chance_rng)
                    .expect("takeover chance state must have an outcome");
                pre_root_actions.push(format!("{action:?}"));
                state.apply(&action).unwrap_or_else(|error| {
                    panic!("failed to apply frozen takeover chance action {action:?}: {error}")
                });
            }
            NodeKind::Decision { actor } => {
                if actor != target_seat {
                    panic!(
                        "takeover snapshot requires non-target seat {actor} to choose before target seat {target_seat}'s first fork; offline stress refuses to invent that policy decision"
                    );
                }
                let legal = state.legal_actions();
                if legal.len() > 1 {
                    return (state, pre_root_actions);
                }
                let action = legal
                    .into_iter()
                    .next()
                    .expect("nonterminal takeover decision must have a legal action");
                pre_root_actions.push(format!("{action:?}"));
                state.apply(&action).unwrap_or_else(|error| {
                    panic!("failed to apply forced takeover protocol action {action:?}: {error}")
                });
            }
            NodeKind::Terminal => {
                panic!("takeover snapshot terminated before the first strategic root fork")
            }
        }
    }
    panic!("takeover snapshot exceeded the bounded pre-root protocol replay")
}

fn run_takeover_snapshot(snapshot: TakeoverSnapshot, args: &Args) -> ScenarioReport {
    let snapshot_id = snapshot.snapshot_id.clone();
    let source_state_hash = snapshot.state_hash.clone();
    let target_seat = snapshot.target_seat;
    let state = snapshot.game_state.restore(snapshot.board_seed, snapshot.players);
    assert_eq!(
        format!("{:016x}", state.state_hash()),
        source_state_hash,
        "takeover snapshot restoration must preserve the recorded state hash"
    );
    assert_eq!(
        state.phase,
        Phase::PreRoll,
        "offline takeover stress accepts stable PreRoll snapshots only"
    );
    assert_eq!(
        state.actor(),
        target_seat,
        "takeover snapshot target seat must own the frozen boundary"
    );
    let (fork_state, pre_root_actions) =
        advance_to_first_takeover_fork(state, snapshot.chance_rng_state, target_seat);
    assert_eq!(
        fork_state.actor(),
        target_seat,
        "target seat must own the first strategic fork"
    );
    assert!(
        fork_state.legal_actions().len() > 1,
        "takeover replay must end at a genuine multi-action root fork"
    );
    run_case(
        CaseDefinition {
            id: snapshot_id,
            family: "FrozenTakeover".into(),
            description: "Existing frozen takeover state advanced only through frozen chance and target-owned forced protocol steps to the first real root fork".into(),
            state: fork_state,
            source_state_hash: Some(source_state_hash),
            pre_root_actions,
            declared_candidate_roots: Vec::new(),
            declared_expected_root: "not-declared".into(),
            declared_negative_control_root: None,
            scenario_is_negative_control: false,
        },
        args,
    )
}

fn main() {
    let args = parse_args();
    let (input_kind, input_path, corpus_schema_version, scenarios) =
        if let Some(path) = args.takeover_snapshot.as_ref() {
            let snapshot = load_takeover_snapshot(path, args.takeover_snapshot_id.as_deref());
            (
                "frozen-takeover-snapshot",
                path.display().to_string(),
                None,
                vec![run_takeover_snapshot(snapshot, &args)],
            )
        } else {
            let corpus = load_tactical_corpus(&args.corpus_path)
                .unwrap_or_else(|error| panic!("failed to load {:?}: {error}", args.corpus_path));
            let selected = corpus
                .scenarios
                .iter()
                .filter(|scenario| {
                    args.scenario_ids.is_empty()
                        || args.scenario_ids.iter().any(|id| id == &scenario.id)
                })
                .collect::<Vec<_>>();
            if selected.is_empty() {
                panic!("hostility stress selection contains no scenarios");
            }
            for requested in &args.scenario_ids {
                if !selected.iter().any(|scenario| &scenario.id == requested) {
                    panic!("unknown tactical scenario: {requested}");
                }
            }
            let scenarios = selected
                .into_iter()
                .map(|scenario| run_scenario(scenario, &args))
                .collect::<Vec<_>>();
            (
                "tactical-corpus",
                args.corpus_path.display().to_string(),
                Some(corpus.schema_version),
                scenarios,
            )
        };

    let switched_scenarios = scenarios
        .iter()
        .filter(|scenario| scenario.first_switch_h.is_some())
        .count();
    let all_h0_equivalent = scenarios.iter().all(|scenario| scenario.h0_equivalent);
    let report = BenchmarkReport {
        kind: "colonist-offline-hostility-stress-benchmark",
        input_kind,
        input_path,
        corpus_schema_version,
        hostility_formula: "U_i(a|h)=(1-h)*V_i(a)+h*(1-V_root(a))",
        stress_parameter_semantics:
            "controlled counterfactual stress parameter; not an inferred hostility probability",
        hostility_grid: HOSTILITY_GRID,
        search: SearchConfigReport {
            depth: args.depth,
            branch_cap: args.branch_cap,
            maximum_nodes: args.maximum_nodes,
        },
        selected_scenarios: scenarios
            .iter()
            .map(|scenario| scenario.id.clone())
            .collect(),
        all_h0_equivalent,
        switched_scenarios,
        stable_scenarios: scenarios.len() - switched_scenarios,
        scenarios,
    };

    let json = serde_json::to_string_pretty(&report).expect("hostility report must serialize");
    if let Some(output) = args.output {
        std::fs::write(&output, format!("{json}\n"))
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
    } else {
        println!("{json}");
    }
}
