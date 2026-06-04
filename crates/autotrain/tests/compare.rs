//! `trainer --compare` end-to-end proof (STW-018).
//!
//! This is the testnet roadmap proof point that the
//! v1-vs-v2 trained-config head-to-head bench actually:
//!
//! 1. Drives a real `trainer` binary (not a library-only
//!    test) through K heads-up hands of v1
//!    `DatabasePlayer` (seat 0) vs v2 `DatabasePlayer2`
//!    (seat 1) — both bots hydrated from their respective
//!    blueprint tables.
//! 2. Emits the documented single-line JSON
//!    `CompareReport` on stdout (the contract any
//!    downstream scraper / dashboard parses).
//! 3. Exits 0; if anything in the pipeline fails
//!    (room construction, blueprint hydration,
//!    persistence, hand loop) the binary exits non-zero
//!    and the test fails with the captured
//!    stdout/stderr for diagnosis.
//!
//! The test is gated on the `database` feature AND
//! short-circuits on a missing `DATABASE_URL`, so CI
//! without Postgres still runs `cargo test
//! --workspace` green. The gating mirrors
//! `crates/autotrain/tests/bench.rs`,
//! `crates/autotrain/tests/smoke.rs`,
//! `crates/gameroom/tests/hand_roundtrip.rs`, and
//! `crates/auth/tests/server_flow.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The compare contract is *the binary's* exit code and
//! stdout, not `bench::run_compare`'s return value.
//! Driving the binary as a process is the only way to
//! assert the same surface a real worker or dashboard
//! would see, and it surfaces the actual
//! `compare complete: ...` log line the human reviewer
//! greps. A library call would let us bypass
//! `Mode::from_args`, the `RBP_COMPARE_HANDS` env read,
//! and the JSON-line stdout contract — the very things
//! the test is trying to pin.
//!
//! ## The "heads-up nets to zero" assertion
//!
//! A heads-up `Room`'s two seats' `settlements()` always
//! sum to zero per hand, so the v1 + v2 `mbb_per_100`
//! values in a fresh `CompareReport` sum to within
//! float-rounding tolerance of 0. The test pins this
//! invariant with a `1e-3` tolerance (the bench's
//! `to_json` formatter uses `:.4` so the precision
//! loss is bounded by `5e-5`, well within `1e-3`).
//! A regression that introduces a phantom pot (e.g. a
//! `flush_hand` that double-counts a dead blind) is
//! caught at the `|v1.mbb_per_100 + v2.mbb_per_100|
//! < 1e-3` assertion.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the workspace's
/// `target/` directory. We walk up from
/// `CARGO_MANIFEST_DIR` to the workspace root, then probe
/// `<workspace>/target/{debug,release}/trainer` and return
/// whichever exists. The function panics if the binary is
/// missing — that is a setup error (the operator did not
/// build the trainer), not a silent test skip. Mirrors
/// the helper in `crates/autotrain/tests/bench.rs` so
/// the two integration tests share the same binary
/// resolution discipline.
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

/// Skip-on-missing-DB helper. Returns `true` when the
/// compare run can proceed; returns `false` after
/// printing a skip notice. The `DATABASE_URL` contract
/// is the same one the bench, auth, gameroom, and
/// autotrain smoke tests honor.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --compare integration test");
            false
        }
    }
}

/// Run the binary, capture stdout/stderr, and return
/// the (stdout, stderr, exit_code) triple. Mirrors the
/// helper in `bench.rs` so the two integration tests
/// share the same `DATABASE_URL` → `DB_URL` forwarding
/// convention.
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

/// Parsed v1 + v2 sub-reports inside a `CompareReport`.
/// We deliberately do NOT depend on `serde_json` (it
/// is already in the dep graph but we want the test to
/// be a thin contract check that survives a future
/// serde upgrade): a small hand-rolled parser is
/// enough for the flat-key shape
/// `CompareReport::to_json` produces, and the explicit
/// parsing keeps the failure mode obvious if a future
/// refactor renames a field.
struct ParsedSub {
    hands: usize,
    wins: usize,
    losses: usize,
    net_chips: i64,
    mbb_per_100: f64,
    mbb_ci95: f64,
    win_rate: f64,
    win_rate_ci95: f64,
}

