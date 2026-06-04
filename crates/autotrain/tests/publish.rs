//! `trainer --publish <receipt-dir>` end-to-end CLI
//! integration test (STW-032).
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh`, the
//! operator runbook that drives the seven-step chain and
//! writes a `receipts/testnet-live-proof-<UTC-ISO>/` bundle.
//! STW-023 shipped the `LiveProofReceipt` verifier the
//! `cargo test` integration test calls. STW-028 wired the
//! verifier into the `trainer` binary as the
//! `trainer --verify-receipt <path>` subcommand. STW-032
//! wires the *publish* step the runbook doc names as the
//! "next slice (`testnet-live-publish`)" — a deterministic,
//! content-addressed portable publish bundle a third
//! party (a testnet dashboard bucket, a CI auditor, a
//! release-gate script) can fetch + re-verify without
//! re-running the chain.
//!
//! The two sub-tests assert the on-the-wire contract the
//! new CLI modes (`trainer --publish` +
//! `trainer --verify-bundle`) expose. They are
//! `cargo test --workspace` green without a live Postgres
//! (the producer is `LiveProofReceipt::write_to`, a pure
//! on-disk writer; the publisher is no-DB; the verifier is
//! no-DB), so a regression in the CLI surface (renamed
//! prefix, missing exit-code, dropped error kind, wrong
//! headline shape) fails CI before it reaches an
//! operator's machine.
//!
//! ## Sub-tests
//!
//! 1. `publish_round_trips_through_real_trainer_binary` —
//!    drop a synthetic green receipt, drive
//!    `trainer --publish <receipt>` end-to-end, assert
//!    exit 0, assert stdout starts with the pinned
//!    `live_proof publish complete: ` prefix and names the
//!    bundle path + sha256 + byte count the publisher
//!    returned. Then drive `trainer --verify-bundle` on
//!    the bundle, assert exit 0 + the pinned
//!    `live_proof bundle verification passed: ` prefix.
//!    The round-trip proves a real subprocess
//!    `trainer --publish` writes a bundle a real
//!    subprocess `trainer --verify-bundle` can
//!    re-verify — the on-the-wire contract the bash
//!    runbook + the testnet dashboard rely on.
//! 2. `publish_run_exits_two_with_receipt_red_line_on_red_receipt` —
//!    drop a green receipt, rewrite
//!    `cluster/exit.txt` to `1` (a red receipt the
//!    receipt verifier will reject), drive
//!    `trainer --publish <receipt>`, assert exit 2,
//!    assert stderr starts with
//!    `live_proof publish error: receipt is red: ` and
//!    references the STW-023 verifier's failure detail.
//!    The pre-tar gate the publisher enforces is the
//!    "no paper-over a red receipt" invariant the bash
//!    runbook also enforces via
//!    `trainer --verify-receipt <receipt>` before tarring.
//!
//! The test deliberately does **not** require a live
//! `DATABASE_URL`. The fixture receipt is dropped with
//! `LiveProofReceipt::write_to` (a pure on-disk writer),
//! and the trainer is run as a subprocess with no env
//! knobs set. The `trainer_bin_path` helper walks up from
//! `CARGO_MANIFEST_DIR` to the workspace root, the same
//! way `crates/autotrain/tests/{smoke,bench,compare,
//! live_proof,script_shape,verify_receipt}.rs` resolve
//! the binary path.

use rbp_autotrain::LiveProofReceipt;
use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Locate the `trainer` binary inside the workspace's
/// `target/` directory. We walk up from
/// `CARGO_MANIFEST_DIR` to the workspace root, then probe
/// `<workspace>/target/{debug,release}/trainer` and return
/// whichever exists. The function panics if the binary
/// is missing — that is a setup error (the operator did
/// not build the trainer), not a silent test skip.
/// Mirrors the helper in
/// `crates/autotrain/tests/{smoke,bench,compare,
/// live_proof,script_shape,verify_receipt}.rs` so the
/// eight integration tests share the same binary
/// resolution discipline.
fn trainer_bin_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // `crates/autotrain/Cargo.toml` -> `crates/autotrain` -> `crates` -> workspace.
    let workspace = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain");
    for profile in ["debug", "release"] {
        let candidate = workspace.join("target").join(profile).join("trainer");
        if candidate.exists() {
            return candidate;
        }
    }
    panic!(
        "trainer binary not found under {}; run `cargo build --bin trainer` first",
        workspace.join("target").display()
    )
}

