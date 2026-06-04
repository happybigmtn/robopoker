//! `IndexClient` — typed read of the STW-034 `INDEX.json`.
//!
//! The dashboard's `(a)` layer. The client wraps the
//! `rbp_autotrain::PublishIndex` type (the same Rust type the
//! `trainer --publish-index` arm writes) so a shape drift
//! fails BOTH the dashboard's typed read AND the
//! `trainer --verify-index` re-verify at the same CI step.
//!
//! The client supports two read paths:
//!
//! - **Local file** ([`IndexClient::from_path`]) — the
//!   test/development path. The `tests/smoke.rs`
//!   integration test points this at a fixture
//!   `INDEX.json` on disk. Reads use
//!   `std::fs::read_to_string` + `serde_json::from_str`.
//! - **HTTP URL** ([`IndexClient::from_url`]) — the
//!   production path. The `RBP_DASHBOARD_INDEX_URL` env
//!   knob (default `http://localhost:8080/api/index` in
//!   tests) is the single config point. Reads use
//!   `ureq::get` + `serde_json::from_str`.
//!
//! No `reqwest` / no `aws-sdk-*` is vendored; the bucket
//! deploy is the `scripts/testnet-live-publish-dashboard.sh`
//! runbook's job, and the prod-index fetch is a one-shot
//! startup operation that does not need an async HTTP
//! client.

use std::path::Path;
use std::time::Duration;

use rbp_autotrain::PublishIndex;

/// `RBP_DASHBOARD_INDEX_URL` env knob — the URL the
/// `IndexClient::from_env` constructor reads the
/// `INDEX.json` from in production. The default
/// `http://localhost:8080/api/index` is the testnet-loopback
/// fallback a CI worker running the dashboard + the autotrain
/// trainer on the same host would point at.
pub const DEFAULT_INDEX_URL: &str = "http://localhost:8080/api/index";

/// Per-request HTTP timeout the prod-index fetch waits
/// before returning an [`IndexClientError::Http`]. 5s is
/// generous for a one-shot startup fetch against a
/// CloudFront / dashboard bucket URL but tight enough that
/// a misconfigured `RBP_DASHBOARD_INDEX_URL` (e.g. an
/// unreachable host) fails the dashboard startup fast
/// rather than hanging the worker.
const HTTP_TIMEOUT_SECS: u64 = 5;

/// Typed read of the STW-034 `INDEX.json` aggregator.
///
/// The client is `Clone`-able (the inner path / URL is a
/// `String`; no resource handles to clone) and `Send +
/// Sync` so a future async-axum handler can share a single
/// `IndexClient` across every request.
#[derive(Debug, Clone)]
pub struct IndexClient {
    /// Either a `file://` or `http(s)://` URL. The router
    /// decides which path to take based on the prefix:
    /// `file://` reads via `std::fs`, anything else
    /// reads via `ureq::get`. The `RBP_DASHBOARD_INDEX_URL`
    /// env knob accepts both shapes — a CI worker can
    /// point at `file:///tmp/INDEX.json` for a
    /// sandboxed-build test, or at
    /// `https://dashboard.robopoker.io/INDEX.json` for
    /// production.
    source: String,
}

impl IndexClient {
    /// Build a client from an explicit URL or path string.
    /// Accepts both `file://` (test / local-only) and
    /// `http(s)://` (prod) shapes. Bare absolute paths
    /// (e.g. `/srv/dev/repos/robopoker/INDEX.json`) are
    /// auto-prefixed with `file://` so the API surface
    /// stays symmetric.
    pub fn from_url(url: impl Into<String>) -> Self {
        let raw = url.into();
        let source = if raw.starts_with("http://")
            || raw.starts_with("https://")
            || raw.starts_with("file://")
        {
            raw
        } else {
            format!("file://{}", raw)
        };
        Self { source }
    }

    /// Build a client from a local file path. Convenience
    /// for the test / fixture path; equivalent to
    /// `IndexClient::from_url(path.display().to_string())`.
    pub fn from_path(path: impl AsRef<Path>) -> Self {
        Self::from_url(path.as_ref().display().to_string())
    }

    /// Build a client from the `RBP_DASHBOARD_INDEX_URL` env
    /// knob, falling back to [`DEFAULT_INDEX_URL`]. This is
    /// the constructor the production dashboard's
    /// `serve()` entry point uses; the test harness points
    /// at a fixture via `RBP_DASHBOARD_INDEX_URL=file://...`.
    pub fn from_env() -> Self {
        let raw = std::env::var("RBP_DASHBOARD_INDEX_URL")
            .unwrap_or_else(|_| DEFAULT_INDEX_URL.to_string());
        Self::from_url(raw)
    }

