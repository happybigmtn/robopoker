//! `trainer --verify-receipt <path>` — re-verify a testnet live
//! launch proof receipt bundle on disk (the directory the
//! `scripts/testnet-live-proof.sh` runbook writes or
//! `LiveProofReceipt::write_to` synthesises).
//!
//! STW-019 shipped the bash runbook; STW-023 shipped the
//! `LiveProofReceipt::read_and_verify` Rust verifier the
//! integration test calls. STW-028 wires the verifier into
//! the `trainer` binary so a downstream tool (a testnet
//! dashboard's "verify" button, a CI check, a release-gate
//! script) can re-verify a receipt an operator dropped
//! *without* re-running `cargo test` or re-installing the
//! workspace's library crates — a single static `trainer`
//! binary + one `cat receipts/.../SUMMARY.txt` is enough.
//!
//! The whole module is a thin wrapper over the STW-023
//! `LiveProofReceipt::read_and_verify` surface. The wrapper
//! converts the three-variant `VerifyError` (recipe shape /
//! step exit / headline) into a single one-line
//! `live_proof receipt verification failed: <kind> <detail>`
//! string the CLI can `eprintln!` so a dashboard scraper
//! can `grep ^live_proof receipt verification failed:` the
//! log. The happy-path print is a one-line
//! `live_proof receipt verification passed: <headline>`
//! line a dashboard can `grep ^live_proof receipt
//! verification passed:` the log. The contract mirrors
//! `crate::replay::run` (the STW-016 `Mode::Replay` arm's
//! handler): `Result<String, String>` where `Ok(s)` is a
//! printable success line and `Err(s)` is a printable
//! failure line; the caller (`Mode::run`) does the
//! `eprintln!` + exit-code mapping.
//!
//! ## Why a new module and not a function on `receipt`
//!
//! `receipt.rs` is the *library* surface a `cargo test`
//! integration test uses; `verify_receipt.rs` is the *CLI*
//! surface a `bash` script or a testnet dashboard uses.
//! Splitting them keeps the verifier's error type
//! (`VerifyError`, a three-variant typed enum) and the
//! CLI's printable error string (`live_proof receipt
//! verification failed: <kind> <detail>`) on different
//! code paths: a regression in the printed line does not
//! change the typed error a Rust caller `match`es on, and
//! vice versa. The `Mode::VerifyReceipt` arm uses only
//! the CLI side.
//!
//! ## Why the handler is sync, not async
//!
//! The whole pipeline is `fs::read_to_string` +
//! `serde_json::from_str` + an in-memory verifier walk.
//! There is no I/O latency worth awaiting, and the
//! upstream `Mode::run` is `async` only because every
//! *other* variant opens a DB. A sync `run` here means
//! the new no-DB early-dispatch arm in `Mode::run` is a
//! plain `match` with no `.await` call.
use std::path::Path;

use crate::LiveProofReceipt;
use crate::VerifyError;

