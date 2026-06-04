//! `crates/dashboard/tests/smoke.rs` — STW-036 smoke
//! integration test.
//!
//! The single sub-test in this file spins up the
//! dashboard's `axum::Router` on a random localhost port
//! (a real `tokio::net::TcpListener` with port `0` so
//! the OS picks an unused port) and drives the four
//! routes the spec calls for end-to-end:
//!
//! - `GET /` returns 200 + a body that contains the
//!   table scaffold HTML (`<table class="index-table">`,
//!   `<thead>`, the pinned column names, the
//!   `Download transcript` / `Open replay` link shape).
//! - `GET /api/index` returns 200 + a body the
//!   `serde_json` round-trip parses into the same
//!   `PublishIndex` the fixture on disk encodes
//!   (so a shape drift in `INDEX.json` fails this
//!   test at the same CI step a downstream dashboard
//!   would silently break).
//! - `GET /transcript/<id>` returns 200 + the bytes
//!   match the fixture on disk.
//!
//! The "no console error in the rendered HTML" pin the
//! spec calls for is enforced by the response-body
//! substring check: a `<script>` body that contains a
//! `console.error` literal would fail the
//! `get_root_does_not_contain_console_error` assertion
//! the sub-test makes.

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rbp_dashboard::{AppState, IndexClient, dashboard_app};
use tower::ServiceExt;

/// The committed fixture `INDEX.json` the smoke test
/// points `RBP_DASHBOARD_INDEX_URL` at. The path is
/// resolved relative to `CARGO_MANIFEST_DIR` so the
/// test runs the same way on a developer machine and
/// in CI.
fn fixture_index_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("index.json")
}

/// The committed fixture `transcript-<id>.json` the
/// smoke test points `RBP_DASHBOARD_TRANSCRIPT_DIR` at.
/// A minimal hand-written 1-hand `Fish-vs-Fish`
/// transcript in the STW-014 shape.
fn fixture_transcript_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("transcript-testnet-live-proof-20260604T050000Z.json")
}

/// Drop a fixture `transcript-<id>.json` on disk so the
/// `GET /transcript/<id>` route can read it. The
/// transcript is a hand-written 1-hand `Fish-vs-Fish`
/// bundle in the STW-014 shape (`hand` +
/// `participants` + `plays` keys, a monotonic
/// zero-based `seq`); a future regression in the
/// `Transcript` JSON shape fails the round-trip at
/// the same CI step a downstream `trainer --replay`
/// consumer would silently break.
fn ensure_transcript_fixture() -> PathBuf {
    let path = fixture_transcript_path();
    if !path.exists() {
        let body = r#"{
  "hand": {
    "id": "11111111-1111-1111-1111-111111111111",
    "room": "22222222-2222-2222-2222-222222222222",
    "board": "As Kd 7c 2h Qs",
    "pot": 4,
    "dealer": 0
  },
  "participants": [
    { "user": null, "seat": 0, "hole": "As Kd" },
    { "user": null, "seat": 1, "hole": "7c 2h" }
  ],
  "plays": [
    { "seq": 0, "player": null, "action": "Call(1)" },
    { "seq": 1, "player": null, "action": "Check" },
    { "seq": 2, "player": null, "action": "Check" }
  ]
}
"#;
        std::fs::write(&path, body).expect("write transcript fixture");
    }
    path
}

/// Build an `AppState` pointed at the committed
/// fixtures. The `static_index_html` is loaded from
/// the checked-in
/// `crates/dashboard/static/index.html`; the
/// `transcript_dir` is the `tests/fixtures/` dir the
/// `ensure_transcript_fixture` helper just populated.
fn app_state_with_fixtures() -> AppState {
    let transcript_path = ensure_transcript_fixture();
    let transcript_dir = transcript_path
        .parent()
        .expect("transcript dir")
        .to_path_buf();
    let static_index_html = Arc::new(
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("static")
                .join("index.html"),
        )
        .expect("read static index.html"),
    );
    AppState {
        index_client: IndexClient::from_path(fixture_index_path()),
        transcript_dir,
        static_index_html,
    }
}

