//! `trainer --publish-remote <receipt-dir> --bucket <s3://...>`
//! + `trainer --verify-remote <remote-dir>` end-to-end CLI
//! integration test (STW-033).
//!
//! STW-032 shipped `trainer --publish <receipt>` +
//! `trainer --verify-bundle <bundle-dir>`, the deterministic
//! content-addressed portable publish bundle a third
//! party (a testnet dashboard bucket, a CI auditor, a
//! release-gate script) can fetch + re-verify without
//! re-running the chain. STW-033 lands the **remote-upload
//! half** the STW-032 runbook doc names as the "next slice
//! (`testnet-live-publish`)" — a deterministic
//! upload plan + a `remote_receipt.json` manifest a
//! dashboard can re-fetch + re-verify without re-running
//! the chain.
//!
//! The three sub-tests assert the on-the-wire contract
//! the new CLI arms (`trainer --publish-remote` +
//! `trainer --verify-remote`) expose. They are
//! `cargo test --workspace` green without a live Postgres
//! (the publisher-remote defaults to `dry-run=true` so the
//! test does not shell out to `aws`; the verifier is
//! no-DB; the producer is `LiveProofReceipt::write_to`),
//! so a regression in the CLI surface (renamed flag,
//! wrong exit code, missing-path-arg convention, missing
//! `bucket=` headline token) fails CI before it reaches
//! an operator's machine.
//!
//! ## Sub-tests
//!
//! 1. `publish_remote_round_trips_through_real_trainer_binary` —
//!    drop a synthetic green receipt, drive
//!    `trainer --publish <receipt>` end-to-end to materialise
//!    the STW-032 publish bundle, then drive
//!    `trainer --publish-remote <receipt> --bucket <s3://...>`
//!    end-to-end, assert exit 0, assert stdout starts
//!    with the pinned
//!    `live_proof publish_remote complete: ` prefix, and
//!    assert the on-disk `remote_plan.json` +
//!    `remote_receipt.json` are readable + re-verifiable
//!    through a real subprocess
//!    `trainer --verify-remote <remote-dir>`. The
//!    round-trip proves a real subprocess
//!    `trainer --publish-remote` writes a remote-receipt
//!    a real subprocess `trainer --verify-remote` can
//!    re-verify — the on-the-wire contract the
//!    testnet dashboard's `aws s3 cp --recursive` +
//!    `trainer --verify-remote` round-trip depends on.
//! 2. `publish_remote_run_exits_two_with_red_receipt_line` —
//!    drop a green receipt, rewrite `cluster/exit.txt`
//!    to `1` (a red receipt the STW-023 verifier
//!    rejects), drive `trainer --publish-remote <receipt>
//!    --bucket <s3://...>`, assert exit 2, assert stderr
//!    starts with
//!    `live_proof publish_remote error: receipt is red: `.
//!    The pre-upload gate the publisher-remote
//!    enforces is the "no paper-over a red receipt"
//!    invariant the STW-023 / STW-032 trainers already
//!    enforce.
//! 3. `publish_remote_run_exits_two_with_missing_bucket` —
//!    drive `trainer --publish-remote <receipt>` with
//!    no `--bucket` flag, assert exit 2, assert stderr
//!    carries the usage line. The on-the-wire contract
//!    is: missing required argv is exit 2 + usage on
//!    stderr, not a silent skip.
//!
//! The test deliberately does **not** require a live
//! `DATABASE_URL` or a live S3 bucket. The fixture
//! receipt is dropped with `LiveProofReceipt::write_to`
//! (a pure on-disk writer), the publisher is invoked as
//! a subprocess with `--no-dry-run` left off (the
//! default is dry-run), and the verifier is invoked
//! against the local on-disk receipt. The
//! `trainer_bin_path` helper walks up from
//! `CARGO_MANIFEST_DIR` to the workspace root, the same
//! way
//! `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish}.rs`
//! resolve the binary path.

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
/// `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish}.rs`
/// so the nine integration tests share the same binary
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
        "rbp-publish-remote-integ-{label}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Drive the trainer binary with
/// `trainer --publish <receipt>` and return the
/// `(stdout, stderr, exit_code)` triple. The fixture
/// publisher step the publish-remote step consumes.
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

