//! `crates/dashboard/tests/fixtures_smoke.rs` —
//! STW-042 fixture-fallback integration test.
//!
//! The single sub-test in this file spins up the
//! dashboard's `axum::Router` on a random localhost
//! port and drives the `GET /bench/compare3-fixture`
//! route the STW-042 fixture-fallback adds. The
//! test owns a temp `INDEX.json` so the
//! `index_client.fetch_index` call the fallback
//! consults returns a *no-match* index (a fresh
//! checkout with no live receipt basenames), and the
//! committed
//! `crates/dashboard/tests/fixtures/compare3-fixture.json`
//! is the demo-data card the fallback renders.
//!
//! The 4 assertions pin the demo-data contract:
//!
//! 1. `GET /bench/compare3-fixture` returns 200
//!    (the fixture-fallback is the active path,
//!    not a 404 from a missing `bench/stdout.txt`).
//! 2. The response body contains the fixture's
//!    `ranked_winner` value (so a future drift in
//!    the rendered HTML breaks the test at the same
//!    CI step a downstream dashboard scraper would
//!    silently break).
//! 3. The response body contains the three
//!    pairwise `delta_mbb_per_100` values (the
//!    `v1_v2_delta` / `v2_v3_delta` /
//!    `v3_v1_delta` keys) so the pairwise-deltas
//!    `<dl>` the `render_compare3_card` emitter
//!    produces is on the page.
//! 4. The committed
//!    `crates/dashboard/tests/fixtures/compare3-fixture.json`
//!    `serde_json::from_str`'s into a typed
//!    `Compare3Report` without error, the
//!    `ranked_winner` ∈ `{V1, V2, V3, Tie}`, and the
//!    three sub-reports' `hands > 0` (so a future
//!    regression that ships an empty / malformed
//!    fixture fails the test).
//!
//! A second sub-test (`real_index_shadows_demo_data`)
//! pins the inverse contract: when a *real*
//! `INDEX.json` has an entry whose `receipt_basename`
//! is `compare3-fixture` (a hypothetical future
//! operator runbook could produce one), the live
//! `bench/stdout.txt` path wins and the demo-data
//! fallback is *not* engaged. The test asserts the
//! `GET /bench/compare3-fixture` request returns 404
//! (no `bench/stdout.txt` for that basename in the
//! temp dir), proving the live path ran first and
//! the demo-data fallback did not shadow it.

use std::path::PathBuf;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rbp_dashboard::{AppState, Compare3Report, Compare3Winner, IndexClient, dashboard_app};
use tower::ServiceExt;

/// The committed fixture `INDEX.json` the smoke
/// test points `RBP_DASHBOARD_INDEX_URL` at. The
/// path is resolved relative to `CARGO_MANIFEST_DIR`
/// so the test runs the same way on a developer
/// machine and in CI.
fn fixture_index_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("index.json")
}

/// Build an `AppState` pointed at the committed
/// `index.json` fixture (the smoke test's
/// "no live receipt basenames" baseline). The
/// `static_index_html` is loaded from the
/// checked-in `crates/dashboard/static/index.html`;
/// the `transcript_dir` is a sibling of the
/// fixtures dir so the test owns the full
/// layout.
fn app_state_with_index_fixture() -> AppState {
    let transcript_dir = fixture_index_path()
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
    AppState {
        index_client: IndexClient::from_path(fixture_index_path()),
        transcript_dir,
        static_index_html,
    }
}

/// Async helper: send a `GET <uri>` request to the
/// dashboard's router and return the
/// `(status, body_bytes)` pair. The helper hides the
/// `Body::collect` plumbing the `axum` body type
/// needs.
async fn get(router: axum::Router, uri: &str) -> (StatusCode, Vec<u8>) {
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
    let body = axum::body::to_bytes(response.into_body(), 1_048_576)
        .await
        .expect("read body");
    (status, body.to_vec())
}

