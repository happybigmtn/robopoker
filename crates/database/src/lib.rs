//! Database pipeline for training artifacts.
//!
//! Bulk data movement between Rust structures and PostgreSQL, optimized for
//! the large-scale writes required during abstraction and blueprint training.
//!
//! ## Connectivity
//!
//! - [`db()`] — Establishes a database connection from `DB_URL`
//!
//! ## Serialization Traits
//!
//! - [`Schema`] — Table metadata and DDL generation
//! - [`Derive`] — INSERT statement generation for enumerable types
//! - [`Hydrate`] — Binary format decoding from rows
//! - [`Row`] — Binary row serialization for COPY protocol
//! - [`Streamable`] — Bulk data upload via COPY
//!
//! ## Core Types
//!
//! - [`Stage`] — Temporary staging table management
//! - [`Check`] — Schema validation and migration status
//!
//! ## Table Names
//!
//! Constants for all persistent entities: abstractions, blueprints,
//! metrics, hands, sessions, and more.
mod check;
mod check2;
mod check3;
mod schema;
mod stage;
mod stage2;
mod stage3;
mod traits;

pub use check::*;
pub use check2::*;
pub use check3::*;
pub use stage::*;
pub use stage2::*;
pub use stage3::*;
// schema module provides trait impls, no items to re-export
pub use traits::*;

use std::sync::Arc;
use tokio_postgres::Client;

/// Resolves the database connection URL from environment.
///
/// Checks `DB_URL` first, then falls back to `DATABASE_URL`.
/// Returns an error only when both are unset.
pub fn db_url() -> Result<String, std::env::VarError> {
    std::env::var("DB_URL").or_else(|_| std::env::var("DATABASE_URL"))
}

/// Establishes a database connection.
///
/// Connects to PostgreSQL using the `DB_URL` environment variable,
/// falling back to `DATABASE_URL` if `DB_URL` is not set.
/// Returns an `Arc<Client>` suitable for sharing across async tasks.
///
/// # Environment
///
/// Requires `DB_URL` or `DATABASE_URL` to be set
/// (e.g., `postgres://user:***@host:port/db`).
///
/// # Panics
///
/// Panics if neither `DB_URL` nor `DATABASE_URL` is set, or if
/// connection fails.
pub async fn db() -> Arc<Client> {
    log::info!("connecting to database");
    let tls = tokio_postgres::tls::NoTls;
    let url = db_url().expect("DB_URL or DATABASE_URL must be set");
    let (client, connection) = tokio_postgres::connect(&url, tls)
        .await
        .expect("database connection failed");
    tokio::spawn(connection);
    client
        .execute("SET client_min_messages TO WARNING", &[])
        .await
        .expect("set client_min_messages");
    Arc::new(client)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// DATABASE_URL fallback: when only DATABASE_URL is set,
    /// db_url() resolves to it.
    #[test]
    fn db_url_falls_back_to_database_url() {
        let old_db_url = std::env::var("DB_URL").ok();
        let old_database_url = std::env::var("DATABASE_URL").ok();

        unsafe {
            std::env::remove_var("DB_URL");
        }
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://fallback/db");
        }

        let result = db_url();

        match old_db_url {
            Some(v) => unsafe { std::env::set_var("DB_URL", v) },
            None => unsafe { std::env::remove_var("DB_URL") },
        }
        match old_database_url {
            Some(v) => unsafe { std::env::set_var("DATABASE_URL", v) },
            None => unsafe { std::env::remove_var("DATABASE_URL") },
        }

        assert_eq!(result.unwrap(), "postgres://fallback/db");
    }

    /// DB_URL precedence: when both are set, db_url() prefers DB_URL.
    #[test]
    fn db_url_prefers_db_url_when_both_set() {
        let old_db_url = std::env::var("DB_URL").ok();
        let old_database_url = std::env::var("DATABASE_URL").ok();

        unsafe {
            std::env::set_var("DB_URL", "postgres://primary/db");
        }
        unsafe {
            std::env::set_var("DATABASE_URL", "postgres://fallback/db");
        }

        let result = db_url();

        match old_db_url {
            Some(v) => unsafe { std::env::set_var("DB_URL", v) },
            None => unsafe { std::env::remove_var("DB_URL") },
        }
        match old_database_url {
            Some(v) => unsafe { std::env::set_var("DATABASE_URL", v) },
            None => unsafe { std::env::remove_var("DATABASE_URL") },
        }

        assert_eq!(result.unwrap(), "postgres://primary/db");
    }

    /// db_url() errors when neither env var is set.
    #[test]
    fn db_url_errors_when_neither_set() {
        let old_db_url = std::env::var("DB_URL").ok();
        let old_database_url = std::env::var("DATABASE_URL").ok();

        unsafe {
            std::env::remove_var("DB_URL");
        }
        unsafe {
            std::env::remove_var("DATABASE_URL");
        }

        let result = db_url();

        match old_db_url {
            Some(v) => unsafe { std::env::set_var("DB_URL", v) },
            None => unsafe { std::env::remove_var("DB_URL") },
        }
        match old_database_url {
            Some(v) => unsafe { std::env::set_var("DATABASE_URL", v) },
            None => unsafe { std::env::remove_var("DATABASE_URL") },
        }

        assert!(result.is_err());
    }
}

/// PostgreSQL error type alias.
pub type PgErr = tokio_postgres::Error;

