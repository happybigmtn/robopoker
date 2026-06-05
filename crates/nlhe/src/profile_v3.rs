//! STW-029: schema contracts + hydration for the v3 trained
//! config (`Flagship3`).
//!
//! The v3 trained config is the *third* MCCFR run with
//! the missing cross-product cell of the v1 / v2 regret /
//! policy combination (the CEO roadmap's "third
//! DCFR-with-LinearWeight variant"):
//!
//! - v1 [`crate::Flagship`]  = `PluribusRegret` +
//!   `LinearWeight` + `PluribusSampling`.
//! - v2 [`crate::Flagship2`] = `DiscountedRegret` +
//!   `QuadraticWeight` + `PluribusSampling`.
//! - v3 [`crate::Flagship3`] = `DiscountedRegret` +
//!   `LinearWeight` + `PluribusSampling`.
//!
//! Like the v2 path, the v3 path must store its profile
//! + epoch in a separate pair of tables so v1 / v2 / v3
//! snapshots can coexist in the same database without
//! overwriting each other. This module provides the v3
//! side of the persistence contract:
//!
//! - [`NlheProfileV3`] — wraps [`crate::NlheProfile`],
//!   implements [`rbp_database::Schema`] +
//!   [`rbp_database::BulkSchema`] +
//!   [`rbp_database::Hydrate`] against
//!   [`rbp_database::BLUEPRINT3`] (the v3 blueprint
//!   table). The wrapped `NlheProfile` is byte-for-byte
//!   the same in memory as the v1 / v2; the newtype
//!   exists only to point the trait impls at the v3
//!   table name without leaking a
//!   `cfg(feature = "database")` `impl` for `NlheProfile`
//!   on the v3 path.
//! - [`EpochMetaV3`] — mirrors [`crate::EpochMetaV2`]
//!   (the v2 epoch table wrapper) targeting
//!   [`rbp_database::EPOCH3`]. The single row is keyed
//!   [`rbp_database::EPOCH3_KEY`] = `"current_v3"`, so a
//!   v1 / v2 `Mode::reset` (which only touches the v1 /
//!   v2 rows) does not zero the v3 row.
//!
//! ## Why a newtype instead of `impl Schema for NlheProfile`
//!
//! The v1 [`crate::NlheProfile`] already has a `Schema`
//! impl targeting [`rbp_database::BLUEPRINT`], and the
//! v2 [`crate::NlheProfileV2`] has a `Schema` impl
//! targeting [`rbp_database::BLUEPRINT2`]. Adding a
//! third `impl Schema for NlheProfile` on a different
//! table name would create conflicting trait impls on
//! the same type, and Rust's coherence rules forbid it.
//! A newtype wrapper is the canonical fix; it lets the
//! v3 path reuse the in-memory representation
//! (`NlheProfile`) without paying the cost of a parallel
//! data structure or a parallel `Hydrate` decode.
//!
//! ## Why the wrapped type is a tuple struct
//!
//! The newtype is `pub struct NlheProfileV3(pub NlheProfile)`
//! (not `pub struct NlheProfileV3 { profile: NlheProfile }`)
//! because the bench / trainer paths only ever need the
//! inner profile (for the `averaged_distribution(&info)`
//! call); the schema contracts consume the newtype
//! positionally. A tuple struct keeps the call site short
//! (`profile_v3.0.averaged_distribution(...)`) and the
//! `Deref`/`From` impls light.
use super::*;
use rbp_database::*;
use rbp_mccfr::Encounter;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-029: v3 trained-config profile wrapper.
///
/// Holds an [`crate::NlheProfile`] whose rows are
/// persisted to [`rbp_database::BLUEPRINT3`] (instead of
/// the v1 [`rbp_database::BLUEPRINT`] or the v2
/// [`rbp_database::BLUEPRINT2`]). Constructed either
/// from an empty default (the start of a `--fast3` run)
/// or via [`NlheProfileV3::hydrate`] (the bench's
/// `DatabasePlayer3` constructor).
#[derive(Default)]
pub struct NlheProfileV3(pub NlheProfile);

