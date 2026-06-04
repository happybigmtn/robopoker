//! `trainer --publish-index <publish-root>` +
//! `trainer --verify-index <index-path>` end-to-end CLI
//! integration test (STW-034).
//!
//! STW-032 shipped `trainer --publish <receipt>` +
//! `trainer --verify-bundle <bundle-dir>`, the
//! deterministic content-addressed portable publish
//! bundle a third party (a testnet dashboard bucket, a
//! CI auditor, a release-gate script) can fetch +
//! re-verify without re-running the chain. STW-033
//! shipped the S3 / GCS / git-tag upload half the
//! publish runbook doc names as the next slice
//! (`testnet-live-publish-s3`) — a deterministic
//! upload plan + a `remote_receipt.json` manifest
//! a dashboard can re-fetch + re-verify without
//! re-running the chain. **STW-034 lands the
//! *aggregator* a testnet dashboard naturally
//! wants** (the next slice the STW-033 publish-remote
//! step defers to): a single `INDEX.json` +
//! `SUMMARY.txt` pair a dashboard can scrape
//! instead of listing the bucket + fetching N
//! manifests.
//!
//! The four sub-tests assert the on-the-wire
//! contract the new CLI arms
//! (`trainer --publish-index` +
//! `trainer --verify-index`) expose. They are
//! `cargo test --workspace` green without a live
//! Postgres (the producer is
//! `LiveProofReceipt::write_to`, a pure on-disk
//! writer; the publisher + publish-remote + indexer
//! + verifier are all no-DB no-network), so a
//! regression in the CLI surface (renamed flag,
//! wrong exit code, missing-path-arg convention,
//! missing headline token) fails CI before it
//! reaches an operator's machine.
//!
//! ## Sub-tests
//!
//! 1. `publish_index_round_trips_through_real_trainer_binary`
//!    — drop a synthetic green receipt, drive the
//!    full STW-032 → STW-033 → STW-034 chain
//!    end-to-end (publish the bundle, publish-remote
//!    to a stub `s3://...` bucket, then publish-index
//!    the resulting publish root), assert exit 0,
//!    assert stdout starts with the pinned
//!    `live_proof publish_index complete: ` prefix
//!    and names every field a dashboard scraper
//!    reads (`root=` + `entries=` + `total_bytes=` +
//!    `index=`). Then drive `trainer --verify-index
//!    <index-path>` end-to-end, assert exit 0 +
//!    the pinned `live_proof index verification
//!    passed: ` prefix. The round-trip proves a
//!    real subprocess `trainer --publish-index`
//!    writes an `INDEX.json` a real subprocess
//!    `trainer --verify-index` can re-verify — the
//!    on-the-wire contract the testnet dashboard's
//!    `aws s3 cp` + `trainer --verify-index`
//!    round-trip depends on.
//! 2. `publish_index_run_exits_two_with_red_remote_receipt`
//!    — drive the STW-032 → STW-033 chain to
//!    materialise a green publish root, tamper
//!    with the `remote_receipt.json`'s first
//!    `s3_objects[].sha256` to a bogus value, drive
//!    `trainer --publish-index <root>`, assert
//!    exit 2, assert stderr carries the pinned
//!    `live_proof publish_index error: remote
//!    receipt is red: ` prefix. The on-the-wire
//!    contract is: a `trainer --publish-index`
//!    over a red `remote_receipt.json` is a hard
//!    error, not a warning. The per-entry
//!    pre-index gate is the "refuse to paper-over
//!    a red remote receipt" invariant the
//!    STW-028 receipt verifier + STW-032 bundle
//!    verifier + STW-033 remote-receipt verifier
//!    already enforce.
//! 3. `publish_index_run_exits_two_with_missing_publish_root`
//!    — drive `trainer --publish-index` against a
//!    nonexistent publish root, assert exit 2 +
//!    stderr carries the pinned
//!    `live_proof publish_index error: publish
//!    root: ` prefix. The on-the-wire contract
//!    is: a missing publish root is exit 2 + a
//!    pinned error line, not a silent skip.
//! 4. `verify_index_round_trips_through_real_trainer_binary`
//!    — drive the STW-032 → STW-033 → STW-034 chain
//!    end-to-end to materialise a fresh
//!    `INDEX.json`, then drive a real subprocess
//!    `trainer --verify-index <index-path>` and
//!    assert exit 0 + the pinned
//!    `live_proof index verification passed: `
//!    prefix. The standalone test exercises the
//!    verifier arm end-to-end through a real
//!    subprocess so a regression in the verifier
//!    CLI surface (renamed flag, wrong exit code,
//!    missing headline token) fails CI before it
//!    reaches an operator's machine.
//!
//! The test deliberately does **not** require a
//! live `DATABASE_URL` or a live S3 bucket. The
//! fixture receipt is dropped with
//! `LiveProofReceipt::write_to` (a pure on-disk
//! writer), the publisher is invoked as a
//! subprocess with `--no-dry-run` left off (the
//! default is dry-run), the publish-remote arm
//! defaults to dry-run, and the indexer + verifier
//! are no-network by design (the aggregator is
//! always read-only with respect to the publish
//! root + the underlying `remote_receipt.json`
//! files). The `trainer_bin_path` helper walks up
//! from `CARGO_MANIFEST_DIR` to the workspace root,
//! the same way
//! `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish,publish_remote}.rs`
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
/// `crates/autotrain/tests/{smoke,bench,compare,live_proof,script_shape,verify_receipt,publish,publish_remote}.rs`
/// so the ten integration tests share the same binary
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
/// from the parent (the test process) so the
/// `trainer --publish-index` arm reads a real
/// ISO-8601 stamp and the aggregator no longer
/// falls back to the literal `<unknown>`
/// sentinel. The pre-STW-051 helper did not
/// set the env knob because the aggregator
/// accepted an `Option<&str>` + wrote the
/// `<unknown>` sentinel; the new shape fails
/// fast with `PublishIndexError::MissingArg` on
/// a missing arg, so the helper is now the
/// shared fixture for the 4 integration
/// sub-tests the `--publish-index` arm drives.
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
/// `publish_root`) and the STW-034 aggregator
/// scans *only* the current test's `publish/`
/// subtree, never stale files from a previous
/// test run.
static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fresh_test_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbp-publish-index-integ-{label}-{}-{}",
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
/// step (and the STW-034 publish-index step) both
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
/// The integration-test mirror of the
/// `crate::publish_index::publish_index` lib entry
/// point so a regression in the CLI surface
/// (renamed flag, wrong exit code, missing-path-arg
/// convention) fails here.
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
/// `trainer --verify-index <index-path>` and return
/// the `(stdout, stderr, exit_code)` triple. The
/// integration-test mirror of the
/// `crate::publish_index::PublishIndex::verify`
/// lib verifier.
fn run_verify_index(index_path: &PathBuf) -> (String, String, i32) {
    let out = trainer_bin()
        .arg("--verify-index")
        .arg(index_path)
        .output()
        .expect("spawn trainer --verify-index <index-path>");
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
/// The `bundle=` token points at the `bundle.tar.gz`
/// file (not its parent dir); the test derives the
/// bundle directory as the file's parent. Mirrors
/// the `extract_bundle_dir` helper in
/// `crates/autotrain/tests/publish_remote.rs`.
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
fn publish_index_round_trips_through_real_trainer_binary() {
    // Drop a green receipt the dashboard will chart:
    // smoke=12, status=12, bench=4, compare=4,
    // replay=256. The full STW-019 → STW-032 →
    // STW-033 → STW-034 chain (publish the bundle
    // + publish-remote the bundle + publish-index
    // the resulting publish root) must land
    // end-to-end on a fixture green receipt, and
    // the resulting `INDEX.json` must round-trip
    // through a real subprocess
    // `trainer --verify-index <index-path>`.
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
    // The publish root is the per-test parent
    // dir the trainer binary's `--publish` step
    // wrote the bundle under (a per-test
    // `publish_root` so the STW-034 aggregator
    // scans only the current test's
    // `publish/*/remote/` subtree).
    let publish_root = parent.clone();
    // 2. STW-033: publish-remote the bundle (the
    // STW-034 indexer consumes the
    // `remote_receipt.json` each STW-033
    // invocation wrote under
    // `<bundle>/remote/`).
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
    // 3. STW-034: publish-index the publish root.
    let (pi_stdout, pi_stderr, pi_code) = run_publish_index(&publish_root);
    assert_eq!(
        pi_code, 0,
        "trainer --publish-index <green-root> must exit 0 on a fixture green publish root. \
         --- stdout ---\n{pi_stdout}\n--- stderr ---\n{pi_stderr}"
    );
    assert!(
        pi_stdout.starts_with("live_proof publish_index complete: "),
        "publish-index stdout must start with the pinned success prefix; got: {pi_stdout:?}"
    );
    // The headline must carry every field the
    // dashboard scraper reads: `root=` +
    // `entries=` + `total_bytes=` + `index=`.
    for token in ["root=", "entries=", "total_bytes=", "index="] {
        assert!(
            pi_stdout.contains(token),
            "publish-index stdout must carry the {token} token; got: {pi_stdout:?}"
        );
    }
    // The on-disk `INDEX.json` must exist at
    // `<publish_root>/index/INDEX.json`. A
    // regression that drops the file (or writes
    // it under a different name) fails the
    // verify step below.
    let index_dir = publish_root.join("index");
    assert!(
        index_dir.is_dir(),
        "publish-index must write an `index/` dir under {}",
        publish_root.display()
    );
    assert!(
        index_dir.join("INDEX.json").is_file(),
        "publish-index must write an `INDEX.json` under {}",
        index_dir.display()
    );
    assert!(
        index_dir.join("SUMMARY.txt").is_file(),
        "publish-index must write a `SUMMARY.txt` under {}",
        index_dir.display()
    );
    // 4. Re-verify the index through a real
    // subprocess. The round-trip proves the
    // `trainer --publish-index` +
    // `trainer --verify-index` pair the
    // testnet dashboard depends on is
    // end-to-end green.
    let (vi_stdout, vi_stderr, vi_code) = run_verify_index(&index_dir);
    assert_eq!(
        vi_code, 0,
        "trainer --verify-index <index-path> must exit 0 on a fresh dry-run index. \
         --- stdout ---\n{vi_stdout}\n--- stderr ---\n{vi_stderr}"
    );
    assert!(
        vi_stdout.starts_with("live_proof index verification passed: "),
        "verify-index stdout must start with the pinned success prefix; got: {vi_stdout:?}"
    );
    // The verifier headline must carry the
    // dashboard-scraper tokens.
    for token in ["index=", "entries=", "total_bytes=", "runbook_version="] {
        assert!(
            vi_stdout.contains(token),
            "verify-index stdout must carry the {token} token; got: {vi_stdout:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn publish_index_run_exits_two_with_red_remote_receipt() {
    // The publish-index step's per-entry pre-index
    // gate is "refuse to index a red
    // `remote_receipt.json`". Drive the
    // STW-032 → STW-033 chain to materialise a
    // green publish root, tamper with the
    // `remote_receipt.json`'s first
    // `s3_objects[].sha256` to a bogus value, drive
    // `trainer --publish-index <root>`, assert
    // exit 2 + stderr carries the pinned
    // `live_proof publish_index error: remote
    // receipt is red: ` prefix. The on-the-wire
    // contract is: a `trainer --publish-index` over
    // a red `remote_receipt.json` is a hard error,
    // not a warning.
    let parent = fresh_test_dir("red-remote");
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
    let (publish_stdout, publish_stderr, publish_code) = run_publish(&receipt);
    assert_eq!(
        publish_code, 0,
        "trainer --publish <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{publish_stdout}\n--- stderr ---\n{publish_stderr}"
    );
    let _bundle_dir = extract_bundle_dir(&publish_stdout);
    let (pr_stdout, pr_stderr, pr_code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof-20260604T050000Z/",
    );
    assert_eq!(
        pr_code, 0,
        "trainer --publish-remote <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{pr_stdout}\n--- stderr ---\n{pr_stderr}"
    );
    let publish_root = parent.clone();
    // Tamper with the `remote_receipt.json`'s
    // first `s3_objects[].sha256` so the STW-033
    // `PublishedRemoteReceipt::verify` rejects it
    // (the re-hash of the local file no longer
    // matches the tampered entry's `sha256`).
    let remote_receipt_path = _bundle_dir.join("remote").join("remote_receipt.json");
    let tampered_str =
        std::fs::read_to_string(&remote_receipt_path).expect("read remote_receipt.json");
    let mut value: serde_json::Value =
        serde_json::from_str(&tampered_str).expect("parse remote_receipt.json");
    if let Some(arr) = value.get_mut("s3_objects").and_then(|o| o.as_array_mut()) {
        if let Some(first) = arr.first_mut() {
            first["sha256"] = serde_json::Value::String("deadbeef".repeat(8));
        }
    }
    let tampered =
        serde_json::to_string_pretty(&value).expect("serialise tampered remote_receipt.json");
    std::fs::write(&remote_receipt_path, tampered)
        .expect("rewrite remote_receipt.json with bogus s3_objects[0].sha256");
    // Drive `trainer --publish-index <root>` and
    // assert exit 2 + the pinned error prefix.
    let (pi_stdout, pi_stderr, pi_code) = run_publish_index(&publish_root);
    assert_eq!(
        pi_code, 2,
        "trainer --publish-index <red-remote-receipt-root> must exit 2 (the \
         data-quality-problem convention). --- stdout---\n{pi_stdout}\n--- stderr---\n{pi_stderr}"
    );
    assert!(
        pi_stderr.starts_with("live_proof publish_index error: remote receipt is red: "),
        "red-remote stderr must start with the pinned `remote receipt is red: ` prefix; \
         got: {pi_stderr:?}"
    );
    // The publish-index step must NOT have
    // written a `index/` dir for a red publish
    // root (the per-entry pre-index gate fires
    // before the `INDEX.json` is written).
    let index_dir = publish_root.join("index");
    assert!(
        !index_dir.exists(),
        "publish-index must NOT write an `index/` dir for a red publish root; \
         got: {}",
        index_dir.display()
    );
    let _ = std::fs::remove_dir_all(&parent);
}

#[test]
fn publish_index_run_exits_two_with_missing_publish_root() {
    // The `trainer --publish-index <root>` arm
    // refuses to run on a missing publish root.
    // The on-the-wire contract is: a missing
    // publish root is exit 2 + a pinned error
    // line, not a silent skip. The error prefix
    // mirrors the `live_proof publish_index
    // error: publish root: ...` shape the
    // STW-033 `PublishRemoteError` Display impl
    // already produces for the `BundleDir` arm
    // (STW-033 maps `BundleDir(s)` to
    // `PublishIndexError::FileUnreadable`).
    let bogus = fresh_test_dir("missing-publish-root");
    // The trainer's `publish_index` step gates
    // on the publish root existing; remove the
    // dir so the gate fires (we do not need the
    // dir to exist on disk — a missing
    // `publish/` subdir is a different error
    // path; a missing publish root is the
    // on-the-wire contract this test pins).
    let _ = std::fs::remove_dir_all(&bogus);
    let (pi_stdout, pi_stderr, pi_code) = run_publish_index(&bogus);
    assert_eq!(
        pi_code, 2,
        "trainer --publish-index <missing-root> must exit 2 (the data-quality-problem \
         convention). --- stdout---\n{pi_stdout}\n--- stderr---\n{pi_stderr}"
    );
    // The stderr must carry the pinned
    // `live_proof publish_index error: publish
    // root: ` prefix.
    assert!(
        pi_stderr.starts_with("live_proof publish_index error: publish root: "),
        "missing-publish-root stderr must start with the pinned `publish root: ` prefix; \
         got: {pi_stderr:?}"
    );
    let _ = std::fs::remove_dir_all(&bogus);
}

#[test]
fn verify_index_round_trips_through_real_trainer_binary() {
    // Drive the STW-032 → STW-033 → STW-034 chain
    // end-to-end to materialise a fresh
    // `INDEX.json`, then drive a real subprocess
    // `trainer --verify-index <index-path>` and
    // assert exit 0 + the pinned
    // `live_proof index verification passed: `
    // prefix. The standalone test exercises the
    // verifier arm end-to-end through a real
    // subprocess so a regression in the verifier
    // CLI surface (renamed flag, wrong exit code,
    // missing headline token) fails CI before it
    // reaches an operator's machine. Mirrors the
    // `verify_bundle_round_trips_through_real_trainer_binary`
    // test in `crates/autotrain/tests/publish.rs`
    // (the STW-032 verifier arm's CLI pin).
    let parent = fresh_test_dir("verify-index");
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
    let (publish_stdout, publish_stderr, publish_code) = run_publish(&receipt);
    assert_eq!(
        publish_code, 0,
        "trainer --publish <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{publish_stdout}\n--- stderr ---\n{publish_stderr}"
    );
    let _bundle_dir = extract_bundle_dir(&publish_stdout);
    let (pr_stdout, pr_stderr, pr_code) = run_publish_remote(
        &receipt,
        "s3://robopoker-testnet-dashboard",
        "testnet-live-proof-20260604T050000Z/",
    );
    assert_eq!(
        pr_code, 0,
        "trainer --publish-remote <green-receipt> must exit 0 on a fixture green receipt. \
         --- stdout ---\n{pr_stdout}\n--- stderr ---\n{pr_stderr}"
    );
    let publish_root = parent.clone();
    let (pi_stdout, pi_stderr, pi_code) = run_publish_index(&publish_root);
    assert_eq!(
        pi_code, 0,
        "trainer --publish-index <green-root> must exit 0 on a fixture green publish root. \
         --- stdout ---\n{pi_stdout}\n--- stderr ---\n{pi_stderr}"
    );
    let index_dir = publish_root.join("index");
    // Re-verify the index through a real
    // subprocess. The standalone round-trip
    // proves the `trainer --verify-index` arm
    // can re-verify a fresh `INDEX.json` without
    // a DB connection.
    let (vi_stdout, vi_stderr, vi_code) = run_verify_index(&index_dir);
    assert_eq!(
        vi_code, 0,
        "trainer --verify-index <index-path> must exit 0 on a fresh dry-run index. \
         --- stdout ---\n{vi_stdout}\n--- stderr ---\n{vi_stderr}"
    );
    assert!(
        vi_stdout.starts_with("live_proof index verification passed: "),
        "verify-index stdout must start with the pinned success prefix; got: {vi_stdout:?}"
    );
    for token in ["index=", "entries=", "total_bytes=", "runbook_version="] {
        assert!(
            vi_stdout.contains(token),
            "verify-index stdout must carry the {token} token; got: {vi_stdout:?}"
        );
    }
    let _ = std::fs::remove_dir_all(&parent);
}
