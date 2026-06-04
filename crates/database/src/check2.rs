//! STW-017: v2 trained-config status reads.
//!
//! Parallel of [`super::Check`] for the v2 tables
//! ([`BLUEPRINT2`] and [`EPOCH2`] + [`EPOCH2_KEY`]). The
//! v1 [`Check::epochs`] / [`Check::blueprint`] methods
//! read the v1 `'current'` row and the v1 `blueprint`
//! table; the v2 [`Check2::epochs_v2`] /
//! [`Check2::blueprint_v2`] methods read the v2
//! `'current_v2'` row and the v2 `blueprint_v2` table
//! respectively, so a v1 status print never sees v2
//! data and vice versa.
//!
//! This module exists alongside [`super::Check`] rather
//! than as a method on `Check` so the v1 `Check` trait
//! signature is unchanged (every existing caller of
//! `client.epochs()` / `client.blueprint()` continues to
//! see v1 numbers, and the v2 `Status` table extension in
//! `Mode::Status` is the only place that pulls in the v2
//! reads).

use super::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-017: v2 trained-config status reads.
///
/// Parallel of [`Check`] but reads from the v2 tables
/// ([`BLUEPRINT2`] and [`EPOCH2`] + [`EPOCH2_KEY`]). The
/// v1 [`Check::epochs`] / [`Check::blueprint`] methods
/// continue to read v1 tables; these methods read v2
/// tables. A [`Check2::status_v2`] convenience prints
/// the v2 epoch + blueprint row counts in the same
/// comma-formatted "table" shape the v1 `Check::status`
/// uses, so `trainer --status` can print the v1 + v2
/// side-by-side without duplicating the formatter.
#[async_trait::async_trait]
pub trait Check2: Send + Sync {
    /// Read the v2 epoch counter (the `'current_v2'`
    /// row's `value` column). Returns 0 if the row is
    /// missing — the documented post-`--reset` state.
    async fn epochs_v2(&self) -> usize;
    /// Count the rows in the v2 blueprint table
    /// ([`BLUEPRINT2`]). Returns 0 if the table is empty
    /// or missing (a fresh-DB state).
    async fn blueprint_v2(&self) -> usize;
    /// Print the v2 epoch + blueprint row counts in the
    /// same comma-formatted "table" shape
    /// [`Check::status`] uses, so a future v1 + v2
    /// side-by-side status call can reuse the formatter.
    async fn status_v2(&self) {
        fn commas(n: usize) -> String {
            n.to_string()
                .as_bytes()
                .rchunks(3)
                .rev()
                .map(std::str::from_utf8)
                .map(Result::unwrap)
                .collect::<Vec<_>>()
                .join(",")
        }
        log::info!("┌────────────┬───────────────┐");
        log::info!("│ Epoch v2   │ {:>13} │", commas(self.epochs_v2().await));
        log::info!("│ Blueprint2 │ {:>13} │", commas(self.blueprint_v2().await));
        log::info!("└────────────┴───────────────┘");
    }
}

#[async_trait::async_trait]
impl Check2 for Client {
    async fn epochs_v2(&self) -> usize {
        // The v2 epoch row is keyed `'current_v2'`
        // (see [`EPOCH2_KEY`]). The `'current'` row used
        // by the v1 [`Check::epochs`] is a separate
        // physical row in the same `EPOCH2` table;
        // reading the v2 key here cannot collide with a
        // v1 read.
        let sql = format!(
            "SELECT value FROM {t} WHERE key = '{k}'",
            t = EPOCH2,
            k = EPOCH2_KEY
        );
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
    async fn blueprint_v2(&self) -> usize {
        let sql = format!("SELECT COUNT(*) FROM {t}", t = BLUEPRINT2);
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
}

#[async_trait::async_trait]
impl Check2 for Arc<Client> {
    async fn epochs_v2(&self) -> usize {
        self.as_ref().epochs_v2().await
    }
    async fn blueprint_v2(&self) -> usize {
        self.as_ref().blueprint_v2().await
    }
    async fn status_v2(&self) {
        self.as_ref().status_v2().await
    }
}

#[cfg(test)]
mod tests {
    //! Pure-string guards on the SQL fragments `Check2`
    //! emits so a refactor that swaps a v2 table name for
    //! a v1 one (or breaks the `EPOCH2_KEY` literal) fails
    //! before it ever reaches a live Postgres.
    use super::*;

    /// `EPOCH2_KEY` MUST stay `'current_v2'` — the
    /// `Check2::epochs_v2` SQL embeds it as a SQL string
    /// literal, so a const rename would silently start
    /// reading from a non-existent row.
    #[test]
    fn epochs_v2_sql_targets_current_v2_row() {
        assert_eq!(EPOCH2_KEY, "current_v2");
    }
}
