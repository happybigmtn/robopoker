use super::*;
use rbp_auth::Member;
use rbp_core::*;
use rbp_gameplay::*;

/// Individual action in a hand.
/// Composite key: (hand_id, seq)
#[derive(Debug, Clone)]
pub struct Play {
    seq: Epoch,
    hand: ID<Hand>,
    player: Option<ID<Member>>,
    action: Action,
}

impl Play {
    pub fn new(hand: ID<Hand>, seq: Epoch, player: Option<ID<Member>>, action: Action) -> Self {
        Self {
            hand,
            seq,
            player,
            action,
        }
    }
    pub fn seq(&self) -> Epoch {
        self.seq
    }
    pub fn hand(&self) -> ID<Hand> {
        self.hand
    }
    pub fn player(&self) -> Option<ID<Member>> {
        self.player
    }
    pub fn action(&self) -> Action {
        self.action
    }
}

#[cfg(feature = "database")]
mod schema {
    use super::*;
    use rbp_database::*;

    impl Schema for Play {
        fn name() -> &'static str {
            ACTIONS
        }
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::INT2,
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::INT4,
            ]
        }
        fn creates() -> &'static str {
            const_format::concatcp!(
                "CREATE TABLE IF NOT EXISTS ",
                ACTIONS,
                " (
                    hand_id     UUID NOT NULL REFERENCES ",
                HANDS,
                "(id) ON DELETE CASCADE,
                    seq         SMALLINT NOT NULL,
                    player_id   UUID REFERENCES ",
                USERS,
                "(id),
                    encoded     INTEGER NOT NULL,
                    PRIMARY KEY (hand_id, seq)
                );"
            )
        }
        fn indices() -> &'static str {
            const_format::concatcp!(
                "CREATE INDEX IF NOT EXISTS idx_actions_player ON ",
                ACTIONS,
                " (player_id);"
            )
        }
        fn copy() -> &'static str {
            // Column order MUST match `columns()` above and the INSERT
            // shape in `HistoryRepository::create_action`. Composite key
            // (hand_id, seq) is preserved by the table PRIMARY KEY.
            const_format::concatcp!(
                "COPY ",
                ACTIONS,
                " (hand_id, seq, player_id, encoded) FROM STDIN BINARY"
            )
        }
        fn truncates() -> &'static str {
            const_format::concatcp!("TRUNCATE TABLE ", ACTIONS, ";")
        }
        fn freeze() -> &'static str {
            // `actions` is append-only — the row is written once when
            // the action occurs and never updated — so disabling
            // autovacuum and packing to fillfactor=100 is the
            // read-heavy tuning. ON-DELETE-CASCADE from `hands` is
            // respected by the row writer, not by TRUNCATE.
            const_format::concatcp!(
                "ALTER TABLE ",
                ACTIONS,
                " SET (fillfactor = 100);
                 ALTER TABLE ",
                ACTIONS,
                " SET (autovacuum_enabled = false);"
            )
        }
    }
}
