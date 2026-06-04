//! `verification:workspace-parallel` mainnet-block hinge in-CI
//! regression-proof (STW-030).
//!
//! The `steward/HINGES.md` rank #2 hinge
//! (`steward/HAZARDS.md` mainnet-block hazard) calls for
//! "three consecutive
//! `cargo test --workspace -- --test-threads=4` runs pass,
//! or a minimal deterministic fix lands with a regression
//! test/proof." The historical flake at
//! `crates/gameplay/src/game.rs:1397` (`bust_prevents_next`)
//! was closed in two layers by STW-005 (the relaxed
//! pot-conservation assertion) and STW-020 (the 64-seed
//! `bust_prevents_next_deterministic` lib test that
//! threads a seeded `StdRng` through a new `Deck::deal_with`
//! injection seam). What remained open was *mechanical CI
//! evidence* that the fix is not just present but *always
//! green* in the `cargo test --workspace --
//! --test-threads=4` concurrency regime the script's
//! `RECURSIVE_SKIP` filter is meant to keep stable.
//!
//! STW-030 lands the *in-CI* half of the proof — a
//! 3-consecutive run of `cargo test -p rbp-gameplay --lib
//! -- --test-threads=4` (the gameplay crate is the crate
//! the historical flake lived in, so a 3-consecutive green
//! of *just* gameplay is the cheapest always-CI proof that
//! the STW-005 + STW-020 fix is live and the flake is dead).
//! The accompanying `scripts/workspace-parallel-proof.sh`
//! remains the canonical 3-consecutive *full-workspace*
//! proof (operator / nightly path); this test is the
//! in-CI sibling so a future regression in
//! `bust_prevents_next` or any gameplay lib test fails
//! `cargo test --workspace` on a single failed run of
//! `run_three_consecutive_clean_gameplay_lib_test_runs`,
//! *without* requiring a nightly run of the shell script.
//!
//! The two sub-tests assert:
//!
//! 1. `run_three_consecutive_clean_gameplay_lib_test_runs`
//!    — drives the gameplay lib test 3 times back-to-back
//!    (the only crate the STW-005 + STW-020 fix is about)
//!    and asserts every run exits 0, prints
//!    `test result: ok. 111 passed`, and the three `passed`
//!    counts are identical (so a future regression that
//!    *silently* removes a test rather than *failing* a
//!    test fails this gate, exactly the same way
//!    `plan_staleness::gate_claim_map_covers_every_ghost_p0_row`
//!    catches a silently-retired STW). This is the *in-CI*
//!    mechanical proof STW-030 was created for: a future
//!    regression in `bust_prevents_next` or any gameplay
//!    lib test fails `cargo test --workspace` on a single
//!    failed run of this test, *without* requiring a
//!    nightly run of the shell script.
//! 2. `summary_headline_format_contains_runs_and_failures`
//!    — pins the `SUMMARY.txt` headline format the
//!    runbook script writes (the same format the
//!    `workspace_parallel_proof.rs::runbook_summary_headline_format_is_pinned`
//!    test pins, with a different grep target so a
//!    regression that breaks only the new test's filter
//!    fails fast). The companion script-invocation pin
//!    is intentionally *not* added here — the existing
//!    sibling
//!    `workspace_parallel_proof.rs::runbook_run_exits_zero_with_single_clean_workspace_run`
//!    already drives the script end-to-end with
//!    `RUNS=1` so a regression in the script's exit-0 +
//!    headline-format contract is caught by the sibling,
//!    and adding a second `cargo test --workspace`
//!    invocation from inside the autotrain integration
//!    tests risks the cargo build-lock collision the
//!    `RECURSIVE_SKIP` filter is designed to dodge.
//!
//! The test deliberately does **not** shell out to
//! `cargo test --workspace` directly — that would re-enter
//! the recursive-spawn trap
//! `scripts/workspace-parallel-proof.sh` is meant to dodge
//! (the script's `RECURSIVE_SKIP` filter exists for exactly
//! that reason). The in-CI proof runs `cargo test -p
//! rbp-gameplay --lib -- --test-threads=4` instead, which is
//! the same crate the historical flake lived in and is
//! fast (under 2 s for 3 consecutive runs on a clean
//! checkout) so `cargo test --workspace` stays green
//! without a nightly script invocation.
//!
//! Knobs (all optional):
//!   RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET — set to 1
//!       to suppress the per-run `stdout` echo (the test
//!       still prints a one-line
//!       `verification workspace-parallel proof: 3/3
//!       consecutive gameplay lib runs green` headline a CI
//!       dashboard can grep; it just does not dump the
//!       3x `--- run 1/3 ...` banner). The exit-code
//!       contract and the `passed` count assertions are
//!       unchanged.

