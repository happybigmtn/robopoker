//! No-Limit Hold'em specific CFR implementation.
//!
//! This module provides the concrete types needed to apply MCCFR to
//! No-Limit Texas Hold'em poker. It implements the abstract CFR traits
//! with poker-specific game rules, information abstraction, and betting.
//!
//! # Architecture
//!
//! This module serves as a bridge between `gameplay` (core poker) and `mccfr`
//! (generic CFR). Type aliases (`NlheEdge`, `NlheTurn`, etc.) make explicit
//! which gameplay types are being used for CFR, preparing for potential
//! crate separation into `nlhe`, `mccfr`, and `nlhe-mccfr`.
//!
//! # Components
//!
//! - [`NlheEdge`] — Discretized betting action (type alias for `Edge`)
//! - [`NlheTurn`] — Player indicator (type alias for `Turn`)
//! - [`NlheGame`] — Game state (type alias for `Game`)
//! - [`NlheSecret`] — Private state (type alias for `Abstraction`)
//! - [`NlhePublic`] — Public state: street + current-street edges
//! - [`Info`] — Information set: public + private state
//! - [`NlheEncoder`] — Maps game states to [`Info`] using clustering
//! - [`NlheProfile`] — Stores accumulated regrets and strategies
//! - [`NlheSolver`] — Generic solver combining encoder and profile
//! - [`Flagship`](crate::Flagship) — Pluribus-configured solver (top-level alias)
//!
//! # Abstraction
//!
//! The key challenge in poker CFR is the enormous state space. This module
//! uses strategic abstraction via the [`Isomorphism`] to [`Abstraction`] mapping:
//! - Suit-isomorphic hands collapse equivalent situations
//! - K-means clustering groups similar equity distributions
//!
//! # Action Space
//!
//! Betting amounts are discretized into a street-dependent grid of pot-fraction
//! raise sizes (see [`Info::raises`]). This keeps the action space tractable
//! while preserving strategically important bet sizes.

mod edge;
mod encoder;
mod game;
mod info;
mod memory;
mod profile;
#[cfg(feature = "database")]
mod profile_v2;
#[cfg(feature = "database")]
mod profile_v3;
mod public;
mod record;
mod secret;
#[cfg(feature = "database")]
mod sink;
mod solver;
#[cfg(feature = "database")]
mod source;
mod strategy;
mod turn;

pub use edge::*;
pub use encoder::*;
pub use game::*;
pub use info::*;
pub use memory::*;
pub use profile::*;
#[cfg(feature = "database")]
pub use profile_v2::*;
#[cfg(feature = "database")]
pub use profile_v3::*;
pub use public::*;
pub use record::*;
pub use secret::*;
#[cfg(feature = "database")]
pub use sink::*;
pub use solver::*;
#[cfg(feature = "database")]
pub use source::*;
pub use strategy::*;
pub use turn::*;

/// Flagship NLHE solver configuration.
///
/// Uses the Pluribus algorithm configuration:
/// - [`rbp_mccfr::PluribusSampling`] — Probabilistic pruning with warm-up period
/// - [`rbp_mccfr::PluribusRegret`] — No discount for positive regrets, t/(t+1) for negative
/// - [`rbp_mccfr::LinearWeight`] — Emphasize more recent iterations in average strategy
pub type Flagship = NlheSolver<
    rbp_mccfr::PluribusRegret,   //
    rbp_mccfr::LinearWeight,     //
    rbp_mccfr::PluribusSampling, //
>;

/// STW-017: second trained config (`v5 second trained config`).
///
/// This is the v2 of the `Flagship` trio with a deliberately
/// different regret/policy combination so a v1 `Flagship` run
/// and a v2 `Flagship2` run can be compared head-to-head via
/// the `trainer --bench` harness:
///
/// - [`rbp_mccfr::DiscountedRegret`] — DCFR (α > β) instead of
///   Pluribus' no-positive-discount / `t/(t+1)`-negative
///   schedule. DCFR's faster per-iteration regret decay on
///   negative regrets is the v2's first axis of difference.
/// - [`rbp_mccfr::QuadraticWeight`] — `t²` policy weight
///   instead of `LinearWeight`'s `t^1.5`. The v2 emphasizes
///   late-iteration strategy more aggressively, which
///   concentrates the averaged policy on the most-recent
///   information set discoveries.
/// - [`rbp_mccfr::PluribusSampling`] — Same as v1; the
///   v2 difference is in the regret/policy combination, not
///   the tree-exploration scheme.
///
/// The `Flagship2` is trained to the v2 tables
/// ([`rbp_database::BLUEPRINT2`], [`rbp_database::EPOCH2`])
/// via the `trainer --fast2` mode and hydrated by
/// `DatabasePlayer2` for the bench seat-0 (so a single
/// `trainer --bench` run can seat `blueprint` vs `fish` or
/// `blueprint2` vs `preflop` without re-training). The v1
/// ([`Flagship`]) and v2 (`Flagship2`) snapshots are
/// independent and coexist in the same database.
#[cfg(feature = "database")]
pub type Flagship2 = NlheSolver<
    rbp_mccfr::DiscountedRegret, //
    rbp_mccfr::QuadraticWeight,  //
    rbp_mccfr::PluribusSampling, //
>;

/// STW-029: third trained config (`v6 third
/// DCFR-with-LinearWeight variant`).
///
/// This is the v3 of the `Flagship` trio with the
/// missing cross-product cell of the v1 / v2 regret /
/// policy combination:
///
/// - v1 `Flagship`  = `PluribusRegret` + `LinearWeight`
///   + `PluribusSampling` (STW-009 / STW-010 default).
/// - v2 `Flagship2` = `DiscountedRegret` +
///   `QuadraticWeight` + `PluribusSampling` (STW-017).
/// - v3 `Flagship3` = `DiscountedRegret` +
///   `LinearWeight` + `PluribusSampling` (STW-029;
///
/// The CEO testnet roadmap explicitly names "a third
/// DCFR-with-LinearWeight variant, or a 'named bot vs
/// second trained config' comparison" as the v6 next
/// slice after STW-017's `Flagship2` trained config;
/// STW-029 lands the third-trained-config half (the
/// comparison half shipped earlier in STW-018).
///
/// `Flagship3` is trained to the v3 tables
/// ([`rbp_database::BLUEPRINT3`],
/// [`rbp_database::EPOCH3`]) via the `trainer --fast3`
/// mode and hydrated by `DatabasePlayer3` for the bench
/// seat-0 (so a single `trainer --bench` run can seat
/// `blueprint` vs `fish`, `blueprint2` vs `preflop`, or
/// `blueprint3` vs `bluffer` without re-training). The
/// v3 regret schedule is DCFR (matches v2) but the
/// policy weight is linear (matches v1) — the v3 is
/// the missing cell in the regret × policy matrix.
#[cfg(feature = "database")]
pub type Flagship3 = NlheSolver<
    rbp_mccfr::DiscountedRegret, //
    rbp_mccfr::LinearWeight,     //
    rbp_mccfr::PluribusSampling, //
>;
