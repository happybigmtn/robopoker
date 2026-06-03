use super::*;
use rbp_core::ID;
use rbp_core::Unique;

/// Persisted session for token management.
#[derive(Debug, Clone)]
pub struct Session {
    id: ID<Self>,
    user: ID<Member>,
    hash: Vec<u8>,
    expires: std::time::SystemTime,
    // can do something with this field later
    #[allow(unused)]
    revoked: bool,
}

impl Unique for Session {
    fn id(&self) -> ID<Self> {
        self.id
    }
}

impl Session {
    pub fn new(id: ID<Self>, user: ID<Member>, hash: Vec<u8>) -> Self {
        Self {
            id,
            user,
            hash,
            expires: std::time::SystemTime::now() + Crypto::duration(),
            revoked: false,
        }
    }
    pub fn user(&self) -> ID<Member> {
        self.user
    }
    pub fn hash(&self) -> &[u8] {
        &self.hash
    }
    pub fn expires_at(&self) -> std::time::SystemTime {
        self.expires
    }
}

#[cfg(feature = "database")]
mod schema {
    use super::*;
    use rbp_database::*;

    impl Schema for Session {
        fn name() -> &'static str {
            SESSIONS
        }
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::BYTEA,
                tokio_postgres::types::Type::TIMESTAMPTZ,
                tokio_postgres::types::Type::BOOL,
            ]
        }
        fn creates() -> &'static str {
            const_format::concatcp!(
                "CREATE TABLE IF NOT EXISTS ",
                SESSIONS,
                " (
                    id          UUID PRIMARY KEY,
                    user_id     UUID NOT NULL REFERENCES ",
                USERS,
                "(id) ON DELETE CASCADE,
                    token_hash  BYTEA NOT NULL,
                    expires_at  TIMESTAMPTZ NOT NULL,
                    revoked     BOOLEAN DEFAULT FALSE
                );"
            )
        }
        fn indices() -> &'static str {
            // `idx_sessions_token` and `idx_sessions_user` are also
            // created by the `SESSIONS` DDL's primary-key path and
            // existing repo code; here we only need the partial
            // `expires_at` index that excludes already-revoked
            // sessions, which is not expressible as a UNIQUE
            // constraint. The other two indices are kept in
            // `Schema::indices` for the production
            // `Streamable::finalize` path that calls
            // `batch_execute(Schema::indices())`.
            const_format::concatcp!(
                "CREATE INDEX IF NOT EXISTS idx_sessions_user ON ",
                SESSIONS,
                " (user_id);
                 CREATE INDEX IF NOT EXISTS idx_sessions_token ON ",
                SESSIONS,
                " (token_hash);
                 CREATE INDEX IF NOT EXISTS idx_sessions_expires ON ",
                SESSIONS,
                " (expires_at) WHERE NOT revoked;"
            )
        }
        fn copy() -> &'static str {
            // Column order MUST match `columns()` above. `sessions`
            // has no `Streamable` impl in the current pipeline — rows
            // are inserted row-at-a-time by `AuthRepository::signin` —
            // so this statement is not actually executed today. We
            // still emit a well-formed `COPY` header so the trait
            // compiles and a future `Streamable` impl (e.g. for bulk
            // session import) can use it without panic.
            const_format::concatcp!(
                "COPY ",
                SESSIONS,
                " (id, user_id, token_hash, expires_at, revoked) FROM STDIN BINARY"
            )
        }
        fn truncates() -> &'static str {
            // `sessions` has no child tables that reference it, so a
            // plain TRUNCATE is sufficient and faster than CASCADE.
            // Plain TRUNCATE acquires an ACCESS EXCLUSIVE lock that
            // cannot deadlock against the user-table cascade, which is
            // why we do not cascade here even though `users` is the
            // parent.
            const_format::concatcp!("TRUNCATE TABLE ", SESSIONS, ";")
        }
        fn freeze() -> &'static str {
            // `sessions` is read-mostly with two UPDATEs per active
            // session (`update_token_hash` and `revoke`). Unlike the
            // strictly append-only hand-history tables, disabling
            // autovacuum here would let dead tuples from the UPDATEs
            // bloat the table, so we deliberately keep autovacuum
            // enabled and only pin `fillfactor = 100` — sessions are
            // inserted once and updated at most twice, so 100 is
            // correct for the steady state and the rare HOT updates
            // are not materially affected.
            const_format::concatcp!("ALTER TABLE ", SESSIONS, " SET (fillfactor = 100);")
        }
    }
}

#[cfg(all(test, feature = "database"))]
mod schema_tests {
    //! Unit tests for the `Session` [`Schema`] contract.
    //!
    //! Pure-string guards on `copy` / `truncates` / `freeze` so a
    //! refactor that drops a column, drops the table name, or breaks
    //! the COPY column arity fails CI before it ever reaches a live
    //! Postgres. No database connection required.
    use super::Session;
    use rbp_database::Schema;

    #[test]
    fn copy_targets_sessions_table() {
        let sql = Session::copy();
        assert!(
            sql.contains("sessions"),
            "copy() must reference the sessions table; got: {sql}"
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
        let sql = Session::copy();
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
            Session::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            Session::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_clears_sessions_table() {
        let sql = Session::truncates();
        assert!(
            sql.contains("TRUNCATE TABLE"),
            "truncates() must issue TRUNCATE TABLE; got: {sql}"
        );
        assert!(
            sql.contains("sessions"),
            "truncates() must target the sessions table; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_but_keeps_autovacuum() {
        // Sessions see two UPDATEs per active row
        // (`update_token_hash`, `revoke`); disabling autovacuum would
        // let dead-tuple bloat dominate, so freeze() must keep
        // autovacuum on.
        let sql = Session::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            !sql.contains("autovacuum_enabled"),
            "freeze() must NOT disable autovacuum for sessions (UPDATE-heavy); got: {sql}"
        );
        assert!(
            sql.contains("sessions"),
            "freeze() must target the sessions table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(Session::name(), rbp_database::SESSIONS);
    }
}
