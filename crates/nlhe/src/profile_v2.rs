//! STW-017: schema contracts + hydration for the v2 trained
//! config (`Flagship2`).
//!
//! The v2 trained config is a *second* MCCFR run with a
//! different regret/policy combination (see
//! [`crate::Flagship2`]); it must store its profile + epoch in
//! a separate pair of tables so a v1 snapshot and a v2 snapshot
//! can coexist in the same database without overwriting each
//! other. This module provides the v2-side of the persistence
//! contract:
//!
//! - [`NlheProfileV2`] — wraps [`NlheProfile`], implements
//!   [`rbp_database::Schema`] + [`rbp_database::BulkSchema`] +
//!   [`rbp_database::Hydrate`] against
//!   [`rbp_database::BLUEPRINT2`] (the v2 blueprint table).
//!   The wrapped `NlheProfile` is byte-for-byte the same in
//!   memory as the v1; the newtype exists only to point the
//!   trait impls at the v2 table name without leaking a
//!   `cfg(feature = "database")` `impl` for `NlheProfile` on
//!   the v2 path.
//! - [`EpochMetaV2`] — mirrors [`crate::epoch_meta::EpochMeta`]
//!   (the v1 epoch table wrapper) targeting
//!   [`rbp_database::EPOCH2`]. The single row is keyed
//!   [`rbp_database::EPOCH2_KEY`] = `"current_v2"`, so a v1
//!   `Mode::reset` (which only touches the v1 row keyed
//!   `'current'`) does not zero the v2 row.
//!
//! ## Why a newtype instead of `impl Schema for NlheProfile`
//!
//! The v1 [`NlheProfile`] already has a `Schema` impl targeting
//! [`rbp_database::BLUEPRINT`]. Adding a second
//! `impl Schema for NlheProfile` on a different table name
//! would create conflicting trait impls on the same type, and
//! Rust's coherence rules forbid it. A newtype wrapper is the
//! canonical fix; it lets the v2 path reuse the in-memory
//! representation (`NlheProfile`) without paying the cost of a
//! parallel data structure or a parallel `Hydrate` decode.
//!
//! ## Why the wrapped type is a tuple struct
//!
//! The newtype is `pub struct NlheProfileV2(pub NlheProfile)`
//! (not `pub struct NlheProfileV2 { profile: NlheProfile }`)
//! because the bench / trainer paths only ever need the
//! inner profile (for the `averaged_distribution(&info)`
//! call); the schema contracts consume the newtype
//! positionally. A tuple struct keeps the call site short
//! (`profile_v2.0.averaged_distribution(...)`) and the
//! `Deref`/`From` impls light.
use super::*;
use rbp_database::*;
use rbp_mccfr::Encounter;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-017: v2 trained config profile wrapper.
///
/// Holds an [`NlheProfile`] whose rows are persisted to
/// [`rbp_database::BLUEPRINT2`] (instead of the v1
/// [`rbp_database::BLUEPRINT`]). Constructed either from an
/// empty default (the start of a `--fast2` run) or via
/// [`NlheProfileV2::hydrate`] (the bench's
/// `DatabasePlayer2` constructor).
#[derive(Default)]
pub struct NlheProfileV2(pub NlheProfile);

impl NlheProfileV2 {
    /// New empty v2 profile. Identical shape to
    /// [`NlheProfile::default`] — the in-memory state is the
    /// same; only the persistence target differs.
    pub fn new() -> Self {
        Self(NlheProfile::default())
    }
    /// Consume the wrapper and yield the rows iterator
    /// identical to [`NlheProfile::rows`]. Used by
    /// `Fast2Session::sync` to drive the binary `COPY`
    /// stream into [`rbp_database::BLUEPRINT2`].
    pub fn into_rows(self) -> impl Iterator<Item = (i64, i16, i64, i64, f32, f32, f32, i32)> {
        self.0.rows()
    }
    /// Borrow the inner profile. `DatabasePlayer2::decide`
    /// uses this to query `averaged_distribution(&info)`,
    /// the same code path the v1 `DatabasePlayer` uses.
    pub fn inner(&self) -> &NlheProfile {
        &self.0
    }
    /// Borrow the inner profile mutably. Used by
    /// `NlheSolver::step` -> `Profile::increment` to advance
    /// the iteration counter during a `--fast2` train.
    pub fn inner_mut(&mut self) -> &mut NlheProfile {
        &mut self.0
    }
    /// Consume the wrapper and yield the inner
    /// [`NlheProfile`]. Used by
    /// [`hydrate_flagship2`] to feed a hydrated
    /// `NlheProfileV2` into `NlheSolver::new`, which
    /// takes an owned `NlheProfile`. The v2 hydration
    /// reuses the v1 in-memory `NlheProfile` shape
    /// verbatim; the wrapper exists only to point the
    /// trait impls at the v2 table name.
    pub fn into_inner(self) -> NlheProfile {
        self.0
    }
}