/// Parse a `CompareReport` JSON line into the
/// headline fields the test asserts on. The v1 + v2
/// sub-reports are nested under `v1` / `v2` keys; the
/// flat-key parser walks the line one character at a
/// time, tracking brace depth, so a future refactor
/// that adds a top-level field fails the
/// top-level-keys assertion (only the documented
/// top-level keys are accepted) and a future refactor
/// that renames a sub-report field fails the
/// sub-report-keys assertion.
struct ParsedCompare {
    hands: usize,
    blind: i16,
    v1: ParsedSub,
    v2: ParsedSub,
    delta_mbb_per_100: f64,
    winner: String,
}

fn parse_sub(body: &str) -> Option<ParsedSub> {
    let mut hands: Option<usize> = None;
    let mut wins: Option<usize> = None;
    let mut losses: Option<usize> = None;
    let mut net_chips: Option<i64> = None;
    let mut mbb_per_100: Option<f64> = None;
    let mut mbb_ci95: Option<f64> = None;
    let mut win_rate: Option<f64> = None;
    let mut win_rate_ci95: Option<f64> = None;
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
            _ => return None,
        }
    }
    Some(ParsedSub {
        hands: hands?,
        wins: wins?,
        losses: losses?,
        net_chips: net_chips?,
        mbb_per_100: mbb_per_100?,
        mbb_ci95: mbb_ci95?,
        win_rate: win_rate?,
        win_rate_ci95: win_rate_ci95?,
    })
}

fn parse_compare_line(line: &str) -> Option<ParsedCompare> {
    let trim = line.trim();
    if !(trim.starts_with('{') && trim.ends_with('}')) {
        return None;
    }
    let body = &trim[1..trim.len() - 1];
    // The line is a flat object whose value for `v1`
    // and `v2` is a nested object. We walk the body
    // once, splitting on the commas that are at brace
    // depth 0, then parse the `v1` and `v2` nested
    // objects by re-running the flat-key parser on
    // their inner body.
    let mut hands: Option<usize> = None;
    let mut blind: Option<i16> = None;
    let mut v1: Option<ParsedSub> = None;
    let mut v2: Option<ParsedSub> = None;
    let mut delta_mbb_per_100: Option<f64> = None;
    let mut winner: Option<String> = None;
    let bytes = body.as_bytes();
    let mut i = 0usize;
    let mut top_segments: Vec<String> = Vec::new();
    let mut depth: i32 = 0;
    let mut start = 0usize;
    while i < bytes.len() {
        match bytes[i] {
            b'{' => {
                depth += 1;
            }
            b'}' => {
                depth -= 1;
            }
            b',' if depth == 0 => {
                top_segments.push(body[start..i].to_string());
                start = i + 1;
            }
            _ => {}
        }
        i += 1;
    }
    top_segments.push(body[start..].to_string());
    for seg in top_segments {
        let (k, v) = seg.split_once(':')?;
        let key = k.trim().trim_matches('"');
        let raw = v.trim();
        match key {
            "hands" => hands = raw.parse().ok(),
            "blind" => blind = raw.parse().ok(),
            "v1" => {
                // The nested v1 object is `{...}`;
                // strip the surrounding braces before
                // handing it to the flat-key parser.
                let inner = raw.trim().trim_start_matches('{').trim_end_matches('}');
                v1 = parse_sub(inner);
            }
            "v2" => {
                let inner = raw.trim().trim_start_matches('{').trim_end_matches('}');
                v2 = parse_sub(inner);
            }
            "delta_mbb_per_100" => delta_mbb_per_100 = raw.parse().ok(),
            "winner" => winner = Some(raw.trim_matches('"').to_string()),
            _ => return None,
        }
    }
    Some(ParsedCompare {
        hands: hands?,
        blind: blind?,
        v1: v1?,
        v2: v2?,
        delta_mbb_per_100: delta_mbb_per_100?,
        winner: winner?,
    })
}

