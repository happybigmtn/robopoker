//! Player implementations for the game room.
//!
//! Concrete types implementing the `Player` trait,
//! providing different decision-making behaviors.
//!
//! ## Implementations
//!
//! - [`Fish`] — Random player for testing and simulation
//! - [`CallStation`] — Deterministic loose-passive baseline (check/call,
//!   never fold, never raise). STW-011 named baseline.
//! - [`Maniac`] — Deterministic loose-aggressive baseline (shove/raise
//!   when legal, call when nothing else is). STW-011 named baseline.
//! - [`Tight`] — Deterministic tight-passive baseline (fold most hands
//!   preflop, check/call postflop). STW-011 named baseline.
//! - [`Human`] — Interactive player receiving input via channel (requires `cli` feature)
//! - [`DatabasePlayer`] — Compute player using blueprint lookup only (requires `database` feature)
//! - [`RealTimePlayer`] — Compute player using real-time subgame solving
//! - [`ZeroTempPlayer`] — Compute player using subgame solving with argmax selection
mod callstation;
#[cfg(feature = "database")]
mod database;
mod fish;
#[cfg(feature = "cli")]
mod human;
mod maniac;
mod realtime;
mod tight;
mod zerotemp;

pub use callstation::*;
#[cfg(feature = "database")]
pub use database::*;
pub use fish::*;
#[cfg(feature = "cli")]
pub use human::*;
pub use maniac::*;
pub use realtime::*;
pub use tight::*;
pub use zerotemp::*;
