use rbp_core::ID;
use rbp_core::Unique;

/// Authenticated user with verified identity.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Member {
    id: ID<Self>,
    username: String,
    email: String,
}

impl Member {
    pub fn new(id: ID<Self>, username: String, email: String) -> Self {
        Self {
            id,
            username,
            email,
        }
    }
    pub fn username(&self) -> &str {
        &self.username
    }
    pub fn email(&self) -> &str {
        &self.email
    }
}

impl Unique for Member {
    fn id(&self) -> ID<Self> {
        self.id
    }
}

#[cfg(feature = "database")]
mod schema {
    use super::*;
    use rbp_database::*;

    /// Schema implementation for Member (users table).
    /// Note: hashword is a database-only field, not part of Member domain type.
    impl Schema for Member {
        fn name() -> &'static str {
            USERS
        }
        fn columns() -> &'static [tokio_postgres::types::Type] {
            &[
                tokio_postgres::types::Type::UUID,
                tokio_postgres::types::Type::VARCHAR,
                tokio_postgres::types::Type::VARCHAR,
                tokio_postgres::types::Type::TEXT,
            ]
        }
        fn creates() -> &'static str {
            const_format::concatcp!(
                "CREATE TABLE IF NOT EXISTS ",
                USERS,
                " (
                    id          UUID PRIMARY KEY,
                    username    VARCHAR(32) UNIQUE NOT NULL,
                    email       VARCHAR(255) UNIQUE NOT NULL,
                    hashword    TEXT NOT NULL
                );"
            )
        }
        fn indices() -> &'static str {
            // The `username` and `email` UNIQUE constraints in
            // `creates()` already build B-tree indices that cover
            // `AuthRepository::exists` and `AuthRepository::lookup`,
            // but we emit explicit `idx_users_*` names here so a
            // future `Streamable::finalize` call (or a manual
            // migration) sees the same named indices the auth
            // services historically documented; the extra `IF NOT
            // EXISTS` makes the statement idempotent against the
            // UNIQUE-derived indices.
            const_format::concatcp!(
                "CREATE INDEX IF NOT EXISTS idx_users_username ON ",
                USERS,
                " (username);
                 CREATE INDEX IF NOT EXISTS idx_users_email ON ",
                USERS,
                " (email);"
            )
        }
        fn copy() -> &'static str {
            // Column order MUST match `columns()` above. `users` has
            // no `Streamable` impl in the current pipeline — rows are
            // inserted row-at-a-time by `AuthRepository::create` — so
            // this statement is not actually executed today. We still
            // emit a well-formed `COPY` header so the trait compiles
            // and a future `Streamable` impl (e.g. for bulk account
            // import) can use it without panic.
            const_format::concatcp!(
                "COPY ",
                USERS,
                " (id, username, email, hashword) FROM STDIN BINARY"
            )
        }
        fn truncates() -> &'static str {
            // `users` is referenced from `sessions` (ON DELETE
            // CASCADE), `players` (nullable, no cascade declared), and
            // `actions` (nullable, no cascade declared). Plain
            // `TRUNCATE` does not fire ON DELETE actions, so we must
            // cascade explicitly to drop sessions in lockstep — the
            // nullable references in `players` and `actions` resolve
            // to NULL on cascade without violation.
            const_format::concatcp!("TRUNCATE TABLE ", USERS, " CASCADE;")
        }
        fn freeze() -> &'static str {
            // `users` is effectively append-only at the row level:
            // the only writes are INSERTs at registration time and
            // SELECTs during login, with no UPDATEs in
            // `AuthRepository`. The row layout is therefore stable
            // and `fillfactor = 100` + disabled autovacuum is the
            // correct read-heavy tuning.
            const_format::concatcp!(
                "ALTER TABLE ",
                USERS,
                " SET (fillfactor = 100);
                 ALTER TABLE ",
                USERS,
                " SET (autovacuum_enabled = false);"
            )
        }
    }
}

#[cfg(all(test, feature = "database"))]
mod schema_tests {
    //! Unit tests for the `Member` [`Schema`] contract.
    //!
    //! Pure-string guards on `copy` / `truncates` / `freeze` so a
    //! refactor that drops a column, drops the table name, or breaks
    //! the COPY column arity fails CI before it ever reaches a live
    //! Postgres. No database connection required.
    use super::Member;
    use rbp_database::Schema;

    #[test]
    fn copy_targets_users_table() {
        let sql = Member::copy();
        assert!(
            sql.contains("users"),
            "copy() must reference the users table; got: {sql}"
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
        let sql = Member::copy();
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
            Member::columns().len(),
            "copy() column arity {} must match columns() arity {} (copy columns: {:?})",
            header_cols.len(),
            Member::columns().len(),
            header_cols,
        );
    }

    #[test]
    fn truncates_clears_users_table() {
        let sql = Member::truncates();
        assert!(
            sql.contains("TRUNCATE TABLE"),
            "truncates() must issue TRUNCATE TABLE; got: {sql}"
        );
        assert!(
            sql.contains("users"),
            "truncates() must target the users table; got: {sql}"
        );
        assert!(
            sql.contains("CASCADE"),
            "truncates() must cascade because sessions FK references users; got: {sql}"
        );
    }

    #[test]
    fn freeze_sets_fillfactor_and_disables_autovacuum() {
        let sql = Member::freeze();
        assert!(
            sql.contains("fillfactor"),
            "freeze() must set fillfactor; got: {sql}"
        );
        assert!(
            sql.contains("autovacuum_enabled"),
            "freeze() must disable autovacuum; got: {sql}"
        );
        assert!(
            sql.contains("users"),
            "freeze() must target the users table; got: {sql}"
        );
    }

    #[test]
    fn name_matches_const_table_name() {
        assert_eq!(Member::name(), rbp_database::USERS);
    }
}
