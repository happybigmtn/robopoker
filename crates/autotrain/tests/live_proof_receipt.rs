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
    LiveProofHeadline, LiveProofReceipt, LiveProofRecipe, STW023_CHAIN_STEPS,
    STW023_HEADLINE_PREFIX,
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

/// The `LiveProofHeadline` typed parser is the
/// dashboard-side surface the `STW-023` verifier ships
/// alongside the substring gate. A runbook receipt's
/// `SUMMARY.txt` headline is parsed into the five
/// `u64` fields (`smoke` / `status` / `bench` /
/// `compare` / `replay`) so a testnet dashboard can
/// chart the per-run artifact counts without re-running
/// a regex on every scrape. This test drops a synthetic
/// green receipt (whose `SUMMARY.txt` headline was
/// stamped by `LiveProofReceipt::write_to`), parses the
/// first line via `LiveProofHeadline::parse`, and asserts
/// the five fields match the values the `write_to`
/// constructor passed in. A regression in the producer
/// (the `write_to` `format!` line) or the consumer (the
/// `parse` tokeniser) fails the same test.
#[test]
fn synthetic_receipt_headline_parses_via_typed_surface() {
    let dir = fresh_dir("typed-headline");
    // Drop a receipt with the five known counts the
    // dashboard will chart: `smoke=12`, `status=12`,
    // `bench=4`, `compare=4`, `replay=256`. (The
    // `write_to` constructor calls
    // `LiveProofReceipt::headline` with these values
    // internally, so the headline is pinned by the
    // `drop_synthetic_green_receipt` helper.)
    drop_synthetic_green_receipt(&dir);
    let summary_text = std::fs::read_to_string(dir.join("SUMMARY.txt"))
        .expect("SUMMARY.txt must exist on a green receipt");
    let first_line = summary_text
        .lines()
        .next()
        .expect("SUMMARY.txt must have a first line");
    let parsed = LiveProofHeadline::parse(first_line).unwrap_or_else(|e| {
        panic!(
            "parse must accept the headline produced by `LiveProofReceipt::write_to`; got: {e:?}"
        )
    });
    assert_eq!(
        parsed.smoke, 12,
        "smoke field must match the write_to input"
    );
    assert_eq!(
        parsed.status, 12,
        "status field must match the write_to input"
    );
    assert_eq!(parsed.bench, 4, "bench field must match the write_to input");
    assert_eq!(
        parsed.compare, 4,
        "compare field must match the write_to input"
    );
    assert_eq!(
        parsed.replay, 256,
        "replay field must match the write_to input"
    );
    // The re-emitted line must also parse back to the
    // same `LiveProofHeadline` (the round-trip property
    // the lib test
    // `live_proof_headline_round_trips_through_parse`
    // pins).
    let reemitted = parsed.to_line();
    let reparsed = LiveProofHeadline::parse(&reemitted)
        .unwrap_or_else(|e| panic!("re-emitted line must parse; got: {e:?}"));
    assert_eq!(
        reparsed, parsed,
        "to_line + parse round-trip must preserve all five fields"
    );
    let _ = std::fs::remove_dir_all(&dir);
}
