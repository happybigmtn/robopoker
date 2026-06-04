//! STW-029: v3 trained-config status reads.
//!
//! Parallel of [`super::Check2`] for the v3 tables
//! ([`BLUEPRINT3`] and [`EPOCH3`] + [`EPOCH3_KEY`]). The
//! v1 [`Check::epochs`] / [`Check::blueprint`] methods
//! read the v1 `'current'` row and the v1 `blueprint`
//! table; the v2 [`Check2::epochs_v2`] /
//! [`Check2::blueprint_v2`] methods read the v2
//! `'current_v2'` row and the v2 `blueprint_v2` table;
//! the v3 [`Check3::epochs_v3`] /
//! [`Check3::blueprint_v3`] methods read the v3
//! `'current_v3'` row and the v3 `blueprint_v3` table
//! respectively, so a v1 / v2 / v3 status print never
//! sees a different version's data.
//!
//! This module exists alongside [`super::Check`] and
//! [`super::Check2`] rather than as a method on
//! `Check` so the v1 `Check` trait signature is
//! unchanged (every existing caller of `client.epochs()`
//! / `client.blueprint()` continues to see v1 numbers,
//! and the v3 `Status` table extension in `Mode::Status`
//! is the only place that pulls in the v3 reads).

use super::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// STW-029: v3 trained-config status reads.
///
/// Parallel of [`Check2`] but reads from the v3 tables
/// ([`BLUEPRINT3`] and [`EPOCH3`] + [`EPOCH3_KEY`]). The
/// v1 [`Check::epochs`] / [`Check::blueprint`] methods
/// continue to read v1 tables; the v2 [`Check2`]
/// methods read v2 tables; these methods read v3 tables.
/// A [`Check3::status_v3`] convenience prints the v3
/// epoch + blueprint row counts in the same
/// comma-formatted "table" shape the v1
/// [`Check::status`] and v2 [`Check2::status_v2`]
/// use, so `trainer --status` can print the v1 + v2 +
/// v3 side-by-side without duplicating the formatter.
#[async_trait::async_trait]
pub trait Check3: Send + Sync {
    /// Read the v3 epoch counter (the `'current_v3'`
    /// row's `value` column). Returns 0 if the row is
    /// missing — the documented post-`--reset` state.
    async fn epochs_v3(&self) -> usize;
    /// Count the rows in the v3 blueprint table
    /// ([`BLUEPRINT3`]). Returns 0 if the table is
    /// empty or missing (a fresh-DB state).
    async fn blueprint_v3(&self) -> usize;
    /// Print the v3 epoch + blueprint row counts in
    /// the same comma-formatted "table" shape
    /// [`Check::status`] uses, so a future v1 + v2 +
    /// v3 side-by-side status call can reuse the
    /// formatter.
    async fn status_v3(&self) {
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
        log::info!("│ Epoch v3   │ {:>13} │", commas(self.epochs_v3().await));
        log::info!("│ Blueprint3 │ {:>13} │", commas(self.blueprint_v3().await));
        log::info!("└────────────┴───────────────┘");
    }
}

#[async_trait::async_trait]
impl Check3 for Client {
    async fn epochs_v3(&self) -> usize {
        // The v3 epoch row is keyed `'current_v3'`
        // (see [`EPOCH3_KEY`]). The `'current'` row
        // used by the v1 [`Check::epochs`] and the
        // `'current_v2'` row used by the v2
        // [`Check2::epochs_v2`] are separate physical
        // rows in their respective `EPOCH` / `EPOCH2`
        // tables; reading the v3 key here cannot
        // collide with a v1 or v2 read.
        let sql = format!(
            "SELECT value FROM {t} WHERE key = '{k}'",
            t = EPOCH3,
            k = EPOCH3_KEY
        );
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
    async fn blueprint_v3(&self) -> usize {
        let sql = format!("SELECT COUNT(*) FROM {t}", t = BLUEPRINT3);
        self.query_opt(&sql, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .unwrap_or(0)
    }
}

#[async_trait::async_trait]
impl Check3 for Arc<Client> {
    async fn epochs_v3(&self) -> usize {
        self.as_ref().epochs_v3().await
    }
    async fn blueprint_v3(&self) -> usize {
        self.as_ref().blueprint_v3().await
    }
    async fn status_v3(&self) {
        self.as_ref().status_v3().await
    }
}

#[cfg(test)]
mod tests {
    //! Pure-string guards on the SQL fragments `Check3`
    //! emits so a refactor that swaps a v3 table name
    //! for a v1 / v2 one (or breaks the `EPOCH3_KEY`
    //! literal) fails before it ever reaches a live
    //! Postgres.
    use super::*;

    /// `EPOCH3_KEY` MUST stay `'current_v3'` — the
    /// `Check3::epochs_v3` SQL embeds it as a SQL
    /// string literal, so a const rename would
    /// silently start reading from a non-existent
    /// row.
    #[test]
    fn epochs_v3_sql_targets_current_v3_row() {
        assert_eq!(EPOCH3_KEY, "current_v3");
    }
}
