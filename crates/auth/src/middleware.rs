use super::*;
use actix_web::FromRequest;
use actix_web::HttpRequest;
use actix_web::dev::Payload;
use actix_web::web;
use rbp_core::ID;
use rbp_database::*;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_postgres::Client;

/// Extractor for authenticated requests.
///
/// Validates the JWT, checks that the session row exists, that the
/// session is not revoked, and that `sha256(presented_jwt)` matches the
/// `token_hash` persisted at login. The hash comparison is the binding
/// `STW-004` adds — without it, any valid JWT for the same user (e.g.
/// a parallel login on another device) would pass authentication.
pub struct Auth(pub Claims);

impl Auth {
    pub fn claims(&self) -> &Claims {
        &self.0
    }
    pub fn user(&self) -> ID<Member> {
        self.0.user()
    }
}

impl FromRequest for Auth {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;
    fn from_request(req: &HttpRequest, _payload: &mut Payload) -> Self::Future {
        let token_service = req.app_data::<web::Data<Crypto>>().cloned();
        let db = req.app_data::<web::Data<Arc<Client>>>().cloned();
        let auth_header = req
            .headers()
            .get("Authorization")
            .and_then(|h| h.to_str().ok())
            .map(|s| s.to_owned());
        Box::pin(async move {
            let header = auth_header.ok_or_else(|| {
                actix_web::error::ErrorUnauthorized("missing authorization header")
            })?;
            let token = header.strip_prefix("Bearer ").ok_or_else(|| {
                actix_web::error::ErrorUnauthorized("invalid authorization format")
            })?;
            let service = token_service.ok_or_else(|| {
                actix_web::error::ErrorInternalServerError("token service not configured")
            })?;
            let claims = service
                .decode(token)
                .map_err(|_| actix_web::error::ErrorUnauthorized("invalid token"))?;
            if claims.expired() {
                return Err(actix_web::error::ErrorUnauthorized("token expired"));
            }
            let db = db.ok_or_else(|| {
                actix_web::error::ErrorInternalServerError("database not configured")
            })?;
            // Single round-trip: read `revoked` and `token_hash` together
            // so a missing/empty hash (legacy row from before STW-004) is
            // also rejected rather than silently accepted.
            let row = db
                .query_opt(
                    const_format::concatcp!(
                        "SELECT revoked, token_hash FROM ",
                        SESSIONS,
                        " WHERE id = $1"
                    ),
                    &[&claims.session().inner()],
                )
                .await
                .map_err(|_| actix_web::error::ErrorInternalServerError("database error"))?
                .ok_or_else(|| actix_web::error::ErrorUnauthorized("session not found"))?;
            let revoked: bool = row.get(0);
            if revoked {
                return Err(actix_web::error::ErrorUnauthorized("session revoked"));
            }
            let stored_hash: Vec<u8> = row.get(1);
            check_session_binding(&stored_hash, token)
                .map_err(actix_web::error::ErrorUnauthorized)?;
            Ok(Auth(claims))
        })
    }
}

/// Pure session-binding check, extracted from [`Auth::from_request`] so the
/// security-critical logic can be unit-tested without a database connection.
///
/// - `stored_hash` is the `sha256(jwt)` written at login.
/// - `presented_token` is the raw JWT string from the `Authorization: Bearer`
///   header.
///
/// Returns `Ok(())` only when the presented token's SHA-256 exactly equals
/// the stored hash. Empty stored hashes are rejected — they indicate either
/// a legacy row predating `STW-004` or a tampering attempt.
pub(crate) fn check_session_binding(
    stored_hash: &[u8],
    presented_token: &str,
) -> Result<(), &'static str> {
    if stored_hash.is_empty() {
        return Err("session has no bound token");
    }
    let presented_hash = Crypto::hash(presented_token);
    if !constant_time_eq(stored_hash, &presented_hash) {
        return Err("token does not match session");
    }
    Ok(())
}

/// Optional authentication extractor - does not fail if unauthenticated.
pub struct MaybeAuth(pub Option<Claims>);

impl MaybeAuth {
    pub fn claims(&self) -> Option<&Claims> {
        self.0.as_ref()
    }
    pub fn user(&self) -> Option<ID<Member>> {
        self.0.as_ref().map(|c| c.user())
    }
}

impl FromRequest for MaybeAuth {
    type Error = actix_web::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Self, Self::Error>>>>;
    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let auth_future = Auth::from_request(req, payload);
        Box::pin(async move {
            match auth_future.await {
                Ok(Auth(claims)) => Ok(MaybeAuth(Some(claims))),
                Err(_) => Ok(MaybeAuth(None)),
            }
        })
    }
}

/// Constant-time byte slice comparison. Falls back to `false` on length
/// mismatch (the length leak is unavoidable but does not help an attacker
/// locate a matching byte, only narrow the search space by length, which
/// for SHA-256 is fixed at 32 bytes for any valid row).
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff: u8 = 0;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_TOKEN: &str = "eyJhbGciOiJIUzI1NiJ9.test.signature";

    #[test]
    fn constant_time_eq_matches() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 5];
        assert!(constant_time_eq(&a, &b));
    }

    #[test]
    fn constant_time_eq_rejects_mismatch() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4, 6];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn constant_time_eq_rejects_length_mismatch() {
        let a = [1u8, 2, 3, 4, 5];
        let b = [1u8, 2, 3, 4];
        assert!(!constant_time_eq(&a, &b));
    }

    #[test]
    fn constant_time_eq_rejects_empty() {
        assert!(!constant_time_eq(&[], &[1, 2]));
        assert!(constant_time_eq(&[], &[]));
    }

    #[test]
    fn check_session_binding_accepts_matching_token() {
        let stored = Crypto::hash(TEST_TOKEN);
        assert!(check_session_binding(&stored, TEST_TOKEN).is_ok());
    }

    #[test]
    fn check_session_binding_rejects_different_token() {
        let stored = Crypto::hash(TEST_TOKEN);
        let other_token = "eyJhbGciOiJIUzI1NiJ9.different.signature";
        match check_session_binding(&stored, other_token) {
            Err("token does not match session") => {}
            other => panic!("expected token mismatch, got {other:?}"),
        }
    }

    #[test]
    fn check_session_binding_rejects_empty_stored_hash() {
        match check_session_binding(&[], TEST_TOKEN) {
            Err("session has no bound token") => {}
            other => panic!("expected empty-hash error, got {other:?}"),
        }
    }

    #[test]
    fn check_session_binding_rejects_wrong_length_hash() {
        // Simulate a row with a truncated hash.
        let stored = vec![0u8; 16];
        match check_session_binding(&stored, TEST_TOKEN) {
            Err("token does not match session") => {}
            other => panic!("expected mismatch, got {other:?}"),
        }
    }
}