/// Re-verify a testnet live launch proof receipt bundle
/// on disk. Thin wrapper over
/// [`crate::LiveProofReceipt::read_and_verify`] so the
/// autotrain `Mode::VerifyReceipt` arm has a single
/// `Result<String, String>` shape to print-and-exit on.
///
/// On success the returned `String` is a one-line
/// `live_proof receipt verification passed: <headline>`
/// line a dashboard scraper can `grep`. The `<headline>`
/// is the verbatim first line of the receipt's
/// `SUMMARY.txt` (the `testnet live_proof complete:
/// smoke=... status=... bench=... compare=... replay=...`
/// line the runbook writes). The handler does NOT add a
/// trailing newline — the caller (`Mode::run`) does
/// `print!` (not `println!`) and exits 0.
///
/// On error, the returned `String` is a one-line
/// `live_proof receipt verification failed: <kind>:
/// <detail>` diagnostic. The `<kind>` is one of
/// `recipe_shape` / `step_failed` / `headline` (the
/// three `VerifyError` variants in their
/// `snake_case` form), and `<detail>` is the variant's
/// payload (a `String` for the recipe shape + headline
/// variants, a `step: String, exit: i32` struct for the
/// step-failed variant — both rendered via
/// `verify_error_kind_and_detail` below). The caller
/// prints to stderr and exits 2.
pub fn run(path: &Path) -> Result<String, String> {
    match LiveProofReceipt::read_and_verify(path) {
        Ok(()) => {
            // Re-read the SUMMARY.txt headline so the
            // success line carries the dashboard-readable
            // counts. We re-read (not pass-through from
            // the verifier) because the verifier is
            // deliberately shape-only — a future
            // `VerifyError` variant that accepts a
            // non-pinned headline must NOT change the
            // success-line format. A read failure here is
            // unlikely (the verifier just walked the same
            // file) but we surface it as a `recipe_shape`
            // error so the CLI exit code stays consistent.
            let summary = std::fs::read_to_string(path.join("SUMMARY.txt")).map_err(|e| {
                format!(
                    "live_proof receipt verification failed: recipe_shape: \
                         SUMMARY.txt unreadable at {}: {e}",
                    path.join("SUMMARY.txt").display()
                )
            })?;
            let first_line = summary.lines().next().unwrap_or("").to_string();
            Ok(format!(
                "live_proof receipt verification passed: {first_line}"
            ))
        }
        Err(e) => {
            let (kind, detail) = verify_error_kind_and_detail(&e);
            Err(format!(
                "live_proof receipt verification failed: {kind}: {detail}"
            ))
        }
    }
}

