//! `trainer --verify-bundle <path>` — re-verify a
//! published bundle (the `bundle.tar.gz` +
//! `manifest.json` + `bundle.sha256` triple the
//! `trainer --publish <receipt-dir>` arm wrote).
//!
//! STW-028 shipped the `trainer --verify-receipt
//! <path>` CLI subcommand that re-verifies a
//! local runbook receipt with a single static
//! binary; STW-032 lands the *bundle* verifier
//! the runbook doc names as the next slice — a
//! `trainer --verify-bundle` arm a third party
//! (a testnet dashboard's "verify" button, a CI
//! auditor, a release-gate script) can run
//! against a `publish/testnet-live-proof-<UTC-ISO>/`
//! directory the publish step wrote.
//!
//! The whole module is a thin wrapper over the
//! STW-032 [`crate::publish::PublishedBundle`]
//! surface (`from_bundle_path` + `verify`). The
//! wrapper converts the typed `PublishError`
//! variants (`ManifestShape` /
//! `BundleHashMismatch` / `MissingFile` /
//! `FileUnreadable`) into a single one-line
//! `live_proof bundle verification failed: <kind>
//! <detail>` string the CLI can `eprintln!` so a
//! dashboard scraper can `grep ^live_proof
//! bundle verification failed:` the log. The
//! happy-path print is a one-line `live_proof
//! bundle verification passed: <headline>` line
//! a dashboard can `grep ^live_proof bundle
//! verification passed:` the log. The contract
//! mirrors `crate::verify_receipt::run`
//! (the STW-028 `Mode::VerifyReceipt` arm's
//! handler): `Result<String, String>` where
//! `Ok(s)` is a printable success line and
//! `Err(s)` is a printable failure line; the
//! caller (`Mode::run`) does the `eprintln!` +
//! exit-code mapping.
//!
//! ## Why a new module and not a function on `publish`
//!
//! `publish.rs` is the *library* surface a
//! `cargo test` integration test uses (the
//! `PublishedBundle` struct + the
//! `publish_receipt` top-level entry point +
//! the typed `PublishError` enum). `verify_bundle.rs`
//! is the *CLI* surface a `bash` script or a
//! testnet dashboard uses. Splitting them keeps
//! the typed `PublishError` enum and the
//! CLI's printable error string
//! (`live_proof bundle verification failed: <kind>
//! <detail>`) on different code paths: a
//! regression in the printed line does not
//! change the typed error a Rust caller
//! `match`es on, and vice versa. The
//! `Mode::VerifyBundle` arm uses only the CLI
//! side. The split mirrors the
//! `crate::verify_receipt` module's split with
//! the underlying [`crate::receipt`] module.
//!
//! ## Why the handler is sync, not async
//!
//! The whole pipeline is `fs::read_to_string` +
//! `serde_json::from_str` + a `sha2::Sha256`
//! walk + a `tar` extract. There is no I/O
//! latency worth awaiting, and the upstream
//! `Mode::run` is `async` only because every
//! *other* variant opens a DB. A sync `run`
//! means the no-DB early-dispatch arm in
//! `Mode::run` is a plain `match` with no
//! `.await` call.

use std::path::Path;

use crate::publish::{
    PublishedBundle, STW032_VERIFY_FAILURE_HEADLINE_PREFIX, STW032_VERIFY_HEADLINE_PREFIX,
};

