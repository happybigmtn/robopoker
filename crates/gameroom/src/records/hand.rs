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
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::INT8,
                tokio_postgres::types::Type::INT2,
                tokio_postgres::types::Type::INT2,
            ]
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
        fn copy() -> &'static str {
            const_format::concatcp!(
                "COPY ",
                HANDS,
                " (id, room_id, board, pot, dealer) FROM STDIN BINARY"
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
}
