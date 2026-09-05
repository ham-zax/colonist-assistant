#![cfg(feature = "cuda-exact")]

use std::fmt;
use std::sync::Arc;
use std::time::Instant;

use colonist_catan_core::{Board, Building, GameState, Phase, Port};
use cudarc::driver::{
    CudaContext, CudaFunction, CudaModule, CudaSlice, DriverError, LaunchConfig, PushKernelArg,
};
use cudarc::nvrtc::{CompileError, CompileOptions, compile_ptx_with_opts};

const MAX_PLAYERS: usize = 4;
const HEX_COUNT: usize = 19;
const VERTEX_COUNT: usize = 54;
const EDGE_COUNT: usize = 72;
const MAX_VERTEX_ADJACENCY: usize = 3;
const MAX_BATCH_CAPACITY: usize = 256;
const THREADS_PER_BLOCK: usize = 128;

const STATE_NUM_PLAYERS: usize = 0;
const STATE_PHASE: usize = 1;
const STATE_CURRENT_PLAYER: usize = 2;
const STATE_ROBBER_HEX: usize = 3;
const STATE_VICTORY_TARGET: usize = 4;
const STATE_DISCARD_LIMIT: usize = 5;
const STATE_BANK_PUBLIC: usize = 6;
const STATE_LONGEST_HOLDER: usize = 7;
const STATE_LARGEST_HOLDER: usize = 8;
const STATE_BANK: usize = 9;
const STATE_DEVELOPMENT_DECK: usize = STATE_BANK + 5;
const STATE_PLAYED_DEVELOPMENT: usize = STATE_DEVELOPMENT_DECK + 5;
const STATE_HEX_RESOURCES: usize = STATE_PLAYED_DEVELOPMENT + 5;
const STATE_HEX_NUMBERS: usize = STATE_HEX_RESOURCES + HEX_COUNT;
const STATE_PORTS: usize = STATE_HEX_NUMBERS + HEX_COUNT;
const STATE_BUILDINGS: usize = STATE_PORTS + VERTEX_COUNT;
const STATE_ROADS: usize = STATE_BUILDINGS + VERTEX_COUNT;
const STATE_PLAYERS: usize = STATE_ROADS + EDGE_COUNT;
const PLAYER_STRIDE: usize = 22;
const STATE_DOMESTIC_TRADE_DISABLED: usize = STATE_PLAYERS + MAX_PLAYERS * PLAYER_STRIDE;
const STATE_WORDS: usize = STATE_DOMESTIC_TRADE_DISABLED + 1;

const TOPO_VERTEX_HEX_COUNTS: usize = 0;
const TOPO_VERTEX_HEXES: usize = TOPO_VERTEX_HEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_VERTEX_COUNTS: usize = TOPO_VERTEX_HEXES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_VERTICES: usize = TOPO_VERTEX_VERTEX_COUNTS + VERTEX_COUNT;
const TOPO_VERTEX_EDGE_COUNTS: usize = TOPO_VERTEX_VERTICES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPO_VERTEX_EDGES: usize = TOPO_VERTEX_EDGE_COUNTS + VERTEX_COUNT;
const TOPO_EDGE_VERTICES: usize = TOPO_VERTEX_EDGES + VERTEX_COUNT * MAX_VERTEX_ADJACENCY;
const TOPOLOGY_WORDS: usize = TOPO_EDGE_VERTICES + EDGE_COUNT * 2;

const BACKEND_NAME: &str = "cuda-exact";

const CUDA_SOURCE: &str = include_str!("cuda/exact_eval.cu");

#[derive(Clone, Debug)]
pub struct CudaDeviceIdentity {
    pub backend: &'static str,
    pub ordinal: usize,
    pub name: String,
    pub compute_capability: (i32, i32),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct CudaExactStats {
    pub batches: u64,
    pub states: u64,
    pub last_batch_size: usize,
    pub last_batch_nanos: u128,
    pub last_pack_nanos: u128,
    pub last_upload_nanos: u128,
    pub last_kernel_nanos: u128,
    pub last_download_nanos: u128,
    pub total_pack_nanos: u128,
    pub total_upload_nanos: u128,
    pub total_kernel_nanos: u128,
    pub total_download_nanos: u128,
    pub total_nanos: u128,
}

impl CudaExactStats {
    pub fn average_batch_size(&self) -> f64 {
        if self.batches == 0 {
            0.0
        } else {
            self.states as f64 / self.batches as f64
        }
    }

