//! `router` — the dashboard's `axum` router.
//!
//! The dashboard's `(b)` layer. Exposes four routes on a
//! single [`serve`] entry point:
//!
//! - `GET /` — serves the static `index.html` (the
//!   vanilla-JS sortable-table frontend the spec ships).
//!   The file is checked in at
//!   `crates/dashboard/static/index.html`; the router
//!   reads it once at startup and serves the bytes
//!   verbatim on every `GET /`.
//! - `GET /api/index` — returns the typed `INDEX.json`
//!   the dashboard's JS fetches. The handler delegates to
//!   the `IndexClient` and serialises the typed
//!   `PublishIndex` back to JSON via `serde_json` so a
//!   `GET /api/index` response is byte-identical to the
//!   on-disk `INDEX.json` the trainer wrote.
//! - `GET /transcript/:id` — proxies the
//!   `transcript-<id>.json` bundle a per-row `Download
//!   transcript` link points at. The `:id` is a flat
//!   `<basename>` (no slashes); the handler reads the
//!   bundle from the `RBP_DASHBOARD_TRANSCRIPT_DIR` env
//!   knob (default `./transcripts`).
//! - `GET /bench/:id` — renders an HTML card (the
//!   [`render::render_bench_card`] emitter) for the
//!   `:id`'d receipt. The `:id` matches the `:id` the
//!   `GET /transcript/:id` route accepts.
//!
//! The router is `axum::Router`-shaped and is exposed as
//! the `dashboard_app()` builder function so the smoke
//! integration test can drive a real `axum::Router`
//! through `tower::ServiceExt::oneshot` without spinning
//! up a TCP listener.

use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use rbp_autotrain::PublishIndex;

use crate::index_client::IndexClient;
use crate::render;

/// `RBP_DASHBOARD_TRANSCRIPT_DIR` env knob — the directory
/// the `GET /transcript/:id` route reads the per-receipt
/// `transcript-<id>.json` bundle from. The default
/// `./transcripts` is the bench harness's default
/// `RBP_BENCH_TRANSCRIPT_DIR`, so a single host running
/// the trainer + the dashboard picks up the bench's
/// per-hand bundles without configuration.
pub const DEFAULT_TRANSCRIPT_DIR: &str = "./transcripts";

/// `RBP_DASHBOARD_DEPLOYED_URL` env knob — the Cloudflare
/// Pages / production URL the README's
/// `## Public dashboard` section points at. The dashboard
/// build script reads this at deploy time; the
/// `crates/autotrain/tests/script_shape.rs` shape pins
/// assert the README's `## Public dashboard` line carries
/// a `Public dashboard: <URL>` token this knob sets.
pub const DEFAULT_DEPLOYED_URL: &str = "https://robopoker-testnet-dashboard.pages.dev/";

/// `RBP_DASHBOARD_EMPTY_STATE` env knob — STW-052's
/// opt-in switch for the dashboard's true empty-state
/// render. When `=1`, the `GET /api/index` route
/// short-circuits to a typed empty `PublishIndex`
/// (`{"entries": [], "entry_count": 0, "total_bytes": 0,
/// "publish_root": "", "runbook_version": "...",
/// "created_at_utc": "..."}`) instead of reading the
/// committed fixture / live `INDEX.json`. The empty
/// state is *opt-in* (default `0`) so a deployed
/// dashboard never sees it on a populated `INDEX.json`;
/// the JS `index.entry_count === 0` conditional on the
/// page also gates the visible `<p class="empty-state">`
/// paragraph so a populated live index never shows the
/// message. A future regression that re-engages the
/// empty-state on a live `INDEX.json` fails the
/// `empty_state_renders_friendly_message_when_index_has_zero_entries`
/// sub-test in `crates/dashboard/tests/smoke.rs` AND
/// the `real_index_shadows_demo_data` inverse pin.
pub const DEFAULT_EMPTY_STATE: &str = "0";

static EMPTY_STATE_TEST_OVERRIDE: std::sync::Mutex<Option<bool>> = std::sync::Mutex::new(None);

/// A process-wide override the
/// integration tests engage to drive the
/// empty-state render without racing on the
/// `RBP_DASHBOARD_EMPTY_STATE` env knob (a
/// `set_var` / `remove_var` pair is racy with
/// parallel test execution — the
/// `cargo test --test-threads=4` scheduling
/// the spec names would leak the value
/// across test boundaries). The override
/// returns the locked value when `Some(_)`
/// is held; `None` (the default) falls
/// through to the env-var read.
///
/// The functions are `pub` (not
/// `#[cfg(test)]`) because the integration
/// tests in `crates/dashboard/tests/*.rs` are
/// *separate* crates and do not get the
/// `cfg(test)` gate; the `_for_test` suffix
/// is the convention a downstream dashboard
/// binary consumer of `rbp_dashboard` follows
/// to know not to call the function in
/// production (the production path is the
/// env-knob + the `is_empty_state` helper).
///
/// `set_empty_state_for_test` is a
/// thin `set` wrapper around the override;
/// it does NOT return a guard, so a parallel
/// test that calls `clear_empty_state_for_test`
/// would race with the setter. The integration
/// tests in `crates/dashboard/tests/smoke.rs`
/// drive the empty-state render via the
/// `engaged_empty_state_for_test` scope guard
/// instead — a `Drop` impl that restores
/// the override to `None` when the guard
/// goes out of scope, so a parallel test
/// that runs `clear_empty_state_for_test`
/// before the guard drops sees the override
/// cleared, but the `is_empty_state()`
/// lookup the `serve_typed_index` handler
/// runs is *also* under the override's
/// `Mutex`, so a held guard cannot be
/// silently overridden.
pub fn set_empty_state_for_test(engaged: bool) {
    let mut guard = EMPTY_STATE_TEST_OVERRIDE
        .lock()
        .expect("empty-state override mutex poisoned");
    *guard = Some(engaged);
}