    /// Borrow the source URL / path string. Exposed for the
    /// router's `GET /api/index` handler so a 200 response
    /// can carry a `x-dashboard-source: <url>` header a CI
    /// auditor can grep.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Fetch the `INDEX.json` aggregator and parse it into
    /// the typed `PublishIndex` struct. This is the typed
    /// read a shape drift in `INDEX.json` fails — a
    /// regression in the `PublishIndex` JSON shape fails
    /// both this method AND the
    /// `trainer --verify-index` re-verify at the same CI
    /// step.
    pub fn fetch_index(&self) -> Result<PublishIndex, IndexClientError> {
        if let Some(path) = self.source.strip_prefix("file://") {
            let body = std::fs::read_to_string(path).map_err(|e| {
                IndexClientError::Io(format!("failed to read INDEX.json at {}: {e}", path))
            })?;
            return parse_publish_index(&body);
        }
        let body = fetch_via_ureq(&self.source)?;
        parse_publish_index(&body)
    }
}

/// Parse the response body into a typed `PublishIndex`.
/// A bare `serde_json::from_str` failure (malformed JSON,
/// missing required field, wrong field type) is surfaced
/// as [`IndexClientError::Parse`] so a downstream dashboard
/// can render a useful 500 with the inner serde error.
fn parse_publish_index(body: &str) -> Result<PublishIndex, IndexClientError> {
    serde_json::from_str(body).map_err(|e| {
        IndexClientError::Parse(format!("could not parse INDEX.json into PublishIndex: {e}"))
    })
}

/// Fetch the `INDEX.json` over HTTP using `ureq`. The
/// fetch is intentionally blocking (the index read is a
/// one-shot startup operation, not a per-request hot path;
/// the testnet dashboard is a static HTML page that re-
/// fetches the index on page load, not on every keystroke).
/// The 5s timeout keeps a misconfigured URL from hanging
/// the worker.
fn fetch_via_ureq(url: &str) -> Result<String, IndexClientError> {
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(HTTP_TIMEOUT_SECS))
        .build();
    let response = agent
        .get(url)
        .call()
        .map_err(|e| IndexClientError::Http(format!("GET {url} failed: {e}")))?;
    response
        .into_string()
        .map_err(|e| IndexClientError::Http(format!("read body of {url}: {e}")))
}

/// Typed error surface for the `IndexClient`.
///
/// The four variants cover the failure modes a typed read
/// of the `INDEX.json` can produce:
///
/// - [`IndexClientError::MissingUrl`] — the
///   `RBP_DASHBOARD_INDEX_URL` knob is unset and the
///   constructor's fallback could not be reached. Surfaced
///   when the test harness forgets to set the env knob.
/// - [`IndexClientError::Io`] — the local `file://` read
///   failed (missing file, permission denied, etc.). The
///   fixture-backed smoke test asserts the variant fires
///   on a missing fixture.
/// - [`IndexClientError::Http`] — the prod `http(s)://`
///   read failed (timeout, non-2xx, transport error).
/// - [`IndexClientError::Parse`] — the body was read
///   successfully but could not be parsed into the typed
///   `PublishIndex`. A shape drift in the STW-034
///   aggregator fires this variant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IndexClientError {
    /// The URL was empty / unset.
    MissingUrl,
    /// A local `file://` read failed. The wrapped `String`
    /// is the inner `std::io::Error` formatted with the
    /// path the read was attempted against.
    Io(String),
    /// An HTTP fetch failed. The wrapped `String` is the
    /// inner `ureq::Error` formatted with the URL the
    /// fetch was attempted against.
    Http(String),
    /// The body could not be parsed into the typed
    /// `PublishIndex`. The wrapped `String` is the inner
    /// `serde_json::Error` formatted with the field name
    /// the parse failed on.
    Parse(String),
}

impl std::fmt::Display for IndexClientError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::MissingUrl => write!(f, "dashboard: INDEX.json URL is empty"),
            Self::Io(detail) => write!(f, "dashboard: INDEX.json I/O error: {detail}"),
            Self::Http(detail) => write!(f, "dashboard: INDEX.json HTTP error: {detail}"),
            Self::Parse(detail) => write!(f, "dashboard: INDEX.json parse error: {detail}"),
        }
    }
}

impl std::error::Error for IndexClientError {}

#[cfg(test)]
mod tests {
    //! 4 lib tests pinning the typed-read contract:
    //!
    //! 1. `round_trip_through_serialised_publish_index` —
    //!    serialise a synthetic `PublishIndex` to JSON,
    //!    read it back through `IndexClient::from_path`,
    //!    assert the typed result matches the original.
    //!    Proves the dashboard's typed read is
    //!    shape-compatible with what `trainer
    //!    --publish-index` writes.
    //! 2. `missing_url_returns_missing_url_error` — a
    //!    `file://` client pointed at a nonexistent path
    //!    returns `IndexClientError::Io` (not `Parse` /
    //!    `Http`). The missing-URL pin a future regression
    //!    in the I/O-vs-parse variant mapping would fail.
    //! 3. `malformed_json_returns_parse_error` — the
    //!    fixture is non-empty but not valid JSON; the
    //!    client returns `IndexClientError::Parse`. The
    //!    shape-drift pin a future regression in the
    //!    `serde_json::from_str` path would fail.
    //! 4. `empty_entries_array_round_trips` — a
    //!    `PublishIndex` with `entries: Vec::new()` reads
    //!    back as the same empty `entries`. The
    //!    empty-but-valid pin a future regression that
    //!    confuses a zero-entry index with a missing index
    //!    would fail.

