use super::*;
use rbp_auth::Member;
use rbp_cards::Hole;
use rbp_core::*;

/// Player participation in a hand.
/// Composite key: (hand_id, seat)
#[derive(Debug, Clone)]
pub struct Participant {
    hand: ID<Hand>,
    user: Option<ID<Member>>,
    seat: Position,
    hole: Hole,
    stack: Chips,
    showed: bool, // are these the same thing?
    mucked: bool, // are these the same thing?
}

impl Participant {
    pub fn new(
        hand: ID<Hand>,
        user: Option<ID<Member>>,
        seat: Position,
        hole: Hole,
        stack: Chips,
    ) -> Self {
        Self {
            hand,
            user,
            seat,
            hole,
            stack,
            showed: false,
            mucked: false,
        }
    }
    pub fn hand(&self) -> ID<Hand> {
        self.hand
    }
    pub fn user(&self) -> Option<ID<Member>> {
        self.user
    }
    pub fn seat(&self) -> Position {
        self.seat
    }
    pub fn hole(&self) -> Hole {
        self.hole
    }
    pub fn stack(&self) -> Chips {
        self.stack
    }
    pub fn showed(&self) -> bool {
        self.showed
    }
    pub fn mucked(&self) -> bool {
        self.mucked
    }
    pub fn show(&mut self) {
        self.showed = true;
    }
    pub fn muck(&mut self) {
        self.mucked = true;
    }
}

#[cfg(feature = "database")]
mod schema {
    use super::*;
    use rbp_database::*;

    impl Schema for Participant {
        fn name() -> &'static str {
            PLAYERS
        }
        fn creates() -> &'static str {
            const_format::concatcp!(
                "CREATE TABLE IF NOT EXISTS ",
                PLAYERS,
                " (
                    hand_id     UUID NOT NULL REFERENCES ",
                HANDS,
                "(id) ON DELETE CASCADE,
                    user_id     UUID REFERENCES ",
                USERS,
                "(id),
                    seat        SMALLINT NOT NULL,
                    hole        BIGINT NOT NULL,
                    stack       SMALLINT NOT NULL,
                    showed      BOOLEAN DEFAULT FALSE, -- are these the same thing?
                    mucked      BOOLEAN DEFAULT FALSE, -- are these the same thing?
                    PRIMARY KEY (hand_id, seat)
                );"
            )
        }
        fn indices() -> &'static str {
            const_format::concatcp!(
                "CREATE INDEX IF NOT EXISTS idx_players_user ON ",
                PLAYERS,
                " (user_id);"
            )
        }
        fn truncates() -> &'static str {
            const_format::concatcp!("TRUNCATE TABLE ", PLAYERS, ";")
        }
        fn freeze() -> &'static str {
            // `players` is mostly append-only — the only UPDATEs are
            // `update_showed` / `update_mucked` toggling the two
            // booleans at showdown — so the row layout is stable and
            // fillfactor=100 + disabled autovacuum is still the right
            // read-heavy tuning; the rare post-insert UPDATE is
            // unaffected.
            const_format::concatcp!(
                "ALTER TABLE ",
                PLAYERS,
                " SET (fillfactor = 100);
                 ALTER TABLE ",
                PLAYERS,
                " SET (autovacuum_enabled = false);"
            )
        }
    }

    impl BulkSchema for Participant {
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::INT2,
                tokio_postgres::types::Type::INT8,
                tokio_postgres::types::Type::INT2,
                tokio_postgres::types::Type::BOOL,
                tokio_postgres::types::Type::BOOL,
            ]
        }
        fn copy() -> &'static str {
            // Column order MUST match `columns()` above and the INSERT
            // shape in `HistoryRepository::create_player`. Composite
            // key (hand_id, seat) is preserved by the table PRIMARY KEY.
            // `user_id` is nullable (anonymous seats); `showed`/`mucked`
            // arrive as the participant's final state at flush time.
            const_format::concatcp!(
                "COPY ",
                PLAYERS,
                " (hand_id, user_id, seat, hole, stack, showed, mucked) FROM STDIN BINARY"
            )
        }
    }
}

#[cfg(all(test, feature = "database"))]
mod schema_tests {
    //! Unit tests for the `Participant` [`Schema`] contract.
    //!
    //! Pure-string guards on `copy` / `truncates` / `freeze` so a
    //! refactor that drops a column, drops the table name, or breaks
    //! the COPY column arity fails CI before it ever reaches a live
    //! Postgres. No database connection required.
    use super::Participant;
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn copy_targets_players_table() {
        let sql = Participant::copy();
        assert!(
            sql.contains("players"),
            "copy() must reference the players table; got: {sql}"
        );
        assert!(
            sql.contains("FROM STDIN BINARY"),
            "copy() must use the binary COPY protocol; got: {sql}"
        );
    }

    #[test]
    fn copy_column_arity_matches_columns_helper() {
        let sql = Participant::copy();
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
            Participant::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            Participant::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_clears_players_table() {
        let sql = Participant::truncates();
        assert!(
            sql.contains("TRUNCATE TABLE"),
            "truncates() must issue TRUNCATE TABLE; got: {sql}"
        );
        assert!(
            sql.contains("players"),
            "truncates() must target the players table; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_and_disables_autovacuum() {
        let sql = Participant::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum; got: {sql}"
        );
        assert!(
            sql.contains("players"),
            "freeze() must target the players table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(Participant::name(), rbp_database::PLAYERS);
    }
}