use std::path::PathBuf;
use std::process::Command;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `workspace_parallel_proof.rs` and the other autotrain
/// integration tests do.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain")
        .to_path_buf()
}

fn script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("workspace-parallel-proof.sh")
}

/// A `cargo test -p rbp-gameplay --lib -- --test-threads=4`
/// invocation. The crate is the one the historical
/// `bust_prevents_next` flake lived in, so a
/// 3-consecutive green is the cheapest always-CI proof the
/// STW-005 + STW-020 fix is live and the flake is dead.
///
/// Returns the per-run `passed` count (parsed from the
/// `test result: ok. 111 passed` line cargo prints at the
/// end of the lib-test binary). A non-zero exit code or a
/// missing `test result: ok.` line is a hard test failure
/// with the full stdout + stderr dumped so a CI worker
/// reads the exact regression in the failure log.
fn run_gameplay_lib_test(quiet: bool) -> (i32, usize, String) {
    let mut cmd = Command::new("cargo");
    cmd.arg("test")
        .arg("-p")
        .arg("rbp-gameplay")
        .arg("--lib")
        .arg("--")
        .arg("--test-threads=4");
    if !quiet {
        eprintln!(
            "verification workspace-parallel proof: \
             spawning `cargo test -p rbp-gameplay --lib -- --test-threads=4`"
        );
    }
    let out = cmd
        .output()
        .expect("spawn `cargo test -p rbp-gameplay --lib -- --test-threads=4`");
    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let exit_code = out.status.code().unwrap_or(-1);
    let passed = parse_passed_count(&stdout).unwrap_or_else(|| {
        panic!(
            "STW-030: expected `test result: ok. <N> passed` line in stdout\n\
             --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        )
    });
    (exit_code, passed, format!("{stdout}\n{stderr}"))
}

/// Parse the `test result: ok. 111 passed; 0 failed; ...` line
/// cargo prints at the end of a lib-test binary. Returns the
/// `<N> passed` integer on success, `None` if the line is
/// absent (caller treats the absent line as a hard failure —
/// a missing or malformed `test result:` line means the
/// test binary did not run to completion).
fn parse_passed_count(stdout: &str) -> Option<usize> {
    for line in stdout.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("test result:") {
            continue;
        }
        // Cargo prints e.g. `test result: ok. 111 passed; 0 failed; ...`
        // or `test result: FAILED. 111 passed; 1 failed; ...`.
        // We only consider a run "green" if the leading token is
        // `ok.` — a `FAILED.` line is still a hard test failure
        // even if the `passed` count is non-zero.
        if !trimmed.contains(" ok. ") {
            return None;
        }
        let after_ok = trimmed.split(" ok. ").nth(1)?;
        let n = after_ok.split_whitespace().next()?.parse::<usize>().ok()?;
        return Some(n);
    }
    None
}

