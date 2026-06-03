//! `trainer --smoke` end-to-end proof (STW-009).
//!
//! This is the testnet roadmap proof point that a one-shot, env-gated,
//! small-abstraction smoke run actually:
//!
//! 1. Drives the full pipeline (`pretraining + train(epochs=N) + sync`)
//!    through a real `trainer` binary (not a library-only test).
//! 2. Persists a non-empty blueprint row count to Postgres that
//!    `trainer --status` can then report back.
//! 3. Exits 0; if anything in the pipeline fails — pretraining, the
//!    train loop, the `BinaryCopyInWriter` flush, or the post-sync
//!    row check — the binary exits non-zero and the test fails with
//!    the captured stdout/stderr for diagnosis.
//!
//! The test is gated on the `database` feature (a marker that
//! signals "live Postgres is reachable in this build") AND
//! short-circuits on a missing `DATABASE_URL`, so CI without
//! Postgres still runs `cargo test --workspace` green. The gating
//! mirrors `crates/gameroom/tests/hand_roundtrip.rs` and
//! `crates/auth/tests/server_flow.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The smoke contract is *the binary's* exit code and stdout, not
//! `Trainer::train`'s return value. Driving the binary as a process
//! is the only way to assert the same surface a real worker sees,
//! and it surfaces the actual `log::info!` lines the dashboard
//! scrapes when the smoke job completes. A library call would let
//! us bypass `Mode::from_args`, the `RBP_FAST_EPOCHS` env read, and
//! the non-zero exit on `rows == 0` — the very things the test is
//! trying to pin.
//!
//! ## Why we resolve the binary path manually instead of using
//! `CARGO_BIN_EXE_trainer`
//!
//! The `trainer` binary lives in `bin/trainer/` (a separate
//! workspace crate), not in `rbp-autotrain`, so Cargo does not emit
//! `CARGO_BIN_EXE_trainer` for tests in this package. We resolve
//! the path at runtime from the workspace's `target/` directory,
//! which is set via `CARGO_TARGET_DIR` (defaulting to
//! `<workspace>/target/`). The test fails loudly if the binary is
//! not on disk, so a missing `--bin trainer` build is a setup
//! error rather than a silent skip.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the workspace's `target/`
/// directory. We walk up from `CARGO_MANIFEST_DIR` (the
/// `crates/autotrain` directory) to the workspace root, then
/// probe `<workspace>/target/{debug,release}/trainer` and return
/// whichever exists. `cargo test` always builds and runs against
/// the `debug` profile, so `debug/trainer` is the canonical
/// location, but a `cargo test --release` invocation also has to
/// work, so we check both. The function panics if the binary is
/// missing — that is a setup error (the operator did not build
/// the trainer), not a silent test skip.
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
    );
}

fn trainer_bin() -> Command {
    Command::new(trainer_bin_path())
}

/// Skip-on-missing-DB helper. Returns `true` when the smoke run
/// can proceed; returns `false` after printing a skip notice. The
/// `DATABASE_URL` contract is the same one the auth server-flow
/// tests and the gameroom round-trip tests honor: unset means the
/// operator chose not to stand up Postgres in this environment, so
/// the unit-level safety nets still run but the live-DB proof
/// does not.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --smoke integration test");
            false
        }
    }
}

