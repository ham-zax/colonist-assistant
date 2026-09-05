#![cfg(feature = "cuda-sim")]

use std::fmt;
use std::sync::Arc;

use colonist_catan_core::{
    Action, Board, Building, DevCard, GameState, Phase, Port, Resource, TradeOffer,
};
use cudarc::driver::{
    CudaContext, CudaFunction, CudaModule, CudaSlice, DriverError, LaunchConfig, PushKernelArg,
};
use cudarc::nvrtc::Ptx;

use crate::rollout_cutoff::ROLLOUT_CUTOFF_SCALE;

const MAX_PLAYERS: usize = 4;
const HEX_COUNT: usize = 19;
const VERTEX_COUNT: usize = 54;
const EDGE_COUNT: usize = 72;
const MAX_VERTEX_ADJACENCY: usize = 3;
const INITIAL_BATCH_CAPACITY: usize = 256;
const THREADS_PER_BLOCK: usize = 128;
const SEARCH_CHUNK_LANES: usize = 65_536;
const SEED_INDEX_MIX: u64 = 0xd134_2543_de82_ef95;
const GAME_RNG_DOMAIN: u64 = 0x243f_6a88_85a3_08d3;
const GAME_CHANCE_RNG_DOMAIN: u64 = 0x4528_21e6_38d0_1377;
const BOARD_RNG_DOMAIN: u64 = 0x1319_8a2e_0370_7344;
const CANDIDATE_DECISION_DOMAIN: u64 = 0xbe54_66cf_34e9_0c6c;

const STATE_NUM_PLAYERS: usize = 0;
const STATE_PHASE: usize = 1;
const STATE_PHASE_ARG: usize = 2;
const STATE_CURRENT_PLAYER: usize = 3;
const STATE_ROBBER_HEX: usize = 4;
const STATE_VICTORY_TARGET: usize = 5;
const STATE_DISCARD_LIMIT: usize = 6;
const STATE_BANK_PUBLIC: usize = 7;
const STATE_LONGEST_HOLDER: usize = 8;
const STATE_LARGEST_HOLDER: usize = 9;
const STATE_TURN: usize = 10;
const STATE_LAST_ROLL: usize = 11;
const STATE_FRIENDLY_ROBBER: usize = 12;
const STATE_SETUP_STEP: usize = 13;
const STATE_DISCARD_CURSOR: usize = 14;
const STATE_ROBBER_RETURN_PHASE: usize = 15;
const STATE_ROBBER_RETURN_ARG: usize = 16;
const STATE_FREE_ROADS: usize = 17;
const STATE_DOMESTIC_TRADE_USED: usize = 18;
const STATE_DOMESTIC_TRADE_COUNT: usize = 19;
const STATE_PLAYER_TRADES_ENABLED: usize = 20;
const STATE_TRADE_CURSOR: usize = 21;
const STATE_TRADE_NEGOTIATION_ROUND: usize = 22;
const TRADE_STRIDE: usize = 15;
const TRADE_PRESENT: usize = 0;
const TRADE_CREATOR: usize = 1;
const TRADE_RECIPIENTS: usize = 2;
const TRADE_GIVE: usize = 3;
const TRADE_RECEIVE: usize = 8;
const TRADE_ACCEPTED: usize = 13;
const TRADE_REJECTED: usize = 14;
const STATE_TRADE: usize = 23;
const STATE_LAST_REJECTED_TRADE: usize = STATE_TRADE + TRADE_STRIDE;
const STATE_BANK: usize = STATE_LAST_REJECTED_TRADE + TRADE_STRIDE;
const STATE_DEVELOPMENT_DECK: usize = STATE_BANK + 5;
const STATE_PLAYED_DEVELOPMENT: usize = STATE_DEVELOPMENT_DECK + 5;
const STATE_DISCARD_REMAINING: usize = STATE_PLAYED_DEVELOPMENT + 5;
const STATE_HEX_RESOURCES: usize = STATE_DISCARD_REMAINING + MAX_PLAYERS;
const STATE_HEX_NUMBERS: usize = STATE_HEX_RESOURCES + HEX_COUNT;
const STATE_PORTS: usize = STATE_HEX_NUMBERS + HEX_COUNT;
const STATE_BUILDINGS: usize = STATE_PORTS + VERTEX_COUNT;
const STATE_ROADS: usize = STATE_BUILDINGS + VERTEX_COUNT;
const STATE_PLAYERS: usize = STATE_ROADS + EDGE_COUNT;
const PLAYER_STRIDE: usize = 28;
const STATE_DOMESTIC_TRADE_DISABLED: usize = STATE_PLAYERS + MAX_PLAYERS * PLAYER_STRIDE;
const STATE_DOMESTIC_TRADE_EMBARGOES: usize = STATE_DOMESTIC_TRADE_DISABLED + 1;
const STATE_DICE_MODEL: usize = STATE_DOMESTIC_TRADE_EMBARGOES + 1;
const STATE_DICE_COUNT: usize = STATE_DICE_MODEL + 1;
const STATE_DICE_PARTICLES: usize = STATE_DICE_COUNT + 1;
const DICE_PARTICLE_WORDS: usize = 28;
const MAX_DICE_PARTICLES: usize = colonist_catan_core::REFERENCE_PARTICLES;
const STATE_WORDS: usize = STATE_DICE_PARTICLES + MAX_DICE_PARTICLES * DICE_PARTICLE_WORDS;

const ACTION_TAG: usize = 0;
const ACTION_ARG0: usize = 1;
const ACTION_WORDS: usize = 12;
const SUMMARY_WORDS: usize = 7;
const ROOT_STATS_WORDS: usize = 12;
const MATCHUP_PROFILE_WORDS: usize = 10;

pub type CudaSimPolicyProfile = [u8; 5];