#[cfg(feature = "database")]
impl Schema for NlheProfileV2 {
    fn name() -> &'static str {
        BLUEPRINT2
    }
    fn creates() -> &'static str {
        // Identical DDL to the v1 NlheProfile::creates();
        // the v2 table is a separate physical table, so the
        // index name and UNIQUE constraint can be (and are)
        // re-stated verbatim.
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            BLUEPRINT2,
            " (
                edge       BIGINT,
                past       BIGINT,
                present    SMALLINT,
                choices    BIGINT,
                weight     REAL,
                regret     REAL,
                evalue     REAL,
                counts     INT DEFAULT 0,
                UNIQUE     (past, present, choices, edge)
            );"
        )
    }
    fn indices() -> &'static str {
        // Same five-index recipe as the v1 NlheProfile, with
        // the index name prefix changed to `blueprint_v2_*` so
        // the two schemas do not collide on the
        // `idx_blueprint_*` namespace.
        const_format::concatcp!(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_blueprint_v2_upsert  ON ",
            BLUEPRINT2,
            " (present, past, choices, edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v2_bucket  ON ",
            BLUEPRINT2,
            " (present, past, choices);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v2_present ON ",
            BLUEPRINT2,
            " (present);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v2_edge    ON ",
            BLUEPRINT2,
            " (edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_v2_past    ON ",
            BLUEPRINT2,
            " (past);"
        )
    }
    fn truncates() -> &'static str {
        const_format::concatcp!("TRUNCATE TABLE ", BLUEPRINT2, ";")
    }
    fn freeze() -> &'static str {
        const_format::concatcp!(
            "ALTER TABLE ",
            BLUEPRINT2,
            " SET (fillfactor = 100);
            ALTER TABLE ",
            BLUEPRINT2,
            " SET (autovacuum_enabled = false);"
        )
    }
}