/// Drive the trainer binary with
/// `trainer --publish-remote <receipt> --bucket <bucket>
/// [--prefix <prefix>]` and return the `(stdout, stderr,
/// exit_code)` triple. The function is the
/// integration-test mirror of the
/// `crate::publish_remote::publish_remote_receipt` lib
/// entry point so a regression in the CLI surface
/// (renamed flag, wrong exit code, missing-bucket-arg
/// convention) fails here.
fn run_publish_remote(receipt_dir: &PathBuf, bucket: &str, prefix: &str) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.arg("--publish-remote").arg(receipt_dir);
    cmd.arg("--bucket").arg(bucket);
    if !prefix.is_empty() {
        cmd.arg("--prefix").arg(prefix);
    }
    let out = cmd
        .output()
        .expect("spawn trainer --publish-remote <receipt>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Drive the trainer binary with
/// `trainer --verify-remote <remote-dir>` and return the
/// `(stdout, stderr, exit_code)` triple. The function is
/// the integration-test mirror of the
/// `crate::publish_remote::PublishedRemoteReceipt::verify`
/// lib verifier.
fn run_verify_remote(remote_dir: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--verify-remote")
        .arg(remote_dir)
        .output()
        .expect("spawn trainer --verify-remote <path>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Drive the trainer binary with
/// `trainer --publish-remote <receipt>` (no `--bucket`)
/// to exercise the missing-required-argv path.
fn run_publish_remote_missing_bucket(receipt_dir: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--publish-remote")
        .arg(receipt_dir)
        .output()
        .expect("spawn trainer --publish-remote <receipt> (no --bucket)");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Extract the bundle directory from the
/// `trainer --publish <receipt>` headline. The headline
/// format is
/// `live_proof publish complete: bundle=<bundle.tar.gz path> sha256=<64hex> bytes=<N> files=<N> basename=<name>`.
/// The `bundle=` token points at the `bundle.tar.gz`
/// file (not its parent dir); the test derives the
/// bundle directory as the file's parent.
fn extract_bundle_dir(publish_stdout: &str) -> PathBuf {
    let bundle_token = publish_stdout
        .split_whitespace()
        .find(|tok| tok.starts_with("bundle="))
        .expect("publish headline must contain a `bundle=<path>` token");
    let bundle_file = PathBuf::from(bundle_token.trim_start_matches("bundle="));
    bundle_file
        .parent()
        .expect("bundle= path must have a parent directory")
        .to_path_buf()
}

#[test]
fn publish_remote_round_trips_through_real_trainer_binary() {
    // Drop a green receipt the dashboard will chart:
    // smoke=12, status=12, bench=4, compare=4,
    // replay=256. The publisher-remote step
    // re-verifies the receipt as a pre-upload gate,
    // so the round-trip test exercises both the
    // publish-remote step AND the underlying
    // `LiveProofReceipt::read_and_verify` surface on
    // a green receipt.
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
    // First materialise the STW-032 publish bundle
    // (the publish-remote step is a *consumer* of
    // it, not a refactor of it). The publish
    // step writes the bundle to
    // `<parent>/publish/<basename>/` so the test
    // reads the bundle path from the trainer's
    // stdout.
    let (publish_stdout, publish_stderr, publish_code) = run_publish(&receipt);
    assert_eq!(
        publish_code, 0,
        "trainer --publish <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{publish_stdout}\n--- stderr ---\n{publish_stderr}"
    );
    let bundle_dir = extract_bundle_dir(&publish_stdout);
    assert!(
        bundle_dir.is_dir(),
        "publish headline's bundle path {} must be a real directory on disk",
        bundle_dir.display()
    );
    // Now drive the publish-remote arm. The arm
    // writes the upload plan + the post-upload
    // receipt to `<bundle_dir>/remote/`.
    let (pr_stdout, pr_stderr, pr_code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof-20260604T050000Z/",
    );
    assert_eq!(
        pr_code, 0,
        "trainer --publish-remote <green-receipt> --bucket <s3://...> must exit 0 on a \
         fixture green receipt. --- stdout ---\n{pr_stdout}\n--- stderr ---\n{pr_stderr}"
    );
    assert!(
        pr_stdout.starts_with("live_proof publish_remote complete: "),
        "publish-remote stdout must start with the pinned success prefix; got: {pr_stdout:?}"
    );
    // The headline must carry every field the
    // dashboard scraper reads: `bucket=` +
    // `prefix=` + `files=` + `bytes=` +
    // `bundle_sha256=` + `basename=` + `dry_run=`.
    // The exact sha256 is the parent
    // `PublishedBundle::bundle_sha256` (we do not
    // pin the value — the bundle is a fixture,
    // not a byte-exact value).
    for token in [
        "bucket=",
        "prefix=",
        "files=",
        "bytes=",
        "bundle_sha256=",
        "basename=",
        "dry_run=",
    ] {
        assert!(
            pr_stdout.contains(token),
            "publish-remote stdout must carry the {token} token; got: {pr_stdout:?}"
        );
    }
    // The on-disk `remote/` directory must
    // contain the two pinned files the verifier
    // reads: `remote_plan.json` +
    // `remote_receipt.json`. A regression that
    // drops either file (or writes them under a
    // different name) fails the verify step
    // below.
    let remote_dir = bundle_dir.join("remote");
    assert!(
        remote_dir.is_dir(),
        "publish-remote must write a `remote/` dir under {}",
        bundle_dir.display()
    );
    assert!(
        remote_dir.join("remote_plan.json").is_file(),
        "publish-remote must write a `remote_plan.json` under {}",
        remote_dir.display()
    );
    assert!(
        remote_dir.join("remote_receipt.json").is_file(),
        "publish-remote must write a `remote_receipt.json` under {}",
        remote_dir.display()
    );
    // Now re-verify the remote receipt through a
    // real subprocess. The round-trip proves the
    // `trainer --publish-remote` +
    // `trainer --verify-remote` pair the
    // testnet dashboard depends on is
    // end-to-end green.
    let (vr_stdout, vr_stderr, vr_code) = run_verify_remote(&remote_dir);
    assert_eq!(
        vr_code, 0,
        "trainer --verify-remote <publish-output>/remote/ must exit 0 on a fresh \
         dry-run receipt. --- stdout ---\n{vr_stdout}\n--- stderr ---\n{vr_stderr}"
    );
    assert!(
        vr_stdout.starts_with("live_proof remote verification passed: "),
        "verify-remote stdout must start with the pinned success prefix; got: {vr_stdout:?}"
    );
    // The verifier headline must carry the
    // dashboard-scraper tokens.
    for token in [
        "bucket=",
        "prefix=",
        "files=",
        "bytes=",
        "bundle_sha256=",
        "basename=",
    ] {
        assert!(
            vr_stdout.contains(token),
            "verify-remote stdout must carry the {token} token; got: {vr_stdout:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&receipt);
    let _ = std::fs::remove_dir_all(&bundle_dir);
}

#[test]
fn publish_remote_run_exits_two_with_red_receipt_line() {
    // The publish-remote step's pre-upload gate
    // is "refuse to plan an upload for a red
    // receipt". Drop a green receipt, rewrite
    // `cluster/exit.txt` to `1` (a red receipt
    // the STW-023 verifier rejects), drive
    // `trainer --publish-remote <receipt>
    // --bucket <s3://...>`, assert exit 2 +
    // stderr carries the pinned
    // `live_proof publish_remote error: receipt is red: `
    // prefix. The on-the-wire contract is: a
    // publish-remote of a red receipt is a hard
    // error, not a warning.
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
    // Make the receipt red by rewriting
    // `cluster/exit.txt` to `1`. The STW-023
    // verifier rejects the receipt with
    // `step_failed: ...` which the
    // publisher-remote surfaces as
    // `PublishRemoteError::ReceiptRed(...)` and
    // the CLI prints with the
    // `live_proof publish_remote error: receipt is red: `
    // prefix.
    std::fs::write(receipt.join("cluster").join("exit.txt"), "1\n")
        .expect("rewrite cluster/exit.txt to 1");
    let (stdout, stderr, code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof-20260604T050000Z/",
    );
    assert_eq!(
        code, 2,
        "trainer --publish-remote <red-receipt> must exit 2 (the data-quality-problem convention). \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("live_proof publish_remote error: receipt is red: "),
        "red-receipt stderr must start with the pinned `receipt is red: ` prefix; got: {stderr:?}"
    );
    // The publish-remote step must NOT have
    // written a `remote/` dir for a red
    // receipt. The on-disk contract is:
    // `trainer --publish-remote <red-receipt>
    // --bucket <s3://...>` exits 2 with the
    // pinned error prefix AND the
    // `remote/` dir does not exist. A
    // regression that accidentally plans an
    // upload for a red receipt (or writes a
    // partial plan) fails here.
    let parent = receipt.parent().expect("receipt must have a parent");
    let basename = receipt
        .file_name()
        .and_then(|n| n.to_str())
        .expect("receipt must have a basename");
    let publish_dir = parent.join("publish").join(basename);
    let remote_dir = publish_dir.join("remote");
    assert!(
        !remote_dir.exists(),
        "publish-remote must NOT write a remote/ dir for a red receipt ({} exists)",
        remote_dir.display()
    );
    let _ = std::fs::remove_dir_all(&receipt);
    let _ = std::fs::remove_dir_all(&publish_dir);
}

#[test]
fn publish_remote_run_exits_two_with_missing_bucket() {
    // The publish-remote arm requires a
    // `--bucket <s3://...>` flag. Missing the
    // required flag is exit 2 + a usage line on
    // stderr (the same "missing required argv"
    // convention the `Mode::VerifyBundle` /
    // `Mode::VerifyReceipt` arms enforce).
    let receipt = fresh_dir("missing-bucket");
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
    let (stdout, stderr, code) = run_publish_remote_missing_bucket(&receipt);
    assert_eq!(
        code, 2,
        "trainer --publish-remote <receipt> with no --bucket must exit 2 (the missing-arg convention). \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // The stderr must carry the usage line so an
    // operator reading the log sees the
    // `trainer --publish-remote <receipt-dir>
    // --bucket <s3://...>` shape they need to
    // re-run with.
    assert!(
        stderr.contains("--bucket"),
        "missing-bucket stderr must carry the `--bucket` flag in the usage line; got: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&receipt);
}