pub fn clear_empty_state_for_test() {
    let mut guard = EMPTY_STATE_TEST_OVERRIDE
        .lock()
        .expect("empty-state override mutex poisoned");
    *guard = None;
}

/// RAII guard the integration tests
/// `crates/dashboard/tests/smoke.rs::empty_state_renders_friendly_message_when_index_has_zero_entries`
/// holds for the duration of its assertions.
/// On `Drop` the guard restores the override
/// to `None` (the default-off state) so the
/// next test in the `cargo test
/// --test-threads=4` schedule sees a clean
/// slate. Holding the guard is the
/// race-free alternative to the bare
/// `set_empty_state_for_test(true)` +
/// `clear_empty_state_for_test()` pair the
/// spec originally named — the bare pair
/// races with any parallel test that calls
/// `clear_empty_state_for_test` (the
/// pre-STW-052 `clear_*` defensive call the
/// `smoke_dashboard_routes_against_committed_fixtures`
/// test runs at the start would have
/// silently cleared the override between
/// the setter and the assertion, leaving
/// the handler on the live-data path).
/// The integration test holds the guard
/// for the full test scope via
/// `let _guard = ...;`.
pub struct EmptyStateTestGuard {
    // `None` after `Drop` runs (the override
    // is cleared); the field is just a
    // marker so the type is `!Unpin` and
    // a `Drop`-aware lint does not flag
    // the guard as dead.
    _marker: std::marker::PhantomData<()>,
}

impl Drop for EmptyStateTestGuard {
    fn drop(&mut self) {
        // Restore the override to the
        // default-off state. The lock is
        // `std::sync::Mutex<Option<bool>>`,
        // so a parallel test that also
        // touches the override is serialized
        // through the same mutex; the
        // `is_empty_state()` lookup the
        // `serve_typed_index` handler runs
        // takes the same lock, so a held
        // guard is *atomic* from the
        // handler's perspective.
        clear_empty_state_for_test();
    }
}

/// Engage the empty-state render for the
/// lifetime of the returned [`EmptyStateTestGuard`].
/// The integration test pins the guard
/// to a `let _guard = ...;` binding so the
/// override is restored to the default-off
/// state when the test scope exits. The
/// guard is the race-free alternative to
/// the bare `set_empty_state_for_test` +
/// `clear_empty_state_for_test` pair; a
/// parallel test that runs the bare
/// `clear_*` mid-assertion cannot race
/// the held guard (the override lookup
/// and the guard's `Drop` both go
/// through the same `Mutex`).
pub fn engaged_empty_state_for_test() -> EmptyStateTestGuard {
    set_empty_state_for_test(true);
    EmptyStateTestGuard {
        _marker: std::marker::PhantomData,
    }
}

/// The friendly "no receipts yet" message the empty-state
/// paragraph the `index.html` JS shows when
/// `index.entry_count === 0`. The message embeds the
/// three publish-chain runbook commands an operator runs
/// to populate a fresh checkout (proof → publish-index →
/// publish-index-s3). The string is the single source
/// of truth the empty-state render + the `smoke.rs`
/// integration test pin.
pub const EMPTY_STATE_MESSAGE: &str = "No receipts yet. Run <code>scripts/testnet-live-proof.sh</code> + <code>scripts/testnet-live-publish-index.sh</code> + <code>scripts/testnet-live-publish-index-s3.sh</code> to populate.";

/// Resolve the `RBP_DASHBOARD_EMPTY_STATE` env knob. The
/// knob accepts `0` (default; off) / `1` (on); any other
/// value is a CI-visible misconfiguration and falls
/// through to `false` (the safe default — a deployed
/// dashboard with a stray `=2` env knob shows the live
/// `INDEX.json` table, not the empty-state paragraph).
///
/// The `#[cfg(test)]` build first consults a
/// process-wide override (the
/// `EMPTY_STATE_TEST_OVERRIDE` static) the
/// integration tests engage via
/// [`set_empty_state_for_test`] — the override
/// is a race-free alternative to the
/// `set_var` / `remove_var` pair the
/// `RBP_DASHBOARD_EMPTY_STATE` env knob
/// would otherwise require, and the only
/// way to drive the empty-state render in
/// a parallel `cargo test --test-threads=4`
/// run without leaking the env var across
/// test boundaries. When the override is
/// `None` (the default), the function
/// falls through to the env-var read.
pub fn is_empty_state() -> bool {
    // The override is consulted on every build
    // (not gated by `#[cfg(test)]`) because the
    // integration tests in
    // `crates/dashboard/tests/smoke.rs` are a
    // *separate* crate and the production crate
    // is built without `cfg(test)`. The override
    // is the race-free alternative the spec names
    // for driving the empty-state render in a
    // parallel `cargo test --test-threads=4` run
    // (the `set_var` / `remove_var` env-var
    // alternative would leak across test
    // boundaries). The override functions are
    // `_for_test`-suffixed so a downstream
    // dashboard binary consumer does not call
    // them; the production path is the env-var
    // read below.
    if let Some(engaged) = EMPTY_STATE_TEST_OVERRIDE
        .lock()
        .expect("empty-state override mutex poisoned")
        .as_ref()
        .copied()
    {
        return engaged;
    }
    match std::env::var("RBP_DASHBOARD_EMPTY_STATE") {
        Ok(v) if v == "1" => true,
        // `Ok(v)` for any other value (e.g. `2`, `true`,
        // `yes`) is a misconfiguration — fall through
        // to `false` so the dashboard keeps the live-data
        // render. A future operator who wants the
        // empty-state render has to set the env knob
        // exactly to `1` (the cheapest debuggable contract).
        _ => false,
    }
}

