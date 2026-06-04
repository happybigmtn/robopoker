//! `trainer --publish-index-remote <publish-root>
//! --bucket <s3://...> [--prefix <prefix/>] [--no-dry-run]`
//! + `trainer --verify-index-remote <remote-dir>`
//! end-to-end CLI integration test (STW-035).
//!
//! STW-034 shipped `trainer --publish-index <publish-root>`
//! + `trainer --verify-index <index-path>`, the
//! deterministic testnet dashboard aggregator the
//! STW-033 `trainer --publish-remote` chain
//! `remote_receipt.json` files. **STW-035 lands the
//! *remote-upload* of that `INDEX.json`** a CI worker
//! naturally wants (the next slice the STW-034
//! publish-index step's scope-boundary defers to):
//! a deterministic upload plan +
//! `index_remote_receipt.json` pair a CI worker can
//! `aws s3 cp` to push the aggregator to a dashboard
//! bucket, AND a no-DB no-rebuild re-verify path that
//! re-hashes the local `INDEX.json` the post-upload
//! receipt claims to have uploaded.
//!
//! The three sub-tests assert the on-the-wire
//! contract the new CLI arms
//! (`trainer --publish-index-remote` +
//! `trainer --verify-index-remote`) expose. They are
//! `cargo test --workspace` green without a live
//! Postgres (the producer is
//! `LiveProofReceipt::write_to`, a pure on-disk
//! writer; the publisher + publish-remote + indexer +
//! index-remote + verifier are all no-DB no-network),
//! so a regression in the CLI surface (renamed flag,
//! wrong exit code, missing-path-arg convention,
//! missing headline token) fails CI before it reaches
//! an operator's machine.
//!
//! ## Sub-tests
//!
//! 1. `publish_index_remote_round_trips_through_real_trainer_binary`
//!    — drop a synthetic green receipt, drive the
//!    full STW-032 → STW-033 → STW-034 → STW-035
//!    chain end-to-end (publish the bundle,
//!    publish-remote the bundle, publish-index the
//!    resulting publish root, publish-index-remote the
//!    resulting `INDEX.json` to a stub
//!    `s3://robopoker-testnet-dashboard` bucket),
//!    assert exit 0, assert stdout starts with the
//!    pinned
//!    `live_proof publish_index_remote complete: `
//!    prefix and names every field a dashboard
//!    scraper reads (`bucket=` + `prefix=` +
//!    `files=` + `bytes=` + `index_path=` +
//!    `runbook_version=` + `dry_run=`). Then drive
//!    `trainer --verify-index-remote
//!    <remote-dir>` end-to-end, assert exit 0 +
//!    the pinned
//!    `live_proof index_remote verification passed: `
//!    prefix. The round-trip proves a real subprocess
//!    `trainer --publish-index-remote` writes an
//!    `index_remote_receipt.json` a real subprocess
//!    `trainer --verify-index-remote` can re-verify
//!    — the on-the-wire contract the testnet
//!    dashboard's `aws s3 cp` +
//!    `trainer --verify-index-remote` round-trip
//!    depends on.
//! 2. `publish_index_remote_run_exits_two_with_red_index`
//!    — drive the STW-032 → STW-033 → STW-034 chain
//!    to materialise a green publish root, tamper
//!    with the `INDEX.json`'s first per-entry
//!    `remote_receipt.s3_objects[].sha256` to a bogus
//!    value, drive
//!    `trainer --publish-index-remote <root> --bucket
//!    <s3://...>`, assert exit 2, assert stderr
//!    carries the pinned
//!    `live_proof publish_index_remote error: index
//!    is red: ` prefix. The on-the-wire contract
//!    is: a `trainer --publish-index-remote` over a
//!    red `INDEX.json` is a hard error, not a
//!    warning. The pre-upload gate is the "refuse
//!    to paper-over a red index" invariant the
//!    STW-028 receipt verifier + STW-032 bundle
//!    verifier + STW-033 remote-receipt verifier +
//!    STW-034 index verifier already enforce.
//! 3. `publish_index_remote_run_exits_two_with_missing_bucket`
//!    — drive
//!    `trainer --publish-index-remote <publish-root>`
//!    with no `--bucket` flag + assert exit 2 +
//!    the stderr carries the `--bucket` usage
//!    line.
//!
//! The test deliberately does **not** require a
//! live `DATABASE_URL` or a live S3 bucket. The
//! fixture receipt is dropped with
//! `LiveProofReceipt::write_to` (a pure on-disk
//! writer), the publisher is invoked as a
//! subprocess with `--no-dry-run` left off (the
//! default is dry-run), the publish-remote arm
//! defaults to dry-run, the indexer arm is
//! no-network, and the index-remote arm defaults
//! to dry-run (the live `aws s3 cp` shell-out is
//! gated by `--no-dry-run` so a regression in the
//! CLI surface fails CI without an `aws`
//! credential or a live bucket). The
//! `trainer_bin_path` helper walks up from
//! `CARGO_MANIFEST_DIR` to the workspace root, the
//! same way
//! `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish,publish_remote,publish_index}.rs`
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
/// `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish,publish_remote,publish_index}.rs`
/// so the integration tests share the same binary
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

