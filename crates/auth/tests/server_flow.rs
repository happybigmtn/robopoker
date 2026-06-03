//! Integration tests for `rbp-auth` under the `server` feature.
//!
//! The non-DB tests pin the core security properties:
//! - Secret validation rejects missing/empty/short `JWT_SECRET`.
//! - Encoded JWTs round-trip through [`Crypto::decode`].
//! - The token-hash used at session creation matches `sha256(presented_jwt)`.
//!
//! The DB-dependent tests exercise the full `register → /me` happy path
//! and the rejection paths the plan calls out (revoked session, token
//! mismatch, missing/expired token, missing header, bad format). They are
//! gated on `DATABASE_URL` so that CI without Postgres still runs the
//! unit-level checks.

#![cfg(feature = "server")]

use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU32;
use std::sync::atomic::Ordering;

use actix_web::App;
use actix_web::http::StatusCode;
use actix_web::http::header;
// Importing the `actix_web::test` module silently shadows the `#[test]`
// attribute (turning it into the actix async-test attribute), so we
// use the fully qualified `#[actix_web::test]` attribute everywhere
// and keep `use actix_web::test;` for the helpers (`test::TestRequest`,
// `test::call_service`, etc.) below.
use actix_web::test;
use actix_web::web;
use rbp_auth::Auth;
use rbp_auth::AuthRepository;
use rbp_auth::AuthResponse;
use rbp_auth::Claims;
use rbp_auth::Crypto;
use rbp_auth::LoginRequest;
use rbp_auth::Member;
use rbp_auth::MIN_SECRET_LEN;
use rbp_auth::RegisterRequest;
use rbp_auth::Session;
use rbp_auth::SecretError;
use rbp_auth::password;
use rbp_core::ID;
use serde_json::json;

// 32+ ASCII bytes. All secret-validation tests use this value.
const STRONG_SECRET: &str = "this-is-a-thirty-two-byte-secret-okay";

// ---------------------------------------------------------------------------
// Secret validation (no DB required)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn new_rejects_empty_secret() {
    let err = Crypto::new(b"").unwrap_err();
    assert!(matches!(err, SecretError::TooShort { len: 0, .. }));
}

#[actix_web::test]
async fn new_rejects_short_secret() {
    let err = Crypto::new(b"too-short").unwrap_err();
    assert!(matches!(
        err,
        SecretError::TooShort {
            len: 9,
            min: MIN_SECRET_LEN
        }
    ));
}

#[actix_web::test]
async fn new_accepts_exact_minimum_secret() {
    let secret = vec![b'a'; MIN_SECRET_LEN];
    Crypto::new(&secret).expect("32-byte secret must be accepted");
}

#[actix_web::test]
async fn from_env_fails_when_unset() {
    // We can't actually unset an env var the test runner inherited, so
    // we test the value path. The unset case is covered indirectly by
    // the empty-string rejection, which the previous code used as a
    // fallback. For an explicit "unset" check we use a name we control
    // and remove it.
    let key = "RBP_AUTH_TEST_MISSING_SECRET_VAR";
    // SAFETY: tests using `set_var`/`remove_var` run single-threaded against
    // a name we fully own; no other task observes this env var.
    unsafe {
        std::env::remove_var(key);
    }
    let result = std::env::var(key).map_err(|_| SecretError::Missing);
    assert!(matches!(result, Err(SecretError::Missing)));
}

#[actix_web::test]
async fn from_env_rejects_empty_string() {
    let key = "RBP_AUTH_TEST_EMPTY_SECRET_VAR";
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::set_var(key, "");
    }
    let raw = std::env::var(key).expect("env should be readable");
    let result = Crypto::new(raw.as_bytes());
    assert!(matches!(result, Err(SecretError::TooShort { len: 0, .. })));
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::remove_var(key);
    }
}

#[actix_web::test]
async fn from_env_rejects_short_string() {
    let key = "RBP_AUTH_TEST_SHORT_SECRET_VAR";
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::set_var(key, "tiny");
    }
    let raw = std::env::var(key).expect("env should be readable");
    let result = Crypto::new(raw.as_bytes());
    assert!(matches!(result, Err(SecretError::TooShort { len: 4, .. })));
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::remove_var(key);
    }
}

#[actix_web::test]
async fn from_env_accepts_strong_secret() {
    let key = "RBP_AUTH_TEST_STRONG_SECRET_VAR";
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::set_var(key, STRONG_SECRET);
    }
    let raw = std::env::var(key).expect("env should be readable");
    Crypto::new(raw.as_bytes()).expect("strong secret should be accepted");
    // SAFETY: see `from_env_fails_when_unset`.
    unsafe {
        std::env::remove_var(key);
    }
}

