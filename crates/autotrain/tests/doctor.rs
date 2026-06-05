//! `trainer --doctor` integration test (STW-067).
//!
//! Drives the `trainer --doctor` CLI arm against a real
//! Postgres (gated on `database` feature + `DATABASE_URL`)
//! and asserts the exit-code + JSON output contracts.
//!
//! The test is the CI counterpart of the `scripts/testnet-live-proof.sh`
//! step-0 `--doctor` invocation: it proves the pre-flight
//! diagnostic surface is live before the expensive `--cluster`
//! step runs.

use std::process::Command;

fn trainer_bin_path() -> std::path::PathBuf {
    let manifest = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace = manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain");
    let debug = workspace.join("target").join("debug").join("trainer");
    let release = workspace.join("target").join("release").join("trainer");
    if debug.is_file() {
        debug
    } else if release.is_file() {
        release
    } else {
        panic!(
            "trainer binary not found at {} or {}",
            debug.display(),
            release.display()
        )
    }
}

/// Skip-on-missing-DB helper. Returns `true` when the
/// doctor test can proceed; returns `false` after printing
/// a skip notice.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --doctor integration test");
            false
        }
    }
}

/// Run `trainer --doctor` with the given env knobs and
/// return (stdout, stderr, exit_code).
fn run_doctor(env_extra: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = Command::new(trainer_bin_path());
    cmd.arg("--doctor");
    // The doctor reads DB_URL (not DATABASE_URL).
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

/// STW-067: `trainer --doctor` with a valid DB_URL exits 0
/// and emits parseable JSON with `healthy: true`.
#[test]
#[cfg(feature = "database")]
fn doctor_run_exits_zero_on_valid_db() {
    if !database_url_set() {
        return;
    }
    let (stdout, stderr, exit) = run_doctor(&[]);
    assert_eq!(
        exit, 0,
        "doctor on valid DB must exit 0; stdout={stdout:?} stderr={stderr:?}"
    );
    let parsed: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor stdout must be parseable JSON");
    assert_eq!(
        parsed["healthy"], true,
        "doctor on valid DB must report healthy=true; stdout={stdout:?}"
    );
    assert_eq!(
        parsed["db_reachable"], true,
        "doctor on valid DB must report db_reachable=true; stdout={stdout:?}"
    );
    assert_eq!(
        parsed["db_url_set"], true,
        "doctor on valid DB must report db_url_set=true; stdout={stdout:?}"
    );
    assert_eq!(
        parsed["trainer_bin_ok"], true,
        "doctor on valid DB must report trainer_bin_ok=true; stdout={stdout:?}"
    );
    assert!(
        parsed["checks"].is_array(),
        "doctor JSON must contain a checks array; stdout={stdout:?}"
    );
    // The stderr must contain the human-readable diagnostics.
    assert!(
        stderr.contains("doctor: all pre-flight checks passed"),
        "doctor stderr must contain the human-readable PASS headline; stderr={stderr:?}"
    );
}

/// STW-067: `trainer --doctor` with a bad DB_URL exits
/// non-zero and emits parseable JSON with `healthy: false`.
#[test]
#[cfg(feature = "database")]
fn doctor_run_exits_nonzero_on_bad_db() {
    let (stdout, stderr, exit) = run_doctor(&[("DB_URL", "postgres://bad:***@localhost:1/db")]);
    assert_ne!(
        exit, 0,
        "doctor on bad DB must exit non-zero; stdout={stdout:?} stderr={stderr:?}"
    );
    let parsed: serde_json::Value = serde_json::from_str(&stdout)
        .expect("doctor stdout must be parseable JSON even on failure");
    assert_eq!(
        parsed["healthy"], false,
        "doctor on bad DB must report healthy=false; stdout={stdout:?}"
    );
    assert_eq!(
        parsed["db_reachable"], false,
        "doctor on bad DB must report db_reachable=false; stdout={stdout:?}"
    );
    assert_eq!(
        parsed["db_url_set"], true,
        "doctor on bad DB must report db_url_set=true (the URL is set, just bad); stdout={stdout:?}"
    );
    assert!(
        stderr.contains("doctor: one or more pre-flight checks failed"),
        "doctor stderr must contain the human-readable FAIL headline; stderr={stderr:?}"
    );
}