impl NlheProfileV3 {
    /// New empty v3 profile. Identical shape to
    /// [`NlheProfile::default`] — the in-memory state is
    /// the same; only the persistence target differs.
    pub fn new() -> Self {
        Self(NlheProfile::default())
    }
    /// Consume the wrapper and yield the rows iterator
    /// identical to [`NlheProfile::rows`]. Used by
    /// `Fast3Session::sync` to drive the binary `COPY`
    /// stream into [`rbp_database::BLUEPRINT3`].
    pub fn into_rows(self) -> impl Iterator<Item = (i64, i16, i64, i16, i64, f32, f32, f32, i32)> {
        self.0.rows()
    }
    /// Borrow the inner profile. `DatabasePlayer3::decide`
    /// uses this to query `averaged_distribution(&info)`,
    /// the same code path the v1 `DatabasePlayer` and v2
    /// `DatabasePlayer2` use.
    pub fn inner(&self) -> &NlheProfile {
        &self.0
    }
    /// Borrow the inner profile mutably. Used by
    /// `NlheSolver::step` -> `Profile::increment` to
    /// advance the iteration counter during a `--fast3`
    /// train.
    pub fn inner_mut(&mut self) -> &mut NlheProfile {
        &mut self.0
    }
    /// Consume the wrapper and yield the inner
    /// [`NlheProfile`]. Used by
    /// [`hydrate_flagship3`] to feed a hydrated
    /// `NlheProfileV3` into `NlheSolver::new`, which
    /// takes an owned `NlheProfile`. The v3 hydration
    /// reuses the v1 in-memory `NlheProfile` shape
    /// verbatim; the wrapper exists only to point the
    /// trait impls at the v3 table name.
    pub fn into_inner(self) -> NlheProfile {
        self.0
    }
}

#[cfg(feature = "database")]
impl Schema for NlheProfileV3 {
    fn name() -> &'static str {
        BLUEPRINT3
    }
    fn creates() -> &'static str {
        // Identical DDL to the v1 NlheProfile::creates();
        // the v3 table is a separate physical table, so
        // the index name and UNIQUE constraint can be
        // (and are) re-stated verbatim.
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            BLUEPRINT3,
            " (
                edge       BIGINT,
                past       BIGINT,
                present    SMALLINT,
                choices    BIGINT,
                position   SMALLINT,
                weight     REAL,
                regret     REAL,
                evalue     REAL,
                counts     INT DEFAULT 0,
                UNIQUE     (past, present, choices, edge, position)
            );"
        )
    }
    fn indices() -> &'static str {
        // Same five-index recipe as the v1 / v2
        // NlheProfile, with the index name prefix changed
        // to `blueprint_v3_*` so the three schemas do
        // not collide on the `idx_blueprint_*` namespace.
        const_format::concatcp!(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_blueprint_v3_upsert  ON ",
            BLUEPRINT3,
            " (present, past, choices, edge, position);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v3_bucket  ON ",
            BLUEPRINT3,
            " (present, past, choices);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v3_present ON ",
            BLUEPRINT3,
            " (present);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v3_edge    ON ",
            BLUEPRINT3,
            " (edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v3_past    ON ",
            BLUEPRINT3,
            " (past);"
        )
    }
    fn truncates() -> &'static str {
        const_format::concatcp!("TRUNCATE TABLE ", BLUEPRINT3, ";")
    }
    fn freeze() -> &'static str {
        const_format::concatcp!(
            "ALTER TABLE ",
            BLUEPRINT3,
            " SET (fillfactor = 100);
            ALTER TABLE ",
            BLUEPRINT3,
            " SET (autovacuum_enabled = false);"
        )
    }
}