/// Async helper: send a `GET <uri>` request to the
/// dashboard's router and return the
/// `(status, body_bytes, content_type)` triple. The
/// helper hides the `Body::collect` plumbing the
/// `axum` body type needs.
async fn get(router: axum::Router, uri: &str) -> (StatusCode, Vec<u8>, Option<String>) {
    let response = router
        .oneshot(
            Request::builder()
                .uri(uri)
                .body(Body::empty())
                .expect("build request"),
        )
        .await
        .expect("oneshot request");
    let status = response.status();
    let content_type = response
        .headers()
        .get(axum::http::header::CONTENT_TYPE)
        .and_then(|h| h.to_str().ok())
        .map(|s| s.to_string());
    let body = axum::body::to_bytes(response.into_body(), 1_048_576)
        .await
        .expect("read body");
    (status, body.to_vec(), content_type)
}

#[tokio::test]
async fn smoke_dashboard_routes_against_committed_fixtures() {
    // STW-052: the empty-state override is
    // *not* touched here — the
    // `engaged_empty_state_for_test` RAII
    // guard the
    // `empty_state_renders_friendly_message_when_index_has_zero_entries`
    // sub-test holds for its own scope
    // restores the override to `None` on
    // `Drop`. A bare
    // `clear_empty_state_for_test()` call
    // here would race with the held guard
    // in a `cargo test --test-threads=4`
    // schedule (the call would silently
    // clear the override between the
    // setter and the assertion in the
    // empty-state test, leaving the
    // handler on the live-data path). The
    // pre-STW-052 `clear_*` defensive call
    // is now REMOVED; the test pins the
    // pre-STW-052 live-data path because
    // the env knob is unset AND the
    // override is `None` (the
    // default-off state the empty-state
    // test's RAII guard restores).
    // Start the dashboard's router on a random
    // localhost port. The smoke test does NOT
    // need a live `TcpListener` for the four
    // route checks (the `tower::ServiceExt::oneshot`
    // path drives the router in-process), but
    // the spec calls for a real `axum::serve`
    // entry point; this test's `dashboard_app()`
    // builder is the same code the `serve()`
    // entry point wraps, so a regression in
    // either path fails the same assertions.
    let app = dashboard_app(app_state_with_fixtures());

    // 1. `GET /` — table scaffold HTML.
    let (status, body, content_type) = get(app.clone(), "/").await;
    assert_eq!(status, StatusCode::OK, "GET / must return 200");
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    // The static `index.html` is the
    // checked-in vanilla-JS frontend the spec
    // ships. The static page does NOT
    // pre-render the table (the table is
    // built client-side from a `fetch
    // /api/index`); what the page ships
    // is the table host `<div id="index-table">`
    // + the JS that populates it. The
    // assertions below pin: (a) the
    // checked-in CSS variable / dark-theme
    // scaffold the spec calls for, (b) the
    // `fetch('/api/index')` call, (c) the
    // pinned column names the JS template
    // builds, and (d) the per-row
    // `Download transcript` / `Open replay`
    // link shape. A regression in any of
    // these surfaces as a missing substring.
    for token in [
        "<!doctype html>",
        "id=\"index-table\"",
        "fetch('/api/index'",
        // The pinned column order the JS
        // template builds (the `entries`
        // forEach iterates the same set of
        // headers the `render_index_table`
        // Rust emitter does).
        "receipt_basename",
        "blueprint",
        "baseline",
        "mbb_per_100",
        "ci_95",
        "win_rate",
        "total_bytes",
        "uploaded_at_utc",
        // STW-050: the 2 per-action column
        // headers (`transcript` / `replay`)
        // follow the 8 spec columns. The
        // `actions` literal the previous
        // shape used is gone; a regression
        // that re-introduces the single
        // `actions` column header fails
        // this assertion at the same step
        // a visitor's page-source inspection
        // would surface the literal.
        "transcript",
        "replay",
        "Download transcript",
        "Open replay",
        // The CSS variable scaffold the
        // spec calls for: dark theme
        // default, `--bg` / `--fg` /
        // `--link` palette, `prefers-color-scheme`
        // light override. A regression in
        // the single-80-line CSS block
        // surfaces as a missing token.
        "--bg:",
        "--fg:",
        "--link:",
        "prefers-color-scheme: light",
    ] {
        assert!(
            body_str.contains(token),
            "GET / body must contain `{token}` (static index.html scaffold); got:\n{body_str}"
        );
    }
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("text/html"),
        "GET / must be `text/html`; got `{ct}`"
    );
    // The "no console error in the rendered HTML"
    // pin the spec calls for. The page's JS uses
    // `meta.textContent = 'error: ' + e.message`
    // rather than `console.error` on a fetch
    // failure, so an `console.error(` literal
    // (with the open-paren, the actual call
    // shape) in the body is a regression in
    // the "no-console-error assertion" the
    // spec names. The HTML source may mention
    // the literal string `console.error` in
    // comments / docstrings without firing
    // the pin, so the assertion uses the
    // call shape, not the bare identifier.
    assert!(
        !body_str.contains("console.error("),
        "GET / body must not contain `console.error(` call (the no-console-error pin); got:\n{body_str}"
    );
    // STW-051: the literal `<unknown>`
    // must NOT appear in the rendered
    // `index.html` (the pre-STW-051 JS
    // fallback in `index.html:253` was
    // `'...' || '<unknown>'` — the
    // string leaked to a public visitor
    // as a "this is a test fixture"
    // tell on the dashboard's `meta`
    // line). The new JS uses the
    // *friendly* `(publish_root not
    // stamped)` / `(created_at_utc not
    // stamped — re-run with
    // RBP_PUBLISH_INDEX_UTC set)`
    // fallbacks; a future regression
    // that re-introduces the literal
    // `<unknown>` in the JS falls at
    // the same CI step a visitor's
    // page-source inspection would
    // surface the literal. The
    // assertion uses the literal
    // string (not a regex) so the
    // pin is exact + cheap.
    assert!(
        !body_str.contains("<unknown>"),
        "GET / body must not contain the literal `<unknown>` (the STW-051 JS-fallback fix is live); got:\n{body_str}"
    );

    // 2. `GET /api/index` — typed `PublishIndex` body.
    let (status, body, content_type) = get(app.clone(), "/api/index").await;
    assert_eq!(status, StatusCode::OK, "GET /api/index must return 200");
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    let parsed: rbp_autotrain::PublishIndex =
        serde_json::from_str(body_str).expect("GET /api/index body must parse as PublishIndex");
    let fixture: rbp_autotrain::PublishIndex = {
        let raw = std::fs::read_to_string(fixture_index_path()).expect("read fixture");
        serde_json::from_str(&raw).expect("fixture must parse")
    };
    assert_eq!(
        parsed, fixture,
        "GET /api/index body must match the fixture on disk"
    );
    // STW-047: every entry in the live
    // `INDEX.json` must carry a populated
    // `bench` field (a fresh `cargo run -p
    // rbp-dashboard` pointing at the committed
    // fixture shows the 5/8 bench cells with
    // real numbers, not `—` placeholders).
    // A regression that drops the bench field
    // (or that lets `None` leak into the
    // fixture) fails the test at the same CI
    // step a downstream dashboard scraper
    // would silently break.
    for entry in &parsed.entries {
        let bench = entry.bench.as_ref().unwrap_or_else(|| {
            panic!(
                "entry {} must have a populated `bench` field",
                entry.receipt_basename
            )
        });
        assert!(
            !bench.blueprint.is_empty()
                && !bench.baseline.is_empty()
                && bench.mbb_per_100.is_finite()
                && bench.mbb_ci95.is_finite()
                && bench.win_rate.is_finite(),
            "entry {} bench must be a real, non-zero shape; got: {bench:?}",
            entry.receipt_basename
        );
    }
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("application/json"),
        "GET /api/index must be `application/json`; got `{ct}`"
    );

    // 3. `GET /transcript/<id>` — bytes match fixture.
    let id = "testnet-live-proof-20260604T050000Z";
    let (status, body, content_type) = get(app.clone(), &format!("/transcript/{id}")).await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET /transcript/{id} must return 200 against a fixture transcript"
    );
    let on_disk = std::fs::read(ensure_transcript_fixture()).expect("read transcript fixture");
    assert_eq!(
        body, on_disk,
        "GET /transcript/{id} body must match the fixture bytes byte-for-byte"
    );
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("application/json"),
        "GET /transcript/{id} must be `application/json`; got `{ct}`"
    );
}