#[cfg(feature = "database")]
impl BulkSchema for NlheProfileV2 {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        // Same arity + same Postgres types as the v1
        // NlheProfile::columns(); the binary row layout is
        // identical so the row writer (`NlheProfile::rows`)
        // can be reused verbatim by Fast2Session::sync.
        &[
            tokio_postgres::types::Type::INT8,   // past
            tokio_postgres::types::Type::INT2,   // present
            tokio_postgres::types::Type::INT8,   // choices
            tokio_postgres::types::Type::INT8,   // edge
            tokio_postgres::types::Type::FLOAT4, // weight
            tokio_postgres::types::Type::FLOAT4, // regret
            tokio_postgres::types::Type::FLOAT4, // evalue
            tokio_postgres::types::Type::INT4,   // counts
        ]
    }
    fn copy() -> &'static str {
        // COPY target is the v2 table; the column list is
        // byte-identical to the v1 NlheProfile::copy().
        const_format::concatcp!(
            "COPY ",
            BLUEPRINT2,
            " (past, present, choices, edge, weight, regret, evalue, counts) FROM STDIN BINARY"
        )
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl Hydrate for NlheProfileV2 {
    async fn hydrate(client: Arc<Client>) -> Self {
        log::info!("{:<32}{:<32}", "loading blueprint v2", "from database");
        // Read the v2 epoch row (keyed 'current_v2'); an
        // empty blueprint + missing 'current_v2' row is the
        // expected post-`--reset` state and the bench's
        // `blueprint_trained: false` path handles it.
        const EPOCH_SQL: &str = const_format::concatcp!(
            "SELECT value FROM ",
            EPOCH2,
            " WHERE key = '",
            EPOCH2_KEY,
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
            "SELECT past, present, choices, edge, weight, regret, evalue, counts FROM ",
            BLUEPRINT2
        );
        let mut encounters = std::collections::BTreeMap::new();
        if let Ok(rows) = client.query(BLUEPRINT_SQL, &[]).await {
            for row in rows {
                let subgame = rbp_gameplay::Path::from(row.get::<_, i64>(0) as u64);
                let present = rbp_gameplay::Abstraction::from(row.get::<_, i16>(1));
                let choices = rbp_gameplay::Path::from(row.get::<_, i64>(2) as u64);
                let edge = NlheEdge::from(row.get::<_, i64>(3) as u64);
                let weight = row.get::<_, f32>(4);
                let regret = row.get::<_, f32>(5);
                let evalue = row.get::<_, f32>(6);
                let counts = row.get::<_, i32>(7) as u32;
                let bucket = NlheInfo::from((subgame, present, choices));
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
        // Wrap the v1 NlheProfile shape verbatim. The inner
        // `metrics` collector is re-seeded with the
        // hydrated epoch count so a `--fast2` continuation
        // resumes at the right training iteration.
        let profile = NlheProfile {
            iterations,
            encounters,
            metrics: rbp_mccfr::Metrics::with_epoch(iterations),
        };
        Self(profile)
    }
}

/// STW-017: v2 epoch table wrapper.
///
/// Mirrors the v1 `EpochMeta` shape but targets
/// [`rbp_database::EPOCH2`] and the [`rbp_database::EPOCH2_KEY`]
/// = `"current_v2"` row. A v1 `Mode::reset` only touches
/// `'current'` and leaves `'current_v2'` untouched; a
/// [`Self::reset`] zeroes the v2 row only. The two epoch
/// counters are independent and both stay present after a
/// fresh `PreTraining::run` so a v2 bench can hydrate
/// against an empty v2 blueprint without dropping the
/// `'current_v2'` row.
pub struct EpochMetaV2;

#[cfg(feature = "database")]
impl Schema for EpochMetaV2 {
    fn name() -> &'static str {
        EPOCH2
    }
    fn creates() -> &'static str {
        // Identical DDL to the v1 EpochMeta::creates() (a
        // key/value table with the 'current_v2' row seeded
        // to 0 on first run). The `ON CONFLICT DO NOTHING`
        // is what makes the v2 path safely co-exist with a
        // v1 row: a second `PreTraining::run` that
        // re-creates the table does not stomp the existing
        // 'current_v2' value.
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            EPOCH2,
            " (
                key   TEXT PRIMARY KEY,
                value BIGINT NOT NULL
            );
            INSERT INTO ",
            EPOCH2,
            " (key, value)
            VALUES ('",
            EPOCH2_KEY,
            "', 0)
            ON CONFLICT (key) DO NOTHING;"
        )
    }
    fn indices() -> &'static str {
        // `epoch_v2` is a 1-row key/value table; the
        // PRIMARY KEY already covers the lookup.
        ""
    }
    fn truncates() -> &'static str {
        // `truncate` semantic for the v2 epoch counter is
        // the same `UPDATE` shape as the v1 EpochMeta (we
        // do not drop the row, we zero the value, so the
        // next `Mode::status` read finds the row shape
        // intact).
        const_format::concatcp!(
            "UPDATE ",
            EPOCH2,
            " SET value = 0 WHERE key = '",
            EPOCH2_KEY,
            "'"
        )
    }
    fn freeze() -> &'static str {
        // Same recipe as v1: pin `fillfactor = 100` but
        // keep autovacuum enabled (the v2 epoch row is
        // UPDATEd on every `--reset` and on every
        // Fast2Session::sync, so dead tuples need
        // reclaiming).
        const_format::concatcp!("ALTER TABLE ", EPOCH2, " SET (fillfactor = 100);")
    }
}

#[cfg(feature = "database")]
impl BulkSchema for EpochMetaV2 {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::TEXT,
            tokio_postgres::types::Type::INT8,
        ]
    }
    fn copy() -> &'static str {
        // Same column order as v1 EpochMeta::copy(); the
        // epoch_v2 table is not currently `Streamable`
        // (the single row is upserted at create time), so
        // this statement is not actually executed. The
        // well-formed header keeps the trait compilable so
        // a future Streamable impl can use it.
        const_format::concatcp!("COPY ", EPOCH2, " (key, value) FROM STDIN BINARY")
    }
}

/// STW-017: hydrate a [`crate::Flagship2`] solver from the
/// v2 tables (`blueprint_v2` + `epoch_v2` `'current_v2'`).
///
/// The v1 [`crate::NlheSolver::hydrate`] is a blanket
/// `impl Hydrate for NlheSolver<R, W, S>` that pulls from
/// the v1 tables. The v2 path can't share that blanket
/// impl (it would read the v1 `BLUEPRINT` / `EPOCH`
/// tables), so the v2 hydration is exposed as a
/// type-specific free function instead. The encoder is
/// the v1 [`NlheEncoder`] (the abstraction clustering is
/// the v1 recipe; only the regret/policy combination
/// differs in the v2 solver), so the v2 hydration
/// delegates to `NlheEncoder::hydrate(client)` for the
/// encoder and to `NlheProfileV2::hydrate(client)` for
/// the profile.
///
/// A v2 hydration against an empty v2 blueprint (the
/// documented post-`--reset` state) yields a default
/// `NlheProfile`; the bench's `blueprint_trained: false`
/// path handles the empty-blueprint case the same way it
/// handles the v1 empty-blueprint case.
#[cfg(feature = "database")]
pub async fn hydrate_flagship2(client: std::sync::Arc<tokio_postgres::Client>) -> crate::Flagship2 {
    crate::Flagship2::new(
        NlheProfileV2::hydrate(client.clone()).await.into_inner(),
        NlheEncoder::hydrate(client).await,
    )
}