fn trainer_bin() -> Command {
    Command::new(trainer_bin_path())
}

/// Per-process unique temp dir for the fixture. The
/// `SEQ` counter disambiguates parallel `cargo test`
/// invocations.
static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fresh_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbp-publish-integ-{label}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Drive the trainer binary with `--publish <receipt>`
/// and return the `(stdout, stderr, exit_code)` triple.
/// The function is the integration-test mirror of the
/// `crate::publish::publish_receipt` lib entry point so a
/// regression in the CLI surface (renamed flag, wrong
/// exit code, missing-path-arg convention) fails here.
fn run_publish(receipt_dir: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--publish")
        .arg(receipt_dir)
        .output()
        .expect("spawn trainer --publish <receipt>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Drive the trainer binary with `--verify-bundle
/// <bundle-dir>` and return the `(stdout, stderr,
/// exit_code)` triple. The function is the
/// integration-test mirror of the
/// `crate::verify_bundle::run` lib entry point so a
/// regression in the CLI surface (renamed flag, wrong
/// exit code, missing-path-arg convention) fails here.
fn run_verify_bundle(bundle_dir: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--verify-bundle")
        .arg(bundle_dir)
        .output()
        .expect("spawn trainer --verify-bundle <path>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn publish_round_trips_through_real_trainer_binary() {
    // Drop a green receipt with the five known counts
    // the dashboard will chart: smoke=12, status=12,
    // bench=4, compare=4, replay=256. The publisher
    // re-verifies the receipt as a pre-tar gate, so the
    // round-trip test exercises both the publish step
    // AND the underlying `LiveProofReceipt::read_and_verify`
    // surface on a green receipt.
    let receipt = fresh_dir("roundtrip-receipt");
    LiveProofReceipt::write_to(
        &receipt,
        12,
        12,
        4,
        4,
        256,
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a synthetic green receipt");
    // The publish step writes the bundle to
    // `<parent>/publish/<basename>/` (the
    // `Mode::Publish` arm's `parent.join("publish").join(basename)`
    // convention), so the test reads the bundle path
    // from the trainer's stdout and uses that for the
    // verify step. Mirrors the bash runbook's
    // `PUBLISH_DIR="$RECEIPT_PARENT/publish/$RECEIPT_BASENAME"`
    // choice.
    let (publish_stdout, publish_stderr, publish_code) = run_publish(&receipt);
    assert_eq!(
        publish_code, 0,
        "trainer --publish <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{publish_stdout}\n--- stderr ---\n{publish_stderr}"
    );
    assert!(
        publish_stdout.starts_with("live_proof publish complete: "),
        "publish stdout must start with the pinned success prefix; got: {publish_stdout:?}"
    );
    // The headline must carry a `bundle=...` token so
    // a dashboard scraper can `awk '{print $5}'` the
    // bundle path. We do not pin the exact path (the
    // temp dir is process-unique); we only assert
    // the headline is non-empty and the bundle path
    // substring is present.
    assert!(
        publish_stdout.contains("bundle=")
            && publish_stdout.contains("sha256=")
            && publish_stdout.contains("bytes="),
        "publish stdout must carry the bundle path + sha256 + byte count the publisher returned; got: {publish_stdout:?}"
    );
    // Extract the bundle path from the headline. The
    // headline format is
    // `live_proof publish complete: bundle=<bundle.tar.gz path> sha256=<64hex> bytes=<N> files=<N> basename=<name>`.
    // The `bundle=` token points at the `bundle.tar.gz`
    // file (not its parent dir); the test derives the
    // bundle directory as the file's parent so the
    // verifier can read the manifest + tarball + sha256
    // sibling files.
    let bundle_token = publish_stdout
        .split_whitespace()
        .find(|tok| tok.starts_with("bundle="))
        .expect("publish headline must contain a `bundle=<path>` token");
    let bundle_file = PathBuf::from(bundle_token.trim_start_matches("bundle="));
    let bundle_dir = bundle_file
        .parent()
        .expect("bundle= path must have a parent directory")
        .to_path_buf();
    assert!(
        bundle_dir.is_dir(),
        "publish headline's bundle path {} must be a real directory on disk",
        bundle_dir.display()
    );
    // The on-disk bundle must contain the three
    // pinned files the verifier reads:
    // `bundle.tar.gz` + `manifest.json` +
    // `bundle.sha256`. A regression that drops a
    // file (or writes them under a different name)
    // fails the verify step below.
    assert!(
        bundle_dir.join("bundle.tar.gz").is_file(),
        "publish step must write a `bundle.tar.gz` file under {}",
        bundle_dir.display()
    );
    assert!(
        bundle_dir.join("manifest.json").is_file(),
        "publish step must write a `manifest.json` file under {}",
        bundle_dir.display()
    );
    assert!(
        bundle_dir.join("bundle.sha256").is_file(),
        "publish step must write a `bundle.sha256` file under {}",
        bundle_dir.display()
    );
    // Now re-verify the bundle through a real
    // subprocess. The round-trip proves the
    // `trainer --publish` + `trainer --verify-bundle`
    // pair the bash runbook + the testnet dashboard
    // depend on is end-to-end green.
    let (verify_stdout, verify_stderr, verify_code) = run_verify_bundle(&bundle_dir);
    assert_eq!(
        verify_code, 0,
        "trainer --verify-bundle <publish-output> must exit 0 on a fresh bundle. \
         --- stdout ---\n{verify_stdout}\n--- stderr ---\n{verify_stderr}"
    );
    assert!(
        verify_stdout.starts_with("live_proof bundle verification passed: "),
        "verify stdout must start with the pinned success prefix; got: {verify_stdout:?}"
    );
    assert!(
        verify_stdout.contains("files=")
            && verify_stdout.contains("bytes=")
            && verify_stdout.contains("sha256="),
        "verify stdout must carry the files + bytes + sha256 the verifier returned; got: {verify_stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&receipt);
    let _ = std::fs::remove_dir_all(&bundle_dir);
}

#[test]
fn publish_run_exits_two_with_receipt_red_line_on_red_receipt() {
    // The publish step's pre-tar gate is "refuse to
    // publish a red receipt". Drop a green receipt,
    // rewrite `cluster/exit.txt` to `1` (a red
    // receipt the STW-023 verifier rejects), drive
    // `trainer --publish <receipt>`, assert exit 2 +
    // stderr carries the pinned
    // `live_proof publish error: receipt is red: `
    // prefix. The on-the-wire contract the bash
    // runbook's `trainer --verify-receipt` pre-gate
    // also enforces: a publish of a red receipt is a
    // hard error, not a warning.
    let receipt = fresh_dir("red-receipt");
    LiveProofReceipt::write_to(
        &receipt,
        12,
        12,
        4,
        4,
        256,
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a green receipt");
    // Make the receipt red by rewriting one
    // step's `exit.txt` to 1. The STW-023 verifier
    // rejects the receipt with `step_failed: ...`
    // which the publisher surfaces as
    // `PublishError::ReceiptRed(...)` and the CLI
    // prints with the
    // `live_proof publish error: receipt is red: `
    // prefix.
    std::fs::write(receipt.join("cluster").join("exit.txt"), "1\n")
        .expect("rewrite cluster/exit.txt to 1");
    let (stdout, stderr, code) = run_publish(&receipt);
    assert_eq!(
        code, 2,
        "trainer --publish <red-receipt> must exit 2 (the data-quality-problem convention). \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("live_proof publish error: receipt is red: "),
        "red-receipt stderr must start with the pinned `receipt is red: ` prefix; got: {stderr:?}"
    );
    // The publisher must NOT have written a bundle
    // for a red receipt. The on-disk contract is:
    // `trainer --publish <red-receipt>` exits 2 with
    // the pinned error prefix AND the publish
    // directory is empty. A regression that
    // accidentally tars a red receipt (or writes a
    // partial bundle) fails here.
    let parent = receipt.parent().expect("receipt must have a parent");
    let basename = receipt
        .file_name()
        .and_then(|n| n.to_str())
        .expect("receipt must have a basename");
    let publish_dir = parent.join("publish").join(basename);
    assert!(
        !publish_dir.join("bundle.tar.gz").exists(),
        "publisher must NOT write a bundle for a red receipt (publish dir = {})",
        publish_dir.display()
    );
    let _ = std::fs::remove_dir_all(&receipt);
    let _ = std::fs::remove_dir_all(&publish_dir);
}
