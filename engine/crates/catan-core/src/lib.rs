//! Clean-room base-game CATAN rules and state representation.
//!
//! The crate intentionally has no browser or third-party bot dependency. Its
//! state is compact enough to copy during search and deterministic under a
//! supplied seed, which lets native tournaments and browser WASM execute the
//! same rules.

mod board;
mod dice;
mod rng;
mod state;
mod types;

pub use board::{Board, Edge, Hex, SyntheticBoardGenerator, Vertex};
pub use dice::{
    BeliefPolicy, ControllerParticle, DiceHistoryProvenance, FIXED_BELIEF_MASS, M0_FAIR_IID_2D6_V1,
    MREF_COLONIST_LINKED_2024_V1, MissingRollGap, PUBLIC_HISTORY_BELIEF_V1, PublicRollObservation,
    REFERENCE_DECK_COUNTS, REFERENCE_PARTICLES, ReferenceController, StochasticBelief,
    StochasticHistoryError, StochasticModel, StochasticState,
};
pub use rng::SplitMix64;
pub use state::{GameState, RuleError};
pub use types::{
    Action, Building, CITY_COST, DEVELOPMENT_COST, DevCard, DiceMode, NodeKind, Phase, PlayerState,
    Port, ROAD_COST, Resource, ResourceHand, SETTLEMENT_COST, TradeOffer,
};