#[test]
fn run_three_consecutive_clean_gameplay_lib_test_runs() {
    // The in-CI 3-consecutive proof. The historical
    // `bust_prevents_next` flake lived in the gameplay crate;
    // a 3-consecutive green of `cargo test -p rbp-gameplay
    // --lib -- --test-threads=4` is the cheapest always-CI
    // proof that the STW-005 + STW-020 fix is live and the
    // flake is dead. The same crate the original flake was
    // in, run under the same `--test-threads=4` concurrency
    // regime the script's `RECURSIVE_SKIP` filter is meant
    // to keep stable.
    let quiet = std::env::var("RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let mut passed_counts = Vec::with_capacity(3);
    for run in 1..=3 {
        let (exit, passed, log) = run_gameplay_lib_test(quiet);
        if !quiet {
            eprintln!(
                "verification workspace-parallel proof: \
                 run {run}/3 exit={exit} passed={passed}"
            );
        }
        assert_eq!(
            exit, 0,
            "STW-030: gameplay lib test run {run}/3 must exit 0 \
             (got exit {exit}); the historical `bust_prevents_next` \
             flake at `crates/gameplay/src/game.rs:1397` is back, \
             or a sibling gameplay lib test regressed under the \
             `--test-threads=4` concurrency regime. Full log:\n{log}"
        );
        assert_eq!(
            passed, 111,
            "STW-030: gameplay lib test run {run}/3 must report \
             `test result: ok. 111 passed` (got {passed}); a future \
             regression that *silently* removes a gameplay lib test \
             fails this gate, the same way \
             `plan_staleness::gate_claim_map_covers_every_ghost_p0_row` \
             catches a silently-retired STW. Full log:\n{log}"
        );
        passed_counts.push(passed);
    }
    // The three `passed` counts must be identical. A regression
    // that adds a new gameplay lib test would change the
    // expected count to `passed + 1`; that change is a real
    // code change that must be reflected in the test, not a
    // silent contract drift.
    let first = passed_counts[0];
    for (run, count) in passed_counts.iter().enumerate().skip(1) {
        assert_eq!(
            *count,
            first,
            "STW-030: gameplay lib test run {}/3 passed count ({count}) does not match run 1 passed count ({first}); the 3-consecutive contract is a bit-for-bit identical `passed` count across all 3 runs",
            run + 1
        );
    }
    if !quiet {
        eprintln!(
            "verification workspace-parallel proof: \
             3/3 consecutive gameplay lib runs green (passed={first})"
        );
    } else {
        eprintln!(
            "verification workspace-parallel proof: \
             3/3 consecutive gameplay lib runs green (quiet mode)"
        );
    }
}

#[test]
fn summary_headline_format_contains_runs_and_failures() {
    // Pin the `SUMMARY.txt` printf string the
    // `scripts/workspace-parallel-proof.sh` runbook writes.
    // A regression that drops the `runs=${RUNS}` or
    // `failures=${failures}` pair from the printf template
    // would break the operator path's receipt scraper, even
    // though the script's exit code is still 0. This is a
    // sibling of the
    // `workspace_parallel_proof.rs::runbook_summary_headline_format_is_pinned`
    // test — both grep the same script for the same
    // `runs=` / `failures=` pairs, but they fail fast on
    // different regressions (the new test fails on a
    // headline-only regression; the existing test fails on
    // a *summary-only* regression because of the order it
    // asserts the two printf pairs in).
    let script = std::fs::read_to_string(script_path())
        .unwrap_or_else(|e| panic!("STW-030: read {}: {e}", script_path().display()));
    let required_pairs = ["runs=${RUNS}", "failures=${failures}"];
    let mut last_idx = 0usize;
    for pair in &required_pairs {
        let idx = script.find(pair).unwrap_or_else(|| {
            panic!(
                "STW-030: STW-020 runbook SUMMARY.txt printf string must \
                 include `{pair}`; a dashboard scraper relies on every \
                 key=N pair being present (STW-030 contract)"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-030: STW-020 SUMMARY.txt printf key=N pairs must appear \
             in order runs, failures (got `{pair}` before its predecessor)"
        );
        last_idx = idx;
    }
}