/// STW-051: every subprocess invocation
/// inherits `RBP_PUBLISH_INDEX_UTC=2026-06-04T00:00:00Z`
/// from the parent (the test process). The
/// `--publish-index-remote` arm does not
/// itself read the env knob (it is a
/// remote-uploader, not the aggregator), but
/// the helper pins the same env knob the
/// `publish_index.rs` integration helper
/// pins so a future regression that
/// re-introduces the env-knob read in a
/// shared helper does not silently break
/// the integration suite.
fn trainer_bin() -> Command {
    let mut cmd = Command::new(trainer_bin_path());
    cmd.env("RBP_PUBLISH_INDEX_UTC", "2026-06-04T00:00:00Z");
    cmd
}

/// Per-process unique temp dir for the fixture. The
/// `SEQ` counter disambiguates parallel `cargo test`
/// invocations. The dir is itself a unique parent
/// of the receipt dir, so the trainer binary's
/// `--publish` step writes the bundle under
/// `<parent>/publish/<basename>/` (a per-test
/// `publish_root`) and the STW-035 index-remote
/// step scans *only* the current test's
/// `publish_root/`, never stale files from a
/// previous test run.
static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fresh_test_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbp-publish-index-remote-integ-{label}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("mkdir fresh_test_dir");
    dir
}