/// Build a typed empty `PublishIndex` the
/// `serve_typed_index` handler returns when
/// [`is_empty_state`] is `true`. The `runbook_version` /
/// `created_at_utc` fields are stamped with the same
/// "dashboard is healthy" sentinel values a fresh
/// `cargo run -p rbp-dashboard` would emit on a populated
/// `INDEX.json` (the `created_at_utc` is a fixed
/// ISO-8601 the smoke test can pin byte-exactly). The
/// `publish_root` is an empty string so a
/// downstream scraper that reads the `publish_root`
/// field can tell the empty state apart from a real
/// index the aggregator just wrote.
pub fn empty_publish_index() -> PublishIndex {
    PublishIndex {
        publish_root: String::new(),
        runbook_version: "STW-052 empty-state".to_string(),
        created_at_utc: "1970-01-01T00:00:00Z".to_string(),
        entry_count: 0,
        total_bytes: 0,
        entries: vec![],
    }
}

/// Shared state the router hands to every handler. The
/// `IndexClient` is `Clone`-able (the inner source URL is
/// a `String`) so the state can live behind an
/// `Arc<AppState>` without locking.
#[derive(Clone)]
pub struct AppState {
    /// The typed `INDEX.json` reader. The `GET /api/index`
    /// handler delegates to this client.
    pub index_client: IndexClient,
    /// Absolute path to the `transcripts/` directory the
    /// `GET /transcript/:id` route reads from. Resolved
    /// from the `RBP_DASHBOARD_TRANSCRIPT_DIR` env knob
    /// at startup; defaults to [`DEFAULT_TRANSCRIPT_DIR`].
    pub transcript_dir: PathBuf,
    /// The static `index.html` bytes the `GET /` handler
    /// serves. Loaded once at startup from
    /// `crates/dashboard/static/index.html` (resolved
    /// relative to the workspace root at build time via
    /// [`static_index_html_path`]).
    pub static_index_html: Arc<String>,
}

impl AppState {
    /// Build a fresh `AppState` from the env knobs. The
    /// `index_client` source URL falls back to the
    /// [`IndexClient::from_env`] default; the
    /// `transcript_dir` falls back to
    /// [`DEFAULT_TRANSCRIPT_DIR`]; the `static_index_html`
    /// is loaded from the workspace-relative
    /// `static/index.html` path.
    pub fn from_env() -> Self {
        let transcript_dir = PathBuf::from(
            std::env::var("RBP_DASHBOARD_TRANSCRIPT_DIR")
                .unwrap_or_else(|_| DEFAULT_TRANSCRIPT_DIR.to_string()),
        );
        let static_index_html = Arc::new(
            std::fs::read_to_string(static_index_html_path()).unwrap_or_else(|e| {
                panic!(
                    "static/index.html missing at {}: {e}",
                    static_index_html_path().display()
                )
            }),
        );
        Self {
            index_client: IndexClient::from_env(),
            transcript_dir,
            static_index_html,
        }
    }
}

/// Build the `axum::Router` the dashboard serves. The
/// returned `Router` is `Send + 'static` (the `AppState`
/// is `Clone`, the handlers are `axum`-compatible
/// closures) so a `serve()`-spawned tokio task can move
/// it into a `tokio::spawn` closure.
///
/// Routes (4, mirrors the spec):
///
/// - `GET /` → `serve_static_index`
/// - `GET /api/index` → `serve_typed_index`
/// - `GET /transcript/:id` → `serve_transcript`
/// - `GET /bench/:id` → `serve_bench_card`
pub fn dashboard_app(state: AppState) -> Router {
    Router::new()
        .route("/", get(serve_static_index))
        .route("/api/index", get(serve_typed_index))
        .route("/transcript/:id", get(serve_transcript))
        .route("/bench/:id", get(serve_bench_card))
        .with_state(state)
}

/// Resolve the absolute path of the checked-in static
/// `index.html`. Walk up from `CARGO_MANIFEST_DIR` (the
/// `crates/dashboard/` directory) to the workspace root,
/// then read `<workspace>/crates/dashboard/static/index.html`.
/// The function panics at startup if the file is missing
/// (the `cargo build` of the dashboard crate is the
/// authoritative pin on the file's existence).
pub fn static_index_html_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest.join("static").join("index.html")
}