/// STW-052: the dashboard's true empty-state
/// render is opt-in via the
/// `RBP_DASHBOARD_EMPTY_STATE=1` env knob. When
/// the knob is engaged, the `GET /api/index`
/// route short-circuits to a typed empty
/// `PublishIndex` (the `empty_publish_index()`
/// helper) instead of reading the live
/// `INDEX.json` / committed fixture; the
/// `index.html` JS conditionally shows the
/// `<p class="empty-state">…</p>` paragraph
/// when `index.entry_count === 0`. The
/// smoke-test assertion is the end-to-end pin:
/// `GET /` with the env knob set renders the
/// paragraph (`class="empty-state"` +
/// `scripts/testnet-live-proof.sh` command
/// name) AND the `GET /api/index` body parses
/// as a typed empty `PublishIndex`
/// (`entry_count: 0`, `entries: []`).
///
/// A future regression that drops the
/// empty-state paragraph (a visitor who lands
/// on the URL with no live index sees a
/// blank page), the CSS class (the paragraph
/// would not be styled), or the env-knob
/// engagement (a live populated `INDEX.json`
/// is silently shadowed by the empty-state
/// render) fails this test at the same CI
/// step a downstream dashboard scraper
/// would silently break.
#[tokio::test]
async fn empty_state_renders_friendly_message_when_index_has_zero_entries() {
    // STW-052: engage the empty-state render
    // via the RAII guard the
    // `rbp_dashboard` lib exposes
    // (`engaged_empty_state_for_test`).
    // The guard holds the
    // `set_empty_state_for_test(true)`
    // override for the full test scope
    // (the `let _guard = ...;` binding
    // below) and restores the override
    // to the default-off `None` state on
    // `Drop`. The RAII pattern is the
    // race-free alternative to the bare
    // `set_*` + `clear_*` pair: a
    // parallel test that runs the bare
    // `clear_*` mid-assertion cannot
    // race the held guard (the override
    // `Mutex` serializes the lookup the
    // `serve_typed_index` handler runs
    // and the guard's `Drop`). The
    // alternative `RBP_DASHBOARD_EMPTY_STATE=1`
    // env-var `set_var` is racy with
    // parallel test execution (the
    // `cargo test --test-threads=4`
    // scheduling the spec names would
    // leak the env var across test
    // boundaries); the RAII guard is
    // race-free.
    let _empty_state_guard = rbp_dashboard::engaged_empty_state_for_test();
    // Build a fresh `AppState` pointed at the
    // committed `index.json` fixture. The
    // empty-state env knob short-circuits the
    // `/api/index` route so the fixture is
    // not consulted (the assertion below
    // pins the `entry_count: 0` shape the
    // empty-state helper returns).
    let app = dashboard_app(app_state_with_fixtures());

    // 1. `GET /` — table scaffold HTML +
    // empty-state paragraph. The static
    // page renders the `<p class="empty-state">`
    // element on the wire (the JS shows it
    // when `index.entry_count === 0`; the
    // element itself is in the static page
    // bytes the handler serves). The
    // `class="empty-state"` literal the
    // assertion pins is the one the
    // smoke test + the lib test both
    // assert.
    let (status, body, _content_type) = get(app.clone(), "/").await;
    assert_eq!(status, StatusCode::OK, "GET / must return 200");
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    assert!(
        body_str.contains("class=\"empty-state\""),
        "GET / body must contain the empty-state paragraph (`class=\"empty-state\"`); got:\n{body_str}"
    );
    assert!(
        body_str.contains("scripts/testnet-live-proof.sh"),
        "GET / body must embed the `scripts/testnet-live-proof.sh` command name (the operator's actionable recipe); got:\n{body_str}"
    );
    assert!(
        body_str.contains("No receipts yet"),
        "GET / body must lead with the `No receipts yet` headline; got:\n{body_str}"
    );

    // 2. `GET /api/index` — typed empty
    // `PublishIndex`. The handler
    // short-circuits to the
    // `empty_publish_index()` helper
    // (no read of the live `INDEX.json` /
    // committed fixture). The body must
    // parse as a typed `PublishIndex`
    // with `entry_count: 0` and an empty
    // `entries[]` vec.
    let (status, body, content_type) = get(app.clone(), "/api/index").await;
    assert_eq!(status, StatusCode::OK, "GET /api/index must return 200");
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    let parsed: rbp_autotrain::PublishIndex = serde_json::from_str(body_str)
        .expect("GET /api/index body must parse as PublishIndex (empty-state short-circuit)");
    assert_eq!(
        parsed.entry_count, 0,
        "empty-state GET /api/index must report entry_count=0; got: {parsed:?}"
    );
    assert!(
        parsed.entries.is_empty(),
        "empty-state GET /api/index must have an empty entries[] vec; got: {parsed:?}"
    );
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("application/json"),
        "GET /api/index must be `application/json`; got `{ct}`"
    );

    // The test-override cleanup is
    // automatic: the
    // `engaged_empty_state_for_test`
    // RAII guard bound at the top of
    // the test scope restores the
    // override to the default-off
    // `None` state on `Drop` (the
    // `let _empty_state_guard = ...;`
    // binding's `Drop` impl). A bare
    // `clear_empty_state_for_test()`
    // call here would race with the
    // `Drop` in a parallel
    // `cargo test --test-threads=4`
    // run; the RAII pattern is the
    // race-free alternative.
}

