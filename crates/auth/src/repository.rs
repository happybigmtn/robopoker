use super::*;
use rbp_core::ID;
use rbp_core::Unique;
use rbp_database::*;
use std::sync::Arc;
use tokio_postgres::Client;

/// Repository trait for authentication database operations.
/// Abstracts SQL from domain modules.
#[allow(async_fn_in_trait)]
pub trait AuthRepository {
    async fn signin(&self, session: &Session) -> Result<(), PgErr>;
    /// Bind a freshly-issued JWT to an existing session row. We sign the
    /// token first, then write `sha256(jwt)` back so the middleware can
    /// verify the presented token against the row that was created at
    /// login. The split (insert empty, then update) keeps the `Session`
    /// constructor pure and avoids threading the JWT through the domain
    /// type.
    async fn update_token_hash(
        &self,
        session: ID<Session>,
        hash: &[u8],
    ) -> Result<(), PgErr>;
    async fn revoke(&self, session: ID<Session>) -> Result<(), PgErr>;
    async fn exists(&self, username: &str, email: &str) -> Result<bool, PgErr>;
    async fn create(&self, member: &Member, hashword: &str) -> Result<(), PgErr>;
    async fn lookup(&self, username: &str) -> Result<Option<(Member, String)>, PgErr>;
    /// Look up the persisted `token_hash` for a session id. Middleware
    /// compares this against `sha256(presented_jwt)` to reject tokens
    /// that decode to a valid `Claims` but do not match the row stored at
    /// login (a different login on another device, a forged session id,
    /// etc.).
    async fn token_hash(&self, session: ID<Session>) -> Result<Option<Vec<u8>>, PgErr>;
}

impl AuthRepository for Arc<Client> {
    async fn exists(&self, username: &str, email: &str) -> Result<bool, PgErr> {
        self.query_opt(
            const_format::concatcp!(
                "SELECT 1 FROM ",
                USERS,
                " WHERE username = $1 OR email = $2"
            ),
            &[&username, &email],
        )
        .await
        .map(|opt| opt.is_some())
    }

    async fn create(&self, member: &Member, hashword: &str) -> Result<(), PgErr> {
        self.execute(
            const_format::concatcp!(
                "INSERT INTO ",
                USERS,
                " (id, username, email, hashword) VALUES ($1, $2, $3, $4)"
            ),
            &[
                &member.id().inner(),
                &member.username(),
                &member.email(),
                &hashword,
            ],
        )
        .await
        .map(|_| ())
    }

    async fn lookup(&self, username: &str) -> Result<Option<(Member, String)>, PgErr> {
        self.query_opt(
            const_format::concatcp!(
                "SELECT id, username, email, hashword FROM ",
                USERS,
                " WHERE username = $1"
            ),
            &[&username],
        )
        .await
        .map(|opt| {
            opt.map(|row| {
                (
                    Member::new(
                        ID::from(row.get::<_, uuid::Uuid>(0)),
                        row.get::<_, String>(1),
                        row.get::<_, String>(2),
                    ),
                    row.get::<_, String>(3),
                )
            })
        })
    }

    async fn signin(&self, session: &Session) -> Result<(), PgErr> {
        self.execute(
            const_format::concatcp!(
                "INSERT INTO ",
                SESSIONS,
                " (id, user_id, token_hash, expires_at) VALUES ($1, $2, $3, $4)"
            ),
            &[
                &session.id().inner(),
                &session.user().inner(),
                &session.hash(),
                &session.expires_at(),
            ],
        )
        .await
        .map(|_| ())
    }

    async fn update_token_hash(
        &self,
        session: ID<Session>,
        hash: &[u8],
    ) -> Result<(), PgErr> {
        self.execute(
            const_format::concatcp!(
                "UPDATE ",
                SESSIONS,
                " SET token_hash = $1 WHERE id = $2"
            ),
            &[&hash, &session.inner()],
        )
        .await
        .map(|_| ())
    }

    async fn revoke(&self, session: ID<Session>) -> Result<(), PgErr> {
        self.execute(
            const_format::concatcp!("UPDATE ", SESSIONS, " SET revoked = TRUE WHERE id = $1"),
            &[&session.inner()],
        )
        .await
        .map(|_| ())
    }

    async fn token_hash(&self, session: ID<Session>) -> Result<Option<Vec<u8>>, PgErr> {
        self.query_opt(
            const_format::concatcp!(
                "SELECT token_hash FROM ",
                SESSIONS,
                " WHERE id = $1"
            ),
            &[&session.inner()],
        )
        .await
        .map(|opt| opt.map(|row| row.get::<_, Vec<u8>>(0)))
    }
}