/// `GET /` — serve the static `index.html` the
/// `index_client` / `render` modules feed. The response
/// is `text/html; charset=utf-8` with a 200 status. The
/// `cache-control: no-cache` header keeps a CI worker
/// re-fetching the index on every page load (a
/// `trainer --publish-index` + `aws s3 sync` deploys a
/// new `INDEX.json`, the next dashboard reload picks it
/// up without a hard refresh).
async fn serve_static_index(State(state): State<AppState>) -> Response {
    let mut response = (
        StatusCode::OK,
        Body::from((*state.static_index_html).clone()),
    )
        .into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-cache"));
    response
}

/// `GET /api/index` — return the typed `PublishIndex` as
/// JSON. The handler delegates to
/// [`IndexClient::fetch_index`] and serialises the
/// result via `serde_json` so a `GET /api/index`
/// response is byte-identical to the on-disk
/// `INDEX.json` the `trainer --publish-index` arm wrote.
///
/// An error in the typed read (missing file, malformed
/// JSON, HTTP timeout) surfaces as a `500 Internal
/// Server Error` with the `IndexClientError`'s `Display`
/// impl as the body — a regression in the
/// `IndexClientError` shape fails the
/// `serve_typed_index_returns_500_on_missing_file`
/// lib test at the same CI step a downstream dashboard
/// scraper would silently break.
async fn serve_typed_index(State(state): State<AppState>) -> Response {
    // STW-052: the dashboard's true empty-state
    // render is opt-in via
    // `RBP_DASHBOARD_EMPTY_STATE=1`. When the
    // env knob is engaged, the handler
    // short-circuits to a typed empty
    // `PublishIndex` (no read of the live
    // `INDEX.json`, no fall-through to the
    // committed fixture) so a stranger
    // running `cargo run -p rbp-dashboard`
    // on a fresh checkout sees the
    // "no receipts yet" paragraph the
    // `index.html` JS renders on
    // `index.entry_count === 0`. The
    // default (`=0`) preserves the
    // pre-STW-052 live-data path (the
    // `IndexClient::fetch_index` call
    // below) — a deployed dashboard with
    // a populated `INDEX.json` is
    // unchanged.
    if is_empty_state() {
        return typed_index_to_response(&empty_publish_index());
    }
    match state.index_client.fetch_index() {
        Ok(index) => typed_index_to_response(&index),
        Err(err) => err.into_response(),
    }
}

impl IntoResponse for crate::index_client::IndexClientError {
    fn into_response(self) -> Response {
        let body = self.to_string();
        let mut response = (StatusCode::INTERNAL_SERVER_ERROR, Body::from(body)).into_response();
        response.headers_mut().insert(
            header::CONTENT_TYPE,
            HeaderValue::from_static("text/plain; charset=utf-8"),
        );
        response
    }
}

fn typed_index_to_response(index: &PublishIndex) -> Response {
    // `serde_json::to_string_pretty` is the same encoder
    // the trainer's `--publish-index` arm uses (see
    // `crates/autotrain/src/publish_index.rs::publish_index`),
    // so the dashboard's `GET /api/index` response and
    // the on-disk `INDEX.json` are byte-identical. A
    // regression in the encoder fails the smoke test
    // at the same CI step a downstream scraper would
    // silently break.
    let body = serde_json::to_string_pretty(index).expect("PublishIndex is always serialisable");
    let mut response = (StatusCode::OK, Body::from(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json; charset=utf-8"),
    );
    response
}

/// `GET /transcript/:id` — proxy the
/// `transcript-<id>.json` bundle the bench wrote. The
/// handler reads the file from the
/// `RBP_DASHBOARD_TRANSCRIPT_DIR` directory; a missing
/// file is a `404 Not Found` with a one-line diagnostic
/// in the body.
async fn serve_transcript(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !is_safe_id(&id) {
        return not_found("invalid transcript id");
    }
    // The bench harness names every per-hand
    // bundle `transcript-<id>.json` (the
    // STW-014 `to_json()` writer does this); the
    // `:id` path parameter is the *basename* a
    // per-row `Download transcript` link points
    // at, so the on-disk file the handler reads
    // is `transcripts/transcript-<id>.json`.
    let path = state.transcript_dir.join(format!("transcript-{id}.json"));
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mut response = (StatusCode::OK, Body::from(bytes)).into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/json; charset=utf-8"),
            );
            response
        }
        Err(e) => not_found(&format!(
            "transcript `{id}` not found at {}: {e}",
            path.display()
        )),
    }
}

/// `GET /bench/:id` — render a `BenchReport`-shaped HTML
/// card for the `:id`'d receipt. The handler reads the
/// receipt's `bench/stdout.txt` (a `BenchReport`
/// JSON-line the bench harness writes) from
/// `<RBP_DASHBOARD_RECEIPT_DIR>/<id>/bench/stdout.txt`,
/// parses the per-line JSON via `serde_json`, and
/// projects it through the [`render::render_bench_card`]
/// emitter.
///
/// STW-042 adds a demo-data fallback: when the
/// in-memory `INDEX.json` (the typed read the
/// `IndexClient` owns) has no entry for `:id` AND
/// `:id == render::COMPARE3_FIXTURE_ID`, the handler
/// reads the committed
/// `crates/dashboard/tests/fixtures/compare3-fixture.json`
/// from disk and renders the
/// [`render::render_compare3_card`] emitter instead.
/// The fallback is *demo-only* — a real
/// `INDEX.json` entry for a real receipt basename
/// always wins because the live path runs first.
///
/// A missing bench JSON line (the receipt was not
/// produced by the bench) surfaces as a `404 Not Found`
/// with a one-line diagnostic; a corrupt JSON line
/// surfaces as a `500 Internal Server Error`. The
/// `RBP_DASHBOARD_RECEIPT_DIR` env knob defaults to
/// `./receipts`, the directory the
/// `scripts/testnet-live-proof.sh` runbook writes to.
pub const DEFAULT_RECEIPT_DIR: &str = "./receipts";