#[tokio::test]
async fn compare3_fixture_renders_bench_card() {
    // Drive the dashboard's router against a
    // *real* (committed) `INDEX.json` fixture so
    // the demo-data fallback's
    // `index.entries.iter().any(|e|
    // e.receipt_basename == "compare3-fixture")`
    // check returns `false` (none of the
    // committed fixture's entries have a
    // `compare3-fixture` basename) and the
    // demo-data fallback is the active path.
    let app = dashboard_app(app_state_with_index_fixture());
    let (status, body) = get(app.clone(), "/bench/compare3-fixture").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET /bench/compare3-fixture must return 200 against a no-match INDEX.json; got {status}"
    );
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");

    // The fixture's `ranked_winner` value
    // (`v3`) must appear in the body, wrapped
    // in the `<strong>` tag the
    // `render_compare3_card` emitter uses. A
    // regression in the rendered card (a missing
    // `<strong>` wrap, a different winner cell)
    // fails this assertion.
    assert!(
        body_str.contains("ranked_winner") && body_str.contains("<strong>v3</strong>"),
        "compare3 card body must show `ranked_winner: <strong>v3</strong>`; got:\n{body_str}"
    );

    // The three pairwise `delta_mbb_per_100`
    // values must appear as `<dt>` cells in the
    // body. A regression that drops one of the
    // deltas (or reorders them) fails this
    // assertion.
    let i_d12 = body_str.find("v1_v2_delta").expect("contains v1_v2_delta");
    let i_d23 = body_str.find("v2_v3_delta").expect("contains v2_v3_delta");
    let i_d31 = body_str.find("v3_v1_delta").expect("contains v3_v1_delta");
    assert!(
        i_d12 < i_d23 && i_d23 < i_d31,
        "pairwise deltas must be ordered v1_v2 < v2_v3 < v3_v1; got: d12={i_d12} d23={i_d23} d31={i_d31}"
    );

    // The committed
    // `tests/fixtures/compare3-fixture.json`
    // must `serde_json::from_str` into a typed
    // `Compare3Report` without error. A
    // regression in the autotrain's
    // `Compare3Report::to_json` field shape
    // (a renamed field, a missing field) fails
    // this assertion at the same CI step a
    // downstream `trainer --compare3 --json`
    // consumer would silently break.
    let path = rbp_dashboard::compare3_fixture_path();
    let raw =
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let parsed: Compare3Report = serde_json::from_str(&raw).unwrap_or_else(|e| {
        panic!("compare3 fixture must parse as Compare3Report: {e}; body:\n{raw}")
    });
    assert!(
        matches!(
            parsed.ranked_winner,
            Compare3Winner::V1 | Compare3Winner::V2 | Compare3Winner::V3 | Compare3Winner::Tie
        ),
        "ranked_winner must be in {{V1, V2, V3, Tie}}; got: {:?}",
        parsed.ranked_winner
    );
    assert!(
        parsed.v1.hands > 0 && parsed.v2.hands > 0 && parsed.v3.hands > 0,
        "compare3 fixture sub-reports must be populated; got: v1.hands={} v2.hands={} v3.hands={}",
        parsed.v1.hands,
        parsed.v2.hands,
        parsed.v3.hands
    );
}