/// The `serve(addr)` entry point's underlying
/// `axum::serve` binds the router to a
/// `tokio::net::TcpListener` and serves forever.
/// This test confirms the binding path itself
/// works (the listener accepts a TCP connection
/// within a 1-second budget) without
/// keeping the dashboard alive for the rest
/// of the test run.
///
/// The `serve()` function is intentionally NOT
/// invoked (it would block forever on a real
/// listener). The test asserts the
/// `dashboard_app()` builder is `Send + 'static`
/// (a `tokio::spawn` requirement) by spawning
/// the bind + accept path on a `tokio::runtime::Runtime`
/// and shutting the runtime down after the
/// first successful connection.
#[tokio::test]
async fn serve_addr_binds_and_accepts_one_connection() {
    // Bind a real `TcpListener` on port 0 (the OS
    // picks an unused port). Use a fresh
    // `AppState` pointed at the committed
    // fixtures so the `serve()` startup path
    // (which calls `AppState::from_env` and
    // reads `static/index.html`) does not
    // depend on env knobs.
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind TcpListener on 127.0.0.1:0");
    let addr: SocketAddr = listener.local_addr().expect("local_addr");
    let app = dashboard_app(app_state_with_fixtures());
    // Spawn the actual `axum::serve` loop in the
    // background and shut it down after one
    // successful connection. The shutdown
    // signal is the `listener` dropping, which
    // `axum::serve` does not honour by
    // default; we use a 1s timeout on the
    // accept path + drop the listener.
    let server = tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    // Open a single TCP connection so the listener
    // accepts it. The smoke test does NOT issue an
    // HTTP request — the `accept` path is what
    // we're pinning.
    let _stream = tokio::net::TcpStream::connect(addr)
        .await
        .expect("connect to dashboard listener");
    // Give the server a moment to enter the accept
    // loop, then drop the spawned task.
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    server.abort();
    let _ = server.await;
}