    pub fn average_batch_nanos(&self) -> f64 {
        if self.batches == 0 {
            0.0
        } else {
            self.total_nanos as f64 / self.batches as f64
        }
    }

    pub fn states_per_second(&self) -> f64 {
        if self.total_nanos == 0 {
            0.0
        } else {
            self.states as f64 * 1_000_000_000.0 / self.total_nanos as f64
        }
    }
}

pub struct CudaExactPackedState {
    words: [u32; STATE_WORDS],
    winner: Option<u8>,
}

impl CudaExactPackedState {
    pub fn new(state: &GameState) -> Result<Self, CudaExactError> {
        let mut words = [0; STATE_WORDS];
        pack_state_words(state, &mut words)?;
        Ok(Self {
            words,
            winner: state.winner(),
        })
    }
}

#[derive(Debug)]
pub enum CudaExactError {
    LearnedValuePromoted,
    EmptyStandardTopology,
    UnsupportedState(&'static str),
    TopologyMismatch,
    BatchTooLarge,
    InvalidOutput {
        state: usize,
        player: usize,
        value: f32,
    },
    TerminalOutputMismatch {
        state: usize,
        winner: usize,
    },
    Cuda(DriverError),
    Nvrtc(CompileError),
}

impl fmt::Display for CudaExactError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::LearnedValuePromoted => formatter.write_str(
                "cuda-exact is disabled because the learned value evaluator is promoted",
            ),
            Self::EmptyStandardTopology => formatter.write_str("standard topology is empty"),
            Self::UnsupportedState(reason) => write!(formatter, "unsupported CUDA state: {reason}"),
            Self::TopologyMismatch => formatter.write_str(
                "CUDA exact evaluator requires the standard 19-hex, 54-vertex, 72-edge topology",
            ),
            Self::BatchTooLarge => formatter.write_str("CUDA exact evaluator batch is too large"),
            Self::InvalidOutput {
                state,
                player,
                value,
            } => write!(
                formatter,
                "CUDA exact evaluator returned a non-finite output at state {state}, player {player}: {value}"
            ),
            Self::TerminalOutputMismatch { state, winner } => write!(
                formatter,
                "CUDA exact evaluator returned a non-exact terminal output at state {state} (winner {winner})"
            ),
            Self::Cuda(error) => write!(formatter, "CUDA driver error: {error}"),
            Self::Nvrtc(error) => write!(formatter, "NVRTC compile error: {error}"),
        }
    }
}

impl std::error::Error for CudaExactError {}

impl From<DriverError> for CudaExactError {
    fn from(error: DriverError) -> Self {
        Self::Cuda(error)
    }
}

impl From<CompileError> for CudaExactError {
    fn from(error: CompileError) -> Self {
        Self::Nvrtc(error)
    }
}

pub struct CudaExactEvaluator {
    // These owners intentionally remain resident: the stream/function and
    // device allocations borrow the CUDA context/module lifetime indirectly.
    _context: Arc<CudaContext>,
    _module: Arc<CudaModule>,
    stream: Arc<cudarc::driver::CudaStream>,
    kernel: CudaFunction,
    topology_device: CudaSlice<u32>,
    state_device: CudaSlice<u32>,
    output_device: CudaSlice<f32>,
    packed_host: Vec<u32>,
    output_host: Vec<f32>,
    capacity: usize,
    identity: CudaDeviceIdentity,
    stats: CudaExactStats,
}

impl CudaExactEvaluator {
    pub fn new() -> Result<Self, CudaExactError> {
        Self::new_on_device(0)
    }

