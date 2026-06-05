//! `trainer --doctor` end-to-end proof (STW-067).
//!
//! This integration test pins the `--doctor` pre-flight CLI:
//! it runs the trainer binary with `--doctor` and asserts the
//! exit code + JSON contract for both a healthy and an unhealthy
//! configuration. The test is the CI-visible counterpart of the
//! `scripts/testnet-live-proof.sh` step-0 `--doctor` invocation.
//!
//! The test is gated on the `database` feature (a marker that
//! signals "live Postgres is reachable in this build") AND
//! short-circuits on a missing `DATABASE_URL`, so CI without
//! Postgres still runs `cargo test --workspace` green. The gating
//! mirrors `crates/autotrain/tests/{smoke,bench,compare,live_proof}.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The doctor contract is *the binary's* exit code and stdout, not
//! `doctor::run`'s return value. Driving the binary as a process
//! is the only way to assert the same surface a real worker or
//! runbook sees, and it surfaces the actual JSON line a CI
//! dashboard scrapes.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the workspace's `target/`
/// directory. Mirrors the helper in `smoke.rs` / `bench.rs` /
/// `compare.rs` / `live_proof.rs`.
fn trainer_bin_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
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
    );
}

fn trainer_bin() -> Command {
    Command::new(trainer_bin_path())
}

/// Skip-on-missing-DB helper. Returns `true` when the doctor run
/// can proceed; returns `false` after printing a skip notice.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --doctor integration test");
            false
        }
    }
}

/// Run the binary with `--doctor`, capture stdout/stderr, and
/// return the (stdout, stderr, exit_code) triple.
fn run_doctor(env_extra: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.arg("--doctor");
    cmd.env("RUST_LOG", "info");
    // Forward DATABASE_URL as DB_URL so the doctor sees the same
    // Postgres the rest of the autotrain tests use.
    if let Ok(url) = std::env::var("DATABASE_URL") {
        cmd.env("DB_URL", url);
    }
    for (k, v) in env_extra {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn trainer --doctor");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// STW-067: `trainer --doctor` exits 0 and prints healthy JSON
/// when connected to a valid database.
#[test]
fn doctor_run_exits_zero_on_valid_db() {
    if !database_url_set() {
        return;
    }

    let (stdout, stderr, code) = run_doctor(&[]);
    assert_eq!(
        code, 0,
        "trainer --doctor must exit 0 when prerequisites are healthy. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // The last line of stdout must be parseable JSON with healthy=true.
    let json_line = stdout.lines().last().unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(json_line).unwrap_or_else(|e| {
        panic!(
            "trainer --doctor stdout must end with a parseable JSON line; got error: {e}\n\
             --- stdout ---\n{stdout}"
        )
    });
    assert_eq!(
        parsed.get("healthy").and_then(|v| v.as_bool()),
        Some(true),
        "trainer --doctor JSON must report healthy=true on a valid DB \
         (got: {parsed})"
    );
}

/// STW-067: `trainer --doctor` exits non-zero and prints unhealthy
/// JSON when the database URL is bad.
#[test]
fn doctor_run_exits_nonzero_on_bad_db() {
    let (stdout, stderr, code) = run_doctor(&[
        ("DB_URL", "postgres://bad:***@localhost:1/db"),
        ("DATABASE_URL", ""),
    ]);
    assert_ne!(
        code, 0,
        "trainer --doctor must exit non-zero when DB_URL is bad. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // The last line of stdout must still be parseable JSON with healthy=false.
    let json_line = stdout.lines().last().unwrap_or("");
    let parsed: serde_json::Value = serde_json::from_str(json_line).unwrap_or_else(|e| {
        panic!(
            "trainer --doctor stdout must end with a parseable JSON line even on failure; \
             got error: {e}\n--- stdout ---\n{stdout}"
        )
    });
    assert_eq!(
        parsed.get("healthy").and_then(|v| v.as_bool()),
        Some(false),
        "trainer --doctor JSON must report healthy=false on a bad DB \
         (got: {parsed})"
    );
}