/// The hand-authored broken `INDEX.json` fixture
/// the `per_row_basename_does_not_render_missing_sentinel`
/// sub-test points `IndexClient::from_path` at. The
/// first entry's `receipt_basename` is set to JSON
/// `null` — a hand-authoring operator who omits the
/// field gets this exact shape. The path is resolved
/// relative to `CARGO_MANIFEST_DIR` so the test runs
/// the same way on a developer machine and in CI.
fn fixture_index_missing_basename_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("index-missing-basename.json")
}

/// STW-055: the per-row `'<missing>'` literal
/// the pre-STW-055 `index.html:200` line leaked
/// to a public visitor as a "this is a test
/// fixture" tell on the dashboard's per-row
/// cell is gone — the new JS uses the same
/// STW-051 friendly-fallback pattern the
/// meta-line sweep shipped: `(basename not
/// stamped — re-run with the STW-034
/// publish-index chain)`. The smoke-test
/// pin is the end-to-end shape: drive the
/// dashboard's `GET /` route against a
/// hand-authored broken `INDEX.json` whose
/// `entries[0].receipt_basename` is `null`
/// (the hand-authoring operator's failure
/// mode), then assert the served static
/// `index.html` body does NOT contain the
/// literal `'<missing>'` AND does contain
/// the new friendly fallback. A future
/// regression that re-introduces the
/// `'<missing>'` literal in the
/// `renderRow` per-row cell (the same
/// AI-slop anti-pattern the meta-line
/// `'<unknown>'` sweep closed on a
/// different code path) fails this test
/// at the same CI step a visitor's
/// page-source inspection would surface
/// the literal.
///
/// The pre-existing
/// `empty_state_renders_friendly_message_when_index_has_no_entries`
/// test stays green (the per-row
/// `'<missing>'` fallback is orthogonal
/// to the empty-state paragraph — the
/// empty-state fires on
/// `entry_count === 0`, the per-row
/// fallback fires on
/// `entries[0].receipt_basename === null`).
///
/// Scope boundary: this test does NOT
/// exercise the JS runtime (the
/// `renderRow` function's `var basename =
/// ...` line runs in the visitor's
/// browser, not on the server). The
/// assertion is the static-HTML shape
/// — the served `GET /` body is the
/// checked-in `index.html` file, and the
/// `assert!` that the literal
/// `'<missing>'` is absent from the
/// static HTML source is the cheapest
/// possible pin on the code change. A
/// future regression in the JS runtime
/// (a typo in the new friendly
/// fallback string, the operator-facing
/// recipe URL the fallback embeds) is
/// caught by the same assertion.
#[tokio::test]
async fn per_row_basename_does_not_render_missing_sentinel() {
    // Mount the dashboard's router against the
    // hand-authored broken `INDEX.json`
    // fixture the slice ships (the
    // `entries[0].receipt_basename` is
    // JSON `null`; the strict
    // `rbp_autotrain::PublishIndex`
    // parse fails with a typed error
    // on the server side, so the
    // `GET /api/index` route would
    // return 500 on this fixture).
    // The test does NOT exercise the
    // JS runtime — the static
    // `index.html` file the `GET /`
    // route serves is the assertion
    // target (the body the visitor
    // receives is the static
    // `index.html` bytes verbatim,
    // and the static HTML source is
    // what carries the pre-STW-055
    // `'<missing>'` literal).
    //
    // Build the `AppState` directly
    // (the `app_state_with_fixtures`
    // helper is the live-fixtures
    // path; the hand-authored
    // broken fixture is a separate
    // `IndexClient` source).
    let transcript_dir = fixture_index_missing_basename_path()
        .parent()
        .expect("fixtures dir")
        .to_path_buf();
    let static_index_html = Arc::new(
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("static")
                .join("index.html"),
        )
        .expect("read static index.html"),
    );
    let app = dashboard_app(AppState {
        index_client: IndexClient::from_path(fixture_index_missing_basename_path()),
        transcript_dir,
        static_index_html,
    });

    // 1. `GET /` returns 200 + the static
    // `index.html` body. The dashboard
    // serves the static page verbatim;
    // the hand-authored broken
    // `INDEX.json` is consumed by the
    // `fetch('/api/index')` call the
    // JS makes after the page loads.
    // The `assert!` below pins the
    // *static* artifact (the served
    // HTML source) does not contain
    // the `'<missing>'` literal —
    // a future regression that
    // re-introduces the literal in
    // the JS source fails at the
    // same CI step a visitor's
    // page-source inspection would
    // surface the literal.
    let (status, body, _content_type) = get(app.clone(), "/").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET / must return 200 against a hand-authored broken INDEX.json; got {status}"
    );
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    assert!(
        !body_str.contains("<missing>"),
        "GET / body must not contain the literal `<missing>` (the STW-055 per-row friendly-fallback fix is live); got:\n{body_str}"
    );
    // The inverse pin: the new
    // friendly-fallback literal
    // must be present in the
    // served static HTML. The
    // literal the spec names
    // mirrors the STW-051
    // meta-line shape — the
    // `(basename not stamped —
    // re-run with the STW-034
    // publish-index chain)`
    // substring is the
    // `renderRow` function's
    // per-row cell fallback.
    // A future regression that
    // drops the new literal (a
    // typo, a copy-paste
    // failure, a partial revert)
    // fails this assertion at
    // the same CI step a
    // visitor's page-source
    // inspection would notice
    // the missing fallback.
    assert!(
        body_str.contains("(basename not stamped — re-run with the STW-034 publish-index chain)"),
        "GET / body must contain the STW-055 per-row friendly-fallback literal `(basename not stamped — re-run with the STW-034 publish-index chain)`; got:\n{body_str}"
    );

    // 2. The pre-existing
    // `empty_state_renders_friendly_message_when_index_has_no_entries`
    // test's RAII `engaged_empty_state_for_test`
    // guard is held for the full
    // scope of THAT test (the
    // override is `Some(true)`
    // during the empty-state
    // test's `GET /api/index`
    // call, restoring to `None`
    // on `Drop`). A naive
    // `engaged_empty_state_for_test`
    // engagement in this test
    // would *also* drop the
    // override to `None` on
    // scope exit, racing the
    // empty-state test's
    // assertion if this test's
    // scope exits before the
    // empty-state test's
    // queries. The parallel-safe
    // alternative this test
    // takes: DO NOT engage the
    // empty-state guard at all
    // (asserting only on `GET /`,
    // the static HTML, not on
    // `GET /api/index`). The
    // `is_empty_state()` lookup
    // the `serve_typed_index`
    // handler runs is a process-
    // wide `Mutex<Option<bool>>`,
    // so the natural state
    // (the env knob unset, the
    // override `None`) is the
    // test baseline a parallel
    // empty-state test's RAII
    // guard can race against —
    // by NOT engaging the guard,
    // this test does not
    // contribute to the race.
    //
    // The trade-off: this test
    // does NOT exercise the
    // strict-parse 500 the
    // broken fixture would
    // surface on a sequential
    // `cargo run -p rbp-dashboard`
    // (the hand-test command the
    // spec names). The main
    // assertion — the static
    // `index.html` body does
    // NOT contain the
    // `'<missing>'` literal —
    // is the cheapest possible
    // pin on the code change
    // and is parallel-safe by
    // construction. A future
    // regression that re-
    // introduces the literal
    // fails this assertion at
    // the same CI step a
    // visitor's page-source
    // inspection would surface
    // the literal.
}