#[cfg(feature = "database")]
impl BulkSchema for NlheProfileV3 {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        // Same arity + same Postgres types as the v1 /
        // v2 NlheProfile::columns(); the binary row
        // layout is identical so the row writer
        // (`NlheProfile::rows`) can be reused verbatim
        // by Fast3Session::sync.
        &[
            tokio_postgres::types::Type::INT8,   // past
            tokio_postgres::types::Type::INT2,   // present
            tokio_postgres::types::Type::INT8,   // choices
            tokio_postgres::types::Type::INT2,   // position
            tokio_postgres::types::Type::INT8,   // edge
            tokio_postgres::types::Type::FLOAT4, // weight
            tokio_postgres::types::Type::FLOAT4, // regret
            tokio_postgres::types::Type::FLOAT4, // evalue
            tokio_postgres::types::Type::INT4,   // counts
        ]
    }
    fn copy() -> &'static str {
        // COPY target is the v3 table; the column list
        // is byte-identical to the v1 / v2
        // NlheProfile::copy().
        const_format::concatcp!(
            "COPY ",
            BLUEPRINT3,
            " (past, present, choices, position, edge, weight, regret, evalue, counts) FROM STDIN BINARY"
        )
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl Hydrate for NlheProfileV3 {
    async fn hydrate(client: Arc<Client>) -> Self {
        log::info!("{:<32}{:<32}", "loading blueprint v3", "from database");
        // Read the v3 epoch row (keyed 'current_v3');
        // an empty blueprint + missing 'current_v3' row
        // is the expected post-`--reset` state and the
        // bench's `blueprint_trained: false` path
        // handles it.
        const EPOCH_SQL: &str = const_format::concatcp!(
            "SELECT value FROM ",
            EPOCH3,
            " WHERE key = '",
            EPOCH3_KEY,
            "'"
        );
        let iterations = client
            .query_opt(EPOCH_SQL, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0);
        const BLUEPRINT_SQL: &str = const_format::concatcp!(
            "SELECT past, present, choices, position, edge, weight, regret, evalue, counts FROM ",
            BLUEPRINT3
        );
        let mut encounters = std::collections::BTreeMap::new();
        if let Ok(rows) = client.query(BLUEPRINT_SQL, &[]).await {
            for row in rows {
                let subgame = rbp_gameplay::Path::from(row.get::<_, i64>(0) as u64);
                let present = rbp_gameplay::Abstraction::from(row.get::<_, i16>(1));
                let choices = rbp_gameplay::Path::from(row.get::<_, i64>(2) as u64);
                let position = row.get::<_, i16>(3) as u8;
                let edge = NlheEdge::from(row.get::<_, i64>(4) as u64);
                let weight = row.get::<_, f32>(5);
                let regret = row.get::<_, f32>(6);
                let evalue = row.get::<_, f32>(7);
                let counts = row.get::<_, i32>(8) as u32;
                let bucket = NlheInfo::from((subgame, present, choices, position));
                encounters
                    .entry(bucket)
                    .or_insert_with(std::collections::BTreeMap::default)
                    .entry(edge)
                    .or_insert(Encounter::new(weight, regret, evalue, counts));
            }
        }
        log::info!(
            "{:<32}{:<32}",
            format!("{} infos", encounters.len()),
            "from database"
        );
        log::info!(
            "{:<32}{:<32}",
            format!("{} iters", iterations),
            "from database"
        );
        // Wrap the v1 NlheProfile shape verbatim. The
        // inner `metrics` collector is re-seeded with
        // the hydrated epoch count so a `--fast3`
        // continuation resumes at the right training
        // iteration.
        let profile = NlheProfile {
            iterations,
            encounters,
            metrics: rbp_mccfr::Metrics::with_epoch(iterations),
        };
        Self(profile)
    }
}

/// STW-029: v3 epoch table wrapper.
///
/// Mirrors the v2 `EpochMetaV2` shape but targets
/// [`rbp_database::EPOCH3`] and the
/// [`rbp_database::EPOCH3_KEY`] = `"current_v3"` row. A
/// v1 / v2 `Mode::reset` only touches `'current'` /
/// `'current_v2'` and leaves `'current_v3'` untouched;
/// a [`Self::reset`] zeroes the v3 row only. The three
/// epoch counters are independent and all stay present
/// after a fresh `PreTraining::run` so a v3 bench can
/// hydrate against an empty v3 blueprint without
/// dropping the `'current_v3'` row.
pub struct EpochMetaV3;