    pub fn new_on_device(ordinal: usize) -> Result<Self, CudaExactError> {
        if crate::model::learned_value_promoted() {
            return Err(CudaExactError::LearnedValuePromoted);
        }

        // Tile labels are irrelevant here; this board supplies only the immutable
        // base topology. Keep legacy V1 explicit until topology has its own type.
        let topology_board = Board::randomized_base_v1(0, 4);
        let topology_host = topology_words(&topology_board)?;
        let context = CudaContext::new(ordinal)?;
        // CompileOptions::default intentionally leaves fast math, flush-to-zero,
        // approximate division, and approximate square root disabled.
        let ptx = compile_ptx_with_opts(CUDA_SOURCE, CompileOptions::default())?;
        let module = context.load_module(ptx)?;
        let kernel = module.load_function("evaluate_batch_kernel")?;
        let stream = context.default_stream();
        let topology_device = stream.clone_htod(&topology_host)?;
        let state_device = stream.alloc_zeros(MAX_BATCH_CAPACITY * STATE_WORDS)?;
        let output_device = stream.alloc_zeros(MAX_BATCH_CAPACITY * MAX_PLAYERS)?;
        let identity = CudaDeviceIdentity {
            backend: BACKEND_NAME,
            ordinal: context.ordinal(),
            name: context.name()?,
            compute_capability: context.compute_capability()?,
        };

        Ok(Self {
            _context: context,
            _module: module,
            stream,
            kernel,
            topology_device,
            state_device,
            output_device,
            packed_host: Vec::with_capacity(MAX_BATCH_CAPACITY * STATE_WORDS),
            output_host: Vec::with_capacity(MAX_BATCH_CAPACITY * MAX_PLAYERS),
            capacity: MAX_BATCH_CAPACITY,
            identity,
            stats: CudaExactStats::default(),
        })
    }