#[tokio::test]
async fn real_index_shadows_demo_data() {
    // The demo-data fallback is *only*
    // engaged when the in-memory `INDEX.json`
    // has no entry whose `receipt_basename` is
    // `compare3-fixture`. A live `INDEX.json`
    // with a `compare3-fixture` entry
    // (hypothetical future operator runbook
    // could produce one) must NOT trigger
    // the demo-data fallback — the live
    // `bench/stdout.txt` path runs instead
    // and returns 404 when no `bench/stdout.txt`
    // exists in the temp dir (which is the
    // case here, because we never wrote one).
    //
    // The empty-index tempdir is a fresh
    // directory; an empty `INDEX.json` is
    // explicitly NOT the same as a
    // `compare3-fixture` entry, so the
    // fallback DOES engage here. This is the
    // *opposite* of the assertion we want
    // — we want a tempdir with a single
    // entry whose basename IS
    // `compare3-fixture`. Build it inline.
    let tmpdir = std::env::temp_dir().join(format!(
        "rbp-dashboard-fixtures-smoke-shadow-{}-{}",
        std::process::id(),
        std::sync::atomic::AtomicUsize::fetch_add(
            &std::sync::atomic::AtomicUsize::new(0),
            1,
            std::sync::atomic::Ordering::SeqCst,
        )
    ));
    std::fs::create_dir_all(&tmpdir).expect("mkdir tmpdir");
    let shadow_index = serde_json::json!({
        "publish_root": "/tmp/shadow",
        "runbook_version": "STW-034 v1",
        // STW-053: the
        // pre-STW-053
        // fixture carried
        // the literal
        // `<unknown>`
        // sentinel in the
        // `created_at_utc`
        // field. The
        // STW-051
        // source-side
        // fix removed
        // the
        // `<unknown>`
        // fallback from
        // the
        // aggregator
        // entirely
        // (the new
        // shape fails
        // fast with
        // `PublishIndexError::MissingArg`
        // on a missing
        // arg), but
        // the
        // fixtures_smoke
        // fixture
        // still passed
        // the literal
        // through to
        // the response
        // body — a
        // future
        // regression
        // that
        // re-introduces
        // a
        // `<unknown>`
        // literal in
        // the lib's
        // render path
        // would pass
        // the
        // fixtures_smoke
        // test (because
        // the test
        // feeds a
        // `<unknown>`
        // literal
        // directly into
        // the response
        // body). The
        // fix is a
        // 3-line
        // literal swap
        // to realistic
        // fixed-ISO-8601
        // timestamps;
        // the existing
        // `compare3_fixture_renders_bench_card`
        // +
        // `real_index_shadows_demo_data`
        // sub-tests
        // pin *shape*,
        // not specific
        // timestamp
        // strings, so
        // the
        // timestamp
        // change is
        // transparent to
        // them.
        "created_at_utc": "2026-06-04T05:00:00Z",
        "entry_count": 1,
        "total_bytes": 0,
        "entries": [{
            "receipt_basename": "compare3-fixture",
            "receipt_dir": "/tmp/shadow/compare3-fixture",
            "remote_receipt_path": "/tmp/shadow/compare3-fixture/remote/remote_receipt.json",
            "remote_receipt": {
                "plan": {
                    "bucket": "robopoker-testnet-dashboard",
                    "prefix": "compare3-fixture/",
                    "region": "us-east-1",
                    "s3_objects": [],
                    "bundle_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                    "bundle_bytes": 0,
                    "receipt_basename": "compare3-fixture",
                    "runbook_version": "STW-033 v1",
                    "created_at_utc": "2026-06-04T14:01:07Z",
                    "dry_run": true
                },
                "uploaded_at_utc": "2026-06-04T05:00:01Z",
                "s3_objects": [],
                "total_bytes": 0,
                "bundle_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
                "runbook_version": "STW-033 v1"
            }
        }]
    });
    let index_path = tmpdir.join("INDEX.json");
    std::fs::write(
        &index_path,
        serde_json::to_string_pretty(&shadow_index).expect("serialise shadow index"),
    )
    .expect("write shadow INDEX.json");
    let static_index_html = Arc::new(
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("static")
                .join("index.html"),
        )
        .expect("read static index.html"),
    );
    let app = dashboard_app(AppState {
        index_client: IndexClient::from_path(index_path),
        transcript_dir: tmpdir.clone(),
        static_index_html,
    });
    let (status, _body) = get(app.clone(), "/bench/compare3-fixture").await;
    // The live `bench/stdout.txt` path runs
    // (the demo-data fallback is shadowed),
    // and there is no `bench/stdout.txt` in
    // the temp dir, so the response is 404
    // (not 200 + a compare3 card). A future
    // regression that re-engages the
    // demo-data fallback despite a matching
    // `INDEX.json` entry would fail this
    // assertion.
    assert_eq!(
        status,
        StatusCode::NOT_FOUND,
        "GET /bench/compare3-fixture with a matching INDEX entry must run the live bench path (404), not the demo-data fallback (200); got {status}"
    );
    let _ = std::fs::remove_dir_all(&tmpdir);
}