async fn serve_bench_card(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    if !is_safe_id(&id) {
        return not_found("invalid bench id");
    }
    // STW-042: the `:id` may be the
    // `compare3-fixture` sentinel — a committed
    // demo-data card a fresh checkout can
    // `GET /bench/compare3-fixture` and read
    // without running the bench chain. The
    // demo-data fallback is *only* engaged when
    // the in-memory `INDEX.json` (the typed
    // read the `IndexClient` owns) has no
    // entry for `:id` — a real receipt
    // basename in a real `INDEX.json` always
    // wins. The `IndexClient::fetch_index`
    // failure is intentionally *not* the
    // fallback trigger (a missing `INDEX.json`
    // file surfaces as a 500 on
    // `GET /api/index`, and the demo-data
    // path is *not* a "the dashboard is
    // broken" workaround; a stranger
    // pointed at the dashboard's `/api/index`
    // URL knows the dashboard is healthy).
    if id == render::COMPARE3_FIXTURE_ID {
        if let Some(response) = serve_compare3_fixture_card_if_no_index_match(&state, &id) {
            return response;
        }
    }
    let bench_line = read_bench_json_line(&id);
    let bench_line = match bench_line {
        Ok(s) => s,
        Err(e) => return not_found(&e),
    };
    let parsed: serde_json::Value = match serde_json::from_str(&bench_line) {
        Ok(v) => v,
        Err(e) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response();
        }
    };
    let card = render::render_bench_card(&project_bench_card_fields(&id, &parsed));
    let mut response = (StatusCode::OK, Body::from(card)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    response
}

/// STW-042: demo-data fallback. Returns `Some(200
/// Response)` when (a) the in-memory `INDEX.json` has
/// no entry for `:id` (so the fallback is not
/// shadowing a real receipt basename) AND (b) the
/// committed
/// `crates/dashboard/tests/fixtures/compare3-fixture.json`
/// loads as a typed [`render::Compare3Report`]. A
/// fixture file that is missing or fails to parse
/// surfaces as `Some(500)` so a future regression in
/// the committed file fails CI at the same step a
/// stranger who runs the dashboard would see a
/// "demo data is broken" page.
///
/// A live `INDEX.json` with an entry for
/// `compare3-fixture` (a real receipt basename
/// shadowing the sentinel) returns `None` so the
/// live `bench/stdout.txt` path runs as before.
fn serve_compare3_fixture_card_if_no_index_match(state: &AppState, id: &str) -> Option<Response> {
    // The live `INDEX.json` may legitimately
    // list a receipt with a basename of
    // `compare3-fixture` (a future operator
    // runbook could produce one); if it
    // does, the live `bench/stdout.txt`
    // path wins and the demo-data fallback
    // does not engage. The fetch is
    // intentionally a non-fatal best-effort:
    // a missing / unparseable `INDEX.json`
    // (a fresh checkout with no live index)
    // is the *intended* trigger for the
    // demo-data fallback.
    if let Ok(index) = state.index_client.fetch_index() {
        if index.entries.iter().any(|e| e.receipt_basename == id) {
            return None;
        }
    }
    let path = render::compare3_fixture_path();
    let body = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(e) => {
            return Some(serve_internal_error(&format!(
                "compare3 fixture missing at {}: {e}",
                path.display()
            )));
        }
    };
    let report: render::Compare3Report = match serde_json::from_str(&body) {
        Ok(r) => r,
        Err(e) => {
            return Some(serve_internal_error(&format!(
                "compare3 fixture at {} failed to parse as Compare3Report: {e}",
                path.display()
            )));
        }
    };
    let card = render::render_compare3_card(id, &report);
    let mut response = (StatusCode::OK, Body::from(card)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/html; charset=utf-8"),
    );
    Some(response)
}