    pub fn backend(&self) -> &'static str {
        self.identity.backend
    }

    pub fn device_identity(&self) -> &CudaDeviceIdentity {
        &self.identity
    }

    pub fn stats(&self) -> CudaExactStats {
        self.stats
    }

    pub fn reset_stats(&mut self) {
        self.stats = CudaExactStats::default();
    }

    pub fn evaluate_batch(
        &mut self,
        states: &[GameState],
    ) -> Result<Vec<[f32; 4]>, CudaExactError> {
        let batch_started = Instant::now();
        let pack_started = Instant::now();
        let packed = states
            .iter()
            .map(CudaExactPackedState::new)
            .collect::<Result<Vec<_>, _>>()?;
        let pack_nanos = pack_started.elapsed().as_nanos();
        self.evaluate_packed_batch_inner(&packed, batch_started, pack_nanos)
    }

    pub fn evaluate_packed_batch(
        &mut self,
        states: &[CudaExactPackedState],
    ) -> Result<Vec<[f32; 4]>, CudaExactError> {
        self.evaluate_packed_batch_inner(states, Instant::now(), 0)
    }

    pub fn evaluate_packed_batch_into(
        &mut self,
        states: &[CudaExactPackedState],
        result: &mut Vec<[f32; 4]>,
    ) -> Result<(), CudaExactError> {
        self.evaluate_packed_batch_into_inner(states, Instant::now(), 0, result)
    }

    fn evaluate_packed_batch_inner(
        &mut self,
        states: &[CudaExactPackedState],
        batch_started: Instant,
        pack_nanos: u128,
    ) -> Result<Vec<[f32; 4]>, CudaExactError> {
        let mut result = Vec::with_capacity(states.len());
        self.evaluate_packed_batch_into_inner(states, batch_started, pack_nanos, &mut result)?;
        Ok(result)
    }

    fn evaluate_packed_batch_into_inner(
        &mut self,
        states: &[CudaExactPackedState],
        batch_started: Instant,
        pack_nanos: u128,
        result: &mut Vec<[f32; 4]>,
    ) -> Result<(), CudaExactError> {
        result.clear();
        if states.is_empty() {
            return Ok(());
        }
        if states.len() > u32::MAX as usize {
            return Err(CudaExactError::BatchTooLarge);
        }

        self.packed_host.clear();
        let packed_words = states.len() * STATE_WORDS;
        if self.packed_host.capacity() < packed_words {
            self.packed_host.reserve(packed_words);
        }
        for state in states {
            self.packed_host.extend_from_slice(&state.words);
        }

        self.ensure_capacity(states.len())?;

        let upload_started = Instant::now();
        self.stream
            .memcpy_htod(&self.packed_host, &mut self.state_device)?;
        let upload_nanos = upload_started.elapsed().as_nanos();

        let kernel_started = Instant::now();
        {
            let batch = states.len() as u32;
            let config = LaunchConfig {
                grid_dim: (states.len().div_ceil(THREADS_PER_BLOCK) as u32, 1, 1),
                block_dim: (THREADS_PER_BLOCK as u32, 1, 1),
                shared_mem_bytes: 0,
            };
            let mut arguments = self.stream.launch_builder(&self.kernel);
            arguments.arg(&self.state_device);
            arguments.arg(&self.topology_device);
            arguments.arg(&mut self.output_device);
            arguments.arg(&batch);
            unsafe { arguments.launch(config)? };
            self.stream.synchronize()?;
        }
        let kernel_nanos = kernel_started.elapsed().as_nanos();

        let download_started = Instant::now();
        let output_values = states.len() * MAX_PLAYERS;
        self.output_host.resize(output_values, 0.0);
        {
            let output = self.output_device.slice(0..output_values);
            self.stream.memcpy_dtoh(&output, &mut self.output_host)?;
        }
        self.stream.synchronize()?;
        let download_nanos = download_started.elapsed().as_nanos();

        if result.capacity() < states.len() {
            result.reserve(states.len());
        }
        for (index, (state, values)) in states
            .iter()
            .zip(self.output_host.chunks_exact(MAX_PLAYERS))
            .enumerate()
        {
            for (player, value) in values.iter().copied().enumerate() {
                if !value.is_finite() {
                    return Err(CudaExactError::InvalidOutput {
                        state: index,
                        player,
                        value,
                    });
                }
            }
            if let Some(winner) = state.winner {
                let winner = winner as usize;
                if values
                    .iter()
                    .enumerate()
                    .any(|(player, value)| *value != if player == winner { 1.0 } else { 0.0 })
                {
                    return Err(CudaExactError::TerminalOutputMismatch {
                        state: index,
                        winner,
                    });
                }
            }
            result.push([values[0], values[1], values[2], values[3]]);
        }

        let batch_nanos = batch_started.elapsed().as_nanos();
        self.stats.batches = self.stats.batches.saturating_add(1);
        self.stats.states = self.stats.states.saturating_add(states.len() as u64);
        self.stats.last_batch_size = states.len();
        self.stats.last_batch_nanos = batch_nanos;
        self.stats.last_pack_nanos = pack_nanos;
        self.stats.last_upload_nanos = upload_nanos;
        self.stats.last_kernel_nanos = kernel_nanos;
        self.stats.last_download_nanos = download_nanos;
        self.stats.total_pack_nanos = self.stats.total_pack_nanos.saturating_add(pack_nanos);
        self.stats.total_upload_nanos = self.stats.total_upload_nanos.saturating_add(upload_nanos);
        self.stats.total_kernel_nanos = self.stats.total_kernel_nanos.saturating_add(kernel_nanos);
        self.stats.total_download_nanos = self
            .stats
            .total_download_nanos
            .saturating_add(download_nanos);
        self.stats.total_nanos = self.stats.total_nanos.saturating_add(batch_nanos);
        Ok(())
    }

    fn ensure_capacity(&mut self, required: usize) -> Result<(), CudaExactError> {
        if required <= self.capacity {
            return Ok(());
        }
        let mut capacity = self.capacity.max(1);
        while capacity < required {
            capacity = capacity
                .checked_mul(2)
                .ok_or(CudaExactError::BatchTooLarge)?;
        }
        self.state_device = self.stream.alloc_zeros(capacity * STATE_WORDS)?;
        self.output_device = self.stream.alloc_zeros(capacity * MAX_PLAYERS)?;
        self.capacity = capacity;
        Ok(())
    }
}

