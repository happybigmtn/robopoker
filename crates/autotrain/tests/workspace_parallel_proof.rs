//! `scripts/workspace-parallel-proof.sh` shape contract (STW-020).
//!
//! This integration test pins the *shape* of the STW-020
//! runbook without requiring 3 full `cargo test --workspace`
//! invocations. It runs in `cargo test --workspace` (no
//! `database` feature gate) so a regression in the runbook's
//! surface (file missing, syntax broken, executable bit
//! cleared, doc drift) fails CI before it ever reaches a
//! nightly run.
//!
//! The four sub-tests assert the runbook's static contract:
//!
//! 1. `script_exists_and_is_executable` — the runbook is on
//!    disk and has its executable bit set (a worker can invoke
//!    it via `bash scripts/workspace-parallel-proof.sh`).
//! 2. `script_parses_with_bash_n` — `bash -n` parses the
//!    script without error (a syntax regression fails the gate
//!    at CI time).
//! 3. `runbook_summary_headline_format_is_pinned` — the
//!    `SUMMARY.txt` headline line the runbook writes starts
//!    with the literal prefix `workspace parallel proof
//!    complete:` and includes both `runs=N` and `failures=N`
//!    pairs a CI dashboard can grep.
//! 4. `runbook_run_exits_zero_with_single_clean_workspace_run`
//!    — actually drives the script end-to-end with
//!    `RBP_WORKSPACE_PARALLEL_RUNS=1` to keep CI cost bounded
//!    (the default `RUNS=3` knob is for the operator / nightly
//!    path). The single-run proof still proves the headline
//!    format and the exit-0 contract; the 3-consecutive
//!    version is gated to nightly CI.

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `script_shape.rs` and the other autotrain integration
/// tests do.
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

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn script_exists_and_is_executable() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-020 runbook script missing at {}; \
         the workspace parallel test proof has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        assert!(
            mode & 0o100 != 0,
            "STW-020 runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
}

#[test]
fn script_parses_with_bash_n() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-020 runbook script missing at {} (cannot bash -n a missing file)",
        p.display()
    );
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/workspace-parallel-proof.sh");
    assert!(
        out.status.success(),
        "STW-020 runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn runbook_summary_headline_format_is_pinned() {
    let script = read(&script_path());
    // The headline line a CI dashboard greps.
    assert!(
        script.contains("workspace parallel proof complete: runs="),
        "STW-020 runbook must print a `workspace parallel proof complete: runs=...` headline line; \
         the dashboard scraper relies on this exact prefix"
    );
    // Both `runs=N` and `failures=N` pairs must appear in the
    // printf string the script writes, in the order runs first
    // then failures (the same order the headline line uses).
    let required_pairs = ["runs=${RUNS}", "failures=${failures}"];
    let mut last_idx = 0usize;
    for pair in &required_pairs {
        let idx = script.find(pair).unwrap_or_else(|| {
            panic!(
                "STW-020 runbook SUMMARY.txt printf string must include `{pair}`; \
                 a dashboard scraper relies on every key=N pair being present"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-020 SUMMARY.txt printf key=N pairs must appear in order \
             runs, failures (got `{pair}` before its predecessor)"
        );
        last_idx = idx;
    }
}

#[test]
fn runbook_run_exits_zero_with_single_clean_workspace_run() {
    // Drives the runbook end-to-end with RBP_WORKSPACE_PARALLEL_RUNS=1
    // so the test fits in CI wall-clock budget. The full
    // 3-consecutive proof is a nightly / operator path (just
    // `bash scripts/workspace-parallel-proof.sh` from a clean
    // checkout). The single-run proof still pins the headline
    // format, the exit-0 contract, and the SUMMARY.txt layout.
    let p = script_path();
    assert!(
        p.exists(),
        "STW-020 runbook script missing at {} (cannot drive it from this test)",
        p.display()
    );
    let out = std::process::Command::new("bash")
        .arg(&p)
        .env("RBP_WORKSPACE_PARALLEL_RUNS", "1")
        .env("RBP_WORKSPACE_PARALLEL_SKIP_BUILD", "1")
        .env("WORKSPACE_ROOT", workspace_root())
        .output()
        .expect("spawn bash scripts/workspace-parallel-proof.sh");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        out.status.success(),
        "STW-020 runbook must exit 0 on a single clean workspace run (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        stdout,
        stderr
    );
    assert!(
        stdout.contains("workspace parallel proof complete: runs=1 failures=0"),
        "STW-020 runbook must print the `workspace parallel proof complete: runs=1 failures=0` \
         headline line on success; got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
}