/// Drive the trainer binary with
/// `trainer --publish <receipt>` and return the
/// `(stdout, stderr, exit_code)` triple. The
/// STW-032 publish step the STW-033 publish-remote
/// step (and the STW-034 publish-index step + the
/// STW-035 publish-index-remote step) all
/// consume.
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
/// [--prefix <prefix>]` and return the
/// `(stdout, stderr, exit_code)` triple. The
/// STW-033 step the STW-034 indexer consumes (a
/// per-receipt `remote_receipt.json` under
/// `<publish>/<basename>/remote/`).
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
/// `trainer --publish-index <publish-root>` and
/// return the `(stdout, stderr, exit_code)` triple.
/// The STW-034 step the STW-035 index-remote
/// consumes (an `INDEX.json` under
/// `<publish_root>/index/`).
fn run_publish_index(publish_root: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--publish-index")
        .arg(publish_root)
        .output()
        .expect("spawn trainer --publish-index <publish-root>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Drive the trainer binary with
/// `trainer --publish-index-remote <publish-root>
/// --bucket <s3://...> [--prefix <prefix/>]
/// [--no-dry-run]` and return the
/// `(stdout, stderr, exit_code)` triple. The
/// integration-test mirror of the
/// `crate::publish_index_remote::publish_index_remote_receipt`
/// lib entry point so a regression in the CLI
/// surface (renamed flag, wrong exit code,
/// missing-path-arg convention) fails here.
fn run_publish_index_remote(
    publish_root: &PathBuf,
    bucket: &str,
    prefix: &str,
) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.arg("--publish-index-remote").arg(publish_root);
    cmd.arg("--bucket").arg(bucket);
    if !prefix.is_empty() {
        cmd.arg("--prefix").arg(prefix);
    }
    let out = cmd
        .output()
        .expect("spawn trainer --publish-index-remote <publish-root>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Drive the trainer binary with
/// `trainer --verify-index-remote <remote-dir>`
/// and return the `(stdout, stderr, exit_code)`
/// triple. The integration-test mirror of the
/// `crate::publish_index_remote::PublishedIndexRemoteReceipt::verify`
/// lib verifier.
fn run_verify_index_remote(remote_dir: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--verify-index-remote")
        .arg(remote_dir)
        .output()
        .expect("spawn trainer --verify-index-remote <remote-dir>");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Extract the bundle directory from the
/// `trainer --publish <receipt>` headline. The
/// headline format is
/// `live_proof publish complete: bundle=<bundle.tar.gz path> sha256=<64hex> bytes=<N> files=<N> basename=<name>`.
/// The `bundle=` token points at the
/// `bundle.tar.gz` file (not its parent dir); the
/// test derives the bundle directory as the file's
/// parent. Mirrors the `extract_bundle_dir` helper
/// in `crates/autotrain/tests/publish_remote.rs` +
/// `crates/autotrain/tests/publish_index.rs`.
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
fn publish_index_remote_round_trips_through_real_trainer_binary() {
    // Drop a green receipt the dashboard will
    // chart: smoke=12, status=12, bench=4,
    // compare=4, replay=256. The full
    // STW-019 → STW-032 → STW-033 → STW-034 →
    // STW-035 chain (publish the bundle +
    // publish-remote the bundle + publish-index
    // the resulting publish root +
    // publish-index-remote the resulting
    // `INDEX.json` to a stub
    // `s3://robopoker-testnet-dashboard` bucket)
    // must land end-to-end on a fixture green
    // receipt, and the resulting
    // `index_remote_receipt.json` must
    // round-trip through a real subprocess
    // `trainer --verify-index-remote
    // <remote-dir>`.
    let parent = fresh_test_dir("roundtrip");
    let receipt = parent.join("receipt");
    LiveProofReceipt::write_to(
        &receipt,
        12,  // smoke_rows
        12,  // status_blueprint
        4,   // bench_hands
        4,   // compare_hands
        256, // replay_bytes
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a synthetic green receipt");
    // 1. STW-032: publish the bundle.
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
    // 2. STW-033: publish-remote the bundle.
    let publish_root = bundle_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("bundle_dir must have a `<publish_root>/publish/<basename>/` parent")
        .to_path_buf();
    let (_pr_stdout, pr_stderr, pr_code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof/",
    );
    assert_eq!(
        pr_code, 0,
        "trainer --publish-remote <green-receipt> must exit 0. --- stderr ---\n{pr_stderr}"
    );
    // 3. STW-034: publish-index the publish root.
    let (_pi_stdout, pi_stderr, pi_code) = run_publish_index(&publish_root);
    assert_eq!(
        pi_code, 0,
        "trainer --publish-index <publish-root> must exit 0 on a green publish root. \
         --- stderr ---\n{pi_stderr}"
    );
    let index_dir = publish_root.join("index");
    assert!(
        index_dir.is_dir(),
        "STW-034 index dir {} must be a real directory on disk",
        index_dir.display()
    );
    let index_json = index_dir.join("INDEX.json");
    assert!(
        index_json.is_file(),
        "STW-034 INDEX.json must be on disk at {}",
        index_json.display()
    );
    // 4. STW-035: publish-index-remote the
    //    INDEX.json to a stub
    //    `s3://robopoker-testnet-dashboard`
    //    bucket. The arm defaults to
    //    `--dry-run` (the live `aws s3 cp`
    //    shell-out is gated by `--no-dry-run` so
    //    a regression in the CLI surface fails
    //    CI without an `aws` credential or a
    //    live bucket).
    let (pir_stdout, pir_stderr, pir_code) = run_publish_index_remote(
        &publish_root,
        "s3://robopoker-testnet-dashboard",
        "testnet-index/",
    );
    assert_eq!(
        pir_code, 0,
        "trainer --publish-index-remote <green-publish-root> must exit 0 on a green \
         INDEX.json. --- stdout ---\n{pir_stdout}\n--- stderr ---\n{pir_stderr}"
    );
    // The headline must start with the pinned
    // `live_proof publish_index_remote
    // complete: ` prefix.
    assert!(
        pir_stdout.starts_with("live_proof publish_index_remote complete: "),
        "trainer --publish-index-remote headline must start with the pinned prefix; \
         got: {pir_stdout:?}"
    );
    // The headline must name every field a
    // dashboard scraper reads.
    for token in [
        "bucket=",
        "prefix=",
        "files=",
        "bytes=",
        "index_path=",
        "runbook_version=",
        "dry_run=",
    ] {
        assert!(
            pir_stdout.contains(token),
            "trainer --publish-index-remote headline must contain the {token:?} token; \
             got: {pir_stdout:?}"
        );
    }
    // The `index_remote_plan.json` +
    // `index_remote_receipt.json` files must be
    // on disk.
    let remote_dir = publish_root.join("index_remote");
    assert!(
        remote_dir.is_dir(),
        "STW-035 index_remote dir {} must be a real directory on disk",
        remote_dir.display()
    );
    let plan_json = remote_dir.join("index_remote_plan.json");
    let receipt_json = remote_dir.join("index_remote_receipt.json");
    assert!(
        plan_json.is_file(),
        "STW-035 index_remote_plan.json must be on disk at {}",
        plan_json.display()
    );
    assert!(
        receipt_json.is_file(),
        "STW-035 index_remote_receipt.json must be on disk at {}",
        receipt_json.display()
    );
    // 5. STW-035: re-verify the
    //    `index_remote_receipt.json` through
    //    a real subprocess
    //    `trainer --verify-index-remote
    //    <remote-dir>`. The verifier re-hashes
    //    the local `INDEX.json` the receipt
    //    claims to have uploaded + asserts every
    //    digest matches + prints a one-line
    //    `live_proof index_remote verification
    //    passed: ` headline.
    let (vir_stdout, vir_stderr, vir_code) = run_verify_index_remote(&remote_dir);
    assert_eq!(
        vir_code, 0,
        "trainer --verify-index-remote <green-remote-dir> must exit 0 on a green \
         index_remote_receipt.json. --- stdout ---\n{vir_stdout}\n--- stderr ---\n{vir_stderr}"
    );
    assert!(
        vir_stdout.starts_with("live_proof index_remote verification passed: "),
        "trainer --verify-index-remote headline must start with the pinned prefix; \
         got: {vir_stdout:?}"
    );
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn publish_index_remote_run_exits_two_with_red_index() {
    // Drop a green receipt, drive the
    // STW-032 → STW-033 → STW-034 chain to
    // materialise a green publish root, tamper
    // with the `INDEX.json`'s first per-entry
    // `remote_receipt.s3_objects[].sha256` to a
    // bogus value, drive
    // `trainer --publish-index-remote <root>
    // --bucket <s3://...>`, assert exit 2 +
    // stderr carries the pinned
    // `live_proof publish_index_remote error:
    // index is red: ` prefix. The on-the-wire
    // contract is: a
    // `trainer --publish-index-remote` over a
    // red `INDEX.json` is a hard error, not a
    // warning. The pre-upload
    // `PublishIndex::verify` gate is the
    // "refuse to paper-over a red index"
    // invariant the STW-034 index verifier
    // already enforces.
    let parent = fresh_test_dir("red-index");
    let receipt = parent.join("receipt");
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
    let (_publish_stdout, publish_stderr, publish_code) = run_publish(&receipt);
    assert_eq!(
        publish_code, 0,
        "publish must exit 0. stderr={publish_stderr}"
    );
    let bundle_dir = extract_bundle_dir(&_publish_stdout);
    let publish_root = bundle_dir
        .parent()
        .and_then(|p| p.parent())
        .expect("publish root parent")
        .to_path_buf();
    let (_pr_stdout, pr_stderr, pr_code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof/",
    );
    assert_eq!(pr_code, 0, "publish-remote must exit 0. stderr={pr_stderr}");
    let (_pi_stdout, pi_stderr, pi_code) = run_publish_index(&publish_root);
    assert_eq!(pi_code, 0, "publish-index must exit 0. stderr={pi_stderr}");
    // Tamper with the `INDEX.json`'s first
    // per-entry
    // `remote_receipt.s3_objects[].sha256` to
    // a bogus value. The
    // `PublishIndex::verify` re-runs the
    // per-entry
    // `PublishedRemoteReceipt::verify`
    // (which re-hashes the underlying file
    // and compares); a tampered entry
    // `sha256` fails the re-hash with a
    // `BundleHashMismatch`.
    let index_path = publish_root.join("index").join("INDEX.json");
    let mut index: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&index_path).expect("read INDEX.json"))
            .expect("parse INDEX.json");
    if let Some(entries) = index.get_mut("entries").and_then(|v| v.as_array_mut()) {
        if let Some(first) = entries.first_mut() {
            if let Some(remote_receipt) = first.get_mut("remote_receipt") {
                if let Some(s3_objects) = remote_receipt
                    .get_mut("s3_objects")
                    .and_then(|v| v.as_array_mut())
                {
                    if let Some(obj) = s3_objects.first_mut() {
                        obj["sha256"] = serde_json::Value::String("deadbeef".repeat(8));
                    }
                }
            }
        }
    }
    let tampered_str = serde_json::to_string_pretty(&index).expect("serialise tampered index");
    std::fs::write(&index_path, tampered_str).expect("write tampered INDEX.json");
    let (_pir_stdout, pir_stderr, pir_code) = run_publish_index_remote(
        &publish_root,
        "s3://robopoker-testnet-dashboard",
        "testnet-index/",
    );
    assert_eq!(
        pir_code, 2,
        "trainer --publish-index-remote <red-INDEX.json> must exit 2. \
         --- stdout ---\n{_pir_stdout}\n--- stderr ---\n{pir_stderr}"
    );
    assert!(
        pir_stderr.contains("live_proof publish_index_remote error: index is red: ")
            || pir_stderr
                .contains("live_proof publish_index_remote error: bundle_hash_mismatch: ",),
        "trainer --publish-index-remote stderr must carry the pinned \
         `live_proof publish_index_remote error: index is red: ` prefix (or the \
         bundle_hash_mismatch variant the verify path fires when the underlying \
         `PublishedRemoteReceipt::verify` re-hash fails); got: {pir_stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn publish_index_remote_run_exits_two_with_missing_bucket() {
    // Drive `trainer --publish-index-remote
    // <publish-root>` with no `--bucket` flag
    // + assert exit 2 + the stderr carries
    // the `--bucket` usage line. The
    // on-the-wire contract is: a missing
    // `--bucket` is exit 2 + a one-line
    // usage, not a silent skip.
    let parent = fresh_test_dir("missing-bucket");
    let publish_root = parent.join("publish-root");
    std::fs::create_dir_all(&publish_root).expect("mkdir publish root");
    // Drop a synthetic INDEX.json so the
    // pre-upload gate does not fire BEFORE
    // the missing-bucket-arg check.
    let index_dir = publish_root.join("index");
    std::fs::create_dir_all(&index_dir).expect("mkdir index");
    let stub_index = serde_json::json!({
        "publish_root": publish_root.display().to_string(),
        "runbook_version": "STW-034 v1",
        "created_at_utc": "<unknown>",
        "entry_count": 0,
        "total_bytes": 0,
        "entries": []
    });
    std::fs::write(
        index_dir.join("INDEX.json"),
        serde_json::to_string_pretty(&stub_index).expect("serialise stub INDEX.json"),
    )
    .expect("write stub INDEX.json");
    let out = trainer_bin()
        .arg("--publish-index-remote")
        .arg(&publish_root)
        .output()
        .expect("spawn trainer --publish-index-remote <publish-root> (no --bucket)");
    let code = out.status.code().unwrap_or(-1);
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(
        code,
        2,
        "trainer --publish-index-remote <publish-root> with no --bucket must exit 2. \
         --- stdout ---\n{}\n--- stderr ---\n{stderr}",
        String::from_utf8_lossy(&out.stdout)
    );
    assert!(
        stderr.contains("--bucket"),
        "trainer --publish-index-remote stderr must carry the `--bucket` usage line; \
         got: {stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&parent);
}