#[inline]
fn mix_stream_seed(base_seed: u64, global_index: u64, domain: u64) -> u64 {
    let mut value = base_seed ^ domain ^ global_index.wrapping_mul(SEED_INDEX_MIX);
    value = (value ^ (value >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    value = (value ^ (value >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    value ^ (value >> 31)
}

pub fn cuda_sim_board_seed(base_seed: u64, global_game_index: u64) -> u64 {
    mix_stream_seed(base_seed, global_game_index, BOARD_RNG_DOMAIN)
}

#[inline]
fn cuda_sim_game_rng_seed(base_seed: u64, global_game_index: u64) -> u64 {
    mix_stream_seed(base_seed, global_game_index, GAME_RNG_DOMAIN)
}

#[inline]
fn cuda_sim_game_chance_rng_seed(base_seed: u64, global_game_index: u64) -> u64 {
    mix_stream_seed(base_seed, global_game_index, GAME_CHANCE_RNG_DOMAIN)
}

const TOPO_VERTEX_HEX_COUNTS: usize = 0;
const TOPO_VERTEX_HEXES: usize = TOPO_VERTEX_HEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_VERTEX_COUNTS: usize = TOPO_VERTEX_HEXES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_VERTICES: usize = TOPO_VERTEX_VERTEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_EDGE_COUNTS: usize = TOPO_VERTEX_VERTICES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_EDGES: usize = TOPO_VERTEX_EDGE_COUNTS + VERTEX_COUNT;
const TOPO_EDGE_VERTICES: usize = TOPO_VERTEX_EDGES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPOLOGY_WORDS: usize = TOPO_EDGE_VERTICES + EDGE_COUNT * 2;

const ACTION_PLACE_SETTLEMENT: u32 = 0;
const ACTION_PLACE_ROAD: u32 = 1;
const ACTION_ROLL: u32 = 2;
const ACTION_RESOLVE_ROLL: u32 = 3;
const ACTION_DISCARD: u32 = 4;
const ACTION_MOVE_ROBBER: u32 = 5;
const ACTION_RESOLVE_STEAL: u32 = 6;
const ACTION_BUILD_ROAD: u32 = 7;
const ACTION_BUILD_SETTLEMENT: u32 = 8;
const ACTION_BUILD_CITY: u32 = 9;
const ACTION_BUY_DEVELOPMENT: u32 = 10;
const ACTION_RESOLVE_DEVELOPMENT: u32 = 11;
const ACTION_PLAY_KNIGHT: u32 = 12;
const ACTION_PLAY_ROAD_BUILDING: u32 = 13;
const ACTION_PLAY_YEAR_OF_PLENTY: u32 = 14;
const ACTION_PLAY_MONOPOLY: u32 = 15;
const ACTION_MARITIME_TRADE: u32 = 16;
const ACTION_END_TURN: u32 = 17;
const ACTION_OFFER_TRADE: u32 = 18;
const ACTION_RESPOND_TRADE: u32 = 19;
const ACTION_COUNTER_TRADE: u32 = 20;
const ACTION_CONFIRM_TRADE: u32 = 21;
const ACTION_CANCEL_TRADE: u32 = 22;

const CUDA_PTX: &str = include_str!("cuda/sim.ptx");
const BACKEND_NAME: &str = "cuda-resident-sim";

#[derive(Clone, Debug)]
pub struct CudaSimDeviceIdentity {
    pub backend: &'static str,
    pub ordinal: usize,
    pub name: String,
    pub compute_capability: (i32, i32),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CudaSimGameSummary {
    pub terminal: bool,
    pub winner: Option<u8>,
    pub turn: u32,
    pub victory_points: [u8; 4],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CudaSimArenaConfig {
    pub max_actions: u32,
    pub max_turns: u32,
}

impl Default for CudaSimArenaConfig {
    fn default() -> Self {
        Self {
            max_actions: 20_000,
            max_turns: 600,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CudaSimAgentSearchConfig {
    pub root_samples: usize,
    pub rollouts_per_action: usize,
    pub rollout_steps: usize,
}

impl Default for CudaSimAgentSearchConfig {
    fn default() -> Self {
        Self {
            // Campaign-strength default: enough independent root evidence to
            // materially outperform the weighted policy while keeping the
            // searched arena in the high-throughput regime. Deeper 8/32/64
            // settings remain available explicitly for oracle/calibration runs.
            root_samples: 4,
            rollouts_per_action: 16,
            rollout_steps: 32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CudaSimSearchedArenaResult {
    pub arena: CudaSimArenaResult,
    pub candidate_decisions: u64,
    pub root_actions_evaluated: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CudaSimArenaGameSummary {
    pub game: CudaSimGameSummary,
    pub actions: u32,
    pub truncated: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CudaSimArenaResult {
    pub games: Vec<CudaSimArenaGameSummary>,
    pub wins: [u64; 4],
    pub terminal_games: u64,
    pub truncated_games: u64,
    pub total_actions: u64,
}

#[derive(Clone, Debug)]
pub struct CudaSimRootActionStats {
    pub action: Action,
    pub samples: u32,
    pub errors: u32,
    pub terminal_samples: u32,
    pub wins: u32,
    pub mean_turn: f32,
    pub mean_victory_points: f32,
    pub mean_best_opponent_victory_points: f32,
    pub mean_victory_margin_squared: f32,
    pub mean_victory_points_squared: f32,
    pub mean_best_opponent_victory_points_squared: f32,
    pub mean_strategic_margin: f32,
    pub mean_strategic_margin_squared: f32,
}

impl CudaSimRootActionStats {
    pub fn terminal_rate(&self) -> f32 {
        self.terminal_samples as f32 / self.samples.max(1) as f32
    }

    pub fn win_rate(&self) -> f32 {
        self.wins as f32 / self.terminal_samples.max(1) as f32
    }

    pub fn mean_victory_margin(&self) -> f32 {
        self.mean_victory_points - self.mean_best_opponent_victory_points
    }

    pub fn victory_margin_variance(&self) -> f32 {
        (self.mean_victory_margin_squared - self.mean_victory_margin().powi(2)).max(0.0)
    }

    pub fn victory_points_variance(&self) -> f32 {
        (self.mean_victory_points_squared - self.mean_victory_points.powi(2)).max(0.0)
    }

    pub fn best_opponent_victory_points_variance(&self) -> f32 {
        (self.mean_best_opponent_victory_points_squared
            - self.mean_best_opponent_victory_points.powi(2))
        .max(0.0)
    }

    pub fn strategic_margin_variance(&self) -> f32 {
        (self.mean_strategic_margin_squared - self.mean_strategic_margin.powi(2)).max(0.0)
    }

    pub fn net_terminal_variance(&self) -> f32 {
        (self.terminal_rate() - self.net_terminal_outcome().powi(2)).max(0.0)
    }

    /// Terminal wins count +1, terminal losses -1, and unfinished samples 0.
    /// Keep this separate from VP margin so callers can inspect the evidence
    /// instead of relying on one opaque weighted score.
    pub fn net_terminal_outcome(&self) -> f32 {
        let losses = self.terminal_samples.saturating_sub(self.wins);
        (self.wins as f32 - losses as f32) / self.samples.max(1) as f32
    }
}

#[derive(Clone, Debug)]
pub struct CudaSimRootSearchResult {
    pub rows: Vec<Vec<CudaSimRootActionStats>>,
    pub rollouts_per_action: usize,
    pub rollout_steps: usize,
}

impl CudaSimRootSearchResult {
    /// Picks one root per resident base state using an interpretable ordering:
    /// terminal outcome first, then the parity-tested strategic cutoff, raw VP
    /// margin, and finally shorter mean game length.
    pub fn best_actions(&self) -> Vec<Option<&CudaSimRootActionStats>> {
        self.rows
            .iter()
            .map(|row| {
                row.iter().max_by(|left, right| {
                    left.net_terminal_outcome()
                        .total_cmp(&right.net_terminal_outcome())
                        .then_with(|| {
                            left.mean_strategic_margin
                                .total_cmp(&right.mean_strategic_margin)
                        })
                        .then_with(|| {
                            left.mean_victory_margin()
                                .total_cmp(&right.mean_victory_margin())
                        })
                        .then_with(|| right.mean_turn.total_cmp(&left.mean_turn))
                })
            })
            .collect()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CudaSimPackedState {
    words: [u32; STATE_WORDS],
}

impl CudaSimPackedState {
    pub fn new(state: &GameState) -> Result<Self, CudaSimError> {
        let mut words = [0u32; STATE_WORDS];
        pack_state_words(state, &mut words)?;
        Ok(Self { words })
    }

    pub fn words(&self) -> &[u32; STATE_WORDS] {
        &self.words
    }

    pub fn summary(&self) -> CudaSimGameSummary {
        let players = self.words[STATE_NUM_PLAYERS] as usize;
        let terminal = self.words[STATE_PHASE] == 10;
        let target = self.words[STATE_VICTORY_TARGET];
        let mut victory_points = [0u8; 4];
        let mut winner = None;
        for (player, value) in victory_points.iter_mut().enumerate().take(players) {
            let base = STATE_PLAYERS + player * PLAYER_STRIDE;
            let vp = self.words[base + 15] + self.words[base + 5 + 1];
            *value = vp.min(u8::MAX as u32) as u8;
            if terminal && winner.is_none() && vp >= target {
                winner = Some(player as u8);
            }
        }
        CudaSimGameSummary {
            terminal,
            winner,
            turn: self.words[STATE_TURN],
            victory_points,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CudaSimTransitionStatus {
    Ok,
    UnsupportedAction,
    InvalidPhase,
    InvalidAction,
    InvalidState,
    Unknown(u32),
}

impl CudaSimTransitionStatus {
    fn from_code(code: u32) -> Self {
        match code {
            0 => Self::Ok,
            1 => Self::UnsupportedAction,
            2 => Self::InvalidPhase,
            3 => Self::InvalidAction,
            4 => Self::InvalidState,
            other => Self::Unknown(other),
        }
    }
}

#[derive(Debug)]
pub enum CudaSimError {
    UnsupportedState(&'static str),
    TopologyMismatch,
    BatchTooLarge,
    NoResidentBatch,
    ActionCountMismatch { states: usize, actions: usize },
    RootBatchMismatch { states: usize, rows: usize },
    InvalidRolloutCount,
    Cancelled,
    InvalidArenaChunk,
    UnsupportedAction,
    TransitionFailed {
        index: usize,
        status: CudaSimTransitionStatus,
    },
    Cuda(DriverError),
}

impl fmt::Display for CudaSimError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnsupportedState(reason) => write!(formatter, "unsupported CUDA simulation state: {reason}"),
            Self::TopologyMismatch => formatter.write_str(
                "CUDA simulation requires the standard 19-hex, 54-vertex, 72-edge topology",
            ),
            Self::BatchTooLarge => formatter.write_str("CUDA simulation batch is too large"),
            Self::NoResidentBatch => formatter.write_str("CUDA simulation has no resident state batch"),
            Self::ActionCountMismatch { states, actions } => write!(
                formatter,
                "CUDA simulation action batch has {actions} actions for {states} resident states"
            ),
            Self::RootBatchMismatch { states, rows } => write!(
                formatter,
                "CUDA simulation root batch has {rows} action rows for {states} resident states"
            ),
            Self::InvalidRolloutCount => formatter.write_str(
                "CUDA simulation root search requires nonzero root samples and rollouts",
            ),
            Self::Cancelled => formatter.write_str("CUDA simulation root search was cancelled"),
            Self::InvalidArenaChunk => formatter.write_str(
                "CUDA simulation arena campaign requires a nonzero resident chunk size",
            ),
            Self::UnsupportedAction => formatter.write_str(
                "action is not implemented by the GPU-resident transition kernel",
            ),
            Self::TransitionFailed { index, status } => {
                write!(formatter, "CUDA transition {index} failed with {status:?}")
            }
            Self::Cuda(error) => write!(formatter, "CUDA driver error: {error}"),
        }
    }
}

impl std::error::Error for CudaSimError {}

impl From<DriverError> for CudaSimError {
    fn from(error: DriverError) -> Self {
        Self::Cuda(error)
    }
}

/// Persistent GPU-resident batch of standard 2-4 player game states.
///
/// The state buffer is field-major (SoA): all lanes' `phase` words are
/// contiguous, then all lanes' `current_player` words, and so on. This is the
/// layout the future GPU legal-action/search kernels will consume directly;
/// CPU packing happens only when a batch is initially uploaded or explicitly
/// inspected for parity.
pub struct CudaSimEngine {
    _context: Arc<CudaContext>,
    _module: Arc<CudaModule>,
    stream: Arc<cudarc::driver::CudaStream>,
    transition_kernel: CudaFunction,
    rollout_action_kernel: CudaFunction,
    rollout_steps_kernel: CudaFunction,
    root_rollout_turns_kernel: CudaFunction,
    arena_kernel: CudaFunction,
    run_until_candidate_kernel: CudaFunction,
    sample_candidate_roots_kernel: CudaFunction,
    select_candidate_roots_kernel: CudaFunction,
    apply_candidate_kernel: CudaFunction,
    profile_kernel: CudaFunction,
    summary_kernel: CudaFunction,
    dice_distribution_kernel: CudaFunction,
    expand_roots_kernel: CudaFunction,
    reduce_roots_kernel: CudaFunction,
    topology_device: CudaSlice<u32>,
    topology_host: Vec<u32>,
    state_device: CudaSlice<u32>,
    action_device: CudaSlice<u32>,
    status_device: CudaSlice<u32>,
    rng_device: CudaSlice<u64>,
    chance_rng_device: CudaSlice<u64>,
    summary_device: CudaSlice<u32>,
    arena_action_count_device: CudaSlice<u32>,
    candidate_ready_device: CudaSlice<u32>,
    matchup_profile_device: CudaSlice<u32>,
    search_state_device: CudaSlice<u32>,
    search_action_device: CudaSlice<u32>,
    search_status_device: CudaSlice<u32>,
    search_rng_device: CudaSlice<u64>,
    search_chance_rng_device: CudaSlice<u64>,
    root_action_device: CudaSlice<u32>,
    root_base_index_device: CudaSlice<u32>,
    root_seed_key_device: CudaSlice<u64>,
    root_stats_device: CudaSlice<u64>,
    state_host: Vec<u32>,
    action_host: Vec<u32>,
    status_host: Vec<u32>,
    rng_host: Vec<u64>,
    chance_rng_host: Vec<u64>,
    summary_host: Vec<u32>,
    arena_action_count_host: Vec<u32>,
    candidate_ready_host: Vec<u32>,
    matchup_profile_host: Vec<u32>,
    root_action_host: Vec<u32>,
    root_base_index_host: Vec<u32>,
    root_seed_key_host: Vec<u64>,
    root_stats_host: Vec<u64>,
    capacity: usize,
    search_capacity: usize,
    root_capacity: usize,
    resident_states: usize,
    identity: CudaSimDeviceIdentity,
}

impl CudaSimEngine {
    pub fn new() -> Result<Self, CudaSimError> {
        Self::new_on_device(0)
    }

    pub fn new_on_device(ordinal: usize) -> Result<Self, CudaSimError> {
        // Tile labels are irrelevant here; this board supplies only the immutable
        // base topology. Keep legacy V1 explicit until topology has its own type.
        let topology_board = Board::randomized_base_v1(0, 4);
        let topology_host = topology_words(&topology_board)?;
        let context = CudaContext::new(ordinal)?;
        let module = context.load_module(Ptx::from_src(CUDA_PTX))?;
        let transition_kernel = module.load_function("apply_transition_batch_kernel")?;
        let rollout_action_kernel = module.load_function("generate_rollout_actions_batch_kernel")?;
        let rollout_steps_kernel = module.load_function("run_rollout_steps_kernel")?;
        let root_rollout_turns_kernel = module.load_function("run_root_rollout_turns_kernel")?;
        let arena_kernel = module.load_function("run_games_kernel")?;
        let run_until_candidate_kernel = module.load_function("run_until_candidate_kernel")?;
        let sample_candidate_roots_kernel =
            module.load_function("sample_candidate_root_actions_kernel")?;
        let select_candidate_roots_kernel =
            module.load_function("select_candidate_root_actions_kernel")?;
        let apply_candidate_kernel = module.load_function("apply_candidate_actions_kernel")?;
        let profile_kernel = module.load_function("assign_rotating_profiles_kernel")?;
        let summary_kernel = module.load_function("summarize_games_kernel")?;
        let dice_distribution_kernel = module.load_function("dice_distribution_kernel")?;
        let expand_roots_kernel = module.load_function("expand_root_rollouts_kernel")?;
        let reduce_roots_kernel = module.load_function("reduce_root_rollouts_kernel")?;
        let stream = context.default_stream();
        let contract_kernel = module.load_function("simulation_contract_kernel")?;
        let mut contract_device = stream.alloc_zeros::<u32>(4)?;
        let mut arguments = stream.launch_builder(&contract_kernel);
        arguments.arg(&mut contract_device);
        unsafe { arguments.launch(LaunchConfig::for_num_elems(1))? };
        let contract = stream.clone_dtoh(&contract_device)?;
        stream.synchronize()?;
        if contract != [3, STATE_WORDS as u32, ACTION_WORDS as u32, ROOT_STATS_WORDS as u32] {
            return Err(CudaSimError::UnsupportedState("embedded CUDA artifact ABI does not match Rust; rebuild sim.ptx"));
        }
        let topology_device = stream.clone_htod(&topology_host)?;
        let state_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * STATE_WORDS)?;
        let action_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * ACTION_WORDS)?;
        let status_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let rng_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let chance_rng_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let summary_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * SUMMARY_WORDS)?;
        let arena_action_count_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let candidate_ready_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let matchup_profile_device = stream.alloc_zeros(MATCHUP_PROFILE_WORDS)?;
        let search_state_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * STATE_WORDS)?;
        let search_action_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * ACTION_WORDS)?;
        let search_status_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let search_rng_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let search_chance_rng_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let root_action_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * ACTION_WORDS)?;
        let root_base_index_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let root_seed_key_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY)?;
        let root_stats_device = stream.alloc_zeros(INITIAL_BATCH_CAPACITY * ROOT_STATS_WORDS)?;
        let identity = CudaSimDeviceIdentity {
            backend: BACKEND_NAME,
            ordinal: context.ordinal(),
            name: context.name()?,
            compute_capability: context.compute_capability()?,
        };
        Ok(Self {
            _context: context,
            _module: module,
            stream,
            transition_kernel,
            rollout_action_kernel,
            rollout_steps_kernel,
            root_rollout_turns_kernel,
            arena_kernel,
            run_until_candidate_kernel,
            sample_candidate_roots_kernel,
            select_candidate_roots_kernel,
            apply_candidate_kernel,
            profile_kernel,
            summary_kernel,
            dice_distribution_kernel,
            expand_roots_kernel,
            reduce_roots_kernel,
            topology_device,
            topology_host,
            state_device,
            action_device,
            status_device,
            rng_device,
            chance_rng_device,
            summary_device,
            arena_action_count_device,
            candidate_ready_device,
            matchup_profile_device,
            search_state_device,
            search_action_device,
            search_status_device,
            search_rng_device,
            search_chance_rng_device,
            root_action_device,
            root_base_index_device,
            root_seed_key_device,
            root_stats_device,
            state_host: Vec::new(),
            action_host: Vec::new(),
            status_host: Vec::new(),
            rng_host: Vec::new(),
            chance_rng_host: Vec::new(),
            summary_host: Vec::new(),
            arena_action_count_host: Vec::new(),
            candidate_ready_host: Vec::new(),
            matchup_profile_host: Vec::new(),
            root_action_host: Vec::new(),
            root_base_index_host: Vec::new(),
            root_seed_key_host: Vec::new(),
            root_stats_host: Vec::new(),
            capacity: INITIAL_BATCH_CAPACITY,
            search_capacity: INITIAL_BATCH_CAPACITY,
            root_capacity: INITIAL_BATCH_CAPACITY,
            resident_states: 0,
            identity,
        })
    }

    pub fn device_identity(&self) -> &CudaSimDeviceIdentity {
        &self.identity
    }

    pub fn resident_states(&self) -> usize {
        self.resident_states
    }

    /// Read the resident chance law for exact CPU/GPU semantic comparison.
    /// Non-roll phases return zeroes; M0 retains integer 2d6 weights.
    pub fn dice_distributions(&self) -> Result<Vec<[u64; 11]>, CudaSimError> {
        let count = self.resident_states;
        if count == 0 { return Err(CudaSimError::NoResidentBatch); }
        let stride = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let mut output = self.stream.alloc_zeros::<u64>(11 * count)?;
        let mut arguments = self.stream.launch_builder(&self.dice_distribution_kernel);
        arguments.arg(&self.state_device);
        arguments.arg(&stride);
        arguments.arg(&mut output);
        arguments.arg(&stride);
        // Mref uses more registers than the trivial contract probe. Match the
        // resident simulator's block size, not cudarc's 1024-thread default.
        let config = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        unsafe { arguments.launch(config)? };
        let words = self.stream.clone_dtoh(&output)?;
        Ok((0..count).map(|lane| std::array::from_fn(|i| words[i * count + lane])).collect())
    }

    pub fn upload_states(&mut self, states: &[GameState]) -> Result<(), CudaSimError> {
        if states.is_empty() {
            self.resident_states = 0;
            self.state_host.clear();
            return Ok(());
        }
        let topology = topology_words(states[0].board.as_ref())?;
        for state in states.iter().skip(1) {
            if topology_words(state.board.as_ref())? != topology {
                return Err(CudaSimError::TopologyMismatch);
            }
        }
        if topology != self.topology_host {
            self.stream.memcpy_htod(&topology, &mut self.topology_device)?;
            self.topology_host = topology;
        }
        self.ensure_capacity(states.len())?;
        self.state_host.clear();
        self.state_host.resize(STATE_WORDS * states.len(), 0);
        for (lane, state) in states.iter().enumerate() {
            let packed = CudaSimPackedState::new(state)?;
            for (field, value) in packed.words.iter().copied().enumerate() {
                self.state_host[field * states.len() + lane] = value;
            }
        }
        self.stream
            .memcpy_htod(&self.state_host, &mut self.state_device)?;
        self.stream.synchronize()?;
        self.resident_states = states.len();
        Ok(())
    }

    pub fn seed_rollout_rng(&mut self, base_seed: u64) -> Result<(), CudaSimError> {
        self.seed_rollout_rng_with_offset(base_seed, 0)
    }

    fn assign_rotating_profiles(
        &mut self,
        candidate: CudaSimPolicyProfile,
        baseline: CudaSimPolicyProfile,
        game_offset: usize,
    ) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        let game_offset = u64::try_from(game_offset).map_err(|_| CudaSimError::BatchTooLarge)?;
        self.matchup_profile_host.clear();
        self.matchup_profile_host
            .extend(candidate.into_iter().map(u32::from));
        self.matchup_profile_host
            .extend(baseline.into_iter().map(u32::from));
        self.stream.memcpy_htod(
            &self.matchup_profile_host,
            &mut self.matchup_profile_device,
        )?;

        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let launch = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.profile_kernel);
        arguments.arg(&mut self.state_device);
        arguments.arg(&self.matchup_profile_device);
        arguments.arg(&game_offset);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        unsafe { arguments.launch(launch)? };
        Ok(())
    }

    fn seed_rollout_rng_with_offset(
        &mut self,
        base_seed: u64,
        game_offset: usize,
    ) -> Result<(), CudaSimError> {
        self.seed_rollout_rng_with_blocks(base_seed, game_offset, 1)
    }

    fn seed_rollout_rng_with_blocks(
        &mut self,
        base_seed: u64,
        game_offset: usize,
        games_per_seed_block: usize,
    ) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        if games_per_seed_block == 0 {
            return Err(CudaSimError::InvalidArenaChunk);
        }
        self.rng_host.clear();
        self.rng_host.reserve(count);
        self.chance_rng_host.clear();
        self.chance_rng_host.reserve(count);
        for lane in 0..count {
            let global_lane = game_offset
                .checked_add(lane)
                .ok_or(CudaSimError::BatchTooLarge)?;
            let seed_block = global_lane / games_per_seed_block;
            self.rng_host
                .push(cuda_sim_game_rng_seed(base_seed, global_lane as u64));
            self.chance_rng_host
                .push(cuda_sim_game_chance_rng_seed(base_seed, seed_block as u64));
        }
        self.stream
            .memcpy_htod(&self.rng_host, &mut self.rng_device)?;
        self.stream
            .memcpy_htod(&self.chance_rng_host, &mut self.chance_rng_device)?;
        self.stream.synchronize()?;
        Ok(())
    }

    pub fn generate_rollout_actions(&mut self) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.enqueue_rollout_actions(count)?;
        self.stream.synchronize()?;
        Ok(())
    }

    /// Advances every resident lane for `steps` actions without transferring
    /// state or generated actions to the host between simulated actions.
    pub fn run_rollout_steps(&mut self, steps: usize) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.clear_transition_status(count)?;
        if steps == 0 {
            return Ok(());
        }
        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let steps_u32 = u32::try_from(steps).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let config = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.rollout_steps_kernel);
        arguments.arg(&mut self.state_device);
        arguments.arg(&self.topology_device);
        arguments.arg(&mut self.action_device);
        arguments.arg(&mut self.status_device);
        arguments.arg(&mut self.rng_device);
        arguments.arg(&mut self.chance_rng_device);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        arguments.arg(&steps_u32);
        unsafe { arguments.launch(config)? };
        self.stream.synchronize()?;
        self.check_transition_status(count)
    }

    /// Runs the currently resident states as a whole-game GPU arena. All action
    /// selection, chance resolution, and transitions stay on device until the
    /// terminal/turn/action bound is reached; only compact summaries return.
    pub fn run_arena_games(
        &mut self,
        config: CudaSimArenaConfig,
        seed: u64,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        self.seed_rollout_rng_with_offset(seed, 0)?;
        self.run_arena_games_seeded(config)
    }

    fn run_arena_games_seeded(
        &mut self,
        config: CudaSimArenaConfig,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.clear_transition_status(count)?;
        self.arena_action_count_host.clear();
        self.arena_action_count_host.resize(count, 0);
        self.stream.memcpy_htod(
            &self.arena_action_count_host,
            &mut self.arena_action_count_device,
        )?;

        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let launch = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.arena_kernel);
        arguments.arg(&mut self.state_device);
        arguments.arg(&self.topology_device);
        arguments.arg(&mut self.action_device);
        arguments.arg(&mut self.status_device);
        arguments.arg(&mut self.rng_device);
        arguments.arg(&mut self.chance_rng_device);
        arguments.arg(&mut self.arena_action_count_device);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        arguments.arg(&config.max_actions);
        arguments.arg(&config.max_turns);
        unsafe { arguments.launch(launch)? };
        self.stream.synchronize()?;
        self.check_transition_status(count)?;

        let summaries = self.download_game_summaries()?;
        self.arena_action_count_host.resize(count, 0);
        let action_counts = self.arena_action_count_device.slice(0..count);
        self.stream
            .memcpy_dtoh(&action_counts, &mut self.arena_action_count_host)?;
        self.stream.synchronize()?;

        let mut wins = [0u64; 4];
        let mut terminal_games = 0u64;
        let mut truncated_games = 0u64;
        let mut total_actions = 0u64;
        let mut games = Vec::with_capacity(count);
        for (game, actions) in summaries
            .into_iter()
            .zip(self.arena_action_count_host.iter().copied())
        {
            if let Some(winner) = game.winner
                && let Some(count) = wins.get_mut(winner as usize)
            {
                *count = count.saturating_add(1);
            }
            if game.terminal {
                terminal_games = terminal_games.saturating_add(1);
            }
            let truncated = !game.terminal
                && (game.turn >= config.max_turns || actions >= config.max_actions);
            if truncated {
                truncated_games = truncated_games.saturating_add(1);
            }
            total_actions = total_actions.saturating_add(actions as u64);
            games.push(CudaSimArenaGameSummary {
                game,
                actions,
                truncated,
            });
        }
        Ok(CudaSimArenaResult {
            games,
            wins,
            terminal_games,
            truncated_games,
            total_actions,
        })
    }

    /// Runs an existing host corpus through a bounded resident GPU working set.
    /// RNG identity uses the global game index, so changing `chunk_games` does
    /// not change the simulated trajectories. The input corpus and returned
    /// per-game summaries remain host-resident and therefore O(total games).
    pub fn run_arena_campaign(
        &mut self,
        states: &[GameState],
        config: CudaSimArenaConfig,
        seed: u64,
        chunk_games: usize,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        self.run_arena_campaign_inner(states, config, seed, chunk_games, None)
    }

    /// Runs a matched candidate-vs-baseline campaign while rotating the
    /// candidate seat by global game index. Profiles are assigned on device so
    /// large parameter sweeps can reuse the same CPU base-state corpus.
    pub fn run_rotating_profile_campaign(
        &mut self,
        states: &[GameState],
        candidate: CudaSimPolicyProfile,
        baseline: CudaSimPolicyProfile,
        config: CudaSimArenaConfig,
        seed: u64,
        chunk_games: usize,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        self.run_arena_campaign_inner(
            states,
            config,
            seed,
            chunk_games,
            Some((candidate, baseline)),
        )
    }

    /// Runs one host-generated campaign chunk while preserving global game
    /// identity for seat rotation and RNG derivation. This is the streaming
    /// primitive used by large strength sweeps so host memory stays O(chunk).
    pub fn run_rotating_profile_chunk(
        &mut self,
        states: &[GameState],
        candidate: CudaSimPolicyProfile,
        baseline: CudaSimPolicyProfile,
        config: CudaSimArenaConfig,
        seed: u64,
        game_offset: usize,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        self.run_rotating_profile_chunk_with_seed_blocks(
            states,
            candidate,
            baseline,
            config,
            seed,
            game_offset,
            1,
        )
    }

    /// Runs one host-generated campaign chunk while reusing the same chance
    /// seed for each consecutive `games_per_seed_block` games. Strength
    /// campaigns use one block per seat rotation so board/chance luck is paired
    /// across candidate seats without changing chunk-independent game identity.
    #[allow(clippy::too_many_arguments)]
    pub fn run_rotating_profile_chunk_with_seed_blocks(
        &mut self,
        states: &[GameState],
        candidate: CudaSimPolicyProfile,
        baseline: CudaSimPolicyProfile,
        config: CudaSimArenaConfig,
        seed: u64,
        game_offset: usize,
        games_per_seed_block: usize,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        if states.is_empty() {
            return Ok(CudaSimArenaResult {
                games: Vec::new(),
                wins: [0; 4],
                terminal_games: 0,
                truncated_games: 0,
                total_actions: 0,
            });
        }
        self.upload_states(states)?;
        self.assign_rotating_profiles(candidate, baseline, game_offset)?;
        self.seed_rollout_rng_with_blocks(seed, game_offset, games_per_seed_block)?;
        self.run_arena_games_seeded(config)
    }

    /// Runs one matched chunk with the rotating candidate using GPU root-rollout
    /// search at every candidate decision while all opponent decisions and
    /// chance transitions use the resident GPU weighted policy. Root proposals,
    /// rollouts, reduction, selection, and application remain on device.
    #[allow(clippy::too_many_arguments)]
    pub fn run_searched_candidate_chunk_with_seed_blocks(
        &mut self,
        states: &[GameState],
        candidate: CudaSimPolicyProfile,
        baseline: CudaSimPolicyProfile,
        arena_config: CudaSimArenaConfig,
        search_config: CudaSimAgentSearchConfig,
        game_seed: u64,
        search_seed: u64,
        game_offset: usize,
        games_per_seed_block: usize,
    ) -> Result<CudaSimSearchedArenaResult, CudaSimError> {
        if states.is_empty() {
            return Ok(CudaSimSearchedArenaResult {
                arena: CudaSimArenaResult {
                    games: Vec::new(),
                    wins: [0; 4],
                    terminal_games: 0,
                    truncated_games: 0,
                    total_actions: 0,
                },
                candidate_decisions: 0,
                root_actions_evaluated: 0,
            });
        }
        if search_config.root_samples == 0
            || search_config.rollouts_per_action == 0
            || search_config.rollout_steps == 0
        {
            return Err(CudaSimError::InvalidRolloutCount);
        }
        if games_per_seed_block == 0 {
            return Err(CudaSimError::InvalidArenaChunk);
        }

        self.upload_states(states)?;
        self.assign_rotating_profiles(candidate, baseline, game_offset)?;
        self.seed_rollout_rng_with_blocks(game_seed, game_offset, games_per_seed_block)?;

        let count = self.resident_states;
        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let game_offset_u64 =
            u64::try_from(game_offset).map_err(|_| CudaSimError::BatchTooLarge)?;
        let game_launch = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };

        self.clear_transition_status(count)?;
        self.arena_action_count_host.clear();
        self.arena_action_count_host.resize(count, 0);
        self.stream.memcpy_htod(
            &self.arena_action_count_host,
            &mut self.arena_action_count_device,
        )?;
        self.candidate_ready_host.clear();
        self.candidate_ready_host.resize(count, 0);
        self.stream.memcpy_htod(
            &self.candidate_ready_host,
            &mut self.candidate_ready_device,
        )?;

        let roots_per_game = search_config.root_samples;
        let roots_per_game_u32 =
            u32::try_from(roots_per_game).map_err(|_| CudaSimError::BatchTooLarge)?;
        let root_count = count
            .checked_mul(roots_per_game)
            .ok_or(CudaSimError::BatchTooLarge)?;
        let root_count_u32 =
            u32::try_from(root_count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let rollouts_per_action = search_config.rollouts_per_action;
        let rollouts_u32 =
            u32::try_from(rollouts_per_action).map_err(|_| CudaSimError::BatchTooLarge)?;
        let rollout_steps_u32 =
            u32::try_from(search_config.rollout_steps).map_err(|_| CudaSimError::BatchTooLarge)?;
        let chunk_rollouts_cap = (SEARCH_CHUNK_LANES / root_count).max(1);
        let max_chunk_lanes = root_count
            .checked_mul(chunk_rollouts_cap.min(rollouts_per_action))
            .ok_or(CudaSimError::BatchTooLarge)?;
        self.ensure_root_capacity(root_count)?;
        self.ensure_search_capacity(root_count.max(max_chunk_lanes))?;

        let mut decision_round = 0u64;
        let mut candidate_decisions = 0u64;
        let mut root_actions_evaluated = 0u64;
        loop {
            {
                let mut arguments = self.stream.launch_builder(&self.run_until_candidate_kernel);
                arguments.arg(&mut self.state_device);
                arguments.arg(&self.topology_device);
                arguments.arg(&mut self.action_device);
                arguments.arg(&mut self.status_device);
                arguments.arg(&mut self.rng_device);
                arguments.arg(&mut self.chance_rng_device);
                arguments.arg(&mut self.arena_action_count_device);
                arguments.arg(&mut self.candidate_ready_device);
                arguments.arg(&game_offset_u64);
                arguments.arg(&stride);
                arguments.arg(&count_u32);
                arguments.arg(&arena_config.max_actions);
                arguments.arg(&arena_config.max_turns);
                unsafe { arguments.launch(game_launch)? };
            }
            self.stream.synchronize()?;
            self.check_transition_status(count)?;

            let ready = self.candidate_ready_device.slice(0..count);
            self.stream
                .memcpy_dtoh(&ready, &mut self.candidate_ready_host)?;
            self.stream.synchronize()?;
            let ready_count = self
                .candidate_ready_host
                .iter()
                .filter(|value| **value != 0)
                .count();
            if ready_count == 0 {
                break;
            }
            candidate_decisions = candidate_decisions.saturating_add(ready_count as u64);
            root_actions_evaluated = root_actions_evaluated.saturating_add(
                (ready_count as u64).saturating_mul(roots_per_game as u64),
            );

            let decision_seed = mix_stream_seed(
                search_seed,
                decision_round,
                CANDIDATE_DECISION_DOMAIN,
            );
            let root_launch = LaunchConfig {
                grid_dim: (root_count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
                block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
                shared_mem_bytes: 0,
            };
            {
                let mut arguments = self
                    .stream
                    .launch_builder(&self.sample_candidate_roots_kernel);
                arguments.arg(&self.state_device);
                arguments.arg(&stride);
                arguments.arg(&self.topology_device);
                arguments.arg(&self.candidate_ready_device);
                arguments.arg(&mut self.search_state_device);
                arguments.arg(&mut self.root_action_device);
                arguments.arg(&mut self.search_rng_device);
                arguments.arg(&mut self.search_chance_rng_device);
                arguments.arg(&mut self.root_base_index_device);
                arguments.arg(&mut self.root_seed_key_device);
                arguments.arg(&game_offset_u64);
                arguments.arg(&decision_seed);
                arguments.arg(&count_u32);
                arguments.arg(&roots_per_game_u32);
                arguments.arg(&root_count_u32);
                unsafe { arguments.launch(root_launch)? };
            }

            self.root_stats_host.clear();
            self.root_stats_host.resize(ROOT_STATS_WORDS * root_count, 0);
            self.stream
                .memcpy_htod(&self.root_stats_host, &mut self.root_stats_device)?;

            let mut rollout_offset = 0usize;
            while rollout_offset < rollouts_per_action {
                let chunk_rollouts =
                    (rollouts_per_action - rollout_offset).min(chunk_rollouts_cap);
                let lane_count = root_count
                    .checked_mul(chunk_rollouts)
                    .ok_or(CudaSimError::BatchTooLarge)?;
                let lane_count_u32 =
                    u32::try_from(lane_count).map_err(|_| CudaSimError::BatchTooLarge)?;
                let chunk_rollouts_u32 =
                    u32::try_from(chunk_rollouts).map_err(|_| CudaSimError::BatchTooLarge)?;
                let rollout_offset_u32 =
                    u32::try_from(rollout_offset).map_err(|_| CudaSimError::BatchTooLarge)?;
                let rollout_stride = lane_count_u32;
                let rollout_launch = LaunchConfig {
                    grid_dim: (lane_count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
                    block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
                    shared_mem_bytes: 0,
                };
                {
                    let mut arguments = self.stream.launch_builder(&self.expand_roots_kernel);
                    arguments.arg(&self.state_device);
                    arguments.arg(&stride);
                    arguments.arg(&self.topology_device);
                    arguments.arg(&self.root_action_device);
                    arguments.arg(&self.root_base_index_device);
                    arguments.arg(&self.root_seed_key_device);
                    arguments.arg(&root_count_u32);
                    arguments.arg(&chunk_rollouts_u32);
                    arguments.arg(&rollouts_u32);
                    arguments.arg(&rollout_offset_u32);
                    arguments.arg(&mut self.search_state_device);
                    arguments.arg(&mut self.search_action_device);
                    arguments.arg(&mut self.search_status_device);
                    arguments.arg(&mut self.search_rng_device);
                    arguments.arg(&mut self.search_chance_rng_device);
                    arguments.arg(&rollout_stride);
                    arguments.arg(&decision_seed);
                    arguments.arg(&lane_count_u32);
                    unsafe { arguments.launch(rollout_launch)? };
                }
                {
                    let mut arguments = self.stream.launch_builder(&self.rollout_steps_kernel);
                    arguments.arg(&mut self.search_state_device);
                    arguments.arg(&self.topology_device);
                    arguments.arg(&mut self.search_action_device);
                    arguments.arg(&mut self.search_status_device);
                    arguments.arg(&mut self.search_rng_device);
                    arguments.arg(&mut self.search_chance_rng_device);
                    arguments.arg(&rollout_stride);
                    arguments.arg(&lane_count_u32);
                    arguments.arg(&rollout_steps_u32);
                    unsafe { arguments.launch(rollout_launch)? };
                }
                {
                    let mut arguments = self.stream.launch_builder(&self.reduce_roots_kernel);
                    arguments.arg(&self.search_state_device);
                    arguments.arg(&self.search_status_device);
                    arguments.arg(&self.state_device);
                    arguments.arg(&self.topology_device);
                    arguments.arg(&self.root_base_index_device);
                    arguments.arg(&mut self.root_stats_device);
                    arguments.arg(&rollout_stride);
                    arguments.arg(&stride);
                    arguments.arg(&root_count_u32);
                    arguments.arg(&chunk_rollouts_u32);
                    arguments.arg(&lane_count_u32);
                    unsafe { arguments.launch(rollout_launch)? };
                }
                rollout_offset += chunk_rollouts;
            }

            {
                let mut arguments = self
                    .stream
                    .launch_builder(&self.select_candidate_roots_kernel);
                arguments.arg(&self.root_action_device);
                arguments.arg(&self.root_stats_device);
                arguments.arg(&self.candidate_ready_device);
                arguments.arg(&mut self.action_device);
                arguments.arg(&mut self.status_device);
                arguments.arg(&stride);
                arguments.arg(&count_u32);
                arguments.arg(&roots_per_game_u32);
                arguments.arg(&root_count_u32);
                unsafe { arguments.launch(game_launch)? };
            }
            {
                let mut arguments = self.stream.launch_builder(&self.apply_candidate_kernel);
                arguments.arg(&mut self.state_device);
                arguments.arg(&self.topology_device);
                arguments.arg(&self.action_device);
                arguments.arg(&mut self.status_device);
                arguments.arg(&mut self.arena_action_count_device);
                arguments.arg(&self.candidate_ready_device);
                arguments.arg(&stride);
                arguments.arg(&count_u32);
                unsafe { arguments.launch(game_launch)? };
            }
            self.stream.synchronize()?;
            self.check_transition_status(count)?;
            decision_round = decision_round.wrapping_add(1);
        }

        let arena = self.collect_current_arena_result(arena_config)?;
        Ok(CudaSimSearchedArenaResult {
            arena,
            candidate_decisions,
            root_actions_evaluated,
        })
    }

    fn collect_current_arena_result(
        &mut self,
        config: CudaSimArenaConfig,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        let count = self.resident_states;
        let summaries = self.download_game_summaries()?;
        self.arena_action_count_host.resize(count, 0);
        let action_counts = self.arena_action_count_device.slice(0..count);
        self.stream
            .memcpy_dtoh(&action_counts, &mut self.arena_action_count_host)?;
        self.stream.synchronize()?;

        let mut wins = [0u64; 4];
        let mut terminal_games = 0u64;
        let mut truncated_games = 0u64;
        let mut total_actions = 0u64;
        let mut games = Vec::with_capacity(count);
        for (game, actions) in summaries
            .into_iter()
            .zip(self.arena_action_count_host.iter().copied())
        {
            if let Some(winner) = game.winner
                && let Some(count) = wins.get_mut(winner as usize)
            {
                *count = count.saturating_add(1);
            }
            if game.terminal {
                terminal_games = terminal_games.saturating_add(1);
            }
            let truncated = !game.terminal
                && (game.turn >= config.max_turns || actions >= config.max_actions);
            if truncated {
                truncated_games = truncated_games.saturating_add(1);
            }
            total_actions = total_actions.saturating_add(actions as u64);
            games.push(CudaSimArenaGameSummary {
                game,
                actions,
                truncated,
            });
        }
        Ok(CudaSimArenaResult {
            games,
            wins,
            terminal_games,
            truncated_games,
            total_actions,
        })
    }

    fn run_arena_campaign_inner(
        &mut self,
        states: &[GameState],
        config: CudaSimArenaConfig,
        seed: u64,
        chunk_games: usize,
        profiles: Option<(CudaSimPolicyProfile, CudaSimPolicyProfile)>,
    ) -> Result<CudaSimArenaResult, CudaSimError> {
        if chunk_games == 0 {
            return Err(CudaSimError::InvalidArenaChunk);
        }
        if states.is_empty() {
            return Ok(CudaSimArenaResult {
                games: Vec::new(),
                wins: [0; 4],
                terminal_games: 0,
                truncated_games: 0,
                total_actions: 0,
            });
        }

        let mut result = CudaSimArenaResult {
            games: Vec::with_capacity(states.len()),
            wins: [0; 4],
            terminal_games: 0,
            truncated_games: 0,
            total_actions: 0,
        };
        let mut game_offset = 0usize;
        for chunk in states.chunks(chunk_games) {
            self.upload_states(chunk)?;
            if let Some((candidate, baseline)) = profiles {
                self.assign_rotating_profiles(candidate, baseline, game_offset)?;
            }
            self.seed_rollout_rng_with_offset(seed, game_offset)?;
            let chunk_result = self.run_arena_games_seeded(config)?;
            for player in 0..4 {
                result.wins[player] = result.wins[player].saturating_add(chunk_result.wins[player]);
            }
            result.terminal_games = result
                .terminal_games
                .saturating_add(chunk_result.terminal_games);
            result.truncated_games = result
                .truncated_games
                .saturating_add(chunk_result.truncated_games);
            result.total_actions = result
                .total_actions
                .saturating_add(chunk_result.total_actions);
            result.games.extend(chunk_result.games);
            game_offset = game_offset
                .checked_add(chunk.len())
                .ok_or(CudaSimError::BatchTooLarge)?;
        }
        Ok(result)
    }

    fn enqueue_rollout_actions(&mut self, count: usize) -> Result<(), CudaSimError> {
        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let config = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.rollout_action_kernel);
        arguments.arg(&self.state_device);
        arguments.arg(&self.topology_device);
        arguments.arg(&mut self.action_device);
        arguments.arg(&mut self.rng_device);
        arguments.arg(&mut self.chance_rng_device);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        unsafe { arguments.launch(config)? };
        Ok(())
    }

    pub fn apply_generated_actions(&mut self) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.clear_transition_status(count)?;
        self.enqueue_transition(count)?;
        self.stream.synchronize()?;
        self.check_transition_status(count)
    }

    pub fn download_generated_actions(&mut self) -> Result<Vec<Action>, CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.action_host.resize(ACTION_WORDS * count, 0);
        let actions = self.action_device.slice(0..ACTION_WORDS * count);
        self.stream.memcpy_dtoh(&actions, &mut self.action_host)?;
        self.stream.synchronize()?;
        let mut result = Vec::with_capacity(count);
        for lane in 0..count {
            let mut words = [0u32; ACTION_WORDS];
            for (field, word) in words.iter_mut().enumerate() {
                *word = self.action_host[field * count + lane];
            }
            result.push(unpack_action_words(&words)?);
        }
        Ok(result)
    }

    pub fn apply_actions(&mut self, actions: &[Action]) -> Result<(), CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        if actions.len() != count {
            return Err(CudaSimError::ActionCountMismatch {
                states: count,
                actions: actions.len(),
            });
        }
        self.action_host.clear();
        self.action_host.resize(ACTION_WORDS * count, 0);
        for (lane, action) in actions.iter().enumerate() {
            let mut packed = [0u32; ACTION_WORDS];
            pack_action_words(action, &mut packed)?;
            for (field, value) in packed.into_iter().enumerate() {
                self.action_host[field * count + lane] = value;
            }
        }
        self.stream
            .memcpy_htod(&self.action_host, &mut self.action_device)?;
        self.clear_transition_status(count)?;
        self.enqueue_transition(count)?;
        self.stream.synchronize()?;
        self.check_transition_status(count)
    }

    fn enqueue_transition(&mut self, count: usize) -> Result<(), CudaSimError> {
        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let config = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.transition_kernel);
        arguments.arg(&mut self.state_device);
        arguments.arg(&self.topology_device);
        arguments.arg(&self.action_device);
        arguments.arg(&mut self.status_device);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        unsafe { arguments.launch(config)? };
        Ok(())
    }

    fn clear_transition_status(&mut self, count: usize) -> Result<(), CudaSimError> {
        self.status_host.clear();
        self.status_host.resize(count, 0);
        self.stream
            .memcpy_htod(&self.status_host, &mut self.status_device)?;
        Ok(())
    }

    fn check_transition_status(&mut self, count: usize) -> Result<(), CudaSimError> {
        self.status_host.resize(count, 0);
        let status = self.status_device.slice(0..count);
        self.stream.memcpy_dtoh(&status, &mut self.status_host)?;
        self.stream.synchronize()?;
        for (index, code) in self.status_host.iter().copied().enumerate() {
            let status = CudaSimTransitionStatus::from_code(code);
            if status != CudaSimTransitionStatus::Ok {
                return Err(CudaSimError::TransitionFailed { index, status });
            }
        }
        Ok(())
    }

    /// Fans each authoritative root action into many GPU-resident rollout lanes,
    /// advances those lanes entirely on device, and returns one compact reduction
    /// row per root action. No rollout state or trajectory is copied to the host.
    pub fn search_root_actions(
        &mut self,
        root_actions: &[Vec<Action>],
        rollouts_per_action: usize,
        rollout_steps: usize,
        seed: u64,
    ) -> Result<CudaSimRootSearchResult, CudaSimError> {
        self.search_root_actions_controlled(
            root_actions,
            rollouts_per_action,
            rollout_steps,
            seed,
            || false,
        )
    }

    pub fn search_root_actions_controlled<F>(
        &mut self,
        root_actions: &[Vec<Action>],
        rollouts_per_action: usize,
        rollout_steps: usize,
        seed: u64,
        mut should_cancel: F,
    ) -> Result<CudaSimRootSearchResult, CudaSimError>
    where
        F: FnMut() -> bool,
    {
        let base_count = self.resident_states;
        if base_count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        if root_actions.len() != base_count {
            return Err(CudaSimError::RootBatchMismatch {
                states: base_count,
                rows: root_actions.len(),
            });
        }
        if rollouts_per_action == 0 {
            return Err(CudaSimError::InvalidRolloutCount);
        }

        let mut flat_actions = Vec::new();
        let mut row_ranges = Vec::with_capacity(root_actions.len());
        self.root_base_index_host.clear();
        for (base, row) in root_actions.iter().enumerate() {
            let start = flat_actions.len();
            for action in row {
                if !cuda_sim_action_supported(action) {
                    return Err(CudaSimError::UnsupportedAction);
                }
                flat_actions.push(action.clone());
                self.root_base_index_host.push(base as u32);
            }
            row_ranges.push((start, flat_actions.len()));
        }

        let root_count = flat_actions.len();
        if root_count == 0 {
            return Ok(CudaSimRootSearchResult {
                rows: root_actions.iter().map(|_| Vec::new()).collect(),
                rollouts_per_action,
                rollout_steps,
            });
        }
        let base_stride = u32::try_from(base_count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let root_count_u32 = u32::try_from(root_count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let rollouts_u32 = u32::try_from(rollouts_per_action).map_err(|_| CudaSimError::BatchTooLarge)?;
        let rollout_steps_u32 = u32::try_from(rollout_steps).map_err(|_| CudaSimError::BatchTooLarge)?;
        let chunk_rollouts_cap = (SEARCH_CHUNK_LANES / root_count).max(1);
        let max_chunk_lanes = root_count
            .checked_mul(chunk_rollouts_cap.min(rollouts_per_action))
            .ok_or(CudaSimError::BatchTooLarge)?;

        self.ensure_root_capacity(root_count)?;
        self.ensure_search_capacity(max_chunk_lanes)?;

        self.root_action_host.clear();
        self.root_action_host.resize(ACTION_WORDS * root_count, 0);
        for (root, action) in flat_actions.iter().enumerate() {
            let mut packed = [0u32; ACTION_WORDS];
            pack_action_words(action, &mut packed)?;
            for (field, value) in packed.into_iter().enumerate() {
                self.root_action_host[field * root_count + root] = value;
            }
        }
        self.stream
            .memcpy_htod(&self.root_action_host, &mut self.root_action_device)?;
        self.stream.memcpy_htod(
            &self.root_base_index_host,
            &mut self.root_base_index_device,
        )?;
        self.root_seed_key_host.clear();
        self.root_seed_key_host.extend((0..root_count).map(|root| root as u64));
        self.stream.memcpy_htod(
            &self.root_seed_key_host,
            &mut self.root_seed_key_device,
        )?;
        self.root_stats_host.clear();
        self.root_stats_host.resize(ROOT_STATS_WORDS * root_count, 0);
        self.stream
            .memcpy_htod(&self.root_stats_host, &mut self.root_stats_device)?;

        let mut rollout_offset = 0usize;
        while rollout_offset < rollouts_per_action {
            if should_cancel() {
                return Err(CudaSimError::Cancelled);
            }
            let chunk_rollouts = (rollouts_per_action - rollout_offset).min(chunk_rollouts_cap);
            let lane_count = root_count
                .checked_mul(chunk_rollouts)
                .ok_or(CudaSimError::BatchTooLarge)?;
            let chunk_rollouts_u32 =
                u32::try_from(chunk_rollouts).map_err(|_| CudaSimError::BatchTooLarge)?;
            let rollout_offset_u32 =
                u32::try_from(rollout_offset).map_err(|_| CudaSimError::BatchTooLarge)?;
            let lane_count_u32 =
                u32::try_from(lane_count).map_err(|_| CudaSimError::BatchTooLarge)?;
            let stride = lane_count_u32;
            let config = LaunchConfig {
                grid_dim: (lane_count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
                block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
                shared_mem_bytes: 0,
            };
            {
                let mut arguments = self.stream.launch_builder(&self.expand_roots_kernel);
                arguments.arg(&self.state_device);
                arguments.arg(&base_stride);
                arguments.arg(&self.topology_device);
                arguments.arg(&self.root_action_device);
                arguments.arg(&self.root_base_index_device);
                arguments.arg(&self.root_seed_key_device);
                arguments.arg(&root_count_u32);
                arguments.arg(&chunk_rollouts_u32);
                arguments.arg(&rollouts_u32);
                arguments.arg(&rollout_offset_u32);
                arguments.arg(&mut self.search_state_device);
                arguments.arg(&mut self.search_action_device);
                arguments.arg(&mut self.search_status_device);
                arguments.arg(&mut self.search_rng_device);
                arguments.arg(&mut self.search_chance_rng_device);
                arguments.arg(&stride);
                arguments.arg(&seed);
                arguments.arg(&lane_count_u32);
                unsafe { arguments.launch(config)? };
            }
            if rollout_steps_u32 > 0 {
                // Preserve the configured effort scale: four legacy action
                // units fund one completed turn. Every root shares the target
                // relative to its pre-action base, including EndTurn itself.
                let turns = rollout_steps_u32.div_ceil(4);
                let mut turns_ahead = 0u32;
                while turns_ahead < turns {
                    if should_cancel() {
                        return Err(CudaSimError::Cancelled);
                    }
                    turns_ahead = (turns_ahead + 4).min(turns);
                    let mut arguments = self.stream.launch_builder(&self.root_rollout_turns_kernel);
                    arguments.arg(&mut self.search_state_device);
                    arguments.arg(&self.topology_device);
                    arguments.arg(&mut self.search_action_device);
                    arguments.arg(&mut self.search_status_device);
                    arguments.arg(&mut self.search_rng_device);
                    arguments.arg(&mut self.search_chance_rng_device);
                    arguments.arg(&self.state_device);
                    arguments.arg(&self.root_base_index_device);
                    arguments.arg(&base_stride);
                    arguments.arg(&chunk_rollouts_u32);
                    arguments.arg(&stride);
                    arguments.arg(&lane_count_u32);
                    arguments.arg(&turns_ahead);
                    unsafe { arguments.launch(config)? };
                    self.stream.synchronize()?;
                }
            }
            {
                let mut arguments = self.stream.launch_builder(&self.reduce_roots_kernel);
                arguments.arg(&self.search_state_device);
                arguments.arg(&self.search_status_device);
                arguments.arg(&self.state_device);
                arguments.arg(&self.topology_device);
                arguments.arg(&self.root_base_index_device);
                arguments.arg(&mut self.root_stats_device);
                arguments.arg(&stride);
                arguments.arg(&base_stride);
                arguments.arg(&root_count_u32);
                arguments.arg(&chunk_rollouts_u32);
                arguments.arg(&lane_count_u32);
                unsafe { arguments.launch(config)? };
            }
            self.stream.synchronize()?;
            rollout_offset += chunk_rollouts;
        }
        let stats = self.root_stats_device.slice(0..ROOT_STATS_WORDS * root_count);
        self.stream.memcpy_dtoh(&stats, &mut self.root_stats_host)?;
        self.stream.synchronize()?;

        let stat = |field: usize, root: usize| self.root_stats_host[field * root_count + root];
        let mut flat_stats = Vec::with_capacity(root_count);
        for (root, action) in flat_actions.into_iter().enumerate() {
            let samples = stat(0, root);
            let errors = stat(1, root);
            let valid = samples.saturating_sub(errors).max(1);
            flat_stats.push(CudaSimRootActionStats {
                action,
                samples: samples.min(u32::MAX as u64) as u32,
                errors: errors.min(u32::MAX as u64) as u32,
                terminal_samples: stat(2, root).min(u32::MAX as u64) as u32,
                wins: stat(3, root).min(u32::MAX as u64) as u32,
                mean_turn: stat(4, root) as f32 / valid as f32,
                mean_victory_points: stat(5, root) as f32 / valid as f32,
                mean_best_opponent_victory_points: stat(6, root) as f32 / valid as f32,
                mean_victory_margin_squared: stat(7, root) as f32 / valid as f32,
                mean_victory_points_squared: stat(8, root) as f32 / valid as f32,
                mean_best_opponent_victory_points_squared: stat(9, root) as f32 / valid as f32,
                mean_strategic_margin: (stat(10, root) as i64) as f32
                    / valid as f32
                    / ROLLOUT_CUTOFF_SCALE as f32,
                mean_strategic_margin_squared: stat(11, root) as f32
                    / valid as f32
                    / (ROLLOUT_CUTOFF_SCALE as f32 * ROLLOUT_CUTOFF_SCALE as f32),
            });
        }
        let rows = row_ranges
            .into_iter()
            .map(|(start, end)| flat_stats[start..end].to_vec())
            .collect();
        Ok(CudaSimRootSearchResult {
            rows,
            rollouts_per_action,
            rollout_steps,
        })
    }

    /// Returns compact campaign results without copying complete resident states.
    pub fn download_game_summaries(&mut self) -> Result<Vec<CudaSimGameSummary>, CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        let count_u32 = u32::try_from(count).map_err(|_| CudaSimError::BatchTooLarge)?;
        let stride = count_u32;
        let config = LaunchConfig {
            grid_dim: (count.div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
            block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
            shared_mem_bytes: 0,
        };
        let mut arguments = self.stream.launch_builder(&self.summary_kernel);
        arguments.arg(&self.state_device);
        arguments.arg(&mut self.summary_device);
        arguments.arg(&stride);
        arguments.arg(&count_u32);
        unsafe { arguments.launch(config)? };
        self.summary_host.resize(SUMMARY_WORDS * count, 0);
        let summaries = self.summary_device.slice(0..SUMMARY_WORDS * count);
        self.stream.memcpy_dtoh(&summaries, &mut self.summary_host)?;
        self.stream.synchronize()?;

        let mut result = Vec::with_capacity(count);
        for lane in 0..count {
            let field = |index: usize| self.summary_host[index * count + lane];
            let winner_code = field(1);
            result.push(CudaSimGameSummary {
                terminal: field(0) != 0,
                winner: (winner_code != 0).then(|| (winner_code - 1) as u8),
                turn: field(2),
                victory_points: [field(3) as u8, field(4) as u8, field(5) as u8, field(6) as u8],
            });
        }
        Ok(result)
    }

    pub fn download_packed_states(&mut self) -> Result<Vec<CudaSimPackedState>, CudaSimError> {
        let count = self.resident_states;
        if count == 0 {
            return Err(CudaSimError::NoResidentBatch);
        }
        self.state_host.resize(STATE_WORDS * count, 0);
        let state = self.state_device.slice(0..STATE_WORDS * count);
        self.stream.memcpy_dtoh(&state, &mut self.state_host)?;
        self.stream.synchronize()?;
        let mut result = Vec::with_capacity(count);
        for lane in 0..count {
            let mut words = [0u32; STATE_WORDS];
            for (field, word) in words.iter_mut().enumerate() {
                *word = self.state_host[field * count + lane];
            }
            result.push(CudaSimPackedState { words });
        }
        Ok(result)
    }

    fn ensure_capacity(&mut self, required: usize) -> Result<(), CudaSimError> {
        if required <= self.capacity {
            return Ok(());
        }
        let mut capacity = self.capacity.max(1);
        while capacity < required {
            capacity = capacity
                .checked_mul(2)
                .ok_or(CudaSimError::BatchTooLarge)?;
        }
        self.state_device = self.stream.alloc_zeros(capacity * STATE_WORDS)?;
        self.action_device = self.stream.alloc_zeros(capacity * ACTION_WORDS)?;
        self.status_device = self.stream.alloc_zeros(capacity)?;
        self.rng_device = self.stream.alloc_zeros(capacity)?;
        self.chance_rng_device = self.stream.alloc_zeros(capacity)?;
        self.summary_device = self.stream.alloc_zeros(capacity * SUMMARY_WORDS)?;
        self.arena_action_count_device = self.stream.alloc_zeros(capacity)?;
        self.candidate_ready_device = self.stream.alloc_zeros(capacity)?;
        self.capacity = capacity;
        Ok(())
    }

    fn ensure_search_capacity(&mut self, required: usize) -> Result<(), CudaSimError> {
        if required <= self.search_capacity {
            return Ok(());
        }
        let mut capacity = self.search_capacity.max(1);
        while capacity < required {
            capacity = capacity
                .checked_mul(2)
                .ok_or(CudaSimError::BatchTooLarge)?;
        }
        self.search_state_device = self.stream.alloc_zeros(capacity * STATE_WORDS)?;
        self.search_action_device = self.stream.alloc_zeros(capacity * ACTION_WORDS)?;
        self.search_status_device = self.stream.alloc_zeros(capacity)?;
        self.search_rng_device = self.stream.alloc_zeros(capacity)?;
        self.search_chance_rng_device = self.stream.alloc_zeros(capacity)?;
        self.search_capacity = capacity;
        Ok(())
    }

    fn ensure_root_capacity(&mut self, required: usize) -> Result<(), CudaSimError> {
        if required <= self.root_capacity {
            return Ok(());
        }
        let mut capacity = self.root_capacity.max(1);
        while capacity < required {
            capacity = capacity
                .checked_mul(2)
                .ok_or(CudaSimError::BatchTooLarge)?;
        }
        self.root_action_device = self.stream.alloc_zeros(capacity * ACTION_WORDS)?;
        self.root_base_index_device = self.stream.alloc_zeros(capacity)?;
        self.root_seed_key_device = self.stream.alloc_zeros(capacity)?;
        self.root_stats_device = self.stream.alloc_zeros(capacity * ROOT_STATS_WORDS)?;
        self.root_capacity = capacity;
        Ok(())
    }
}

pub fn cuda_sim_action_supported(_action: &Action) -> bool {
    true
}

fn resource_from_index(index: u32) -> Result<Resource, CudaSimError> {
    match index {
        0 => Ok(Resource::Lumber),
        1 => Ok(Resource::Brick),
        2 => Ok(Resource::Wool),
        3 => Ok(Resource::Grain),
        4 => Ok(Resource::Ore),
        _ => Err(CudaSimError::UnsupportedAction),
    }
}

fn dev_card_from_index(index: u32) -> Result<DevCard, CudaSimError> {
    match index {
        0 => Ok(DevCard::Knight),
        1 => Ok(DevCard::VictoryPoint),
        2 => Ok(DevCard::RoadBuilding),
        3 => Ok(DevCard::YearOfPlenty),
        4 => Ok(DevCard::Monopoly),
        _ => Err(CudaSimError::UnsupportedAction),
    }
}

fn unpack_action_words(words: &[u32; ACTION_WORDS]) -> Result<Action, CudaSimError> {
    let arg = |index: usize| words[ACTION_ARG0 + index];
    match words[ACTION_TAG] {
        ACTION_PLACE_SETTLEMENT => Ok(Action::PlaceSettlement { vertex: arg(0) as u8 }),
        ACTION_PLACE_ROAD => Ok(Action::PlaceRoad { edge: arg(0) as u8 }),
        ACTION_ROLL => Ok(Action::Roll),
        ACTION_RESOLVE_ROLL => Ok(Action::ResolveRoll { value: arg(0) as u8 }),
        ACTION_DISCARD => Ok(Action::Discard {
            cards: [arg(0) as u8, arg(1) as u8, arg(2) as u8, arg(3) as u8, arg(4) as u8],
        }),
        ACTION_MOVE_ROBBER => Ok(Action::MoveRobber {
            hex: arg(0) as u8,
            victim: (arg(1) != 0).then(|| (arg(1) - 1) as u8),
        }),
        ACTION_RESOLVE_STEAL => Ok(Action::ResolveSteal {
            victim: arg(0) as u8,
            resource: resource_from_index(arg(1))?,
        }),
        ACTION_BUILD_ROAD => Ok(Action::BuildRoad { edge: arg(0) as u8 }),
        ACTION_BUILD_SETTLEMENT => Ok(Action::BuildSettlement { vertex: arg(0) as u8 }),
        ACTION_BUILD_CITY => Ok(Action::BuildCity { vertex: arg(0) as u8 }),
        ACTION_BUY_DEVELOPMENT => Ok(Action::BuyDevelopment),
        ACTION_RESOLVE_DEVELOPMENT => Ok(Action::ResolveDevelopment {
            card: dev_card_from_index(arg(0))?,
        }),
        ACTION_PLAY_KNIGHT => Ok(Action::PlayKnight {
            hex: arg(0) as u8,
            victim: (arg(1) != 0).then(|| (arg(1) - 1) as u8),
        }),
        ACTION_PLAY_ROAD_BUILDING => Ok(Action::PlayRoadBuilding {
            first: arg(0) as u8,
            second: (arg(1) != 0).then(|| (arg(1) - 1) as u8),
        }),
        ACTION_PLAY_YEAR_OF_PLENTY => Ok(Action::PlayYearOfPlenty {
            first: resource_from_index(arg(0))?,
            second: resource_from_index(arg(1))?,
        }),
        ACTION_PLAY_MONOPOLY => Ok(Action::PlayMonopoly {
            resource: resource_from_index(arg(0))?,
        }),
        ACTION_MARITIME_TRADE => Ok(Action::MaritimeTrade {
            give: resource_from_index(arg(0))?,
            receive: resource_from_index(arg(1))?,
            ratio: arg(2) as u8,
        }),
        ACTION_END_TURN => Ok(Action::EndTurn),
        ACTION_OFFER_TRADE => Ok(Action::OfferTrade {
            recipients: arg(0) as u8,
            give: [arg(1) as u8, arg(2) as u8, arg(3) as u8, arg(4) as u8, arg(5) as u8],
            receive: [arg(6) as u8, arg(7) as u8, arg(8) as u8, arg(9) as u8, arg(10) as u8],
        }),
        ACTION_RESPOND_TRADE => Ok(Action::RespondTrade { accept: arg(0) != 0 }),
        ACTION_COUNTER_TRADE => Ok(Action::CounterTrade {
            give: [arg(0) as u8, arg(1) as u8, arg(2) as u8, arg(3) as u8, arg(4) as u8],
            receive: [arg(5) as u8, arg(6) as u8, arg(7) as u8, arg(8) as u8, arg(9) as u8],
        }),
        ACTION_CONFIRM_TRADE => Ok(Action::ConfirmTrade { partner: arg(0) as u8 }),
        ACTION_CANCEL_TRADE => Ok(Action::CancelTrade),
        _ => Err(CudaSimError::UnsupportedAction),
    }
}

fn pack_action_words(action: &Action, words: &mut [u32; ACTION_WORDS]) -> Result<(), CudaSimError> {
    match action {
        Action::PlaceSettlement { vertex } => {
            words[ACTION_TAG] = ACTION_PLACE_SETTLEMENT;
            words[ACTION_ARG0] = *vertex as u32;
        }
        Action::PlaceRoad { edge } => {
            words[ACTION_TAG] = ACTION_PLACE_ROAD;
            words[ACTION_ARG0] = *edge as u32;
        }
        Action::Roll => words[ACTION_TAG] = ACTION_ROLL,
        Action::ResolveRoll { value } => {
            words[ACTION_TAG] = ACTION_RESOLVE_ROLL;
            words[ACTION_ARG0] = *value as u32;
        }
        Action::Discard { cards } => {
            words[ACTION_TAG] = ACTION_DISCARD;
            for (index, count) in cards.iter().copied().enumerate() {
                words[ACTION_ARG0 + index] = count as u32;
            }
        }
        Action::MoveRobber { hex, victim } => {
            words[ACTION_TAG] = ACTION_MOVE_ROBBER;
            words[ACTION_ARG0] = *hex as u32;
            words[ACTION_ARG0 + 1] = victim.map(|player| player as u32 + 1).unwrap_or(0);
        }
        Action::ResolveSteal { victim, resource } => {
            words[ACTION_TAG] = ACTION_RESOLVE_STEAL;
            words[ACTION_ARG0] = *victim as u32;
            words[ACTION_ARG0 + 1] = resource.index() as u32;
        }
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
        Action::ResolveDevelopment { card } => {
            words[ACTION_TAG] = ACTION_RESOLVE_DEVELOPMENT;
            words[ACTION_ARG0] = card.index() as u32;
        }
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
        Action::OfferTrade {
            recipients,
            give,
            receive,
        } => {
            words[ACTION_TAG] = ACTION_OFFER_TRADE;
            words[ACTION_ARG0] = *recipients as u32;
            for resource in 0..5 {
                words[ACTION_ARG0 + 1 + resource] = give[resource] as u32;
                words[ACTION_ARG0 + 6 + resource] = receive[resource] as u32;
            }
        }
        Action::RespondTrade { accept } => {
            words[ACTION_TAG] = ACTION_RESPOND_TRADE;
            words[ACTION_ARG0] = u32::from(*accept);
        }
        Action::CounterTrade { give, receive } => {
            words[ACTION_TAG] = ACTION_COUNTER_TRADE;
            for resource in 0..5 {
                words[ACTION_ARG0 + resource] = give[resource] as u32;
                words[ACTION_ARG0 + 5 + resource] = receive[resource] as u32;
            }
        }
        Action::ConfirmTrade { partner } => {
            words[ACTION_TAG] = ACTION_CONFIRM_TRADE;
            words[ACTION_ARG0] = *partner as u32;
        }
        Action::CancelTrade => words[ACTION_TAG] = ACTION_CANCEL_TRADE,
        Action::EndTurn => words[ACTION_TAG] = ACTION_END_TURN,
    }
    Ok(())
}

fn pack_trade_words(trade: Option<TradeOffer>, words: &mut [u32; STATE_WORDS], base: usize) {
    for field in 0..TRADE_STRIDE {
        words[base + field] = 0;
    }
    let Some(trade) = trade else {
        return;
    };
    words[base + TRADE_PRESENT] = 1;
    words[base + TRADE_CREATOR] = trade.creator as u32;
    words[base + TRADE_RECIPIENTS] = trade.recipients as u32;
    for resource in 0..5 {
        words[base + TRADE_GIVE + resource] = trade.give[resource] as u32;
        words[base + TRADE_RECEIVE + resource] = trade.receive[resource] as u32;
    }
    words[base + TRADE_ACCEPTED] = trade.accepted as u32;
    words[base + TRADE_REJECTED] = trade.rejected as u32;
}

fn pack_state_words(state: &GameState, words: &mut [u32; STATE_WORDS]) -> Result<(), CudaSimError> {
    let board = state.board.as_ref();
    let players = board.num_players as usize;
    if !(2..=MAX_PLAYERS).contains(&players) || state.players.len() != players {
        return Err(CudaSimError::UnsupportedState(
            "only 2-4 player states are supported",
        ));
    }
    if board.hexes.len() != HEX_COUNT
        || board.vertices.len() != VERTEX_COUNT
        || board.edges.len() != EDGE_COUNT
    {
        return Err(CudaSimError::TopologyMismatch);
    }
    if let Some(belief) = state.stochastic.reference_belief() {
        let particles = belief.particles();
        if particles.is_empty() || particles.len() > MAX_DICE_PARTICLES {
            return Err(CudaSimError::UnsupportedState("Mref posterior must contain 1..64 particles"));
        }
        words[STATE_DICE_MODEL] = 1;
        words[STATE_DICE_COUNT] = particles.len() as u32;
        for (index, particle) in particles.iter().enumerate() {
            let base = STATE_DICE_PARTICLES + index * DICE_PARTICLE_WORDS;
            let controller = &particle.controller;
            words[base] = particle.mass as u32;
            words[base + 1] = (particle.mass >> 32) as u32;
            for (i, remaining) in controller.remaining_counts().into_iter().enumerate() {
                words[base + 2 + i] = remaining as u32;
            }
            words[base + 13] = controller.cards_left() as u32;
            let recent = controller.recent_totals();
            for (i, total) in recent.iter().enumerate() { words[base + 14 + i] = *total as u32; }
            words[base + 19] = recent.len() as u32;
            words[base + 20] = controller.initialized_player_mask() as u32;
            for (i, count) in controller.seven_counts().into_iter().enumerate() { words[base + 21 + i] = count; }
            words[base + 25] = controller.seven_streak_owner().map_or(0, |actor| actor as u32 + 1);
            words[base + 26] = controller.seven_streak_count();
            words[base + 27] = controller.prepared_actor().map_or(0, |actor| actor as u32 + 1);
        }
    }
    let (phase, phase_arg) = phase_words(state.phase);
    let (return_phase, return_arg) = phase_words(state.robber_return_phase);
    words[STATE_NUM_PLAYERS] = players as u32;
    words[STATE_PHASE] = phase;
    words[STATE_PHASE_ARG] = phase_arg;
    words[STATE_CURRENT_PLAYER] = state.current_player as u32;
    words[STATE_ROBBER_HEX] = state.robber_hex as u32;
    words[STATE_VICTORY_TARGET] = state.victory_target as u32;
    words[STATE_DISCARD_LIMIT] = state.card_discard_limit as u32;
    words[STATE_BANK_PUBLIC] = u32::from(state.bank_is_public);
    words[STATE_LONGEST_HOLDER] = holder_code(state.longest_road_holder);
    words[STATE_LARGEST_HOLDER] = holder_code(state.largest_army_holder);
    words[STATE_TURN] = state.turn as u32;
    words[STATE_LAST_ROLL] = state.last_roll as u32;
    words[STATE_FRIENDLY_ROBBER] = u32::from(state.friendly_robber);
    words[STATE_SETUP_STEP] = state.setup_step as u32;
    words[STATE_DISCARD_CURSOR] = state.discard_cursor as u32;
    words[STATE_ROBBER_RETURN_PHASE] = return_phase;
    words[STATE_ROBBER_RETURN_ARG] = return_arg;
    words[STATE_FREE_ROADS] = state.free_roads as u32;
    words[STATE_DOMESTIC_TRADE_USED] = u32::from(state.domestic_trade_used);
    words[STATE_DOMESTIC_TRADE_COUNT] = state.domestic_trade_count as u32;
    words[STATE_PLAYER_TRADES_ENABLED] = u32::from(state.player_trades_enabled);
    words[STATE_DOMESTIC_TRADE_DISABLED] = state.domestic_trade_disabled as u32;
    words[STATE_DOMESTIC_TRADE_EMBARGOES] = state.domestic_trade_embargoes as u32;
    words[STATE_TRADE_CURSOR] = state.trade_cursor as u32;
    words[STATE_TRADE_NEGOTIATION_ROUND] = state.trade_negotiation_round as u32;
    pack_trade_words(state.trade, words, STATE_TRADE);
    pack_trade_words(state.last_rejected_trade, words, STATE_LAST_REJECTED_TRADE);

    for resource in 0..5 {
        words[STATE_BANK + resource] = state.bank[resource] as u32;
    }
    for card in 0..5 {
        words[STATE_DEVELOPMENT_DECK + card] = state.development_deck[card] as u32;
        words[STATE_PLAYED_DEVELOPMENT + card] = state.played_development[card] as u32;
    }
    for player in 0..MAX_PLAYERS {
        words[STATE_DISCARD_REMAINING + player] = state.discard_remaining[player] as u32;
    }
    for (hex, tile) in board.hexes.iter().enumerate() {
        words[STATE_HEX_RESOURCES + hex] = tile
            .resource
            .map(|resource| resource.index() as u32 + 1)
            .unwrap_or(0);
        words[STATE_HEX_NUMBERS + hex] = tile.number as u32;
    }
    for (vertex, data) in board.vertices.iter().enumerate() {
        words[STATE_PORTS + vertex] = match data.port {
            None => 0,
            Some(Port::Generic) => 1,
            Some(Port::Resource(resource)) => resource.index() as u32 + 2,
        };
    }
    for (vertex, building) in state.buildings.iter().enumerate() {
        words[STATE_BUILDINGS + vertex] = building.map(building_code).unwrap_or(0);
    }
    for (edge, owner) in state.roads.iter().enumerate() {
        words[STATE_ROADS + edge] = owner.map(|player| player as u32 + 1).unwrap_or(0);
    }
    for (player, player_state) in state.players.iter().enumerate() {
        let base = STATE_PLAYERS + player * PLAYER_STRIDE;
        for resource in 0..5 {
            words[base + resource] = player_state.resources[resource] as u32;
            words[base + 5 + resource] = player_state.development[resource] as u32;
            words[base + 10 + resource] = player_state.bought_development[resource] as u32;
        }
        words[base + 15] = player_state.public_victory_points as u32;
        words[base + 16] = player_state.played_knights as u32;
        words[base + 17] = player_state.roads_left as u32;
        words[base + 18] = player_state.settlements_left as u32;
        words[base + 19] = player_state.cities_left as u32;
        words[base + 20] = u32::from(player_state.has_longest_road);
        words[base + 21] = u32::from(player_state.has_largest_army);
        words[base + 22] = u32::from(player_state.played_development_this_turn);
        for profile in 0..5 {
            words[base + 23 + profile] = player_state.policy_profile[profile] as u32;
        }
    }
    Ok(())
}

fn topology_words(board: &Board) -> Result<Vec<u32>, CudaSimError> {
    if board.vertices.len() != VERTEX_COUNT || board.edges.len() != EDGE_COUNT {
        return Err(CudaSimError::TopologyMismatch);
    }
    let mut words = vec![0u32; TOPOLOGY_WORDS];
    for (vertex, data) in board.vertices.iter().enumerate() {
        if data.adjacent_hexes.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_vertices.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_edges.len() > MAX_VERTEX_ADJACENCY
        {
            return Err(CudaSimError::UnsupportedState(
                "standard vertices have at most three adjacent items",
            ));
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

fn phase_words(phase: Phase) -> (u32, u32) {
    match phase {
        Phase::SetupSettlement => (0, 0),
        Phase::SetupRoad { settlement } => (1, settlement as u32),
        Phase::PreRoll => (2, 0),
        Phase::RollChance => (3, 0),
        Phase::Discard => (4, 0),
        Phase::MoveRobber => (5, 0),
        Phase::ResolveSteal { victim } => (6, victim as u32),
        Phase::Main => (7, 0),
        Phase::DevelopmentChance => (8, 0),
        Phase::TradeResponses => (9, 0),
        Phase::Finished => (10, 0),
    }
}

fn holder_code(holder: Option<u8>) -> u32 {
    holder.map(|player| player as u32 + 1).unwrap_or(0)
}

fn building_code(building: Building) -> u32 {
    match building {
        Building::Settlement(player) => player as u32 + 1,
        Building::City(player) => player as u32 + 5,
    }
}

#[cfg(test)]
mod tests {
    use crate::rollout_cutoff::rollout_cutoff_margin;

    use super::*;

    fn finish_setup(state: &mut GameState) {
        while matches!(
            state.phase,
            Phase::SetupSettlement | Phase::SetupRoad { .. }
        ) {
            let action = state.legal_actions()[0].clone();
            state.apply(&action).unwrap();
        }
    }

    fn hidden_bank_worlds() -> (GameState, GameState) {
        let mut grain_world = GameState::standard(71_003, 2);
        finish_setup(&mut grain_world);
        grain_world.phase = Phase::Main;
        grain_world.current_player = 0;
        grain_world.bank_is_public = false;
        grain_world.player_trades_enabled = false;
        grain_world.domestic_trade_disabled = 0b11;
        grain_world.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        grain_world.bank = [19; 5];
        grain_world.players[0].resources[Resource::Lumber.index()] = 4;
        grain_world.bank[Resource::Lumber.index()] = 15;
        grain_world.players[1].resources[Resource::Grain.index()] = 3;
        grain_world.bank[Resource::Grain.index()] = 16;

        let mut ore_world = grain_world.clone();
        ore_world.players[1].resources = [0; 5];
        ore_world.bank[Resource::Grain.index()] = 19;
        ore_world.players[1].resources[Resource::Ore.index()] = 3;
        ore_world.bank[Resource::Ore.index()] = 16;
        assert_eq!(grain_world.observation_hash(0), ore_world.observation_hash(0));
        grain_world.validate().unwrap();
        ore_world.validate().unwrap();
        (grain_world, ore_world)
    }

    fn generated_action_for_seed(
        engine: &mut CudaSimEngine,
        state: &GameState,
        seed: u64,
    ) -> Result<Action, CudaSimError> {
        engine.upload_states(std::slice::from_ref(state))?;
        engine.seed_rollout_rng(seed)?;
        engine.generate_rollout_actions()?;
        Ok(engine.download_generated_actions()?.remove(0))
    }

    #[test]
    fn root_micro_actions_share_the_pre_action_turn_horizon() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = GameState::standard(71_001, 4);
        finish_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        state.players[0].resources[Resource::Lumber.index()] = 4;
        state.bank = [15, 19, 19, 19, 19];
        let actions = vec![
            Action::MaritimeTrade { give: Resource::Lumber, receive: Resource::Grain, ratio: 4 },
            Action::EndTurn,
        ];
        for action in &actions { assert!(state.legal_actions().contains(action)); }
        let mut engine = CudaSimEngine::new()?;
        engine.upload_states(std::slice::from_ref(&state))?;
        let result = engine.search_root_actions(std::slice::from_ref(&actions), 16, 16, 91_123)?;
        for stat in &result.rows[0] {
            assert_eq!(stat.errors, 0, "{:?}", stat.action);
            assert_eq!(stat.terminal_samples, 0, "early-game fixture should remain nonterminal");
            assert_eq!(stat.mean_turn, (state.turn + 4) as f32,
                "root micro-actions must not shorten the opponent horizon: {:?}", stat.action);
        }
        Ok(())
    }

    #[test]
    fn root_strategic_cutoff_matches_cpu_reference() -> Result<(), Box<dyn std::error::Error>> {
        let mut state = GameState::standard(71_001, 2);
        finish_setup(&mut state);
        state.phase = Phase::Main;
        state.current_player = 0;
        state.players.iter_mut().for_each(|player| player.resources = [0; 5]);
        state.bank = [19; 5];
        state.players[0].resources[Resource::Lumber.index()] = 4;
        state.bank[Resource::Lumber.index()] = 15;
        let actions = vec![
            Action::MaritimeTrade {
                give: Resource::Lumber,
                receive: Resource::Grain,
                ratio: 4,
            },
            Action::EndTurn,
        ];
        for action in &actions {
            assert!(state.legal_actions().contains(action));
        }

        let mut engine = CudaSimEngine::new()?;
        engine.upload_states(std::slice::from_ref(&state))?;
        let result = engine.search_root_actions(std::slice::from_ref(&actions), 1, 0, 91_001)?;
        for stat in &result.rows[0] {
            let mut next = state.clone();
            next.apply(&stat.action)?;
            let expected = rollout_cutoff_margin(&next, 0);
            assert!(
                (stat.mean_strategic_margin - expected).abs()
                    <= 1.0 / ROLLOUT_CUTOFF_SCALE as f32,
                "CPU/GPU strategic cutoff mismatch for {:?}: gpu={} cpu={}",
                stat.action,
                stat.mean_strategic_margin,
                expected,
            );
        }

        let mut road_state = GameState::standard(1, 2);
        finish_setup(&mut road_state);
        road_state.phase = Phase::Main;
        road_state.current_player = 0;
        road_state
            .players
            .iter_mut()
            .for_each(|player| player.resources = [0; 5]);
        road_state.bank = [19; 5];
        road_state.players[0].resources = [2, 2, 0, 0, 0];
        road_state.bank = [17, 17, 19, 19, 19];
        let complete_route = Action::BuildRoad { edge: 23 };
        let local_decoy = Action::BuildRoad { edge: 12 };
        let road_actions = vec![complete_route.clone(), local_decoy.clone()];
        for action in &road_actions {
            assert!(road_state.legal_actions().contains(action));
        }
        engine.upload_states(std::slice::from_ref(&road_state))?;
        let road_result =
            engine.search_root_actions(std::slice::from_ref(&road_actions), 1, 0, 91_003)?;
        for stat in &road_result.rows[0] {
            let mut next = road_state.clone();
            next.apply(&stat.action)?;
            let expected = rollout_cutoff_margin(&next, 0);
            assert!(
                (stat.mean_strategic_margin - expected).abs()
                    <= 1.0 / ROLLOUT_CUTOFF_SCALE as f32,
                "CPU/GPU road cutoff mismatch for {:?}: gpu={} cpu={}",
                stat.action,
                stat.mean_strategic_margin,
                expected,
            );
        }
        let margin = |action: &Action| {
            road_result.rows[0]
                .iter()
                .find(|stat| &stat.action == action)
                .expect("evaluated road root")
                .mean_strategic_margin
        };
        assert!(margin(&complete_route) > margin(&local_decoy));
        Ok(())
    }

    #[test]
    fn hidden_bank_maritime_policy_is_observation_safe_and_available() -> Result<(), Box<dyn std::error::Error>> {
        let (grain_world, ore_world) = hidden_bank_worlds();
        let mut engine = CudaSimEngine::new()?;
        let mut observed_maritime = false;
        for seed in 1..=64 {
            let left = generated_action_for_seed(&mut engine, &grain_world, seed)?;
            let right = generated_action_for_seed(&mut engine, &ore_world, seed)?;
            assert_eq!(left, right, "hidden bank identity changed the policy action at seed {seed}");
            observed_maritime |= matches!(left, Action::MaritimeTrade { .. });
        }
        assert!(observed_maritime, "hidden-bank rollouts must retain self-controlled maritime play");
        Ok(())
    }

    #[test]
    fn hidden_bank_year_of_plenty_policy_is_observation_safe_and_available() -> Result<(), Box<dyn std::error::Error>> {
        let (mut grain_world, mut ore_world) = hidden_bank_worlds();
        for state in [&mut grain_world, &mut ore_world] {
            state.phase = Phase::PreRoll;
            state.players[0].resources = [0; 5];
            state.bank[Resource::Lumber.index()] = 19;
            state.players[0].development[3] = 1;
            state.players[0].bought_development[3] = 0;
        }
        assert_eq!(grain_world.observation_hash(0), ore_world.observation_hash(0));
        let mut engine = CudaSimEngine::new()?;
        let mut observed_yop = false;
        for seed in 1..=64 {
            let left = generated_action_for_seed(&mut engine, &grain_world, seed)?;
            let right = generated_action_for_seed(&mut engine, &ore_world, seed)?;
            assert_eq!(left, right, "hidden bank identity changed the YOP policy action at seed {seed}");
            observed_yop |= matches!(left, Action::PlayYearOfPlenty { .. });
        }
        assert!(observed_yop, "hidden-bank rollouts must retain Year of Plenty play");
        Ok(())
    }

    #[test]
    fn bank_shortage_single_player_cpu_gpu_parity() -> Result<(), Box<dyn std::error::Error>> {
        const ROLL: u8 = 6;
        let resource = Resource::Ore;
        let resource_index = resource.index();
        let mut state = GameState::standard(69_998, 3);
        state.buildings.fill(None);
        state.bank = [19; 5];
        for player in &mut state.players {
            player.resources = [0; 5];
        }
        for hex in &mut Arc::make_mut(&mut state.board).hexes {
            hex.number = 2;
        }
        let target_hex = 0u8;
        {
            let tile = &mut Arc::make_mut(&mut state.board).hexes[target_hex as usize];
            tile.resource = Some(resource);
            tile.number = ROLL;
        }
        let vertex = state
            .board
            .vertices
            .iter()
            .position(|data| data.adjacent_hexes.contains(&target_hex))
            .ok_or("production parity fixture had no vertex touching target hex")?;
        state.buildings[vertex] = Some(Building::City(0));
        state.robber_hex = 18;
        state.bank[resource_index] = 1;
        state.players[2].resources[resource_index] = 18;
        state.phase = Phase::RollChance;

        let mut engine = CudaSimEngine::new()?;
        engine.upload_states(std::slice::from_ref(&state))?;
        let action = Action::ResolveRoll { value: ROLL };
        engine.apply_actions(std::slice::from_ref(&action))?;
        state.apply(&action)?;

        assert_eq!(state.players[0].resources[resource_index], 1);
        assert_eq!(state.bank[resource_index], 0);
        assert_eq!(
            engine.download_packed_states()?.remove(0),
            CudaSimPackedState::new(&state)?
        );
        Ok(())
    }
}
