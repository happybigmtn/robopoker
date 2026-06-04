//! `scripts/testnet-live-proof.sh` shape contract (STW-019).
//!
//! This integration test pins the *shape* of the STW-019 runbook
//! without requiring a live Postgres. It runs in
//! `cargo test --workspace` (no `database` feature gate) so a
//! regression in the runbook's surface (file missing, syntax
//! broken, executable bit cleared, doc drift) fails CI before it
//! ever reaches a live DB.
//!
//! The four sub-tests assert the runbook's static contract:
//!
//! 1. `script_exists_and_is_executable` — the runbook is on disk
//!    and has its executable bit set (a worker can invoke it via
//!    `bash scripts/testnet-live-proof.sh`).
//! 2. `script_parses_with_bash_n` — `bash -n` parses the script
//!    without error (a syntax regression fails the gate at CI time).
//! 3. `runbook_doc_lists_every_env_knob` — every `RBP_FAST_EPOCHS` /
//!    `RBP_FAST_BATCH` / `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` /
//!    `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` / `RBP_BENCH_TRANSCRIPT_DIR`
//!    the runbook honours also appears in
//!    `scripts/testnet-live-proof.md` (catches doc drift where the
//!    script gains a knob but the doc forgets to mention it).
//! 4. `runbook_doc_references_every_chain_step` — the runbook doc
//!    names every chain step the `live_proof.rs` integration test
//!    covers (`--cluster`, `--reset`, `--smoke`, `--status`,
//!    `--bench`, `--compare`, `--replay`). A future refactor that
//!    drops a leg fails here.
//!
//! The test deliberately does **not** shell out to the runbook
//! itself: that would require `DATABASE_URL` and would be a
//! duplicate of the `live_proof` integration test. The shell-shape
//! test is the *no-DB gate* that lets `cargo test --workspace`
//! stay green even on machines that have no Postgres.

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `bench.rs` / `live_proof.rs` do. The shell-shape
/// integration test reads files under `<workspace>/scripts/` and
/// `<workspace>/README.md`; the helper centralises the path
/// resolution so a future test addition reuses the same convention.
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
        .join("testnet-live-proof.sh")
}

fn runbook_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-proof.md")
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn script_exists_and_is_executable() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-019 runbook script missing at {}; \
         the testnet live launch proof has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The executable bit for the owner must be set; a worker
        // running `bash scripts/testnet-live-proof.sh` works even
        // without the executable bit, but the integration test
        // pins the convention `chmod +x` the runbook shipped
        // with, so a future chmod regression fails the test.
        assert!(
            mode & 0o100 != 0,
            "STW-019 runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        // On non-Unix hosts we can only assert the file is
        // present; the bash -n check below covers the "is the
        // file actually a bash script" question.
        let _ = meta;
    }
}

#[test]
fn script_parses_with_bash_n() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-019 runbook script missing at {} (cannot bash -n a missing file)",
        p.display()
    );
    // `bash -n` parses the script without executing it. The test
    // fails on a non-zero exit (a syntax error) so a future edit
    // that breaks the bash grammar fails CI before it reaches a
    // live Postgres.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-proof.sh");
    assert!(
        out.status.success(),
        "STW-019 runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn runbook_doc_lists_every_env_knob() {
    // Every env knob the runbook script honours must also be
    // listed in the runbook doc, so the doc and the script stay
    // in lockstep. We assert each knob by *name* (the
    // `RBP_FOO_BAR` token), not by description, so the test
    // survives a doc rewrite that re-words a paragraph.
    let doc = read(&runbook_doc_path());
    // The env knobs the runbook script honours. Mirrors the
    // `: "${...:=default}"` lines in `testnet-live-proof.sh` plus
    // the `RBP_BENCH_TRANSCRIPT_DIR` knob the runbook sets
    // internally (a future refactor that adds a knob must also
    // add it here, or this test will fail).
    let required_knobs = [
        "RBP_FAST_EPOCHS",
        "RBP_FAST_BATCH",
        "RBP_BENCH_HANDS",
        "RBP_BENCH_BLIND",
        "RBP_COMPARE_HANDS",
        "RBP_COMPARE_BLIND",
        "RBP_BENCH_TRANSCRIPT_DIR",
    ];
    let mut missing: Vec<&str> = Vec::new();
    for knob in &required_knobs {
        if !doc.contains(knob) {
            missing.push(knob);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-019 runbook doc at {} must list every env knob the script honours. \
         Missing from doc: {missing:?}",
        runbook_doc_path().display()
    );
}

#[test]
fn runbook_doc_references_every_chain_step() {
    // The runbook doc must name every chain step the live proof
    // integration test (`crates/autotrain/tests/live_proof.rs`)
    // covers. We assert by flag form (`--smoke`, `--bench`, etc.)
    // because that is the form the operator types and the form
    // a dashboard scraper greps.
    let doc = read(&runbook_doc_path());
    let required_steps = [
        "--cluster",
        "--reset",
        "--smoke",
        "--status",
        "--bench",
        "--compare",
        "--replay",
    ];
    let mut missing: Vec<&str> = Vec::new();
    for step in &required_steps {
        if !doc.contains(step) {
            missing.push(step);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-019 runbook doc at {} must reference every chain step the live proof \
         integration test covers. Missing from doc: {missing:?}",
        runbook_doc_path().display()
    );
}

#[test]
fn script_summary_headline_format_is_pinned() {
    // The `SUMMARY.txt` headline line the runbook writes must
    // start with the literal prefix `testnet live_proof complete:`
    // and include all five `key=N` pairs
    // (`smoke=`, `status=`, `bench=`, `compare=`, `replay=`) so a
    // dashboard scraper can grep either the SUMMARY.txt file or
    // the runbook's stdout with the same regex. We assert the
    // script's source text contains a printf-style line with
    // this exact shape.
    let script = read(&script_path());
    assert!(
        script.contains("testnet live_proof complete: smoke="),
        "STW-019 runbook must print a `testnet live_proof complete: smoke=...` headline line; \
         the dashboard scraper relies on this exact prefix"
    );
    // All five key=N pairs must appear in the printf string the
    // script writes to SUMMARY.txt, in the order
    // smoke, status, bench, compare, replay (the same order the
    // `crates/autotrain/tests/live_proof.rs` integration test's
    // final log line uses).
    let required_pairs = [
        "smoke=$SMOKE_ROWS",
        "status=$STATUS_BLUEPRINT",
        "bench=$BENCH_HANDS",
        "compare=$COMPARE_HANDS",
        "replay=$REPLAY_BYTES",
    ];
    let mut last_idx = 0usize;
    for pair in &required_pairs {
        let idx = script.find(pair).unwrap_or_else(|| {
            panic!(
                "STW-019 runbook SUMMARY.txt printf string must include `{pair}`; \
                 a dashboard scraper relies on every key=N pair being present"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-019 SUMMARY.txt printf key=N pairs must appear in order \
             smoke, status, bench, compare, replay (got `{pair}` before its predecessor)"
        );
        last_idx = idx;
    }
}
