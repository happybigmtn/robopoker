//! Player implementations for the game room.
//!
//! Concrete types implementing the `Player` trait,
//! providing different decision-making behaviors.
//!
//! ## Implementations
//!
//! - [`Fish`] — Random player for testing and simulation
//! - [`EquityBot`] — Rule-based named baseline (Monte Carlo equity + threshold table)
//! - [`Human`] — Interactive player receiving input via channel (requires `cli` feature)
//! - [`DatabasePlayer`] — Compute player using blueprint lookup only (requires `database` feature)
//! - [`RealTimePlayer`] — Compute player using real-time subgame solving
//! - [`ZeroTempPlayer`] — Compute player using subgame solving with argmax selection
#[cfg(feature = "database")]
mod database;
mod equitybot;
mod fish;
#[cfg(feature = "cli")]
mod human;
mod preflopbot;
mod realtime;
mod zerotemp;

#[cfg(feature = "database")]
pub use database::*;
pub use equitybot::*;
pub use fish::*;
#[cfg(feature = "cli")]
pub use human::*;
pub use preflopbot::*;
pub use realtime::*;
pub use zerotemp::*;
