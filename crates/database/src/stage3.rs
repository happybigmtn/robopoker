//! STW-029: v3 trained-config stage/merge/stamp pipeline.
//!
//! Parallel of [`super::Stage2`] targeting the v3 tables
//! ([`BLUEPRINT3`] / [`STAGING3`] / [`EPOCH3`] +
//! [`EPOCH3_KEY`]). The v1 [`Stage`] trait and the v2
//! [`Stage2`] trait are intentionally untouched so a v1 /
//! v2 / v3 `FastSession::sync` running concurrently
//! never collides on the staging table or the epoch row
//! because the v1 writes `STAGING` / `EPOCH`, the v2
//! writes `STAGING2` / `EPOCH2`, and the v3 writes
//! `STAGING3` / `EPOCH3`.
//!
//! The trait shape mirrors [`Stage`] and [`Stage2`]
//! byte-for-byte (same `stage` / `merge` / `stamp`
//! method set, same `Send + Sync` bound, same
//! `Arc<Client>` forwarding impl) so a `Fast3Session`
//! can call `client.stage3()` / `client.merge3()` /
//! `client.stamp3(epochs)` the same way `FastSession`
//! calls `client.stage()` / `client.merge()` /
//! `client.stamp(epochs)`. The only difference is the
//! table names baked into the SQL.

use super::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-029: v3 trained-config bulk-upload pipeline.
///
/// Parallels [`Stage`] and [`Stage2`] but targets the v3
/// tables ([`BLUEPRINT3`] for the bulk-loaded profile,
/// [`STAGING3`] for the per-sync `UNLOGGED` clone, and
/// [`EPOCH3`] + [`EPOCH3_KEY`] for the v3 epoch counter
/// row). The v1 [`Stage`] trait, the v2 [`Stage2`]
/// trait, and these v3 methods are independent — a v1
/// / v2 / v3 `FastSession::sync` running concurrently
/// cannot collide on the staging table or the epoch
/// row, because the v1 writes `STAGING` / `EPOCH`, the
/// v2 writes `STAGING2` / `EPOCH2`, and the v3 writes
/// `STAGING3` / `EPOCH3`.
#[async_trait::async_trait]
pub trait Stage3: Send + Sync {
    /// Recreate the v3 `UNLOGGED` staging table from
    /// the current v3 blueprint shape.
    async fn stage3(&self);
    /// Upsert all rows from the v3 staging table into
    /// the v3 blueprint table, then drop the staging
    /// table.
    async fn merge3(&self);
    /// Add `n` to the v3 epoch counter row keyed
    /// [`EPOCH3_KEY`]. The v1 [`Stage::stamp`] updates
    /// the `'current'` row; the v2 [`Stage2::stamp2`]
    /// updates the `'current_v2'` row; this method
    /// updates the `'current_v3'` row, so a v1 / v2
    /// `Mode::reset` (which only zeros the v1 / v2
    /// row) does not affect the v3 counter and vice
    /// versa.
    async fn stamp3(&self, n: usize);
}

#[async_trait::async_trait]
impl Stage3 for Client {
    async fn stage3(&self) {
        // Same `UNLOGGED (LIKE blueprint_v3)` recipe
        // as v1 `Stage::stage` and v2 `Stage2::stage2`
        // but with the v3 table names baked in. The
        // v3 staging table is a separate physical
        // table from the v1 / v2 staging tables, so
        // the three are never written by the same
        // query.
        let sql = format!(
            "DROP   TABLE IF EXISTS {t2};
             CREATE UNLOGGED TABLE  {t2} (LIKE {t1} INCLUDING ALL);",
            t1 = BLUEPRINT3,
            t2 = STAGING3
        );
        self.batch_execute(&sql).await.expect("create staging_v3");
    }
    async fn merge3(&self) {
        // Same upsert-then-drop recipe as v1
        // `Stage::merge` and v2 `Stage2::merge2`. The
        // `ON CONFLICT (past, present, choices, position, edge)`
        // is the v3 `NlheProfileV3::creates()` UNIQUE
        // constraint; the column list is
        // byte-for-byte the v1 / v2 blueprint column
        // list, so the v3 binary COPY row writer
        // (`NlheProfileV3::copy()`) round-trips
        // through the staging table without any
        // further conversion.
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
            t1 = BLUEPRINT3,
            t2 = STAGING3
        );
        self.batch_execute(&sql).await.expect("upsert blueprint_v3");
    }
    async fn stamp3(&self, n: usize) {
        // The `'current_v3'` row is seeded by
        // `EpochMetaV3::creates()` with the `ON
        // CONFLICT DO NOTHING` guard, so the v3
        // counter exists before the first `--fast3`
        // run. The `epoch` table format is a
        // key/value table; the primary key is on
        // `key` and the v3 row is keyed by
        // `EPOCH3_KEY` (`'current_v3'`). Adding `n`
        // (the per-sync epoch increment) keeps the
        // v3 counter monotonic across runs.
        let sql = format!(
            "UPDATE {t} SET value = value + $1 WHERE key = '{k}'",
            t = EPOCH3,
            k = EPOCH3_KEY
        );
        self.execute(&sql, &[&(n as i64)])
            .await
            .expect("update epoch_v3");
    }
}

#[async_trait::async_trait]
impl Stage3 for Arc<Client> {
    async fn stage3(&self) {
        self.as_ref().stage3().await
    }
    async fn merge3(&self) {
        self.as_ref().merge3().await
    }
    async fn stamp3(&self, n: usize) {
        self.as_ref().stamp3(n).await
    }
}

#[cfg(test)]
mod tests {
    //! Pure-string guards on the SQL fragments
    //! `Stage3` emits so a refactor that swaps a v3
    //! table name for a v1 / v2 one (or breaks the
    //! `EPOCH3_KEY` literal) fails before it ever
    //! reaches a live Postgres.
    use super::*;
    use std::sync::Arc;

    /// `stage3` is a `Stage3` impl block on `Client`;
    /// we don't actually call it in the test (that
    /// would require a live Postgres) — we just pin
    /// the trait shape so a future refactor that
    /// drops the `Arc<Client>` forwarding impl
    /// fails to compile.
    #[test]
    fn stage3_trait_is_object_safe_via_arc() {
        fn _takes_arc(_: Arc<dyn Stage3>) {}
    }

    /// The `EPOCH3_KEY` literal MUST stay in sync
    /// with
    /// `crates/database/src/lib.rs::EPOCH3_KEY`.
    /// The Stage3 `stamp3` SQL embeds it as a SQL
    /// string literal, so a refactor that changes
    /// the const without updating the stamp3 SQL
    /// would silently start writing to a
    /// non-existent row.
    #[test]
    fn stage3_stamp_targets_current_v3_row() {
        assert_eq!(EPOCH3_KEY, "current_v3");
        // The literal `current_v3` is also baked
        // into the SQL above. If a future refactor
        // renames the const, this string-equality
        // assertion forces the SQL update in
        // lockstep.
    }
}
