//! `trainer --verify-receipt <path>` end-to-end CLI
//! integration test (STW-028).
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh`, the
//! operator runbook that drives the seven-step chain and
//! writes a `receipts/testnet-live-proof-<UTC-ISO>/`
//! bundle. STW-023 shipped the `LiveProofReceipt`
//! verifier the `cargo test` integration test calls.
//! STW-028 wires the verifier into the `trainer` binary
//! so an operator (or a testnet dashboard) can re-verify
//! a receipt a runbook run produced *without* running
//! `cargo test` or installing the workspace's library
//! crates — a single static `trainer` binary is enough.
//!
//! The integration test pins the on-the-wire contract
//! the new CLI mode exposes. It is `cargo test
//! --workspace` green without a live Postgres (the
//! verifier is no-DB, the producer is `LiveProofReceipt::
//! write_to` which writes pure on-disk stubs), so a
//! regression in the CLI surface (renamed prefix, missing
//! exit-code, dropped error kind) fails CI before it
//! reaches an operator's machine.
//!
//! The five sub-tests assert:
//!
//! 1. `verify_receipt_run_exits_zero_with_passed_line_on_green_receipt`
//!    — drive `trainer --verify-receipt <dir>` against a
//!    fixture receipt, assert exit 0, assert stdout
//!    starts with the pinned `live_proof receipt
//!    verification passed: ` prefix and carries the
//!    SUMMARY.txt headline verbatim.
//! 2. `verify_receipt_run_exits_two_with_recipe_shape_line_on_missing_dir`
//!    — drive `trainer --verify-receipt <missing-dir>`,
//!    assert exit 2, assert stderr starts with `live_proof
//!    receipt verification failed: recipe_shape: ` and
//!    names the missing `SUMMARY.txt` file.
//! 3. `verify_receipt_run_exits_two_with_step_failed_line_on_nonzero_exit`
//!    — drop a green receipt, rewrite `cluster/exit.txt`
//!    to `1`, drive the new CLI, assert exit 2 + stderr
//!    starts with `live_proof receipt verification failed:
//!    step_failed: ` and names the failing `cluster` step.
//! 4. `verify_receipt_run_exits_two_with_headline_line_on_wrong_prefix`
//!    — drop a green receipt, rewrite SUMMARY.txt with a
//!    non-pinned headline, drive the new CLI, assert
//!    exit 2 + stderr starts with `live_proof receipt
//!    verification failed: headline: `.
//! 5. `verify_receipt_run_exits_two_with_usage_on_missing_path_arg`
//!    — drive `trainer --verify-receipt` (no path) and
//!    assert exit 2 + stderr starts with `Usage: trainer
//!    --verify-receipt <path>` (the "missing path arg"
//!    error the `Mode::VerifyReceipt` arm converts into
//!    a one-line usage + exit 2, matching the
//!    `Mode::Replay` / `Mode::Smoke` "data-quality
//!    problem is a non-zero exit" convention).
//!
//! The test deliberately does **not** require a live
//! `DATABASE_URL`. The fixture receipt is dropped with
//! `LiveProofReceipt::write_to` (a pure on-disk writer),
//! and the trainer is run as a subprocess with no env
//! knobs set. The `trainer_bin_path` helper walks up from
//! `CARGO_MANIFEST_DIR` to the workspace root, the same
//! way `crates/autotrain/tests/{smoke,bench,compare,
//! live_proof}.rs` resolve the binary path.

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
/// live_proof,script_shape}.rs` so the seven integration
/// tests share the same binary resolution discipline.
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
        "rbp-verify-receipt-integ-{label}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Drive the trainer binary with `--verify-receipt
