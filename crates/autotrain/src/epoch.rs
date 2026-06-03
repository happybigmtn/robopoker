//! Epoch metadata table schema
//!
//! `epoch` is a single-row key/value table tracking the current
//! training epoch. The schema is intentionally minimal — there is at
//! most one row keyed `'current'` — and the `truncates()` operation
//! issues an `UPDATE` (not a real `TRUNCATE`) because the semantic of
//! "reset the epoch counter" must preserve the row shape for the
//! key/value contract.

/// Newtype wrapper for epoch counter (enables Schema implementation).
pub struct EpochMeta;

impl rbp_database::Schema for EpochMeta {
    fn name() -> &'static str {
        rbp_database::EPOCH
    }
    fn creates() -> &'static str {
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            rbp_database::EPOCH,
            " (
                key   TEXT PRIMARY KEY,
                value BIGINT NOT NULL
            );
            INSERT INTO ",
            rbp_database::EPOCH,
            " (key, value)
            VALUES ('current', 0)
            ON CONFLICT (key) DO NOTHING;"
        )
    }
    fn indices() -> &'static str {
        // `epoch` is a single-row key/value table (the `'current'`
        // key is the only meaningful row). The `key` PRIMARY KEY in
        // `creates()` already covers the lookup, and there is no
        // range-scan or filter-by-value query in the autotrain path,
        // so no additional indices are required.
        ""
    }
    fn copy() -> &'static str {
        // Column order MUST match `columns()` below. `epoch` has no
        // `Streamable` impl — the single row is upserted at table
        // creation time and updated by `Mode::reset` — so this
        // statement is not actually executed today. We still emit a
        // well-formed `COPY` header so the trait compiles and a
        // future `Streamable` impl (e.g. for checkpoint metadata
        // import) can use it without panic.
        const_format::concatcp!(
            "COPY ",
            rbp_database::EPOCH,
            " (key, value) FROM STDIN BINARY"
        )
    }
    fn truncates() -> &'static str {
        // The "truncate" semantic for the epoch counter is to reset
        // the `'current'` value to 0 without dropping the row — the
        // table shape (single row, key PRIMARY KEY) must survive the
        // reset so subsequent `Mode::status` reads can rely on the
        // key existing. Issuing a real `TRUNCATE` would force the
        // next status call to handle a missing row, so an `UPDATE`
        // is both more efficient and more correct for this table.
        const_format::concatcp!(
            "UPDATE ",
            rbp_database::EPOCH,
            " SET value = 0 WHERE key = 'current'"
        )
    }
    fn freeze() -> &'static str {
        // `epoch` is a 1-row table that is UPDATEd on every
        // `Mode::reset` and read on every `Mode::status` and
        // `FastSession::sync`. Disabling autovacuum here would let
        // dead tuples from the UPDATEs bloat the table past the
        // single-row design, so we keep autovacuum enabled and only
        // pin `fillfactor = 100` (the steady state has exactly one
        // live tuple and the HOT update path is unaffected).
        const_format::concatcp!(
            "ALTER TABLE ",
            rbp_database::EPOCH,
            " SET (fillfactor = 100);"
        )
    }
    fn columns() -> &'static [tokio_postgres::types::Type] {
        // Binary COPY column type list — must match the column order
        // in `copy()`. Two columns: `key` (TEXT) and `value` (BIGINT).
        &[
            tokio_postgres::types::Type::TEXT,
            tokio_postgres::types::Type::INT8,
        ]
    }
}

#[cfg(test)]
mod schema_tests {
    //! Unit tests for the `EpochMeta` [`Schema`] contract.
    //!
    //! Pure-string guards on `copy` / `truncates` / `freeze` so a
    //! refactor that drops a column, drops the table name, or breaks
    //! the COPY column arity fails CI before it ever reaches a live
    //! Postgres. No database connection required.
    use super::EpochMeta;
    use rbp_database::Schema;

    #[test]
    fn copy_targets_epoch_table() {
        let sql = EpochMeta::copy();
        assert!(
            sql.contains("epoch"),
            "copy() must reference the epoch table; got: {sql}"
        );
        assert!(
            sql.contains("FROM STDIN BINARY"),
            "copy() must use the binary COPY protocol; got: {sql}"
        );
    }

    #[test]
    fn copy_column_arity_matches_columns_helper() {
        // The columns listed in the COPY header must match the
        // `columns()` arity byte-for-byte, otherwise a future binary
        // stream would silently desync from the server.
        let sql = EpochMeta::copy();
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
            EpochMeta::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            EpochMeta::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_resets_current_value() {
        // The "truncate" semantic for the single-row key/value
        // counter is an UPDATE that zeroes the value column without
        // dropping the row. A real `TRUNCATE` would force downstream
        // `Mode::status` reads to handle a missing row.
        let sql = EpochMeta::truncates();
        assert!(
            sql.contains("UPDATE"),
            "truncates() must issue UPDATE to preserve the key row; got: {sql}"
        );
        assert!(
            sql.contains("epoch"),
            "truncates() must target the epoch table; got: {sql}"
        );
        assert!(
            sql.contains("'current'"),
            "truncates() must scope the reset to the 'current' key; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_but_keeps_autovacuum() {
        // The epoch table is UPDATEd on every reset, so autovacuum
        // must stay enabled to reclaim the dead tuples.
        let sql = EpochMeta::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            !sql.contains("autovacuum_enabled"),
            "freeze() must NOT disable autovacuum for the UPDATE-heavy epoch table; got: {sql}"
        );
        assert!(
            sql.contains("epoch"),
            "freeze() must target the epoch table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(EpochMeta::name(), rbp_database::EPOCH);
    }
}
