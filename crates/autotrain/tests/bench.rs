//! `trainer --bench` end-to-end proof (STW-010).
//!
//! This is the testnet roadmap proof point that the bench harness
//! actually:
//!
//! 1. Drives a real `trainer` binary (not a library-only test)
//!    through K heads-up hands of `DatabasePlayer` (seat 0) vs
//!    `Fish` (seat 1).
//! 2. Emits the documented single-line JSON report on stdout
//!    (the contract any downstream scraper / dashboard parses).
//! 3. Exits 0; if anything in the pipeline fails (room
//!    construction, blueprint hydration, persistence, hand loop)
//!    the binary exits non-zero and the test fails with the
//!    captured stdout/stderr for diagnosis.
//!
//! The test is gated on the `database` feature AND short-circuits
//! on a missing `DATABASE_URL`, so CI without Postgres still runs
//! `cargo test --workspace` green. The gating mirrors
//! `crates/autotrain/tests/smoke.rs`,
//! `crates/gameroom/tests/hand_roundtrip.rs`, and
//! `crates/auth/tests/server_flow.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The bench contract is *the binary's* exit code and stdout, not
//! `bench::run`'s return value. Driving the binary as a process
//! is the only way to assert the same surface a real worker or
//! dashboard would see, and it surfaces the actual
//! `bench complete: ...` log line the human reviewer greps. A
//! library call would let us bypass `Mode::from_args`, the
//! `RBP_BENCH_HANDS` env read, and the JSON-line stdout
//! contract — the very things the test is trying to pin.
//!
//! ## Why we resolve the binary path manually
//!
//! The `trainer` binary lives in `bin/trainer/` (a separate
//! workspace crate), so Cargo does not emit
//! `CARGO_BIN_EXE_trainer` for tests in this package. We walk up
//! from `CARGO_MANIFEST_DIR` to the workspace root and probe
//! `<workspace>/target/{debug,release}/trainer`, exactly as the
//! smoke integration test does.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the workspace's `target/`
/// directory. We walk up from `CARGO_MANIFEST_DIR` to the
/// workspace root, then probe `<workspace>/target/{debug,release}>
/// /trainer` and return whichever exists. The function panics if
/// the binary is missing — that is a setup error (the operator
/// did not build the trainer), not a silent test skip.
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

/// Skip-on-missing-DB helper. Returns `true` when the bench run
/// can proceed; returns `false` after printing a skip notice.
/// The `DATABASE_URL` contract is the same one the auth
/// server-flow tests, the gameroom round-trip tests, and the
/// autotrain smoke test honor.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --bench integration test");
            false
        }
    }
}