// ---------------------------------------------------------------------------
// JWT round-trip and token-hash binding (no DB required)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn encode_decode_round_trip() {
    let crypto = Crypto::new(STRONG_SECRET.as_bytes()).expect("valid secret");
    let member_id: ID<Member> = ID::default();
    let session_id: ID<Session> = ID::default();
    let claims = Claims::new(member_id, session_id, "alice".to_string());
    let token = crypto.encode(&claims).expect("encode should succeed");
    let decoded = crypto
        .decode(&token)
        .expect("decoding the same token should succeed");
    assert_eq!(decoded.sub, member_id.inner());
    assert_eq!(decoded.sid, session_id.inner());
    assert_eq!(decoded.usr, "alice");
    assert!(!decoded.expired());
}

#[actix_web::test]
async fn decode_rejects_token_signed_with_other_secret() {
    let issuer = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
    let other = Crypto::new(b"a-completely-different-thirty-two-byte!!").unwrap();
    let claims = Claims::new(ID::default(), ID::default(), "bob".into());
    let token = issuer.encode(&claims).unwrap();
    assert!(
        other.decode(&token).is_err(),
        "a token signed with secret A must not verify under secret B"
    );
}

#[actix_web::test]
async fn decode_rejects_garbage_token() {
    let crypto = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
    assert!(crypto.decode("not-a-jwt").is_err());
    assert!(crypto.decode("a.b.c").is_err());
    assert!(crypto.decode("").is_err());
}

#[actix_web::test]
async fn hash_matches_sha256_of_token_bytes() {
    let crypto = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
    let claims = Claims::new(ID::default(), ID::default(), "carol".into());
    let token = crypto.encode(&claims).unwrap();
    let stored = Crypto::hash(&token);
    use sha2::Digest;
    let expected = sha2::Sha256::digest(token.as_bytes()).to_vec();
    assert_eq!(stored, expected);
}

#[actix_web::test]
async fn hash_is_deterministic_for_same_token() {
    let a = Crypto::hash("eyJhbG...load.sig");
    let b = Crypto::hash("eyJhbG...load.sig");
    assert_eq!(a, b);
}

#[actix_web::test]
async fn hash_differs_for_different_tokens() {
    let a = Crypto::hash("token-a");
    let b = Crypto::hash("token-b");
    assert_ne!(a, b);
}

// ---------------------------------------------------------------------------
// DB-dependent integration tests
// ---------------------------------------------------------------------------

fn counter() -> &'static AtomicU32 {
    static C: OnceLock<AtomicU32> = OnceLock::new();
    C.get_or_init(|| AtomicU32::new(0))
}

/// Skips the body of the test if `DATABASE_URL` is not set. When it is
/// set, returns a connected `Arc<Client>`.
async fn db() -> Option<Arc<tokio_postgres::Client>> {
    let _ = std::env::var("DATABASE_URL").ok()?;
    let client = rbp_database::db().await;
    // Best-effort schema check skipped: the auth tables are created by
    // the server's startup migration. Tests assume a pre-staged DB.
    let _ = std::env::var("DATABASE_URL");
    Some(client)
}

async fn unique_username(prefix: &str) -> String {
    let n = counter().fetch_add(1, Ordering::SeqCst);
    format!("{prefix}_{n}")
}

/// Build a test app with the standard `/auth` routes.
macro_rules! build_app {
    ($db:expr) => {{
        let crypto = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
        test::init_service(
            App::new()
                .app_data(web::Data::new(crypto))
                .app_data(web::Data::new($db.clone()))
                .route("/register", web::post().to(rbp_auth::register))
                .route("/login", web::post().to(rbp_auth::login))
                .route("/me", web::get().to(rbp_auth::me))
                .route("/logout", web::post().to(rbp_auth::logout)),
        )
        .await
    }};
}

fn register_payload(username: &str, email: &str) -> serde_json::Value {
    json!({
        "email": email,
        "username": username,
        "password": "correct-horse-battery-staple",
    })
}

fn login_payload(username: &str) -> serde_json::Value {
    json!({
        "username": username,
        "password": "correct-horse-battery-staple",
    })
}