fn pack_state_words(
    state: &GameState,
    words: &mut [u32; STATE_WORDS],
) -> Result<(), CudaExactError> {
    let board = state.board.as_ref();
    let players = board.num_players as usize;
    if !(2..=MAX_PLAYERS).contains(&players) || state.players.len() != players {
        return Err(CudaExactError::UnsupportedState(
            "only 2–4 player states are supported",
        ));
    }
    if board.hexes.len() != HEX_COUNT
        || board.vertices.len() != VERTEX_COUNT
        || board.edges.len() != EDGE_COUNT
    {
        return Err(CudaExactError::TopologyMismatch);
    }
    // `new_on_device` uploads the canonical standard topology once. The
    // arena's state generator preserves that immutable graph topology;
    // only the per-seed hex labels and ports are packed below. Keep this
    // hot path allocation-free and reserve topology validation for the
    // evaluator construction boundary.

    words[STATE_NUM_PLAYERS] = players as u32;
    words[STATE_PHASE] = phase_tag(state.phase);
    words[STATE_CURRENT_PLAYER] = state.current_player as u32;
    words[STATE_ROBBER_HEX] = state.robber_hex as u32;
    words[STATE_VICTORY_TARGET] = state.victory_target as u32;
    words[STATE_DISCARD_LIMIT] = state.card_discard_limit as u32;
    words[STATE_BANK_PUBLIC] = u32::from(state.bank_is_public);
    words[STATE_DOMESTIC_TRADE_DISABLED] = if state.player_trades_enabled {
        u32::from(state.domestic_trade_disabled)
    } else { (1u32 << players) - 1 };
    words[STATE_LONGEST_HOLDER] = holder_code(state.longest_road_holder);
    words[STATE_LARGEST_HOLDER] = holder_code(state.largest_army_holder);

    for resource in 0..5 {
        words[STATE_BANK + resource] = state.bank[resource] as u32;
    }
    for card in 0..5 {
        words[STATE_DEVELOPMENT_DECK + card] = state.development_deck[card] as u32;
        words[STATE_PLAYED_DEVELOPMENT + card] = state.played_development[card] as u32;
    }
    for (hex, tile) in board.hexes.iter().enumerate() {
        words[STATE_HEX_RESOURCES + hex] = tile
            .resource
            .map(|resource| resource as u32 + 1)
            .unwrap_or(0);
        words[STATE_HEX_NUMBERS + hex] = tile.number as u32;
    }
    for (vertex, data) in board.vertices.iter().enumerate() {
        words[STATE_PORTS + vertex] = match data.port {
            None => 0,
            Some(Port::Generic) => 1,
            Some(Port::Resource(resource)) => resource as u32 + 2,
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
    }
    Ok(())
}

fn topology_words(board: &Board) -> Result<Vec<u32>, CudaExactError> {
    if board.vertices.len() != VERTEX_COUNT || board.edges.len() != EDGE_COUNT {
        return Err(CudaExactError::TopologyMismatch);
    }
    let mut words = vec![0u32; TOPOLOGY_WORDS];
    for (vertex, data) in board.vertices.iter().enumerate() {
        if data.adjacent_hexes.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_vertices.len() > MAX_VERTEX_ADJACENCY
            || data.adjacent_edges.len() > MAX_VERTEX_ADJACENCY
        {
            return Err(CudaExactError::UnsupportedState(
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
    if words.is_empty() {
        return Err(CudaExactError::EmptyStandardTopology);
    }
    Ok(words)
}

fn phase_tag(phase: Phase) -> u32 {
    match phase {
        Phase::SetupSettlement => 0,
        Phase::SetupRoad { .. } => 1,
        Phase::PreRoll => 2,
        Phase::RollChance => 3,
        Phase::Discard => 4,
        Phase::MoveRobber => 5,
        Phase::ResolveSteal { .. } => 6,
        Phase::Main => 7,
        Phase::DevelopmentChance => 8,
        Phase::TradeResponses => 9,
        Phase::Finished => 10,
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