/// <path>` and return the `(stdout, stderr, exit_code)`
/// triple. The function is the integration-test mirror of
/// the `LiveProofReceipt::write_to` lib helper so a
/// regression in the CLI surface (renamed flag, wrong
/// exit code) fails here.
fn run_verify_receipt(path: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--verify-receipt")
        .arg(path)
        .output()
        .expect("spawn trainer --verify-receipt <path>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

#[test]
fn verify_receipt_run_exits_zero_with_passed_line_on_green_receipt() {
    let dir = fresh_dir("green");
    // Drop a green receipt with the five known counts
    // the dashboard will chart: smoke=12, status=12,
    // bench=4, compare=4, replay=256.
    LiveProofReceipt::write_to(
        &dir,
        12,
        12,
        4,
        4,
        256,
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a synthetic green receipt");
    let (stdout, stderr, code) = run_verify_receipt(&dir);
    assert_eq!(
        code, 0,
        "trainer --verify-receipt <green> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.starts_with("live_proof receipt verification passed: "),
        "green-receipt stdout must start with the pinned success prefix; got: {stdout:?}"
    );
    assert!(
        stdout.contains(
            "testnet live_proof complete: smoke=12 status=12 bench=4 compare=4 replay=256"
        ),
        "green-receipt stdout must carry the verbatim SUMMARY.txt headline; got: {stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_receipt_run_exits_two_with_recipe_shape_line_on_missing_dir() {
    // A path that does not exist must produce a
    // `recipe_shape:` error line and exit 2 (the
    // "data-quality problem is a non-zero exit"
    // convention shared with the replay + smoke
    // modes). The integration test exercises the
    // same `read_from` -> `io::ErrorKind::NotFound`
    // path the `crates/autotrain/tests/live_proof_
    // receipt.rs` test pins at the lib level; the
    // subprocess boundary adds the exit-code mapping
    // pin.
    let dir = fresh_dir("missing");
    let path = dir.join("receipts-does-not-exist");
    let (stdout, stderr, code) = run_verify_receipt(&path);
    assert_eq!(
        code, 2,
        "trainer --verify-receipt <missing-dir> must exit 2 (the data-quality-problem \
         convention). --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("live_proof receipt verification failed: recipe_shape: "),
        "missing-dir stderr must start with the pinned `recipe_shape:` kind; got: {stderr:?}"
    );
    // The error must name the file the verifier tried
    // to read (SUMMARY.txt) so a dashboard log can
    // pinpoint the missing piece.
    assert!(
        stderr.contains("SUMMARY.txt"),
        "missing-dir stderr must name `SUMMARY.txt` (the file the verifier tried to read); \
         got: {stderr:?}"
    );
}

#[test]
fn verify_receipt_run_exits_two_with_step_failed_line_on_nonzero_exit() {
    let dir = fresh_dir("step-failed");
    LiveProofReceipt::write_to(
        &dir,
        12,
        12,
        4,
        4,
        256,
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a green receipt");
    // Rewrite the `cluster/exit.txt` to `1` to simulate
    // a real receipt whose first chain step failed.
    std::fs::write(dir.join("cluster").join("exit.txt"), "1\n")
        .expect("rewrite cluster/exit.txt to 1");
    let (stdout, stderr, code) = run_verify_receipt(&dir);
    assert_eq!(
        code, 2,
        "trainer --verify-receipt <step-failed> must exit 2. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("live_proof receipt verification failed: step_failed: "),
        "step-failed stderr must start with the pinned `step_failed:` kind; got: {stderr:?}"
    );
    assert!(
        stderr.contains("cluster") && stderr.contains("exit 1"),
        "step-failed stderr must name the failing step and its exit code; got: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_receipt_run_exits_two_with_headline_line_on_wrong_prefix() {
    let dir = fresh_dir("bad-headline");
    LiveProofReceipt::write_to(
        &dir,
        12,
        12,
        4,
        4,
        256,
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a green receipt");
    // Rewrite the SUMMARY.txt headline so it lacks the
    // pinned `testnet live_proof complete:` prefix. The
    // typed `LiveProofHeadline::parse` rejects the
    // receipt with `VerifyError::Headline(...)`.
    let bad_summary = format!(
        "live_proof complete: smoke=0 status=0 bench=0 compare=0 replay=0\n\n  receipt_dir: {}\n",
        dir.display()
    );
    std::fs::write(dir.join("SUMMARY.txt"), bad_summary)
        .expect("rewrite SUMMARY.txt with the wrong prefix");
    let (stdout, stderr, code) = run_verify_receipt(&dir);
    assert_eq!(
        code, 2,
        "trainer --verify-receipt <bad-headline> must exit 2. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("live_proof receipt verification failed: headline: "),
        "bad-headline stderr must start with the pinned `headline:` kind; got: {stderr:?}"
    );
    // The display impl references the expected
    // prefix so a regression that drops the prefix
    // gate surfaces here as a missing substring.
    assert!(
        stderr.contains("testnet live_proof complete:"),
        "bad-headline stderr must reference the expected pinned prefix; got: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn verify_receipt_run_exits_two_with_usage_on_missing_path_arg() {
    // The `trainer --verify-receipt` mode without a
    // path is the "missing path arg" error the
    // `Mode::VerifyReceipt` arm converts into a
    // one-line usage + exit 2 (the "data-quality
    // problem is a non-zero exit" convention shared
    // with the `Mode::Replay` / `Mode::Smoke` arms).
    let out = trainer_bin()
        .arg("--verify-receipt")
        .output()
        .expect("spawn trainer --verify-receipt (no path)");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(
        code, 2,
        "trainer --verify-receipt (no path) must exit 2 (missing-path-arg convention). \
         --- stderr ---\n{stderr}"
    );
    assert!(
        stderr.starts_with("Usage: trainer --verify-receipt <path>"),
        "missing-path-arg stderr must start with the pinned one-line usage; got: {stderr:?}"
    );
}