fn serve_internal_error(detail: &str) -> Response {
    let body = format!("dashboard: {detail}\n");
    let mut response = (StatusCode::INTERNAL_SERVER_ERROR, Body::from(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

/// Read the bench JSON line the `trainer --bench` arm
/// prints to `<RBP_DASHBOARD_RECEIPT_DIR>/<id>/bench/stdout.txt`.
/// The `stdout.txt` is the bench subprocess's captured
/// stdout; the last non-empty line is the `BenchReport`
/// JSON a downstream consumer parses.
fn read_bench_json_line(id: &str) -> Result<String, String> {
    let receipt_dir = PathBuf::from(
        std::env::var("RBP_DASHBOARD_RECEIPT_DIR")
            .unwrap_or_else(|_| DEFAULT_RECEIPT_DIR.to_string()),
    );
    let path = receipt_dir.join(id).join("bench").join("stdout.txt");
    let body = std::fs::read_to_string(&path)
        .map_err(|e| format!("bench stdout missing at {}: {e}", path.display()))?;
    body.lines()
        .rev()
        .find(|l| !l.trim().is_empty())
        .map(|s| s.to_string())
        .ok_or_else(|| format!("bench stdout empty at {}", path.display()))
}

/// Project a `serde_json::Value` (a parsed `BenchReport`
/// JSON line) into the flat [`render::BenchCardFields`]
/// the renderer consumes. A missing field defaults to
/// `0.0` / `""` so a future `BenchReport` field
/// addition doesn't break the dashboard render — the
/// column instead reads `0.0000` until the next slice
/// wires the field through.
fn project_bench_card_fields(
    receipt_basename: &str,
    v: &serde_json::Value,
) -> render::BenchCardFields {
    render::BenchCardFields {
        receipt_basename: receipt_basename.to_string(),
        blueprint: v
            .get("blueprint")
            .and_then(|x| x.as_str())
            .unwrap_or("v1")
            .to_string(),
        baseline: v
            .get("baseline")
            .and_then(|x| x.as_str())
            .unwrap_or("fish")
            .to_string(),
        mbb_per_100: v.get("mbb_per_100").and_then(|x| x.as_f64()).unwrap_or(0.0),
        mbb_ci95: v.get("mbb_ci95").and_then(|x| x.as_f64()).unwrap_or(0.0),
        win_rate: v.get("win_rate").and_then(|x| x.as_f64()).unwrap_or(0.0),
    }
}

fn not_found(detail: &str) -> Response {
    let body = format!("dashboard: {detail}\n");
    let mut response = (StatusCode::NOT_FOUND, Body::from(body)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    response
}

/// Reject `:id` paths that include `..` / `/` / NUL
/// bytes. The handler maps a rejected id to a
/// `404 Not Found` (rather than `400 Bad Request`) so a
/// URL the dashboard would render — but that escapes
/// the `transcripts/` dir — fails with the same shape a
/// genuinely missing file fails with.
fn is_safe_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 256
        && !id.contains("..")
        && !id.contains('/')
        && !id.contains('\\')
        && !id.contains('\0')
}

/// Bind the dashboard's router to a `tokio::net::TcpListener`
/// on `addr` and serve it forever. The function is the
/// `serve(addr)` entry point the spec calls for; it
/// returns when the listener fails (e.g. port already
/// in use) or the runtime is shut down.
///
/// The `AppState` is built from env knobs via
/// [`AppState::from_env`]; a CI worker that wants to
/// point the dashboard at a specific `INDEX.json` (or
/// a specific `transcripts/` dir) sets
/// `RBP_DASHBOARD_INDEX_URL` / `RBP_DASHBOARD_TRANSCRIPT_DIR`
/// before calling `serve`.
pub async fn serve(addr: std::net::SocketAddr) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    let state = AppState::from_env();
    let app = dashboard_app(state);
    axum::serve(listener, app)
        .await
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))
}

#[cfg(test)]
mod tests {
    //! 3 lib tests pinning the per-route shape:
    //!
    //! 1. `serve_typed_index_returns_index_bytes` — a
    //!    `GET /api/index` against a fixture `INDEX.json`
    //!    returns 200 + a body the `serde_json` round-trip
    //!    parses into the same `PublishIndex` the fixture
    //!    started with.
    //! 2. `serve_transcript_returns_404_on_missing` —
    //!    `GET /transcript/nonexistent` against an empty
    //!    transcript dir returns 404.
    //! 3. `serve_bench_card_renders_pinned_columns` —
    //!    `GET /bench/<id>` against a fixture
    //!    `<id>/bench/stdout.txt` returns 200 + a
    //!    response body whose `<dt>` columns are
    //!    `blueprint` / `baseline` / `mbb_per_100` in
    //!    that order.

