//! Optimal transport and Earth Mover's Distance computation.
//!
//! This module computes distances between probability distributions over metric
//! spaces, enabling the clustering algorithms to measure strategic similarity
//! between poker situations.
//!
//! ## Algorithms
//!
//! - [`GreenhornOptimalTransport`] — Sinkhorn-like algorithm with greedy row/column updates
//! - [`GreedyOptimalTransport`] — Fast approximate coupling via greedy matching
//!
//! ## Core Traits
//!
//! - [`Coupling`] — A transport plan between two distributions
//! - [`Density`] — A discrete probability distribution (histogram)
//! - [`Measure`] — Ground metric defining transport costs
//! - [`Support`] — The underlying metric space with pairwise distances
//!
//! ## Concrete implementations
//!
//! - [`coupling::sinkhorn::Sinkhorn`] — Log-domain Sinkhorn-Knopp
//!   entropic OT over `BTreeMap<usize, Probability>` with a
//!   [`UniformMetric`]. Converges to the entropy-regularized EMD,
//!   which approaches the exact EMD as `temperature → 0`.
//! - [`coupling::greedy::Greedy`] — Greedy bipartite matching over
//!   the same types. O(n² log n) and not optimal in general but
//!   matches the exact EMD on uniform marginals.
//! - [`UniformMetric`] — Concrete L1 metric on `usize` buckets
//!   (implements [`Measure`]).
//!
//! ## Naming note
//!
//! The concrete `Coupling` impls live under [`coupling`] (not at the
//! crate root) to avoid name clashes with downstream consumers that
//! define their own `Sinkhorn` / `Greedy` types and import this
//! crate via a glob (e.g. the `clustering` crate's
//! `clustering::Sinkhorn`). Reachable as
//! `rbp_transport::coupling::sinkhorn::Sinkhorn` and
//! `rbp_transport::coupling::greedy::Greedy`.
//!
//! ## Usage
//!
//! The Sinkhorn iterations are controlled by temperature, iteration
//! count, and convergence tolerance parameters defined in the crate
//! root. Lower temperature yields sharper transport plans at the
//! cost of numerical stability.

mod coupling;
mod density;
mod greedy;
mod greenkhorn;
mod measure;
mod support;

pub use coupling::Coupling;
pub use density::Density;
pub use greedy::GreedyOptimalTransport;
pub use greenkhorn::GreenhornOptimalTransport;
pub use measure::Measure;
pub use measure::UniformMetric;
pub use support::Support;

#[cfg(test)]
mod tests;