/// Table for abstraction bucket definitions.
#[rustfmt::skip]
pub const ABSTRACTION: &str = "abstraction";
/// Table for game actions (bets, raises, folds, etc.).
#[rustfmt::skip]
pub const ACTIONS:     &str = "actions";
/// Table for MCCFR blueprint strategies (policy + regret).
#[rustfmt::skip]
pub const BLUEPRINT:   &str = "blueprint";
/// STW-017: second trained config's blueprint table. Mirrors
/// [`BLUEPRINT`] shape-for-shape; the v2 trained config
/// (`Flagship2` = `DiscountedRegret` + `QuadraticWeight` +
/// `PluribusSampling`) writes to and reads from this table so a
/// v1 `Flagship` snapshot and a v2 `Flagship2` snapshot can
/// coexist in the same database without overwriting each other.
#[rustfmt::skip]
pub const BLUEPRINT2:  &str = "blueprint_v2";
/// STW-029: third trained config's blueprint table. Mirrors
/// [`BLUEPRINT`] and [`BLUEPRINT2`] shape-for-shape; the v3
/// trained config (`Flagship3` = `DiscountedRegret` +
/// `LinearWeight` + `PluribusSampling` — the documented
/// "third DCFR-with-LinearWeight variant" the CEO testnet
/// roadmap names as the v6 next slice after STW-017's
/// `Flagship2`) writes to and reads from this table so v1 +
/// v2 + v3 trained-config snapshots can coexist in the same
/// database without overwriting each other. The v3 regret
/// schedule is DCFR (matches v2) but the policy weight is
/// linear (matches v1) — the v3 is the missing
/// cross-product cell in the v1 × v2 regret/policy matrix
/// (PluribusRegret+LinearWeight, DCFR+QuadraticWeight,
/// DCFR+LinearWeight).
#[rustfmt::skip]
pub const BLUEPRINT3:  &str = "blueprint_v3";
/// Table for training epoch metadata and progress.
#[rustfmt::skip]
pub const EPOCH:       &str = "epoch";
/// STW-017: second trained config's epoch table. Mirrors
/// [`EPOCH`] shape-for-shape; the `'current_v2'` key
/// (see [`EPOCH2_KEY`]) tracks the v2 training epoch so a
/// `--reset` of the v1 epoch does not zero the v2 epoch and
/// vice versa.
#[rustfmt::skip]
pub const EPOCH2:      &str = "epoch_v2";
/// STW-017: the single-row key for the v2 epoch table
/// ([`EPOCH2`]). Mirrors the v1 `'current'` convention so the
/// v1 and v2 `EPOCH` rows are both present after a fresh
/// `PreTraining::run` and a v1 `Mode::reset` does not stomp the
/// v2 row.
#[rustfmt::skip]
pub const EPOCH2_KEY:  &str = "current_v2";
/// STW-029: third trained config's epoch table. Mirrors
/// [`EPOCH`] and [`EPOCH2`] shape-for-shape; the
/// `'current_v3'` key (see [`EPOCH3_KEY`]) tracks the v3
/// training epoch so a `--reset` of the v1 / v2 epoch does
/// not zero the v3 epoch and vice versa.
#[rustfmt::skip]
pub const EPOCH3:      &str = "epoch_v3";
/// STW-029: the single-row key for the v3 epoch table
/// ([`EPOCH3`]). Mirrors the v1 `'current'` and v2
/// `'current_v2'` conventions so the v1 / v2 / v3
/// `EPOCH` rows are all present after a fresh
/// `PreTraining::run` and a v1 or v2 `Mode::reset` does
/// not stomp the v3 row.
#[rustfmt::skip]
pub const EPOCH3_KEY:  &str = "current_v3";
/// Table for completed poker hands.
#[rustfmt::skip]
pub const HANDS:       &str = "hands";
/// Table for isomorphism → abstraction mappings.
#[rustfmt::skip]
pub const ISOMORPHISM: &str = "isomorphism";
/// Table for pairwise abstraction distances.
#[rustfmt::skip]
pub const METRIC:      &str = "metric";
/// Table for player participation in hands.
#[rustfmt::skip]
pub const PLAYERS:     &str = "players";
/// Table for active game rooms.
#[rustfmt::skip]
pub const ROOMS:       &str = "rooms";
/// Table for user authentication sessions.
#[rustfmt::skip]
pub const SESSIONS:    &str = "sessions";
/// Table for staging data during bulk operations.
#[rustfmt::skip]
pub const STAGING:     &str = "staging";
/// STW-017: second trained config's staging table. Mirrors
/// [`STAGING`] shape-for-shape (a `UNLOGGED` clone of
/// [`BLUEPRINT2`]); the v2 `Fast2Session::sync` writes rows
/// here first and then upserts them into [`BLUEPRINT2`]. The
/// v1 ([`STAGING`]) and v2 ([`STAGING2`]) tables are
/// independent, so a v1 `FastSession::sync` in flight never
/// touches v2 data and vice versa.
#[rustfmt::skip]
pub const STAGING2:    &str = "staging_v2";
/// STW-029: third trained config's staging table. Mirrors
/// [`STAGING`] and [`STAGING2`] shape-for-shape (a
/// `UNLOGGED` clone of [`BLUEPRINT3`]); the v3
/// `Fast3Session::sync` writes rows here first and then
/// upserts them into [`BLUEPRINT3`]. The v1 / v2 / v3
/// staging tables are independent, so a v1 / v2 / v3
/// `FastSession::sync` in flight never touches another
/// version's data.
#[rustfmt::skip]
pub const STAGING3:    &str = "staging_v3";
/// Table for street-specific metadata.
#[rustfmt::skip]
pub const STREET:      &str = "street";
/// Table for abstraction transition probabilities.
#[rustfmt::skip]
pub const TRANSITIONS: &str = "transitions";
/// Table for registered user accounts.
#[rustfmt::skip]
pub const USERS:       &str = "users";