/// Map a `VerifyError` to a `(kind, detail)` pair the
/// `Mode::VerifyReceipt` arm prints. The `kind` is the
/// snake_case name of the variant (so a future
/// regression that adds a new `VerifyError` variant
/// surfaces as a `kind=unknown` line and the CI log
/// flags it). The `detail` is the variant's payload
/// rendered via the `VerifyError` `Display` impl
/// (which itself prefixes every message with
/// `live_proof ... error:` so the printed line is
/// grep-friendly).
fn verify_error_kind_and_detail(e: &VerifyError) -> (&'static str, String) {
    match e {
        VerifyError::RecipeShape(_) => ("recipe_shape", e.to_string()),
        VerifyError::StepFailed { step, exit } => (
            "step_failed",
            format!("live_proof step `{step}` failed (exit {exit})"),
        ),
        VerifyError::Headline(_) => ("headline", e.to_string()),
    }
}

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-028
    //! `verify_receipt` consumer. These tests do NOT
    //! require a live Postgres (the verifier is the
    //! *consumer* side of the on-disk contract; the
    //! producer side is the bash runbook's `write_recipe`
    //! heredoc or `LiveProofReceipt::write_to`).
    //!
    //! Fixture style: a process-unique
    //! `std::env::temp_dir().join("rbp-verify-test-<n>")`
    //! subdirectory populated by `LiveProofReceipt::write_to`,
    //! then re-read + verified. The tempdir is removed on
    //! drop so re-runs do not see stale files.
    //!
    //! The four sub-tests assert:
    //!
    //! 1. `run_returns_passed_line_on_green_receipt` —
    //!    a fixture receipt verifies green; the handler
    //!    returns `Ok(s)` whose `s` starts with the
    //!    pinned `live_proof receipt verification
    //!    passed:` prefix and includes the SUMMARY.txt
    //!    headline verbatim. The `Mode::VerifyReceipt`
    //!    arm's success path uses this prefix as the
    //!    dashboard scraper grep target.
    //! 2. `run_returns_recipe_shape_error_on_missing_dir`
    //!    — a path that does not exist returns `Err(s)`
    //!    whose `s` starts with `live_proof receipt
    //!    verification failed: recipe_shape:` and
    //!    includes `does not exist` (the message the
    //!    `LiveProofRecipe::from_receipt_dir` read path
    //!    produces). The `Mode::VerifyReceipt` arm
    //!    prints the `Err(s)` to stderr and exits 2.
    //! 3. `run_returns_step_failed_error_on_failed_step`
    //!    — a receipt whose one step's `exit.txt` is
    //!    `1` (a non-zero exit) returns `Err(s)` whose
    //!    `s` starts with `live_proof receipt
    //!    verification failed: step_failed:` and names
    //!    the failing step. A regression that swallows
    //!    the step-exit mismatch (treats `exit=1` as
    //!    green) fails this test.
    //! 4. `run_returns_headline_error_on_bad_headline`
    //!    — a receipt whose `SUMMARY.txt` headline
    //!    lacks the pinned `testnet live_proof
    //!    complete:` prefix returns `Err(s)` whose `s`
    //!    starts with `live_proof receipt verification
    //!    failed: headline:`. A regression that drops
    //!    the prefix gate surfaces here.
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-verify-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    #[test]
    fn run_returns_passed_line_on_green_receipt() {
        let dir = fresh_dir("green");
        LiveProofReceipt::write_to(
            &dir,
            12,  // smoke_rows
            12,  // status_blueprint
            4,   // bench_hands
            4,   // compare_hands
            256, // replay_bytes
            "/srv/dev/repos/robopoker/target/debug/trainer",
            "<redacted: 49 chars>",
        )
        .expect("write_to should drop a synthetic green receipt");
        let result = run(&dir);
        let s = result.unwrap_or_else(|e| panic!("green receipt must verify; got error: {e:?}"));
        assert!(
            s.starts_with("live_proof receipt verification passed: "),
            "success line must start with the pinned prefix; got: {s:?}"
        );
        // The verifier passes the receipt, so the
        // embedded headline must be the verbatim
        // SUMMARY.txt first line — a regression that
        // strips the `testnet live_proof complete:`
        // prefix from the success line fails here.
        assert!(
            s.contains(
                "testnet live_proof complete: smoke=12 status=12 bench=4 compare=4 replay=256"
            ),
            "success line must carry the verbatim SUMMARY.txt headline; got: {s:?}"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_returns_recipe_shape_error_on_missing_dir() {
        // Use a path that is guaranteed not to exist
        // (the fresh_dir helper does NOT create the
        // dir for the missing-dir case; the *green* test
        // helper does — this one is a separate label so
        // a future refactor that auto-creates the dir
        // fails the missing-dir test loudly).
        let dir = fresh_dir("missing");
        let path = dir.join("receipts-does-not-exist");
        let result = run(&path);
        match result {
            Err(s) => {
                assert!(
                    s.starts_with("live_proof receipt verification failed: recipe_shape: "),
                    "missing-dir error must start with the pinned `recipe_shape:` kind; got: {s:?}"
                );
                // The `LiveProofReceipt::read_from` read
                // path produces an `io::Error` whose
                // message names the missing file
                // (`SUMMARY.txt missing or unreadable`).
                // The test does NOT assert on the exact
                // missing-file message (a future refactor
                // could move the dir-presence check from
                // `read_from` to `from_receipt_dir`),
                // only that the handler surfaces *some*
                // `does not exist` / `missing` substring
                // so a dashboard can grep it.
                assert!(
                    s.contains("does not exist")
                        || s.contains("missing")
                        || s.contains("No such file"),
                    "missing-dir error must include a `does not exist` / `missing` / `No such file` \
                     substring; got: {s:?}"
                );
            }
            Ok(s) => panic!("missing-dir must produce an error; got Ok({s:?})"),
        }
    }
    #[test]
    fn run_returns_step_failed_error_on_failed_step() {
        let dir = fresh_dir("step-failed");
        // Drop a green receipt, then rewrite the
        // `cluster/exit.txt` to `1` (a non-zero exit).
        // The verifier rejects the receipt with
        // `VerifyError::StepFailed { step: "cluster", exit: 1 }`.
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
        std::fs::write(dir.join("cluster").join("exit.txt"), "1\n")
            .expect("rewrite cluster/exit.txt to 1");
        let result = run(&dir);
        match result {
            Err(s) => {
                assert!(
                    s.starts_with("live_proof receipt verification failed: step_failed: "),
                    "step-failed error must start with the pinned `step_failed:` kind; got: {s:?}"
                );
                // The handler renders
                // `VerifyError::StepFailed { step, exit }`
                // as `live_proof step \`{step}\` failed
                // (exit {exit})` so a dashboard scraper
                // can `grep -E 'step .* failed'` the
                // stderr. The test asserts both the step
                // name and the exit code appear in the
                // line.
                assert!(
                    s.contains("cluster") && s.contains("exit 1"),
                    "step-failed error must name the failing step and its exit code; got: {s:?}"
                );
            }
            Ok(s) => panic!("step-failed receipt must produce an error; got Ok({s:?})"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_returns_headline_error_on_bad_headline() {
        let dir = fresh_dir("bad-headline");
        // Drop a green receipt, then rewrite the
        // SUMMARY.txt so the headline lacks the pinned
        // `testnet live_proof complete:` prefix. The
        // verifier rejects the receipt with
        // `VerifyError::Headline(...)` because the
        // typed `LiveProofHeadline::parse` cannot
        // find the prefix.
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
        let bad_summary = format!(
            "live_proof complete: smoke=0 status=0 bench=0 compare=0 replay=0\n\n  receipt_dir: {}\n",
            dir.display()
        );
        std::fs::write(dir.join("SUMMARY.txt"), bad_summary)
            .expect("rewrite SUMMARY.txt with the wrong prefix");
        let result = run(&dir);
        match result {
            Err(s) => {
                assert!(
                    s.starts_with("live_proof receipt verification failed: headline: "),
                    "bad-headline error must start with the pinned `headline:` kind; got: {s:?}"
                );
                // The display impl references the
                // expected prefix so a regression that
                // drops the prefix gate surfaces here.
                assert!(
                    s.contains("testnet live_proof complete:"),
                    "bad-headline error must reference the expected pinned prefix; got: {s:?}"
                );
            }
            Ok(s) => panic!("bad-headline receipt must produce an error; got Ok({s:?})"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Re-verify the STW-028 committed no-DB fixture
    /// under `tests/fixtures/testnet-live-proof-fixture/`.
    /// The fixture is a portable reference a downstream
    /// auditor can re-verify on any machine without a
    /// Postgres; the lib test pins the on-disk shape so
    /// a regression in the verifier that would silently
    /// accept a malformed fixture (e.g. an extra step
    /// name) fails `cargo test --workspace` before a
    /// worker can ship the change. The test does NOT
    /// regenerate the fixture — the fixture is committed
    /// in source control so a reviewer can `git diff`
    /// the on-disk shape.
    #[test]
    fn run_verifies_committed_testnet_live_proof_fixture() {
        // Walk up from `CARGO_MANIFEST_DIR` to the
        // crate root (i.e. `crates/autotrain/`), then
        // join the committed fixture path. Mirrors the
        // `workspace_root()` helper in
        // `crates/autotrain/tests/script_shape.rs` but
        // is local to this test (the lib tests in
        // `verify_receipt.rs` do not import the
        // integration-test module).
        let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let fixture = manifest.join("tests/fixtures/testnet-live-proof-fixture");
        let result = run(&fixture);
        let s = result.unwrap_or_else(|e| {
            panic!(
                "STW-028 committed fixture at {} must verify green; got error: {e:?}",
                fixture.display()
            )
        });
        assert!(
            s.starts_with("live_proof receipt verification passed: "),
            "STW-028 committed fixture success line must start with the pinned prefix; got: {s:?}"
        );
        // The fixture is byte-stable (no DB, no real
        // bin) so the embedded headline is the same on
        // every machine. A regression that drifts the
        // fixture's headline (or the verifier's expected
        // shape) surfaces here.
        assert!(
            s.contains(
                "testnet live_proof complete: smoke=12 status=12 bench=4 compare=4 replay=256"
            ),
            "STW-028 committed fixture success line must carry the verbatim pinned headline; \
             got: {s:?}"
        );
    }
}