#[cfg(feature = "database")]
impl Schema for EpochMetaV3 {
    fn name() -> &'static str {
        EPOCH3
    }
    fn creates() -> &'static str {
        // Identical DDL to the v1 EpochMeta::creates()
        // (a key/value table with the 'current_v3' row
        // seeded to 0 on first run). The `ON CONFLICT
        // DO NOTHING` is what makes the v3 path safely
        // co-exist with a v1 / v2 row: a second
        // `PreTraining::run` that re-creates the table
        // does not stomp the existing 'current_v3'
        // value.
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            EPOCH3,
            " (
                key   TEXT PRIMARY KEY,
                value BIGINT NOT NULL
            );
            INSERT INTO ",
            EPOCH3,
            " (key, value)
            VALUES ('",
            EPOCH3_KEY,
            "', 0)
            ON CONFLICT (key) DO NOTHING;"
        )
    }
    fn indices() -> &'static str {
        // `epoch_v3` is a 1-row key/value table; the
        // PRIMARY KEY already covers the lookup.
        ""
    }
    fn truncates() -> &'static str {
        // `truncate` semantic for the v3 epoch counter
        // is the same `UPDATE` shape as the v1 / v2
        // EpochMeta (we do not drop the row, we zero
        // the value, so the next `Mode::status` read
        // finds the row shape intact).
        const_format::concatcp!(
            "UPDATE ",
            EPOCH3,
            " SET value = 0 WHERE key = '",
            EPOCH3_KEY,
            "'"
        )
    }
    fn freeze() -> &'static str {
        // Same recipe as v1 / v2: pin `fillfactor = 100`
        // (the v3 epoch row is UPDATEd on every
        // `--reset` and on every Fast3Session::sync).
        const_format::concatcp!("ALTER TABLE ", EPOCH3, " SET (fillfactor = 100);")
    }
}

#[cfg(feature = "database")]
impl BulkSchema for EpochMetaV3 {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::TEXT,
            tokio_postgres::types::Type::INT8,
        ]
    }
    fn copy() -> &'static str {
        // Same column order as v1 / v2 EpochMeta::copy();
        // the epoch_v3 table is not currently
        // `Streamable` (the single row is upserted at
        // create time), so this statement is not
        // actually executed. The well-formed header
        // keeps the trait compilable so a future
        // Streamable impl can use it.
        const_format::concatcp!("COPY ", EPOCH3, " (key, value) FROM STDIN BINARY")
    }
}

/// STW-029: hydrate a [`crate::Flagship3`] solver from
/// the v3 tables (`blueprint_v3` + `epoch_v3`
/// `'current_v3'`).
///
/// The v1 [`crate::NlheSolver::hydrate`] is a blanket
/// `impl Hydrate for NlheSolver<R, W, S>` that pulls
/// from the v1 tables. The v2 `hydrate_flagship2` is a
/// type-specific free function for the v2 tables. The v3
/// `hydrate_flagship3` parallels the v2 free function
/// for the v3 tables: it can't share the v1 blanket
/// impl (which would read the v1 `BLUEPRINT` / `EPOCH`
/// tables) or the v2 free function (which reads the v2
/// tables), so the v3 hydration is exposed as a
/// type-specific free function too. The encoder is the
/// v1 [`NlheEncoder`] (the abstraction clustering is
/// the v1 recipe; only the regret / policy combination
/// differs in the v3 solver), so the v3 hydration
/// delegates to `NlheEncoder::hydrate(client)` for the
/// encoder and to `NlheProfileV3::hydrate(client)` for
/// the profile.
///
/// A v3 hydration against an empty v3 blueprint (the
/// documented post-`--reset` state) yields a default
/// `NlheProfile`; the bench's `blueprint_trained: false`
/// path handles the empty-blueprint case the same way
/// it handles the v1 / v2 empty-blueprint cases.
#[cfg(feature = "database")]
pub async fn hydrate_flagship3(client: std::sync::Arc<tokio_postgres::Client>) -> crate::Flagship3 {
    crate::Flagship3::new(
        NlheProfileV3::hydrate(client.clone()).await.into_inner(),
        NlheEncoder::hydrate(client).await,
    )
}

#[cfg(test)]
mod hydrate_tests {
    //! Pure-string guards on the v3 schema contracts so
    //! a refactor that drops a column, renames a table,
    //! or breaks the COPY column arity fails CI before
    //! it ever reaches a live Postgres.
    use super::EpochMetaV3;
    use super::NlheProfileV3;
    use rbp_database::{BLUEPRINT3, EPOCH3, EPOCH3_KEY};
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn nlhe_profile_v3_name_matches_const_table_name() {
        assert_eq!(NlheProfileV3::name(), BLUEPRINT3);
    }