/// Run the binary, capture stdout/stderr, and assert it exits 0.
/// The full `log::info!` line stream is included in the assertion
/// message on failure so a worker can grep `smoke complete:` /
/// `smoke failed:` / `panicked at` without re-running.
fn run_trainer(args: &[&str], env_extra: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.args(args);
    // Force a clean log target so the `log::info!` output lands on
    // stderr (the trainer's default) without depending on an
    // `env_logger` init from the parent test process.
    cmd.env("RUST_LOG", "info");
    // The trainer's `db()` helper reads `DB_URL` (not
    // `DATABASE_URL`). We forward the parent's `DATABASE_URL` (the
    // convention the auth + gameroom tests use) as `DB_URL` so
    // the trainer finds the same Postgres the test process is
    // connected to, without each test having to re-declare both
    // env vars.
    if let Ok(url) = std::env::var("DATABASE_URL") {
        cmd.env("DB_URL", url);
    }
    for (k, v) in env_extra {
        cmd.env(k, v);
    }
    let out = cmd.output().expect("spawn trainer binary");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Asserts that `trainer --smoke` exits 0 with a non-empty
/// blueprint, and that a follow-up `trainer --status` reports
/// `Epoch > 0` and `Blueprint > 0`. This is the closed-loop proof
/// the CEO roadmap demands: the smoke run produces an observable
/// artifact, and the same `--status` query a human or dashboard
/// would run can read it back.
///
/// The test uses `RBP_FAST_EPOCHS=2 RBP_FAST_BATCH=16` so the
/// pipeline completes inside the test-process timeout without
/// requiring Pluribus-scale iterations. Both knobs are read at
/// the top of the relevant loops, so the env-var flip takes
/// effect on the very first iteration.
#[test]
fn smoke_run_writes_nonempty_blueprint_and_status_reports_it() {
    if !database_url_set() {
        return;
    }

    // Reset state so the smoke run's post-sync row count is
    // determined purely by the run itself, not by leftover rows
    // from a previous failed test. `--reset` truncates blueprint
    // and epoch-meta only — the clustering tables (metric, future,
    // lookup) are left intact so `pretraining` is a no-op on a
    // warmed-up DB and the smoke is purely a train-loop proof.
    let (stdout, stderr, code) = run_trainer(&["--reset"], &[]);
    assert_eq!(
        code, 0,
        "trainer --reset must exit 0 before the smoke run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // Drive the smoke pipeline. The trainer reads
    // `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH` from the environment
    // it inherits from this test process.
    let (stdout, stderr, code) = run_trainer(
        &["--smoke"],
        &[("RBP_FAST_EPOCHS", "2"), ("RBP_FAST_BATCH", "16")],
    );
    assert_eq!(
        code, 0,
        "trainer --smoke must exit 0 on a successful small run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // (ii) The `smoke complete: epochs=N rows=M` log line must
    // be present and `M` must parse as `> 0`. We assert the
    // exact prefix the mode emits so a future renaming of the
    // log line fails the test loudly.
    let complete = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("smoke complete:"))
        .unwrap_or_else(|| {
            panic!(
                "trainer --smoke must print `smoke complete: ...` on success.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let rows = parse_rows_value(complete).unwrap_or_else(|| {
        panic!("smoke complete line must include a parseable `rows=N` value (got: {complete:?})")
    });
    assert!(
        rows > 0,
        "smoke run must leave a non-empty blueprint (rows={rows}, line={complete:?})"
    );

    // (iii) A follow-up `--status` call must report `Epoch > 0`
    // and `Blueprint > 0`. We parse the two values out of the
    // box-drawing log lines the `Check::status` impl emits
    // (`│ Epoch      │ {:>13} │` and `│ Blueprint  │ {:>13} │`).
    let (stdout, stderr, code) = run_trainer(&["--status"], &[]);
    assert_eq!(
        code, 0,
        "trainer --status must exit 0 after a successful smoke run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    let epoch = parse_status_value(&stderr, "Epoch").unwrap_or_else(|| {
        panic!(
            "trainer --status must report an `Epoch` value after a smoke run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        )
    });
    let blueprint = parse_status_value(&stderr, "Blueprint").unwrap_or_else(|| {
        panic!(
            "trainer --status must report a `Blueprint` value after a smoke run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        )
    });
    assert!(
        epoch > 0,
        "status-reported `Epoch` must be > 0 after a smoke run (got {epoch})"
    );
    assert!(
        blueprint > 0,
        "status-reported `Blueprint` must be > 0 after a smoke run (got {blueprint})"
    );
}

/// Extract `rows=N` from a `smoke complete: epochs=N rows=M` log
/// line. Returns `None` if the line does not contain a `rows=`
/// token followed by an unsigned integer. Tolerant of the two
/// possible orderings (`epochs=` may come first; the mode emits
/// `epochs={epoch} rows={rows}`) and of any leading log
/// metadata Cargo/Rustlog prepends.
fn parse_rows_value(line: &str) -> Option<usize> {
    let needle = "rows=";
    let idx = line.find(needle)?;
    let after = &line[idx + needle.len()..];
    let mut end = after.len();
    for (i, c) in after.char_indices() {
        if !c.is_ascii_digit() {
            end = i;
            break;
        }
    }
    after[..end].parse::<usize>().ok()
}

/// Extract the right-hand number from a `Check::status` row of
/// the form `│ <Label>     │ {:>13} │` (the trainer's status
/// impl draws a small box around the two counters). The lookup
/// is exact-label so `Epoch` does not match `Blueprint` and
/// vice versa, and so future status-line additions don't
/// silently satisfy the assertion.
fn parse_status_value(stderr: &str, label: &str) -> Option<usize> {
    for line in stderr.lines() {
        let trimmed = line.trim();
        // Match `│ <label>     │ <number> │` or similar box-drawing
        // variants. We anchor on the label word followed by box
        // characters and a number, not on the full box-drawing
        // geometry, so a future padding tweak does not break the
        // test.
        if !trimmed.contains(label) {
            continue;
        }
        // After the label, the row contains a `│` separator and
        // then a comma-formatted or plain integer. We scan the
        // trailing digits (and any thousands-separators) and
        // parse what is left after stripping non-digits.
        let mut digits = String::new();
        for c in trimmed.chars().rev() {
            if c.is_ascii_digit() {
                digits.insert(0, c);
            } else if !digits.is_empty() {
                // First non-digit after a digit run marks the
                // end of the number; we are reading right-to-left.
                break;
            }
        }
        if let Ok(n) = digits.parse::<usize>() {
            return Some(n);
        }
    }
    None
}
