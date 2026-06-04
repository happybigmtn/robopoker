//! Automated training pipeline orchestration.
//!
//! This module manages the complete training workflow, from checking database
//! state through clustering and blueprint generation. Supports both single-machine
//! and distributed training modes.
//!
//! ## Pipeline Stages
//!
//! 1. **Pretraining** — Generate abstractions via hierarchical clustering
//! 2. **Fast mode** — Single-machine MCCFR with in-memory profile
//! 3. **Slow mode** — Distributed workers with PostgreSQL synchronization
//!
//! ## Core Types
//!
//! - [`Trainer`] — Main entry point for training orchestration
//! - [`Mode`] — Training configuration (fast vs slow, clustering vs blueprint)
//!
//! ## Submodules
//!
//! - [`workers`] — Distributed training workers for MCCFR
mod bench;
mod epoch;
mod fast;
mod fast2;
mod fast3;
mod mode;
mod pretraining;
mod publish;
mod publish_index;
mod publish_index_remote;
mod publish_remote;
mod receipt;
mod replay;
mod slow;
mod trainer;
mod verify_bundle;
mod verify_receipt;

pub mod workers;

pub use bench::*;
pub use epoch::*;
pub use fast::*;
pub use fast2::*;
pub use fast3::*;
pub use mode::*;
pub use pretraining::*;
pub use publish::*;
pub use publish_index::*;
pub use publish_index_remote::*;
pub use publish_remote::*;
pub use receipt::*;
pub use slow::*;
pub use trainer::*;
pub use workers::*;