    #[test]
    fn nlhe_profile_v3_creates_targets_v3_table() {
        let sql = NlheProfileV3::creates();
        assert!(
            sql.contains(BLUEPRINT3),
            "creates() must reference the v3 blueprint table; got: {sql}"
        );
        assert!(
            sql.contains("CREATE TABLE"),
            "creates() must emit CREATE TABLE; got: {sql}"
        );
    }

    #[test]
    fn nlhe_profile_v3_truncates_targets_v3_table() {
        let sql = NlheProfileV3::truncates();
        assert!(
            sql.contains("TRUNCATE"),
            "truncates() must issue TRUNCATE; got: {sql}"
        );
        assert!(
            sql.contains(BLUEPRINT3),
            "truncates() must target the v3 table; got: {sql}"
        );
    }

    #[test]
    fn nlhe_profile_v3_copy_targets_v3_table_with_matching_arity() {
        let sql = NlheProfileV3::copy();
        assert!(
            sql.contains(BLUEPRINT3),
            "copy() must reference the v3 table; got: {sql}"
        );
        assert!(
            sql.contains("FROM STDIN BINARY"),
            "copy() must use the binary COPY protocol; got: {sql}"
        );
        let parens = sql.split_once('(').expect("copy() has a column list");
        let header_cols: Vec<&str> = parens
            .1
            .split_once(')')
            .expect("copy() has a closing paren")
            .0
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .collect();
        assert_eq!(
            header_cols.len(),
            NlheProfileV3::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            NlheProfileV3::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn nlhe_profile_v3_freeze_disables_autovacuum() {
        // Like the v1 / v2 blueprint, the v3 blueprint
        // is bulk-loaded once and never modified by
        // updates outside of the (separate) epoch
        // table; so disabling autovacuum on the v3
        // blueprint is symmetric with the v1 / v2
        // recipe and saves a background vacuum on the
        // same ~3.02GB river table.
        let sql = NlheProfileV3::freeze();
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum on the v3 bulk-load table; got: {sql}"
        );
        assert!(
            sql.contains(BLUEPRINT3),
            "freeze() must target the v3 table; got: {sql}"
        );
    }

    #[test]
    fn epoch_meta_v3_name_matches_const_table_name() {
        assert_eq!(EpochMetaV3::name(), EPOCH3);
    }

    #[test]
    fn epoch_meta_v3_creates_seeds_current_v3_row() {
        let sql = EpochMetaV3::creates();
        assert!(
            sql.contains(EPOCH3),
            "creates() must reference the v3 epoch table; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH3_KEY),
            "creates() must seed the '{}' row; got: {}",
            EPOCH3_KEY,
            sql
        );
        assert!(
            sql.contains("ON CONFLICT"),
            "creates() must be idempotent (ON CONFLICT DO NOTHING); got: {sql}"
        );
    }

    /// `truncates()` on the v3 epoch table must zero the
    /// v3 `'current_v3'` row (not v1 `'current'` and not
    /// v2 `'current_v2'`). A v3 `Mode::reset` therefore
    /// does not affect a v1 / v2 trained-config
    /// continuation, and a v1 / v2 `Mode::reset` does not
    /// affect a v3 trained-config continuation. The v1 /
    /// v2 epoch tables have the same shape; this assertion
    /// pins that the v3 path does not regress the v1 / v2
    /// recipe.
    #[test]
    fn epoch_meta_v3_truncates_zeros_current_v3_value() {
        let sql = EpochMetaV3::truncates();
        assert!(
            sql.contains("UPDATE"),
            "truncates() must issue UPDATE; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH3),
            "truncates() must target the v3 epoch table; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH3_KEY),
            "truncates() must scope the reset to the '{}' key; got: {}",
            EPOCH3_KEY,
            sql
        );
    }

    /// The v3 epoch table is UPDATEd on every v3 reset
    /// and on every `Fast3Session::sync`, so autovacuum
    /// must stay enabled to reclaim the dead tuples. The
    /// v1 / v2 epoch tables have the same shape; this
    /// assertion pins that the v3 path does not regress
    /// the v1 / v2 recipe.
    #[test]
    fn epoch_meta_v3_freeze_keeps_autovacuum_enabled() {
        let sql = EpochMetaV3::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            !sql.contains("autovacuum_enabled"),
            "freeze() must NOT disable autovacuum for the UPDATE-heavy v3 epoch table; got: {sql}"
        );
    }
}
