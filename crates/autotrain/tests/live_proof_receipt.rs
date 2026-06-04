//! `LiveProofReceipt` no-DB integration test (STW-023).
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh` (the
//! operator runbook) and `crates/autotrain/tests/live_proof.rs`
//! (the cargo-test counterpart) independently, so a future
//! drift in the on-disk receipt shape (renamed step, dropped
//! `exit.txt`, broken headline prefix) fails one surface
//! without a clear "the other is also stale" signal.
//!
//! STW-023 lands a single shared `LiveProofReceipt` verifier
//! in `crates/autotrain/src/receipt.rs` that the runbook
//! + the integration test both call into, plus a
//! `recipe.json` machine-readable manifest the runbook
//! writes alongside `SUMMARY.txt`.
//!
//! This integration test pins the *consumer* side of that
//! contract without requiring a live Postgres. It runs in
//! `cargo test --workspace` (no `database` feature gate) so
//! a regression in the verifier or the on-disk shape
//! fails CI before a worker can ship a planning edit that
//! silently breaks the testnet live launch proof.
//!
//! The five sub-tests assert:
//!
//! 1. `synthetic_receipt_verifies_green_via_lib` — drop a
//!    synthetic receipt, call `LiveProofReceipt::verify`,
//!    assert the verifier returns `Ok(())` (the happy
//!    path the runbook and the integration test take).
//! 2. `synthetic_receipt_manifest_recipes_step_names` —
//!    the on-disk `recipe.json` parses into
//!    `LiveProofRecipe` and the `steps[i].name` field
//!    matches the `STW023_CHAIN_STEPS` order. A
//!    regression that renames a step (e.g. swaps `bench`
//!    for `hand`) or re-orders the manifest fails the
//!    test.
//! 3. `synthetic_receipt_verifier_rejects_renamed_step`
//!    — a receipt whose `recipe.json` declares a step
//!    name not in `STW023_CHAIN_STEPS` verifies with
//!    `Err(VerifyError::RecipeShape(..))`. A regression
//!    that swallows the step-name mismatch fails the
//!    test.
//! 4. `synthetic_receipt_verifier_rejects_missing_exit_code`
//!    — a receipt whose one step directory is missing
//!    the `exit.txt` file verifies with
//!    `Err(VerifyError::RecipeShape(..))` (the read
//!    path returns `io::Error` which we wrap as
//!    `RecipeShape` so the verifier's three-error
//!    surface stays clean). A regression that treats a
//!    missing `exit.txt` as `exit == 0` (i.e. green)
//!    fails the test.
//! 5. `synthetic_receipt_headline_uses_pinned_prefix` —
//!    the verifier rejects a `SUMMARY.txt` whose
//!    headline does not start with the pinned
//!    `testnet live_proof complete:` prefix. A
//!    regression that drops the prefix fails the test.

use rbp_autotrain::{
    LiveProofReceipt, LiveProofRecipe, STW023_CHAIN_STEPS, STW023_HEADLINE_PREFIX,
};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Per-process unique temp dir for the fixture. We
/// `remove_dir_all` on drop so re-runs do not see stale
/// state. The `SEQ` counter disambiguates parallel
/// `cargo test` invocations.
static SEQ: AtomicUsize = AtomicUsize::new(0);

fn fresh_dir(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rbp-live-proof-receipt-{label}-{}-{}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// Drop a synthetic green receipt at `dest` with
/// `STW023_CHAIN_STEPS.len()` per-step directories, each
/// containing `exit.txt=0`. The function is the
/// integration-test mirror of
/// `LiveProofReceipt::write_to` so a regression in the
/// public `write_to` shape fails both surfaces in the
/// same CI run.
fn drop_synthetic_green_receipt(dest: &PathBuf) {
    LiveProofReceipt::write_to(
        dest,
        12,  // smoke_rows
        12,  // status_blueprint
        4,   // bench_hands
        4,   // compare_hands
        256, // replay_bytes
        "/srv/dev/repos/robopoker/target/debug/trainer",
        "<redacted: 49 chars>",
    )
    .expect("write_to should drop a synthetic green receipt");
}

/// Drop a synthetic receipt whose per-step directory
/// name does not match `STW023_CHAIN_STEPS` (so the
/// verifier can detect the rename regression). The
/// `read_from` path reads the per-step directory
/// names — the recipe.json is the audit trail, not the
/// source of truth.
fn drop_renamed_step_receipt(dest: &PathBuf) {
    drop_synthetic_green_receipt(dest);
    // Rename the `smoke` directory to a typo. A
    // regression that swallows the step-name
    // mismatch (e.g. the verifier only checks the
    // recipe.json's step names but ignores the
    // directory layout, or vice versa) fails here.
    let from = dest.join("smoke");
    let to = dest.join("smkoe");
    std::fs::rename(&from, &to).expect("rename smoke/ to smkoe/");
}

/// Drop a synthetic receipt whose one step directory is
/// missing the `exit.txt` file. A regression that
/// treats a missing `exit.txt` as `exit == 0` (green)
/// surfaces here.
fn drop_missing_exit_code_receipt(dest: &PathBuf) {
    drop_synthetic_green_receipt(dest);
    let bench_dir = dest.join("bench");
    std::fs::remove_file(bench_dir.join("exit.txt"))
        .expect("remove bench/exit.txt to simulate the missing-file regression");
}

#[test]
fn synthetic_receipt_verifies_green_via_lib() {
    let dir = fresh_dir("green");
    drop_synthetic_green_receipt(&dir);
    LiveProofReceipt::read_and_verify(&dir)
        .unwrap_or_else(|e| panic!("green receipt must verify; got: {e:?}"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn synthetic_receipt_manifest_recipes_step_names() {
    let dir = fresh_dir("manifest");
    drop_synthetic_green_receipt(&dir);
    let raw = std::fs::read_to_string(dir.join("recipe.json"))
        .expect("recipe.json must exist on a green receipt");
    let parsed: LiveProofRecipe = serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("recipe.json must round-trip through serde_json: {e}"));
    let names: Vec<&str> = parsed.steps.iter().map(|s| s.name.as_str()).collect();
    assert_eq!(
        names, STW023_CHAIN_STEPS,
        "STW-023 recipe.json `steps[i].name` must match the pinned chain order"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn synthetic_receipt_verifier_rejects_renamed_step() {
    let dir = fresh_dir("renamed");
    drop_renamed_step_receipt(&dir);
    let err =
        LiveProofReceipt::read_and_verify(&dir).expect_err("a renamed step must fail verification");
    match err {
        rbp_autotrain::VerifyError::RecipeShape(msg) => {
            assert!(
                msg.contains("smkoe") || msg.contains("smoke"),
                "RecipeShape error must name the offending step (got: {msg:?})"
            );
        }
        other => panic!("renamed step must produce RecipeShape error; got: {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn synthetic_receipt_verifier_rejects_missing_exit_code() {
    let dir = fresh_dir("missing-exit");
    drop_missing_exit_code_receipt(&dir);
    let err = LiveProofReceipt::read_and_verify(&dir)
        .expect_err("a missing exit.txt must fail verification");
    // The read path returns `io::Error(io::ErrorKind::NotFound)`;
    // the verifier wraps that as `RecipeShape` so the
    // verifier's three-error surface stays clean. A
    // regression that treats the read failure as
    // `Ok(())` fails the test.
    match err {
        rbp_autotrain::VerifyError::RecipeShape(msg) => {
            assert!(
                msg.contains("exit.txt") || msg.contains("bench"),
                "RecipeShape error must name the missing exit.txt (got: {msg:?})"
            );
        }
        other => panic!("missing exit.txt must produce RecipeShape error; got: {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn synthetic_receipt_headline_uses_pinned_prefix() {
    // A receipt whose SUMMARY.txt headline does not
    // start with the pinned `testnet live_proof complete:`
    // prefix must fail verification. The check guards
    // the dashboard scraper's `grep` contract.
    let dir = fresh_dir("headline");
    drop_synthetic_green_receipt(&dir);
    // Rewrite the SUMMARY.txt headline so it lacks the
    // pinned prefix.
    let bad_summary = format!(
        "live_proof complete: smoke=0 status=0 bench=0 compare=0 replay=0\n\n  receipt_dir: {}\n",
        dir.display()
    );
    std::fs::write(dir.join("SUMMARY.txt"), bad_summary)
        .expect("rewrite SUMMARY.txt with the wrong prefix");
    let err = LiveProofReceipt::read_and_verify(&dir)
        .expect_err("a non-pinned headline must fail verification");
    match err {
        rbp_autotrain::VerifyError::Headline(msg) => {
            assert!(
                msg.contains(STW023_HEADLINE_PREFIX),
                "Headline error must reference the pinned prefix (got: {msg:?})"
            );
        }
        other => panic!("wrong-prefix headline must produce Headline error; got: {other:?}"),
    }
    let _ = std::fs::remove_dir_all(&dir);
}