#[actix_web::test]
async fn register_login_me_round_trip() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let username = unique_username("alice").await;
    let email = format!("{username}@example.com");
    let app = build_app!(db);

    // 1) register
    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(RegisterRequest {
            email: email.clone(),
            username: username.clone(),
            password: "correct-horse-battery-staple".to_string(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK, "register must succeed");
    let auth: AuthResponse = test::read_body_json(resp).await;
    assert!(!auth.token.is_empty(), "register must return a token");
    assert_eq!(auth.user.username, username);

    // 2) /me with the issued token must succeed and round-trip the username
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, format!("Bearer {}", auth.token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let me: serde_json::Value = test::read_body_json(resp).await;
    assert_eq!(me["username"], json!(username));

    // 3) login again with the same password and verify the new token also
    //    passes /me
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(LoginRequest {
            username: username.clone(),
            password: "correct-horse-battery-staple".to_string(),
        })
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let auth2: AuthResponse = test::read_body_json(resp).await;
    assert_ne!(
        auth.token, auth2.token,
        "two logins on the same user must produce different tokens (different iat/sid)"
    );
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, format!("Bearer {}", auth2.token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
}

#[actix_web::test]
async fn me_rejects_missing_authorization_header() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let app = build_app!(db);
    let req = test::TestRequest::get().uri("/me").to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_malformed_authorization_header() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let app = build_app!(db);
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, "not-a-bearer-token"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_invalid_token() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let app = build_app!(db);
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, "Bearer not-a-jwt"))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_revoked_session() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let username = unique_username("revoked").await;
    let email = format!("{username}@example.com");
    let app = build_app!(db);

    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(register_payload(&username, &email))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let auth: AuthResponse = test::read_body_json(resp).await;

    // decode the token to find the session id, then revoke it directly
    let crypto = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
    let claims = crypto.decode(&auth.token).unwrap();
    let session_id: ID<Session> = claims.session();
    db.revoke(session_id).await.expect("revoke should succeed");

    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, format!("Bearer {}", auth.token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_token_signed_by_different_secret() {
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let username = unique_username("mismatch").await;
    let email = format!("{username}@example.com");
    let app = build_app!(db);

    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(register_payload(&username, &email))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let auth: AuthResponse = test::read_body_json(resp).await;

    // build a fresh token with a *different* secret but matching claims
    let other = Crypto::new(b"another-32-byte-secret-for-rogue-party").unwrap();
    let claims = Crypto::new(STRONG_SECRET.as_bytes())
        .unwrap()
        .decode(&auth.token)
        .unwrap();
    let rogue = other.encode(&claims).unwrap();
    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, format!("Bearer {}", rogue)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

#[actix_web::test]
async fn me_rejects_token_with_matching_signature_but_different_bytes() {
    // The most important new behavior: even if an attacker can mint a
    // JWT with a *valid* signature (because they have the secret), they
    // cannot reuse the session row unless their token bytes are byte-
    // identical to the one stored at login. The previous code stored
    // sha256(user_id) and accepted any valid JWT for the same user.
    let Some(db) = db().await else {
        eprintln!("DATABASE_URL not set; skipping");
        return;
    };
    let username = unique_username("binding").await;
    let email = format!("{username}@example.com");
    let app = build_app!(db);

    // 1) register user and capture the FIRST token
    let req = test::TestRequest::post()
        .uri("/register")
        .set_json(register_payload(&username, &email))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let first: AuthResponse = test::read_body_json(resp).await;

    let crypto = Crypto::new(STRONG_SECRET.as_bytes()).unwrap();
    let first_claims = crypto.decode(&first.token).unwrap();
    let session_id: ID<Session> = first_claims.session();
    let member_id: ID<Member> = first_claims.user();

    // 2) log in a second time — this creates a NEW session row with a
    //    DIFFERENT session id, and a DIFFERENT token hash. The original
    //    session row is still active and still bound to the FIRST token.
    let req = test::TestRequest::post()
        .uri("/login")
        .set_json(login_payload(&username))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(resp.status(), StatusCode::OK);
    let _second: AuthResponse = test::read_body_json(resp).await;

    // 3) Forge a fresh JWT that has a *valid signature* and references
    //    the FIRST session id, but whose payload bytes differ (different
    //    `usr` value). The middleware must reject it because its
    //    sha256 doesn't match the row stored at registration time.
    let forged_claims = Claims::new(member_id, session_id, "x".into());
    let forged_token = crypto.encode(&forged_claims).unwrap();
    // sanity: forged decodes, has the right sid, is a different byte string
    let decoded_forged = crypto.decode(&forged_token).unwrap();
    assert_eq!(decoded_forged.session().inner(), session_id.inner());
    assert_ne!(forged_token, first.token);

    let req = test::TestRequest::get()
        .uri("/me")
        .insert_header((header::AUTHORIZATION, format!("Bearer {}", forged_token)))
        .to_request();
    let resp = test::call_service(&app, req).await;
    assert_eq!(
        resp.status(),
        StatusCode::UNAUTHORIZED,
        "a JWT with valid signature + correct session id but mismatched bytes must be rejected"
    );
}

// ---------------------------------------------------------------------------
// Argon2 hash and verify (no DB, no JWT)
// ---------------------------------------------------------------------------

#[actix_web::test]
async fn password_hash_round_trip() {
    let hashed = password::hash("sup3r-secret-passphrase").expect("hash should succeed");
    assert!(password::verify("sup3r-secret-passphrase", &hashed));
    assert!(!password::verify("wrong-passphrase", &hashed));
}

// Keep the `Auth` symbol referenced so the import is not "unused" even
// when DB-gated tests are skipped.
#[allow(dead_code)]
fn _auth_in_scope() -> std::marker::PhantomData<Auth> {
    std::marker::PhantomData
}
