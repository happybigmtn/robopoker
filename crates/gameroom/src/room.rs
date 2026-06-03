use super::*;
use rbp_auth::Member;
use rbp_auth::User;
use rbp_core::*;
use rbp_database::*;
use rbp_gameplay::Turn;
use std::sync::Arc;
use tokio_postgres::Client;

/// Live poker room coordinator.
/// Imperative shell that owns Engine (functional core) and handles
/// identity, user tracking, and persistence concerns.
pub struct Room {
    id: ID<Self>,
    db: Arc<Client>,
    stakes: Chips,
    engine: EngineState,
    context: HandContext,
    users: Vec<User>,
}

impl Room {
    pub fn new(id: ID<Self>, stakes: Chips, db: Arc<Client>) -> Self {
        Self {
            id,
            db,
            stakes,
            users: Vec::new(),
            engine: EngineState::default(),
            context: HandContext::default(),
        }
    }
    pub fn stakes(&self) -> Chips {
        self.stakes
    }
    pub fn sit<P, U>(&mut self, player: P, user: U)
    where
        P: Player + 'static,
        U: Into<User>,
    {
        self.engine.as_seating().sit(player);
        self.users.push(user.into());
    }
}

impl Room {
    pub async fn run(
        mut self,
        start: tokio::sync::oneshot::Receiver<()>,
        done: tokio::sync::oneshot::Sender<()>,
    ) {
        log::debug!("[room {}] waiting for player", self.id);
        let _ = start.await;
        log::debug!("[room {}] starting game loop", self.id);
        self.engine.start();
        loop {
            self.reset_hand();
            self.play_hand().await;
            self.engine.into_showdown();
            self.run_showdown().await;
            self.flush_hand().await;
            if self.should_stop() {
                break;
            }
            self.engine.conclude();
            if self.engine.is_finished() {
                log::info!("[room {}] game over", self.id);
                break;
            }
        }
        let _ = done.send(());
    }
    async fn play_hand(&mut self) {
        let engine = match &mut self.engine {
            EngineState::Dealing(e) => e,
            _ => panic!("play_hand called in wrong phase"),
        };
        loop {
            match engine.turn() {
                Turn::Chance => engine.deal().await,
                Turn::Choice(p) => {
                    let action = engine.ask(p).await;
                    self.context.record(p, action);
                }
                Turn::Terminal => break,
            }
        }
    }
    async fn run_showdown(&mut self) {
        let engine = match &mut self.engine {
            EngineState::Showdown(e) => e,
            _ => panic!("run_showdown called in wrong phase"),
        };
        engine.showdown().await;
        engine.settle();
    }
    fn should_stop(&self) -> bool {
        match &self.engine {
            EngineState::Showdown(e) => e.human_disconnected(),
            _ => false,
        }
    }
}

impl Room {
    fn user(&self, pos: Position) -> Option<ID<Member>> {
        self.users.get(pos).and_then(User::id)
    }
    fn reset_hand(&mut self) {
        let (hand_number, game) = match &self.engine {
            EngineState::Dealing(e) => (e.hand(), e.game()),
            _ => panic!("reset_hand called in wrong phase"),
        };
        self.context = HandContext::new(hand_number, game);
    }
    async fn flush_hand(&self) {
        let game = match &self.engine {
            EngineState::Showdown(e) => e.game(),
            _ => panic!("flush_hand called in wrong phase"),
        };
        let hand = self
            .context
            .to_hand(self.id().cast(), game.board(), game.pot());
        self.db
            .create_hand(&hand)
            .await
            .expect("failed to record hand");
        for ref player in self.context.participants(hand.id(), |p| self.user(p)) {
            self.db
                .create_player(player)
                .await
                .expect("failed to record player");
        }
        for ref play in self.context.plays(hand.id(), |p| self.user(p)) {
            self.db
                .create_action(play)
                .await
                .expect("failed to record action");
        }
        log::info!("recorded hand {}", hand.id());
    }
}

impl Unique for Room {
    fn id(&self) -> ID<Self> {
        self.id
    }
}

impl Schema for Room {
    fn name() -> &'static str {
        ROOMS
    }
    fn creates() -> &'static str {
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            ROOMS,
            " (
                id          UUID PRIMARY KEY,
                stakes      SMALLINT NOT NULL
            );"
        )
    }
    fn indices() -> &'static str {
        // `rooms` is small and queried by primary key (the `id`); the
        // primary-key index already covers lookups, so no extra
        // indices are required.
        ""
    }
    fn truncates() -> &'static str {
        // `rooms` has FK references from `hands` (`ON DELETE CASCADE`
        // is not declared — see `Hand::creates` for the FK shape), and
        // `TRUNCATE` does not fire `ON DELETE`, so we must cascade
        // explicitly or the statement fails with `foreign key
        // violation` if any hand row references the room.
        const_format::concatcp!("TRUNCATE TABLE ", ROOMS, " CASCADE;")
    }
    fn freeze() -> &'static str {
        // `rooms` is append-only after a room opens (no UPDATE path
        // exists for `rooms` rows), so the row layout is stable and
        // `fillfactor = 100` + disabled autovacuum is the right
        // read-heavy tuning.
        const_format::concatcp!(
            "ALTER TABLE ",
            ROOMS,
            " SET (fillfactor = 100);
             ALTER TABLE ",
            ROOMS,
            " SET (autovacuum_enabled = false);"
        )
    }
}

impl BulkSchema for Room {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::UUID,
            tokio_postgres::types::Type::INT2,
        ]
    }
    fn copy() -> &'static str {
        // Column order MUST match `columns()` above. `rooms` has no
        // `Streamable` impl in the current pipeline — rows are inserted
        // row-at-a-time by `HistoryRepository::create_room` — so this
        // statement is not actually executed today. We still emit a
        // well-formed `COPY` header so the trait compiles and a future
        // `Streamable` impl (e.g. for room-batch imports) can use it
        // without panic.
        const_format::concatcp!("COPY ", ROOMS, " (id, stakes) FROM STDIN BINARY")
    }
}

#[cfg(all(test, feature = "database"))]
mod schema_tests {
    //! Unit tests for the `Room` [`Schema`] contract.
    //!
    //! Pure-string guards on `copy` / `truncates` / `freeze` so a
    //! refactor that drops a column, drops the table name, or breaks
    //! the COPY column arity fails CI before it ever reaches a live
    //! Postgres. No database connection required.
    use super::Room;
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn copy_targets_rooms_table() {
        let sql = Room::copy();
        assert!(
            sql.contains("rooms"),
            "copy() must reference the rooms table; got: {sql}"
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
        let sql = Room::copy();
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
            Room::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            Room::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_clears_rooms_table() {
        let sql = Room::truncates();
        assert!(
            sql.contains("TRUNCATE TABLE"),
            "truncates() must issue TRUNCATE TABLE; got: {sql}"
        );
        assert!(
            sql.contains("rooms"),
            "truncates() must target the rooms table; got: {sql}"
        );
        assert!(
            sql.contains("CASCADE"),
            "truncates() must cascade because hands FK references rooms; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_and_disables_autovacuum() {
        let sql = Room::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum; got: {sql}"
        );
        assert!(
            sql.contains("rooms"),
            "freeze() must target the rooms table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(Room::name(), rbp_database::ROOMS);
    }
}