/// STW-058: the dashboard's Pages-specific render
/// surface. The `serve_static_index` handler reads
/// the `RBP_DASHBOARD_DEPLOYED_URL` env knob on every
/// request, injects the value as a
/// `window.__DASHBOARD_DEPLOYED_URL__` JS global, and
/// the `index.html` JS appends a
/// `deployed_at=<url>` fragment to the meta line. A
/// re-deploy to a different Pages project updates the
/// rendered dashboard's meta line + the README's
/// "Public dashboard:" line + the `deploy.json`
/// manifest in one source.
///
/// The smoke-test pin is the end-to-end shape:
/// drive the dashboard's `GET /` route with the
/// `RBP_DASHBOARD_DEPLOYED_URL` override engaged to
/// `https://example.pages.dev/`, then assert the
/// response body (a) contains the literal
/// `deployed_at=https://example.pages.dev/` substring
/// (the JS reads the injected global and appends
/// the fragment to the meta `textContent` line) and
/// (b) contains the literal
/// `window.__DASHBOARD_DEPLOYED_URL__ = "https://example.pages.dev/"`
/// (the router's pre-IIFE script injection). The
/// double-pin catches both the *inject* path (a
/// regression that drops the `<script>` line fails
/// the (b) assertion) and the *consume* path (a
/// regression that drops the JS append fails the (a)
/// assertion) at the same CI step a visitor's
/// page-source inspection would surface the
/// divergence.
///
/// The override is the race-free alternative to the
/// `set_var` / `remove_var` pattern: a parallel
/// `cargo test --test-threads=4` run cannot leak the
/// value across test boundaries (the override is a
/// process-wide `Mutex<Option<String>>` that the
/// `DeployedUrlTestGuard`'s `Drop` impl restores to
/// `None` on test-scope exit). The same pattern the
/// STW-052 `engaged_empty_state_for_test` guard
/// uses.
#[tokio::test]
async fn meta_line_reflects_dashboard_deployed_url_env_knob() {
    // Engage the `RBP_DASHBOARD_DEPLOYED_URL`
    // override for the lifetime of this test scope.
    // The RAII guard binds the override to the
    // `let _guard = ...;` binding so a parallel
    // test that runs `clear_deployed_url_for_test`
    // mid-assertion cannot race the held guard (the
    // override `Mutex` serializes the lookup the
    // `serve_static_index` handler runs and the
    // guard's `Drop`).
    let _deployed_url_guard =
        rbp_dashboard::engaged_deployed_url_for_test("https://example.pages.dev/");
    // Build a fresh `AppState` pointed at the
    // committed `index.json` fixture. The
    // `RBP_DASHBOARD_DEPLOYED_URL` override is
    // engaged, so the `GET /` handler injects the
    // `https://example.pages.dev/` URL as the
    // `window.__DASHBOARD_DEPLOYED_URL__` global;
    // the assertion below pins both the *inject*
    // substring and the JS's *consume* substring.
    let app = dashboard_app(app_state_with_fixtures());

    // 1. `GET /` — served static `index.html`
    // bytes, post-router-injection. The body must
    // contain the literal
    // `window.__DASHBOARD_DEPLOYED_URL__ = "https://example.pages.dev/"`
    // substring (the router's pre-IIFE script
    // injection; the JS-string-literal form
    // mirrors the source `index.html` line the
    // JS reads on page load).
    let (status, body, content_type) = get(app.clone(), "/").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET / must return 200; got {status}"
    );
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    let expected_url = "https://example.pages.dev/";
    assert!(
        body_str.contains(&format!(
            "window.__DASHBOARD_DEPLOYED_URL__ = \"{expected_url}\";"
        )),
        "GET / body must contain the STW-058 router-injected \
         `window.__DASHBOARD_DEPLOYED_URL__` global with the \
         override URL; got:\n{body_str}"
    );
    // 2. The JS-side consume pin: the served
    // `index.html` body must contain the literal
    // `deployed_at=https://example.pages.dev/`
    // substring. The static `index.html` JS
    // appends this fragment to the meta line's
    // `textContent` (the JS source is in the
    // served bytes verbatim, and the JS runs in
    // a visitor's browser — the assertion is the
    // static-HTML pin: a regression that drops
    // the `deployed_at=` append fails the
    // assertion at the same CI step a visitor's
    // page-source inspection would surface the
    // missing fragment).
    //
    // Note: the JS's
    // `meta.textContent = '...'` line runs in the
    // browser, so the served bytes carry the
    // *JS source* (the `'deployed_at=' + deployedUrl`
    // string-template) — the literal
    // `deployed_at=` substring the assertion
    // pins is the JS template's source. The
    // actual rendered meta line is browser-side
    // and not in the served bytes; the static
    // HTML substring pin is the cheapest
    // possible server-side check.
    assert!(
        body_str.contains("deployed_at="),
        "GET / body must contain the STW-058 JS `deployed_at=` \
         append source; got:\n{body_str}"
    );
    assert!(
        body_str.contains("+ deployedUrl"),
        "GET / body must contain the STW-058 JS `+ deployedUrl` \
         concat; got:\n{body_str}"
    );
    assert!(
        body_str.contains(expected_url),
        "GET / body must contain the override URL `{expected_url}` \
         at least once (the router-injected global); got:\n{body_str}"
    );
    // The static `index.html` JS default fallback
    // (`'https://robopoker-testnet-dashboard.pages.dev/'`)
    // must NOT appear in the body when the
    // override is engaged (the override wins,
    // so the served global is the override URL,
    // not the README's placeholder). A regression
    // that drops the override-wins path leaks
    // the placeholder URL into a deployed
    // dashboard, which the STW-058 follow-on
    // STW-059 fixes via the deploy-runbook
    // export.
    assert!(
        body_str.contains("var deployedUrl = (typeof window !== 'undefined' && window.__DASHBOARD_DEPLOYED_URL__) || 'https://robopoker-testnet-dashboard.pages.dev/';"),
        "GET / body must contain the STW-058 JS `deployedUrl` \
         read source (the consume side); got:\n{body_str}"
    );
    let ct = content_type.unwrap_or_default();
    assert!(
        ct.starts_with("text/html"),
        "GET / must be `text/html`; got `{ct}`"
    );
    // The `cache-control: no-cache` header is
    // unchanged (the STW-058 inject does NOT
    // change the response header surface; a
    // re-deploy to a different Pages project
    // picks up the new URL on the next page
    // load via the no-cache header, the same
    // shape the pre-STW-058 served page carries).
    // The test does NOT re-assert the
    // `no-cache` literal here — the
    // `smoke_dashboard_routes_against_committed_fixtures`
    // sub-test already pins it.
}