/// CLI entry point. Re-verify a publish bundle:
/// parse the on-disk `manifest.json`, re-hash the
/// tarball, re-hash every file inside the tarball,
/// and assert every digest matches the manifest.
///
/// On success the returned `String` is the
/// `live_proof bundle verification passed: ...`
/// headline a dashboard scraper can `grep ^live_proof
/// bundle verification passed:`. The headline names
/// the bundle's `bundle_filename`, the
/// `files.len()` (so a `awk '{print $5}'` scraper can
/// group reports by file count), the `total_bytes`
/// (so a scraper can graph bundle size growth), and
/// the `bundle_sha256` (so a `sha256sum -c` step
/// against the on-disk `.sha256` file has a
/// human-readable anchor in the log).
///
/// On error, the returned `String` is the
/// `live_proof bundle verification failed: ...` line
/// that names the failure mode + the precise detail
/// (the file that mismatched, the missing path, the
/// unreadable file). The `kind` token in the line
/// (`manifest_shape` / `bundle_hash_mismatch` /
/// `missing_file` / `file_unreadable`) is the prefix
/// a dashboard scraper can `awk '{print $5}'` to
/// group failures.
pub fn run(bundle_dir: &Path) -> Result<String, String> {
    let manifest = PublishedBundle::from_bundle_path(bundle_dir)
        .map_err(|e| format!("{STW032_VERIFY_FAILURE_HEADLINE_PREFIX} manifest_shape: {e}"))?;
    manifest
        .verify(bundle_dir)
        .map_err(|e| format!("{STW032_VERIFY_FAILURE_HEADLINE_PREFIX} {e}"))?;
    Ok(format!(
        "{STW032_VERIFY_HEADLINE_PREFIX} bundle={} files={} bytes={} sha256={}",
        manifest.bundle_filename,
        manifest.files.len(),
        manifest.total_bytes,
        manifest.bundle_sha256
    ))
}

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-032
    //! verify-bundle CLI surface. These tests do NOT
    //! require a live Postgres (the verifier is the
    //! *consumer* side of the on-disk bundle
    //! contract). The tests are constants-only
    //! (the prefix-pin contract is the surface a
    //! future dashboard scraper depends on); the
    //! publish-then-verify round-trip is exercised
    //! end-to-end in [`crate::publish::tests`]
    //! (`verify_round_trips_published_bundle`) +
    //! the new `crates/autotrain/tests/publish.rs`
    //! integration test.
    use super::*;

    /// The CLI headline prefix is pinned. A
    /// regression in the `live_proof bundle
    /// verification passed:` prefix would break
    /// every dashboard scraper that greps the
    /// prefix from the log; the lib test pins the
    /// exact bytes.
    #[test]
    fn verify_headline_prefix_is_pinned() {
        assert_eq!(
            STW032_VERIFY_HEADLINE_PREFIX, "live_proof bundle verification passed:",
            "the verify-bundle success headline prefix is pinned so dashboard \
             scrapers can grep it; a drift here is a breaking change"
        );
    }

    /// The CLI failure headline prefix is pinned.
    /// A regression in the `live_proof bundle
    /// verification failed:` prefix would break
    /// every dashboard scraper that greps the
    /// failure prefix from the log; the lib test
    /// pins the exact bytes.
    #[test]
    fn verify_failure_headline_prefix_is_pinned() {
        assert_eq!(
            STW032_VERIFY_FAILURE_HEADLINE_PREFIX, "live_proof bundle verification failed:",
            "the verify-bundle failure headline prefix is pinned so dashboard \
             scrapers can grep it; a drift here is a breaking change"
        );
    }

    /// Re-verify the committed no-DB publish
    /// fixture the repo ships at
    /// `crates/autotrain/tests/fixtures/publish-fixture/`
    /// on every `cargo test --workspace` run. A
    /// drift in either the fixture (a tarball
    /// byte change, a manifest field drift) or the
    /// verifier (a `from_bundle_path` /
    /// `PublishedBundle::verify` regression) fails
    /// this test before a downstream auditor can
    /// trust the bundle. The fixture is a
    /// byte-stable green-bundle contract a CI
    /// worker can `trainer --verify-bundle` against
    /// on any machine without a Postgres.
    #[test]
    fn run_verifies_committed_publish_fixture() {
        // Walk up from `CARGO_MANIFEST_DIR` to the
        // workspace root, then into the committed
        // `tests/fixtures/publish-fixture/`
        // directory. The fixture is a portable
        // reference bundle; a regression that
        // removes the fixture (or moves the
        // `tests/fixtures/` directory) fails the
        // test at the `fixture.exists()` check
        // before reaching the verifier.
        let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let workspace_root = manifest_dir
            .parent()
            .and_then(|p| p.parent())
            .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain");
        let fixture = workspace_root
            .join("crates")
            .join("autotrain")
            .join("tests")
            .join("fixtures")
            .join("publish-fixture");
        assert!(
            fixture.is_dir(),
            "STW-032 committed publish fixture missing at {}; \
             the testnet live launch publish surface has no portable \
             no-DB reference bundle for downstream auditors",
            fixture.display()
        );
        let manifest = PublishedBundle::from_bundle_path(&fixture)
            .expect("committed publish fixture must parse manifest.json");
        manifest
            .verify(&fixture)
            .expect("committed publish fixture must verify cleanly");
        // The CLI handler returns the pinned
        // success headline; pin the prefix here
        // too so a future regression that breaks
        // the headline's prefix (without breaking
        // the verify step) fails this test.
        let cli = run(&fixture).expect("verify_bundle::run must pass on the fixture");
        assert!(
            cli.starts_with(STW032_VERIFY_HEADLINE_PREFIX),
            "verify-bundle CLI headline must start with the pinned prefix; got: {cli:?}"
        );
    }
}
