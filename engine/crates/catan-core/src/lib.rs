//! Clean-room base-game CATAN rules and state representation.
//!
//! The crate intentionally has no browser or third-party bot dependency. Its
//! state is compact enough to copy during search and deterministic under a
//! supplied seed, which lets native tournaments and browser WASM execute the
//! same rules.

mod board;
mod rng;
mod state;
mod types;

pub use board::{Board, Edge, Hex, Vertex};
pub use rng::SplitMix64;
pub use state::{GameState, RuleError};
pub use types::{
    Action, Building, CITY_COST, DEVELOPMENT_COST, DevCard, NodeKind, Phase, PlayerState, Port,
    ROAD_COST, Resource, ResourceHand, SETTLEMENT_COST, TradeOffer,
};
