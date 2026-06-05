//! `crates/dashboard/tests/seed_local.rs` — STW-068 integration test.
//!
//! Drives the `scripts/seed-dashboard-local.sh` runbook against a
//! synthetic receipt directory and asserts the dashboard renders a
//! non-empty table from the seeded layout.

use std::path::PathBuf;
use std::process::Command;
use std::sync::Arc;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use rbp_dashboard::{AppState, IndexClient, dashboard_app};
use tower::ServiceExt;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/dashboard")
        .to_path_buf()
}

/// Run the seed script against `receipt_dir` and return the seed root.
fn run_seed_script(receipt_dir: &PathBuf) -> PathBuf {
    let script = workspace_root()
        .join("scripts")
        .join("seed-dashboard-local.sh");
    let output = Command::new("bash")
        .arg(&script)
        .arg(receipt_dir)
        .current_dir(&workspace_root())
        .output()
        .expect("spawn seed-dashboard-local.sh");
    assert!(
        output.status.success(),
        "seed-dashboard-local.sh must exit 0 (got exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );
    workspace_root().join(".auto").join("dashboard-seed")
}

/// Async helper: send a `GET <uri>` request to the dashboard's router
/// and return the `(status, body_bytes, content_type)` triple.
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
async fn seed_local_run_produces_dashboard_readable_layout() {
    // 1. Create a synthetic receipt directory with a valid bench
    //    stdout so the dashboard's /bench/:id route can render it.
    let receipt_dir =
        std::env::temp_dir().join(format!("robopoker-seed-test-{}", std::process::id()));
    let _cleanup = {
        struct Cleanup {
            path: PathBuf,
        }
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
        Cleanup {
            path: receipt_dir.clone(),
        }
    };

    std::fs::create_dir_all(receipt_dir.join("cluster")).expect("create cluster dir");
    std::fs::write(receipt_dir.join("cluster").join("exit.txt"), "0\n")
        .expect("write cluster exit");
    std::fs::create_dir_all(receipt_dir.join("bench")).expect("create bench dir");

    let bench_json = r#"{"hands":10,"wins":6,"losses":4,"net_chips":20,"mbb_per_100":10.0000,"mbb_ci95":2.0000,"win_rate":0.6000,"win_rate_ci95":0.1000,"blind":2,"blueprint_trained":true,"blueprint":"v1","baseline":"fish","transcript":false}"#;
    std::fs::write(receipt_dir.join("bench").join("stdout.txt"), bench_json)
        .expect("write bench stdout");

    // 2. Run the seed script.
    let seed_root = run_seed_script(&receipt_dir);

    // 3. Build AppState pointed at the seed layout.
    let static_index_html = Arc::new(
        std::fs::read_to_string(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("static")
                .join("index.html"),
        )
        .expect("read static index.html"),
    );
    let app = dashboard_app(AppState {
        index_client: IndexClient::from_path(seed_root.join("INDEX.json")),
        transcript_dir: seed_root.join("transcripts"),
        static_index_html,
    });

    // 4. GET /api/index must return 200 with at least 1 entry.
    let (status, body, ct) = get(app.clone(), "/api/index").await;
    assert_eq!(
        status,
        StatusCode::OK,
        "GET /api/index must return 200 against seeded INDEX.json"
    );
    let ct = ct.unwrap_or_default();
    assert!(
        ct.starts_with("application/json"),
        "GET /api/index must be application/json; got `{ct}`"
    );
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    let parsed: rbp_autotrain::PublishIndex =
        serde_json::from_str(body_str).expect("body must parse as PublishIndex");
    assert!(
        parsed.entry_count >= 1,
        "seeded INDEX.json must have at least 1 entry; got: {parsed:?}"
    );
    assert!(
        !parsed.entries.is_empty(),
        "seeded INDEX.json entries[] must be non-empty"
    );

    // 5. GET /bench/:id must return 200 with the bench card.
    //    The dashboard reads RBP_DASHBOARD_RECEIPT_DIR on each
    //    request; set it to the seeded receipts dir for this
    //    assertion. In Rust 2024 set_var is unsafe.
    let receipt_basename = receipt_dir.file_name().unwrap().to_str().unwrap();
    let seeded_receipt_dir = seed_root.join("receipts");
    unsafe { std::env::set_var("RBP_DASHBOARD_RECEIPT_DIR", &seeded_receipt_dir) };
    let (status, body, ct) = get(app.clone(), &format!("/bench/{receipt_basename}")).await;
    unsafe { std::env::remove_var("RBP_DASHBOARD_RECEIPT_DIR") };
    assert_eq!(
        status,
        StatusCode::OK,
        "GET /bench/{receipt_basename} must return 200 against seeded bench/stdout.txt"
    );
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    assert!(
        body_str.contains("blueprint"),
        "bench card must contain 'blueprint'; got: {body_str}"
    );
    assert!(
        body_str.contains("baseline"),
        "bench card must contain 'baseline'; got: {body_str}"
    );
    let ct = ct.unwrap_or_default();
    assert!(
        ct.starts_with("text/html"),
        "GET /bench/{receipt_basename} must be text/html; got `{ct}`"
    );

    // 6. GET / must contain the table scaffold (non-empty table
    //    host — the JS will populate rows from the fetched index).
    let (status, body, ct) = get(app.clone(), "/").await;
    assert_eq!(status, StatusCode::OK, "GET / must return 200");
    let body_str = std::str::from_utf8(&body).expect("utf-8 body");
    assert!(
        body_str.contains("id=\"index-table\""),
        "GET / body must contain the table scaffold"
    );
    let ct = ct.unwrap_or_default();
    assert!(
        ct.starts_with("text/html"),
        "GET / must be text/html; got `{ct}`"
    );
}