#[cfg(test)]
mod hydrate_tests {
    //! Pure-string guards on the v2 schema contracts so a
    //! refactor that drops a column, renames a table, or
    //! breaks the COPY column arity fails CI before it
    //! ever reaches a live Postgres.
    use super::EpochMetaV2;
    use super::NlheProfileV2;
    use rbp_database::{BLUEPRINT2, EPOCH2, EPOCH2_KEY};
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn nlhe_profile_v2_name_matches_const_table_name() {
        assert_eq!(NlheProfileV2::name(), BLUEPRINT2);
    }

    #[test]
    fn nlhe_profile_v2_creates_targets_v2_table() {
        let sql = NlheProfileV2::creates();
        assert!(
            sql.contains(BLUEPRINT2),
            "creates() must reference the v2 blueprint table; got: {sql}"
        );
        assert!(
            sql.contains("CREATE TABLE"),
            "creates() must emit CREATE TABLE; got: {sql}"
        );
    }

    #[test]
    fn nlhe_profile_v2_truncates_targets_v2_table() {
        let sql = NlheProfileV2::truncates();
        assert!(
            sql.contains("TRUNCATE"),
            "truncates() must issue TRUNCATE; got: {sql}"
        );
        assert!(
            sql.contains(BLUEPRINT2),
            "truncates() must target the v2 table; got: {sql}"
        );
    }

    #[test]
    fn nlhe_profile_v2_copy_targets_v2_table_with_matching_arity() {
        let sql = NlheProfileV2::copy();
        assert!(
            sql.contains(BLUEPRINT2),
            "copy() must reference the v2 table; got: {sql}"
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
            NlheProfileV2::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            NlheProfileV2::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn nlhe_profile_v2_freeze_disables_autovacuum() {
        // Like the v1 blueprint, the v2 blueprint is
        // bulk-loaded once and never modified by updates
        // outside of the (separate) epoch table; so
        // disabling autovacuum on the v2 blueprint is
        // symmetric with the v1 recipe and saves a
        // background vacuum on the same ~3.02GB river
        // table.
        let sql = NlheProfileV2::freeze();
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum on the v2 bulk-load table; got: {sql}"
        );
        assert!(
            sql.contains(BLUEPRINT2),
            "freeze() must target the v2 table; got: {sql}"
        );
    }

    #[test]
    fn epoch_meta_v2_name_matches_const_table_name() {
        assert_eq!(EpochMetaV2::name(), EPOCH2);
    }

    #[test]
    fn epoch_meta_v2_creates_seeds_current_v2_row() {
        let sql = EpochMetaV2::creates();
        assert!(
            sql.contains(EPOCH2),
            "creates() must reference the v2 epoch table; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH2_KEY),
            "creates() must seed the '{}' row; got: {sql}",
            EPOCH2_KEY
        );
        assert!(
            sql.contains("ON CONFLICT"),
            "creates() must be idempotent (ON CONFLICT DO NOTHING); got: {sql}"
        );
    }

    #[test]
    fn epoch_meta_v2_truncates_zeros_current_v2_value() {
        let sql = EpochMetaV2::truncates();
        assert!(
            sql.contains("UPDATE"),
            "truncates() must issue UPDATE; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH2),
            "truncates() must target the v2 epoch table; got: {sql}"
        );
        assert!(
            sql.contains(EPOCH2_KEY),
            "truncates() must scope the reset to the '{}' key; got: {sql}",
            EPOCH2_KEY
        );
    }

    #[test]
    fn epoch_meta_v2_freeze_keeps_autovacuum_enabled() {
        // The v2 epoch table is UPDATEd on every v2
        // reset and on every Fast2Session::sync, so
        // autovacuum must stay enabled to reclaim the
        // dead tuples. The v1 epoch table has the same
        // shape; this assertion pins that the v2 path
        // does not regress the v1 recipe.
        let sql = EpochMetaV2::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            !sql.contains("autovacuum_enabled"),
            "freeze() must NOT disable autovacuum for the UPDATE-heavy v2 epoch table; got: {sql}"
        );
    }
}