    use super::*;
    use rbp_autotrain::{IndexedEntry, PublishIndex, PublishedRemoteReceipt};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "rbp-dashboard-router-{label}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir_all(&dir).expect("mkdir tempdir");
            Self { path: dir }
        }
        fn path(&self) -> &std::path::Path {
            &self.path
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }

    fn fixture_index() -> PublishIndex {
        let receipt = PublishedRemoteReceipt {
            plan: rbp_autotrain::PublishRemotePlan {
                bucket: "robopoker-testnet-dashboard".to_string(),
                prefix: "testnet-live-proof-20260604T050000Z/".to_string(),
                region: "us-east-1".to_string(),
                s3_objects: vec![],
                bundle_sha256: "cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313"
                    .to_string(),
                bundle_bytes: 20503,
                receipt_basename: "testnet-live-proof-20260604T050000Z".to_string(),
                runbook_version: "STW-033 v1".to_string(),
                // STW-050: a realistic
                // fixed-ISO-8601 timestamp the
                // dashboard's `meta` line
                // (index.html:211) renders
                // verbatim. The previous
                // `<unknown>` literal was a
                // "this is a test fixture" tell
                // a public visitor saw. The
                // fixture pins the dash-suffixed
                // publish-time shape the
                // committed
                // `tests/fixtures/index.json`
                // entries use. The existing
                // router tests pin *shape*
                // (typed `PublishIndex` →
                // `INDEX.json` on disk → typed
                // read), not the specific
                // timestamp string, so the
                // timestamp change is
                // transparent to them.
                created_at_utc: "2026-06-04T05:00:00Z".to_string(),
                dry_run: true,
            },
            uploaded_at_utc: "2026-06-04T05:00:01Z".to_string(),
            s3_objects: vec![],
            total_bytes: 20503,
            bundle_sha256: "cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313"
                .to_string(),
            runbook_version: "STW-033 v1".to_string(),
        };
        PublishIndex {
            publish_root: "/tmp/publish-root".to_string(),
            runbook_version: "STW-034 v1".to_string(),
            created_at_utc: "2026-06-04T05:00:00Z".to_string(),
            entry_count: 1,
            total_bytes: 20503,
            entries: vec![IndexedEntry {
                receipt_basename: "testnet-live-proof-20260604T050000Z".to_string(),
                receipt_dir: "/tmp/publish-root/testnet-live-proof-20260604T050000Z"
                    .to_string(),
                remote_receipt_path:
                    "/tmp/publish-root/testnet-live-proof-20260604T050000Z/remote/remote_receipt.json"
                        .to_string(),
                remote_receipt: receipt,
                bench: None,
            }],
        }
    }

    fn write_index(dir: &std::path::Path, index: &PublishIndex) {
        let body = serde_json::to_string_pretty(index).expect("serialise index");
        std::fs::write(dir.join("INDEX.json"), body).expect("write INDEX.json");
    }

    /// Build an `AppState` pointing at a fixture
    /// `INDEX.json` in a temp dir, with the bench /
    /// transcript dirs pointing at sibling subdirs of
    /// the same temp dir so the test owns the full
    /// fixture layout.
    fn app_state_for(dir: &TempDir) -> AppState {
        write_index(dir.path(), &fixture_index());
        AppState {
            index_client: IndexClient::from_path(dir.path().join("INDEX.json")),
            transcript_dir: dir.path().to_path_buf(),
            static_index_html: Arc::new("<!doctype html><title>fixture</title>".to_string()),
        }
    }

    #[tokio::test]
    async fn serve_typed_index_returns_index_bytes() {
        let dir = TempDir::new("index");
        let app = dashboard_app(app_state_for(&dir));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot /api/index");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .expect("read body");
        let body_str = std::str::from_utf8(&body).expect("utf-8 body");
        let parsed: PublishIndex =
            serde_json::from_str(body_str).expect("body must be a valid PublishIndex");
        assert_eq!(parsed, fixture_index());
    }

    #[tokio::test]
    async fn serve_transcript_returns_404_on_missing() {
        let dir = TempDir::new("transcript");
        let app = dashboard_app(app_state_for(&dir));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri("/transcript/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot /transcript/nonexistent");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn serve_bench_card_renders_pinned_columns() {
        let dir = TempDir::new("bench");
        write_index(dir.path(), &fixture_index());
        // Drop a synthetic `bench/stdout.txt` with a
        // single `BenchReport` JSON line the bench
        // arm produces. The router's `serve_bench_card`
        // reads this file + projects it through
        // `project_bench_card_fields` + emits the
        // rendered HTML.
        let id = "testnet-live-proof-20260604T050000Z";
        let bench_dir = dir.path().join(id).join("bench");
        std::fs::create_dir_all(&bench_dir).expect("mkdir bench");
        std::fs::write(
            bench_dir.join("stdout.txt"),
            r#"{"hands":200,"wins":114,"losses":86,"net_chips":1234,"mbb_per_100":12.3456,"mbb_ci95":1.2345,"win_rate":0.5700,"win_rate_ci95":0.0345,"blind":2,"blueprint_trained":true,"blueprint":"v1","baseline":"fish","transcript":true}
"#,
        )
        .expect("write bench stdout");
        // SAFETY: the test owns this env knob for the
        // duration of the test; the env var is
        // `remove_var`'d before the test returns. A
        // parallel `cargo test` invocation cannot read
        // a meaningful value from this knob (the
        // dashboard startup picks up whatever is
        // current at the moment of the read), so the
        // racy nature of `set_var` does not surface a
        // flaky assertion in this test.
        unsafe {
            std::env::set_var("RBP_DASHBOARD_RECEIPT_DIR", dir.path());
        }
        let app = dashboard_app(app_state_for(&dir));
        let response = app
            .oneshot(
                axum::http::Request::builder()
                    .uri(&format!("/bench/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot /bench/<id>");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .expect("read body");
        let body_str = std::str::from_utf8(&body).expect("utf-8 body");
        // Pinned column order: `blueprint` / `baseline`
        // / `mbb_per_100` (in that order in the
        // response body).
        let i_bp = body_str.find("blueprint").expect("contains `blueprint`");
        let i_ba = body_str.find("baseline").expect("contains `baseline`");
        let i_mbb = body_str
            .find("mbb_per_100")
            .expect("contains `mbb_per_100`");
        assert!(
            i_bp < i_ba && i_ba < i_mbb,
            "bench card must be ordered blueprint < baseline < mbb_per_100; got: bp={i_bp} ba={i_ba} mbb={i_mbb}\nbody: {body_str}"
        );
        // The headline numbers the spec pins (the
        // `12.3456` and `1.2345` mbb/100 / CI values
        // from the fixture stdout line) must appear in
        // the rendered body.
        assert!(
            body_str.contains("12.3456"),
            "body must contain mbb/100 headline: {body_str}"
        );
        assert!(
            body_str.contains("1.2345"),
            "body must contain mbb CI half-width: {body_str}"
        );
        // SAFETY: see the `set_var` call above — the
        // racy `set_var` / `remove_var` pair is
        // acceptable in a `#[cfg(test)]` integration
        // test where the env knob is opaque to any
        // other test.
        unsafe {
            std::env::remove_var("RBP_DASHBOARD_RECEIPT_DIR");
        }
    }

    /// STW-052: the `RBP_DASHBOARD_EMPTY_STATE` env
    /// knob is the dashboard's true-empty-state
    /// opt-in switch. When `=1`, the `GET
    /// /api/index` route short-circuits to a
    /// typed empty `PublishIndex` (the
    /// `empty_publish_index()` helper) instead of
    /// reading the live `INDEX.json` / committed
    /// fixture. When the knob is unset or `=0`,
    /// the live-data path runs as before. The
    /// `is_empty_state()` helper is the cheap
    /// pure-function the integration test
    /// exercises end-to-end.
    ///
    /// The test exercises the production path
    /// through the test-only `set_empty_state_for_test`
    /// override (a `Mutex<Option<bool>>`); the
    /// `RBP_DASHBOARD_EMPTY_STATE` env-var
    /// `set_var` alternative is racy with parallel
    /// test execution (the
    /// `cargo test --test-threads=4` scheduling
    /// the spec names would leak the env var
    /// across test boundaries). The override
    /// is consulted first; the env-var read
    /// is the production fallback the override
    /// shadows in `#[cfg(test)]` builds.
    ///
    /// A regression that re-engages the empty-state
    /// on a live `INDEX.json` (a
    /// `set_empty_state_for_test(true)` leak
    /// from a parallel test) would fail this
    /// test at the same step a downstream
    /// dashboard scraper would silently break.
    #[tokio::test]
    async fn router_empty_state_env_knob_engages_when_set() {
        // Default-off: the override is cleared, the
        // empty-state branch is NOT engaged, the
        // handler returns the live `INDEX.json`
        // the fixture wrote.
        set_empty_state_for_test(false);
        assert!(
            !is_empty_state(),
            "is_empty_state() must be false when override is Some(false)"
        );
        let dir = TempDir::new("env-knob-default-off");
        let app = dashboard_app(app_state_for(&dir));
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot /api/index (default-off)");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .expect("read body");
        let body_str = std::str::from_utf8(&body).expect("utf-8 body");
        let parsed: PublishIndex = serde_json::from_str(body_str)
            .expect("body must be a valid PublishIndex (default-off)");
        assert_eq!(
            parsed,
            fixture_index(),
            "default-off knob must return the live INDEX.json the fixture wrote"
        );
        assert_eq!(
            parsed.entry_count, 1,
            "live INDEX.json must have one entry (the default-off path runs)"
        );

        // =1: the empty-state branch IS engaged,
        // the handler returns the typed empty
        // `PublishIndex` (no read of the live
        // `INDEX.json`).
        set_empty_state_for_test(true);
        assert!(
            is_empty_state(),
            "is_empty_state() must be true when override is Some(true)"
        );
        let response = app
            .clone()
            .oneshot(
                axum::http::Request::builder()
                    .uri("/api/index")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .expect("oneshot /api/index (=1)");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), 65536)
            .await
            .expect("read body");
        let body_str = std::str::from_utf8(&body).expect("utf-8 body");
        let parsed: PublishIndex =
            serde_json::from_str(body_str).expect("body must be a valid PublishIndex (=1)");
        assert_eq!(
            parsed,
            empty_publish_index(),
            "=1 knob must return the typed empty PublishIndex"
        );
        assert_eq!(
            parsed.entry_count, 0,
            "empty PublishIndex must have zero entries"
        );
        assert!(
            parsed.entries.is_empty(),
            "empty PublishIndex must have an empty entries[] vec"
        );
        assert_eq!(
            parsed.publish_root, "",
            "empty PublishIndex publish_root must be the empty string"
        );

        // Restore the default state for the
        // next test in the schedule. The
        // `clear_*` call removes the override;
        // the next test that needs the
        // override re-engages it.
        clear_empty_state_for_test();
        assert!(
            !is_empty_state(),
            "is_empty_state() must be false when override is cleared (falls through to env-var unset)"
        );
    }

    /// STW-052: the typed empty `PublishIndex` the
    /// `serve_typed_index` handler returns when
    /// `RBP_DASHBOARD_EMPTY_STATE=1` is a *typed*
    /// value (not a free-form JSON literal). The
    /// `empty_publish_index()` helper is the
    /// single source of truth the smoke test +
    /// the lib test share, and a future
    /// regression in the field shape (a renamed
    /// field, a missing field) fails this test
    /// at the same CI step a downstream
    /// dashboard scraper would silently break.
    #[test]
    fn empty_publish_index_serialises_to_zero_entries() {
        let index = empty_publish_index();
        assert_eq!(index.entry_count, 0);
        assert_eq!(index.total_bytes, 0);
        assert!(index.entries.is_empty());
        assert!(index.publish_root.is_empty());
        let body = serde_json::to_string(&index).expect("serialise empty PublishIndex");
        let parsed: PublishIndex =
            serde_json::from_str(&body).expect("empty PublishIndex must round-trip");
        assert_eq!(parsed, index);
    }
}
