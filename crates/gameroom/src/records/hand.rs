use super::*;
use rbp_cards::*;
use rbp_core::*;

/// Persistent hand record for a completed poker hand.
#[derive(Debug, Clone)]
pub struct Hand {
    id: ID<Self>,
    room: ID<Room>,
    pot: Chips,
    board: Board,
    dealer: Position,
}

impl Hand {
    pub fn new(id: ID<Self>, room: ID<Room>, board: Board, pot: Chips, dealer: Position) -> Self {
        Self {
            id,
            room,
            board,
            pot,
            dealer,
        }
    }
    pub fn room(&self) -> ID<Room> {
        self.room
    }
    pub fn board(&self) -> Board {
        self.board
    }
    pub fn pot(&self) -> Chips {
        self.pot
    }
    pub fn dealer(&self) -> Position {
        self.dealer
    }
}

impl Unique for Hand {
    fn id(&self) -> ID<Self> {
        self.id
    }
}

#[cfg(feature = "database")]
mod schema {
    use super::*;
    use rbp_database::*;

    impl Schema for Hand {
        fn name() -> &'static str {
            HANDS
        }
        fn creates() -> &'static str {
            const_format::concatcp!(
                "CREATE TABLE IF NOT EXISTS ",
                HANDS,
                " (
                    id          UUID PRIMARY KEY,
                    room_id     UUID NOT NULL REFERENCES ",
                ROOMS,
                "(id),
                    board       BIGINT NOT NULL,
                    pot         SMALLINT NOT NULL,
                    dealer      SMALLINT NOT NULL
                );"
            )
        }
        fn indices() -> &'static str {
            const_format::concatcp!(
                "CREATE INDEX IF NOT EXISTS idx_hands_room ON ",
                HANDS,
                " (room_id);"
            )
        }
        fn truncates() -> &'static str {
            // `hands` has FK references from `players` and `actions`
            // (`ON DELETE CASCADE`), and `TRUNCATE` does not fire
            // `ON DELETE`, so we must cascade explicitly or the
            // statement fails with `foreign key violation`.
            const_format::concatcp!("TRUNCATE TABLE ", HANDS, " CASCADE;")
        }
        fn freeze() -> &'static str {
            // `hands` is append-only after a hand completes (no
            // UPDATE path in `HistoryRepository`), so the row layout
            // is stable and `fillfactor = 100` + disabled autovacuum
            // is the right read-heavy tuning.
            const_format::concatcp!(
                "ALTER TABLE ",
                HANDS,
                " SET (fillfactor = 100);
                 ALTER TABLE ",
                HANDS,
                " SET (autovacuum_enabled = false);"
            )
        }
    }

    impl BulkSchema for Hand {
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::INT8,
                tokio_postgres::types::Type::INT2,
                tokio_postgres::types::Type::INT2,
            ]
        }
        fn copy() -> &'static str {
            const_format::concatcp!(
                "COPY ",
                HANDS,
                " (id, room_id, board, pot, dealer) FROM STDIN BINARY"
            )
        }
    }
}

#[cfg(all(test, feature = "database"))]
mod schema_tests {
    //! Unit tests for the `Hand` [`Schema`] contract.
    //!
    //! These tests guard the `copy` / `truncates` / `freeze` SQL
    //! strings so a refactor that drops a column, drops the table
    //! name, or breaks the COPY column arity fails CI before it ever
    //! reaches a live Postgres. They are pure string checks — no
    //! database connection required.
    use super::Hand;
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn copy_targets_hand_table() {
        let sql = Hand::copy();
        assert!(
            sql.contains("hands"),
            "copy() must reference the hands table; got: {sql}"
        );
        assert!(
            sql.contains("FROM STDIN BINARY"),
            "copy() must use the binary COPY protocol; got: {sql}"
        );
    }

    #[test]
    fn copy_column_arity_matches_columns_helper() {
        // The columns listed in the COPY header must match the
        // `columns()` arity byte-for-byte, otherwise the binary
        // stream rows would silently desync from the server.
        let sql = Hand::copy();
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
            Hand::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            Hand::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_clears_hand_table() {
        let sql = Hand::truncates();
        assert!(
            sql.contains("TRUNCATE TABLE"),
            "truncates() must issue TRUNCATE TABLE; got: {sql}"
        );
        assert!(
            sql.contains("hands"),
            "truncates() must target the hands table; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_and_disables_autovacuum() {
        let sql = Hand::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum; got: {sql}"
        );
        assert!(
            sql.contains("hands"),
            "freeze() must target the hands table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(Hand::name(), rbp_database::HANDS);
    }
}