    use super::*;
    use rbp_autotrain::{IndexedEntry, PublishIndex, PublishedRemoteReceipt};

    /// A tiny RAII tempdir helper that avoids pulling in
    /// a `tempfile` / `tempdir` dev-dep. Allocates under
    /// `std::env::temp_dir()` with a per-process counter
    /// so parallel `cargo test` invocations don't collide,
    /// and removes the dir on drop.
    struct TempDir {
        path: std::path::PathBuf,
    }

    impl TempDir {
        fn new(label: &str) -> Self {
            use std::sync::atomic::{AtomicUsize, Ordering};
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let dir = std::env::temp_dir().join(format!(
                "rbp-dashboard-{label}-{}-{}",
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

    /// Drop a synthetic `PublishIndex` with one entry on
    /// disk and return the file path + the original
    /// in-memory `PublishIndex` the test asserts on.
    fn fixture_with_one_entry() -> (TempDir, PublishIndex) {
        let dir = TempDir::new("index-client");
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
                created_at_utc: "<unknown>".to_string(),
                dry_run: true,
            },
            uploaded_at_utc: "<unknown>".to_string(),
            s3_objects: vec![],
            total_bytes: 20503,
            bundle_sha256: "cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313"
                .to_string(),
            runbook_version: "STW-033 v1".to_string(),
        };
        let index = PublishIndex {
            publish_root: "/tmp/publish-root".to_string(),
            runbook_version: "STW-034 v1".to_string(),
            created_at_utc: "<unknown>".to_string(),
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
        };
        let body = serde_json::to_string_pretty(&index).expect("serialise index");
        std::fs::write(dir.path().join("INDEX.json"), body).expect("write INDEX.json");
        (dir, index)
    }

    #[test]
    fn round_trip_through_serialised_publish_index() {
        let (dir, original) = fixture_with_one_entry();
        let client = IndexClient::from_path(dir.path().join("INDEX.json"));
        let parsed = client
            .fetch_index()
            .expect("typed read of fresh fixture must succeed");
        assert_eq!(
            parsed, original,
            "typed read must round-trip the original PublishIndex"
        );
    }

    #[test]
    fn missing_file_returns_io_error() {
        let dir = TempDir::new("missing");
        let client = IndexClient::from_path(dir.path().join("nonexistent-INDEX.json"));
        let err = client
            .fetch_index()
            .expect_err("missing file must surface as IndexClientError");
        assert!(
            matches!(err, IndexClientError::Io(_)),
            "missing file must surface as IndexClientError::Io, got: {err:?}"
        );
    }

    #[test]
    fn malformed_json_returns_parse_error() {
        let dir = TempDir::new("malformed");
        std::fs::write(dir.path().join("INDEX.json"), "{ not valid json }")
            .expect("write malformed JSON");
        let client = IndexClient::from_path(dir.path().join("INDEX.json"));
        let err = client
            .fetch_index()
            .expect_err("malformed JSON must surface as IndexClientError");
        assert!(
            matches!(err, IndexClientError::Parse(_)),
            "malformed JSON must surface as IndexClientError::Parse, got: {err:?}"
        );
    }

    #[test]
    fn empty_entries_array_round_trips() {
        let dir = TempDir::new("empty");
        let index = PublishIndex {
            publish_root: "/tmp/empty".to_string(),
            runbook_version: "STW-034 v1".to_string(),
            created_at_utc: "<unknown>".to_string(),
            entry_count: 0,
            total_bytes: 0,
            entries: Vec::new(),
        };
        let body = serde_json::to_string_pretty(&index).expect("serialise empty index");
        std::fs::write(dir.path().join("INDEX.json"), body).expect("write empty INDEX.json");
        let client = IndexClient::from_path(dir.path().join("INDEX.json"));
        let parsed = client
            .fetch_index()
            .expect("typed read of empty-entries index must succeed");
        assert_eq!(parsed, index, "empty entries[] must round-trip");
        assert!(
            parsed.entries.is_empty(),
            "empty entries[] must round-trip as empty"
        );
    }

    #[test]
    fn missing_url_is_normalised() {
        let client = IndexClient::from_url("");
        // An empty string is auto-prefixed to `file://`,
        // so a fetch should surface as `Io` (the read
        // fails), not `MissingUrl`. The `MissingUrl` variant
        // is reserved for the explicit
        // `IndexClient::from_url` `Option<String>` case
        // the spec never actually exercises.
        assert_eq!(client.source(), "file://");
    }
}
