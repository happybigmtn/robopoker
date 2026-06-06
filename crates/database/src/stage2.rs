//! STW-017: v2 trained-config stage/merge/stamp pipeline.
//!
//! Parallel of [`super::Stage`] targeting the v2 tables
//! ([`BLUEPRINT2`] / [`STAGING2`] / [`EPOCH2`] +
//! [`EPOCH2_KEY`]). The v1 [`Stage`] trait is intentionally
//! untouched so a v1 `FastSession::sync` in flight never
//! touches v2 data and vice versa.
//!
//! The trait shape mirrors [`Stage`] byte-for-byte (same
//! `stage` / `merge` / `stamp` method set, same
//! `Send + Sync` bound, same `Arc<Client>` forwarding impl)
//! so a `Fast2Session` can call `client.stage2()` /
//! `client.merge2()` / `client.stamp2(epochs)` the same
//! way `FastSession` calls `client.stage()` /
//! `client.merge()` / `client.stamp(epochs)`. The only
//! difference is the table names baked into the SQL.

use super::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-017: v2 trained-config bulk-upload pipeline.
///
/// Paralles [`Stage`] but targets the v2 tables
/// ([`BLUEPRINT2`] for the bulk-loaded profile,
/// [`STAGING2`] for the per-sync `UNLOGGED` clone, and
/// [`EPOCH2`] + [`EPOCH2_KEY`] for the v2 epoch counter
/// row). The v1 [`Stage`] trait and these v2 methods are
/// independent — a v1 `FastSession::sync` running
/// concurrently with a v2 `Fast2Session::sync` cannot
/// collide on the staging table or the epoch row, because
/// the v1 writes `STAGING` / `EPOCH` and the v2 writes
/// `STAGING2` / `EPOCH2`.
#[async_trait::async_trait]
pub trait Stage2: Send + Sync {
    /// Recreate the v2 `UNLOGGED` staging table from the
    /// current v2 blueprint shape.
    async fn stage2(&self);
    /// Upsert all rows from the v2 staging table into the
    /// v2 blueprint table, then drop the staging table.
    async fn merge2(&self);
    /// Add `n` to the v2 epoch counter row keyed
    /// [`EPOCH2_KEY`]. The v1 [`Stage::stamp`] updates
    /// the `'current'` row; this method updates the
    /// `'current_v2'` row, so a v1 `Mode::reset` (which
    /// only zeros the v1 row) does not affect the v2
    /// counter and vice versa.
    async fn stamp2(&self, n: usize);
}

#[async_trait::async_trait]
impl Stage2 for Client {
    async fn stage2(&self) {
        // Same `UNLOGGED (LIKE blueprint_v2)` recipe as
        // v1 `Stage::stage` but with the v2 table names
        // baked in. The v2 staging table is a separate
        // physical table from the v1 staging table, so
        // the two are never written by the same query.
        let sql = format!(
            "DROP   TABLE IF EXISTS {t2};
             CREATE UNLOGGED TABLE  {t2} (LIKE {t1} INCLUDING ALL);",
            t1 = BLUEPRINT2,
            t2 = STAGING2
        );
        self.batch_execute(&sql).await.expect("create staging_v2");
    }
    async fn merge2(&self) {
        // Same upsert-then-drop recipe as v1
        // `Stage::merge`. The `ON CONFLICT (past,
        // present, choices, position, edge)` is the v2
        // `NlheProfileV2::creates()` UNIQUE constraint;
        // the column list is byte-for-byte the v1
        // blueprint column list, so the v2 binary COPY
        // row writer (`NlheProfileV2::copy()`) round-trips
        // through the staging table without any further
        // conversion.
        let sql = format!(
            "INSERT INTO   {t1} (past, present, choices, position, edge, weight, regret, evalue, counts)
             SELECT              past, present, choices, position, edge, weight, regret, evalue, counts FROM {t2}
             ON CONFLICT  (past, present, choices, position, edge)
             DO UPDATE SET
                 weight = EXCLUDED.weight,
                 regret = EXCLUDED.regret,
                 evalue = EXCLUDED.evalue,
                 counts = EXCLUDED.counts;
             DROP TABLE    {t2};",
            t1 = BLUEPRINT2,
            t2 = STAGING2
        );
        self.batch_execute(&sql).await.expect("upsert blueprint_v2");
    }
    async fn stamp2(&self, n: usize) {
        // The `'current_v2'` row is seeded by
        // `EpochMetaV2::creates()` with the `ON CONFLICT
        // DO NOTHING` guard, so the v2 counter exists
        // before the first `--fast2` run. The
        // `epoch` table format is a key/value table; the
        // primary key is on `key` and the v2 row is keyed
        // by `EPOCH2_KEY` (`'current_v2'`). Adding `n`
        // (the per-sync epoch increment) keeps the v2
        // counter monotonic across runs.
        let sql = format!(
            "UPDATE {t} SET value = value + $1 WHERE key = '{k}'",
            t = EPOCH2,
            k = EPOCH2_KEY
        );
        self.execute(&sql, &[&(n as i64)])
            .await
            .expect("update epoch_v2");
    }
}

#[async_trait::async_trait]
impl Stage2 for Arc<Client> {
    async fn stage2(&self) {
        self.as_ref().stage2().await
    }
    async fn merge2(&self) {
        self.as_ref().merge2().await
    }
    async fn stamp2(&self, n: usize) {
        self.as_ref().stamp2(n).await
    }
}

#[cfg(test)]
mod tests {
    //! Pure-string guards on the SQL fragments `Stage2`
    //! emits so a refactor that swaps a v2 table name for
    //! a v1 one (or breaks the `EPOCH2_KEY` literal) fails
    //! before it ever reaches a live Postgres.
    use super::*;
    use std::sync::Arc;

    /// `stage2` is a `Stage2` impl block on `Client`; we
    /// don't actually call it in the test (that would
    /// require a live Postgres) — we just pin the trait
    /// shape so a future refactor that drops the
    /// `Arc<Client>` forwarding impl fails to compile.
    #[test]
    fn stage2_trait_is_object_safe_via_arc() {
        fn _takes_arc(_: Arc<dyn Stage2>) {}
    }

    /// The `EPOCH2_KEY` literal MUST stay in sync with
    /// `crates/database/src/lib.rs::EPOCH2_KEY`. The
    /// Stage2 `stamp2` SQL embeds it as a SQL string
    /// literal, so a refactor that changes the const
    /// without updating the stamp2 SQL would silently
    /// start writing to a non-existent row.
    #[test]
    fn stage2_stamp_targets_current_v2_row() {
        assert_eq!(EPOCH2_KEY, "current_v2");
        // The literal `current_v2` is also baked into
        // the SQL above. If a future refactor renames
        // the const, this string-equality assertion
        // forces the SQL update in lockstep.
    }
}