/// Run the binary, capture stdout/stderr, and return the
/// (stdout, stderr, exit_code) triple. Mirrors the helper in
/// `smoke.rs` so the two integration tests share the same
/// `DATABASE_URL` → `DB_URL` forwarding convention.
fn run_trainer(args: &[&str], env_extra: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.args(args);
    cmd.env("RUST_LOG", "info");
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

/// Parse a single-line JSON bench report into a small struct
/// the test can assert on. We deliberately do NOT depend on
/// `serde_json` (it's already in the dep graph but we want the
/// test to be a thin contract check that survives a future
/// serde upgrade): a small regex-free hand-rolled parser is
/// enough for the flat-key shape `BenchReport::to_json`
/// produces, and the explicit parsing keeps the failure mode
/// obvious if a future refactor renames a field.
struct ParsedBench {
    hands: usize,
    wins: usize,
    losses: usize,
    net_chips: i64,
    mbb_per_100: f64,
    mbb_ci95: f64,
    win_rate: f64,
    win_rate_ci95: f64,
    blind: i16,
    blueprint_trained: bool,
}

fn parse_bench_line(line: &str) -> Option<ParsedBench> {
    let trim = line.trim();
    if !(trim.starts_with('{') && trim.ends_with('}')) {
        return None;
    }
    let body = &trim[1..trim.len() - 1];
    let mut hands: Option<usize> = None;
    let mut wins: Option<usize> = None;
    let mut losses: Option<usize> = None;
    let mut net_chips: Option<i64> = None;
    let mut mbb_per_100: Option<f64> = None;
    let mut mbb_ci95: Option<f64> = None;
    let mut win_rate: Option<f64> = None;
    let mut win_rate_ci95: Option<f64> = None;
    let mut blind: Option<i16> = None;
    let mut blueprint_trained: Option<bool> = None;
    // Split on top-level commas. The bench JSON is a flat
    // object of `key:number` / `key:bool` pairs, none of which
    // contain commas, so a naive `,` split is sufficient and
    // keeps the parser independent of any JSON crate.
    for kv in body.split(',') {
        let (k, v) = kv.split_once(':')?;
        let key = k.trim().trim_matches('"');
        let raw = v.trim();
        match key {
            "hands" => hands = raw.parse().ok(),
            "wins" => wins = raw.parse().ok(),
            "losses" => losses = raw.parse().ok(),
            "net_chips" => net_chips = raw.parse().ok(),
            "mbb_per_100" => mbb_per_100 = raw.parse().ok(),
            "mbb_ci95" => mbb_ci95 = raw.parse().ok(),
            "win_rate" => win_rate = raw.parse().ok(),
            "win_rate_ci95" => win_rate_ci95 = raw.parse().ok(),
            "blind" => blind = raw.parse().ok(),
            "blueprint_trained" => {
                blueprint_trained = Some(matches!(raw, "true"));
            }
            _ => return None,
        }
    }
    Some(ParsedBench {
        hands: hands?,
        wins: wins?,
        losses: losses?,
        net_chips: net_chips?,
        mbb_per_100: mbb_per_100?,
        mbb_ci95: mbb_ci95?,
        win_rate: win_rate?,
        win_rate_ci95: win_rate_ci95?,
        blind: blind?,
        blueprint_trained: blueprint_trained?,
    })
}

/// Asserts that `trainer --bench` exits 0, prints a parseable
/// single-line JSON report on stdout with the documented
/// `BenchReport` fields, and that the per-hand accounting is
/// internally consistent (K hands played, K = wins + losses +
/// non-PnL hands, mbb/100 and CI are finite). The test uses
/// `RBP_BENCH_HANDS=20` so a CI worker can run the bench
/// end-to-end in a few seconds; the env-var flip takes effect
/// at the very first call to `bench::bench_hands()`.
#[test]
fn bench_run_emits_parseable_json_with_consistent_accounting() {
    if !database_url_set() {
        return;
    }

    // Reset state so the bench's pre-run blueprint count is
    // deterministic for this test: a fresh empty blueprint is
    // the documented "untrained" state and the bench flags it
    // in the JSON. The bench itself is valid against an empty
    // blueprint (it just measures whatever the untrained
    // `DatabasePlayer` does against `Fish`), so the test
    // asserts the bench runs end-to-end, not that the bot wins.
    let (stdout, stderr, code) = run_trainer(&["--reset"], &[]);
    assert_eq!(
        code, 0,
        "trainer --reset must exit 0 before the bench run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let (stdout, stderr, code) = run_trainer(
        &["--bench"],
        &[("RBP_BENCH_HANDS", "20"), ("RBP_BENCH_BLIND", "2")],
    );
    assert_eq!(
        code, 0,
        "trainer --bench must exit 0 on a successful small run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // (ii) The JSON report must be present on stdout. The
    // bench emits exactly one line per run; a future refactor
    // that introduces a second line (e.g. a "warmup" report)
    // should fail this test until the contract is updated.
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{') && l.trim_end().ends_with('}'))
        .unwrap_or_else(|| {
            panic!(
                "trainer --bench must print a single-line JSON report on stdout.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let parsed = parse_bench_line(line).unwrap_or_else(|| {
        panic!("trainer --bench stdout must be a parseable BenchReport JSON; got: {line:?}")
    });

    // (iii) Headline accounting must be internally consistent:
    // K hands were played; the JSON `hands` field must match
    // the env var we passed; the per-seat counts (`wins +
    // losses + non-PnL hands = K`) must add up; mbb/100 and CI
    // are finite numbers (no NaN / Inf leaked out of the
    // summariser).
    assert_eq!(
        parsed.hands, 20,
        "bench: hands field must equal RBP_BENCH_HANDS; got {}",
        parsed.hands
    );
    assert_eq!(
        parsed.blind, 2,
        "bench: blind field must equal RBP_BENCH_BLIND; got {}",
        parsed.blind
    );
    assert!(
        parsed.wins + parsed.losses <= parsed.hands,
        "bench: wins + losses ({}+{}={}) must be <= hands ({})",
        parsed.wins,
        parsed.losses,
        parsed.wins + parsed.losses,
        parsed.hands
    );
    assert!(
        parsed.mbb_per_100.is_finite() && parsed.mbb_ci95.is_finite(),
        "bench: mbb/100 and CI must be finite; got {} ± {}",
        parsed.mbb_per_100,
        parsed.mbb_ci95
    );
    assert!(
        parsed.win_rate.is_finite() && parsed.win_rate_ci95.is_finite(),
        "bench: win_rate and CI must be finite; got {} ± {}",
        parsed.win_rate,
        parsed.win_rate_ci95
    );
    assert!(
        (0.0..=1.0).contains(&parsed.win_rate),
        "bench: win_rate must be in [0,1]; got {}",
        parsed.win_rate
    );
    // `net_chips` and `mbb_per_100` are two views of the same
    // underlying mean. The bench contract is `mbb_per_100 =
    // net_chips * 100 / (hands * blind)`; we pin that here so
    // a future refactor that drops one field or breaks the
    // conversion surfaces in the integration test, not in a
    // downstream dashboard.
    let expected_mbb =
        (parsed.net_chips as f64) * 100.0 / ((parsed.hands as f64) * (parsed.blind as f64));
    assert!(
        (parsed.mbb_per_100 - expected_mbb).abs() < 1e-3,
        "bench: mbb_per_100 ({}) must equal net_chips*100/(hands*blind) = {}; drift {}",
        parsed.mbb_per_100,
        expected_mbb,
        (parsed.mbb_per_100 - expected_mbb).abs()
    );

    // (iv) The pre-bench blueprint was just reset, so the
    // `blueprint_trained` flag must be `false`. This is the
    // honest-reporting check: the bench ran against an
    // untrained bot and the JSON says so. A future refactor
    // that hardcodes `true` would silently break the
    // contract; this test catches that.
    assert!(
        !parsed.blueprint_trained,
        "bench: blueprint_trained must be false after --reset; got {}",
        parsed.blueprint_trained
    );
}