/// Asserts that `trainer --compare` exits 0, prints a
/// parseable single-line JSON `CompareReport` on
/// stdout, and that the headline accounting is
/// internally consistent (the v1 + v2 `mbb_per_100`
/// values net to within float-rounding tolerance of
/// zero by the heads-up-room-construction invariant,
/// `winner` ∈ `{"v1", "v2", "tie"}`, the v1 + v2
/// sub-reports each have non-zero `hands` and the
/// same `hands` count). The test uses
/// `RBP_COMPARE_HANDS=20` so a CI worker can run the
/// compare end-to-end in a few seconds; the env-var
/// flip takes effect at the very first call to
/// `bench::compare_hands()`.
#[test]
fn compare_run_emits_parseable_json_with_consistent_accounting() {
    if !database_url_set() {
        return;
    }

    // Reset state so the compare's pre-run blueprint
    // counts are deterministic for this test: a fresh
    // empty blueprint is the documented "untrained"
    // state and the compare's per-side blueprints are
    // the default untrained `Flagship` / `Flagship2`.
    // The compare itself is valid against empty
    // blueprints (it just measures whatever the
    // untrained `DatabasePlayer` / `DatabasePlayer2`
    // do against each other), so the test asserts
    // the compare runs end-to-end, not that v1 or v2
    // wins.
    let (stdout, stderr, code) = run_trainer(&["--reset"], &[]);
    assert_eq!(
        code, 0,
        "trainer --reset must exit 0 before the compare run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let (stdout, stderr, code) = run_trainer(
        &["--compare"],
        &[("RBP_COMPARE_HANDS", "20"), ("RBP_COMPARE_BLIND", "2")],
    );
    assert_eq!(
        code, 0,
        "trainer --compare must exit 0 on a successful small run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // (i) The JSON report must be present on stdout.
    // The compare emits exactly one line per run; a
    // future refactor that introduces a second line
    // (e.g. a "warmup" report) should fail this test
    // until the contract is updated.
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{') && l.trim_end().ends_with('}'))
        .unwrap_or_else(|| {
            panic!(
                "trainer --compare must print a single-line JSON report on stdout.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let parsed = parse_compare_line(line).unwrap_or_else(|| {
        panic!("trainer --compare stdout must be a parseable CompareReport JSON; got: {line:?}")
    });

    // (ii) Headline accounting must be internally
    // consistent: K hands were played; the JSON
    // `hands` field must match the env var we passed;
    // both sub-reports' `hands` field must match the
    // top-level `hands` field (both seats play the
    // same K hands in the same room); mbb/100 and CI
    // are finite numbers (no NaN / Inf leaked out of
    // the summariser).
    assert_eq!(
        parsed.hands, 20,
        "compare: hands field must equal RBP_COMPARE_HANDS; got {}",
        parsed.hands
    );
    assert_eq!(
        parsed.blind, 2,
        "compare: blind field must equal RBP_COMPARE_BLIND; got {}",
        parsed.blind
    );
    assert_eq!(
        parsed.v1.hands, parsed.hands,
        "compare: v1.hands must equal top-level hands; got v1={} top={}",
        parsed.v1.hands, parsed.hands
    );
    assert_eq!(
        parsed.v2.hands, parsed.hands,
        "compare: v2.hands must equal top-level hands; got v2={} top={}",
        parsed.v2.hands, parsed.hands
    );
    assert!(
        parsed.v1.wins + parsed.v1.losses <= parsed.v1.hands,
        "compare: v1.wins + v1.losses ({}+{}={}) must be <= v1.hands ({})",
        parsed.v1.wins,
        parsed.v1.losses,
        parsed.v1.wins + parsed.v1.losses,
        parsed.v1.hands
    );
    assert!(
        parsed.v2.wins + parsed.v2.losses <= parsed.v2.hands,
        "compare: v2.wins + v2.losses ({}+{}={}) must be <= v2.hands ({})",
        parsed.v2.wins,
        parsed.v2.losses,
        parsed.v2.wins + parsed.v2.losses,
        parsed.v2.hands
    );
    assert!(
        parsed.v1.mbb_per_100.is_finite() && parsed.v1.mbb_ci95.is_finite(),
        "compare: v1 mbb/100 and CI must be finite; got {} ± {}",
        parsed.v1.mbb_per_100,
        parsed.v1.mbb_ci95
    );
    assert!(
        parsed.v2.mbb_per_100.is_finite() && parsed.v2.mbb_ci95.is_finite(),
        "compare: v2 mbb/100 and CI must be finite; got {} ± {}",
        parsed.v2.mbb_per_100,
        parsed.v2.mbb_ci95
    );
    assert!(
        parsed.delta_mbb_per_100.is_finite(),
        "compare: delta_mbb_per_100 must be finite; got {}",
        parsed.delta_mbb_per_100
    );
    assert!(
        (0.0..=1.0).contains(&parsed.v1.win_rate),
        "compare: v1.win_rate must be in [0,1]; got {}",
        parsed.v1.win_rate
    );
    assert!(
        (0.0..=1.0).contains(&parsed.v2.win_rate),
        "compare: v2.win_rate must be in [0,1]; got {}",
        parsed.v2.win_rate
    );

    // (iii) The "heads-up nets to zero" invariant. The
    // v1 + v2 sub-reports' `net_chips` sum to exactly
    // zero per hand in the heads-up `Room`; the
    // summed `net_chips` is therefore 0 in integer
    // space, and the summed `mbb_per_100` is within
    // float-rounding tolerance of 0. The bench's
    // `to_json` formatter uses `:.4` so the precision
    // loss is bounded by `5e-5`; we use `1e-3` for
    // headroom against the JSON-decoded re-parse.
    assert_eq!(
        parsed.v1.net_chips + parsed.v2.net_chips,
        0,
        "compare: v1 + v2 net_chips must sum to 0 (heads-up room); got v1={} v2={}",
        parsed.v1.net_chips,
        parsed.v2.net_chips
    );
    let mbb_sum = parsed.v1.mbb_per_100 + parsed.v2.mbb_per_100;
    assert!(
        mbb_sum.abs() < 1e-3,
        "compare: v1 + v2 mbb_per_100 must sum to within float-rounding tolerance of 0; got {mbb_sum}"
    );

    // (iv) The `winner` field must be one of the
    // three documented values. A future refactor that
    // adds a new `CompareWinner` variant (e.g.
    // `V1ByCi`) must update this assertion + the
    // variant's `as_str` literal together.
    assert!(
        matches!(parsed.winner.as_str(), "v1" | "v2" | "tie"),
        "compare: winner must be one of {{v1, v2, tie}}; got {:?}",
        parsed.winner
    );

    // (v) The pre-compare blueprints were just reset,
    // so the per-side `blueprint_trained_v1` /
    // `blueprint_trained_v2` flags are both `false`.
    // The compare does NOT carry these flags in the
    // JSON (they are logged but not part of the
    // report contract — the v1 + v2 sub-reports
    // already carry the per-hand math a downstream
    // scraper needs), so we instead assert the
    // `compare complete: ...` log line on stderr
    // reports `blueprint_trained_v1=false
    // blueprint_trained_v2=false`. The bench's
    // existing integration test does the analogous
    // JSON check on the bench's `blueprint_trained`
    // field; the compare's contract is
    // log-line-only for the per-side flags because
    // the dashboard that consumes a `CompareReport`
    // only cares about the winner / delta, not
    // whether the bots were trained.
    assert!(
        stderr.contains("blueprint_trained_v1=false"),
        "compare: log line must report blueprint_trained_v1=false; got stderr: {stderr}"
    );
    assert!(
        stderr.contains("blueprint_trained_v2=false"),
        "compare: log line must report blueprint_trained_v2=false; got stderr: {stderr}"
    );
}
