//! `trainer --compare3` end-to-end proof (STW-031).
//!
//! This is the testnet roadmap proof point that the
//! v1-vs-v2-vs-v3 trained-config three-way compare
//! actually:
//!
//! 1. Drives a real `trainer` binary (not a
//!    library-only test) through three pairwise
//!    heads-up rotations of the v1 / v2 / v3
//!    `DatabasePlayer` / `DatabasePlayer2` /
//!    `DatabasePlayer3` — K hands per pair, each
//!    config plays both seat 0 and seat 1 across
//!    the three rotations so the per-config
//!    `mbb_per_100` is unbiased by seat position.
//! 2. Emits the documented single-line JSON
//!    `Compare3Report` on stdout (the contract
//!    any downstream scraper / dashboard parses).
//! 3. Exits 0; if anything in the pipeline fails
//!    (room construction, blueprint hydration,
//!    persistence, hand loop) the binary exits
//!    non-zero and the test fails with the
//!    captured stdout/stderr for diagnosis.
//!
//! The test is gated on the `database` feature
//! AND short-circuits on a missing `DATABASE_URL`,
//! so CI without Postgres still runs
//! `cargo test --workspace` green. The gating
//! mirrors `crates/autotrain/tests/compare.rs`,
//! `crates/autotrain/tests/bench.rs`,
//! `crates/autotrain/tests/smoke.rs`,
//! `crates/gameroom/tests/hand_roundtrip.rs`, and
//! `crates/auth/tests/server_flow.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The compare3 contract is *the binary's* exit
//! code and stdout, not `bench::run_compare3`'s
//! return value. Driving the binary as a process
//! is the only way to assert the same surface a
//! real worker or dashboard would see, and it
//! surfaces the actual `compare3 complete: ...`
//! log line the human reviewer greps. A library
//! call would let us bypass `Mode::from_args`,
//! the `RBP_COMPARE3_HANDS` env read, and the
//! JSON-line stdout contract — the very things
//! the test is trying to pin.
//!
//! ## The "each pair nets to zero" assertion
//!
//! A heads-up `Room`'s two seats' `settlements()`
//! always sum to zero per hand, so the per-pair
//! `p0 + p1` integer sum is 0 across all three
//! pairs. The per-config mbb/100 sum-to-zero
//! invariant (v1 + v2 + v3 mbb/100 = 0 by the
//! per-pair zero-sum + per-config aggregate)
//! is the float-axis pin the test asserts on
//! the parsed JSON report.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the
/// workspace's `target/` directory. We walk up
/// from `CARGO_MANIFEST_DIR` to the workspace
/// root, then probe
/// `<workspace>/target/{debug,release}/trainer`
/// and return whichever exists. The function
/// panics if the binary is missing — that is a
/// setup error (the operator did not build the
/// trainer), not a silent test skip. Mirrors
/// the helper in `crates/autotrain/tests/compare.rs`
/// and `crates/autotrain/tests/bench.rs` so the
/// three integration tests share the same
/// binary resolution discipline.
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

/// Skip-on-missing-DB helper. Returns `true` when
/// the compare3 run can proceed; returns `false`
/// after printing a skip notice. The
/// `DATABASE_URL` contract is the same one the
/// bench, auth, gameroom, and autotrain smoke
/// tests honor.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer --compare3 integration test");
            false
        }
    }
}

/// Run the binary, capture stdout/stderr, and
/// return the (stdout, stderr, exit_code) triple.
/// Mirrors the helper in `compare.rs` and
/// `bench.rs` so the three integration tests
/// share the same `DATABASE_URL` → `DB_URL`
/// forwarding convention.
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

/// Parsed per-config sub-report inside a
/// `Compare3Report`. We deliberately do NOT
/// depend on `serde_json` (it is already in the
/// dep graph but we want the test to be a thin
/// contract check that survives a future serde
/// upgrade): a small hand-rolled parser is
/// enough for the flat-key shape
/// `Compare3Report::to_json` produces, and the
/// explicit parsing keeps the failure mode
/// obvious if a future refactor renames a
/// field.
#[derive(Debug)]
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

/// Parse a `Compare3SubReport` JSON object
/// body into the headline fields the test
/// asserts on.
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

/// Parse a `Compare3Report` JSON line into the
/// headline fields the test asserts on. The
/// `v1` / `v2` / `v3` sub-reports are nested
/// under the `v1` / `v2` / `v3` keys; the
/// flat-key parser walks the line one
/// character at a time, tracking brace depth,
/// so a future refactor that adds a top-level
/// field fails the top-level-keys assertion
/// (only the documented top-level keys are
/// accepted) and a future refactor that
/// renames a sub-report field fails the
/// sub-report-keys assertion.
#[derive(Debug)]
struct ParsedCompare3 {
    hands_per_pair: usize,
    blind: i16,
    v1: ParsedSub,
    v2: ParsedSub,
    v3: ParsedSub,
    v1_v2_delta: f64,
    v2_v3_delta: f64,
    v3_v1_delta: f64,
    ranked_winner: String,
}

fn parse_compare3_line(line: &str) -> Option<ParsedCompare3> {
    let trim = line.trim();
    if !(trim.starts_with('{') && trim.ends_with('}')) {
        return None;
    }
    let body = &trim[1..trim.len() - 1];
    let mut hands_per_pair: Option<usize> = None;
    let mut blind: Option<i16> = None;
    let mut v1: Option<ParsedSub> = None;
    let mut v2: Option<ParsedSub> = None;
    let mut v3: Option<ParsedSub> = None;
    let mut v1_v2_delta: Option<f64> = None;
    let mut v2_v3_delta: Option<f64> = None;
    let mut v3_v1_delta: Option<f64> = None;
    let mut ranked_winner: Option<String> = None;
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
            "hands_per_pair" => hands_per_pair = raw.parse().ok(),
            "blind" => blind = raw.parse().ok(),
            "v1" => {
                let inner = raw.trim().trim_start_matches('{').trim_end_matches('}');
                v1 = parse_sub(inner);
            }
            "v2" => {
                let inner = raw.trim().trim_start_matches('{').trim_end_matches('}');
                v2 = parse_sub(inner);
            }
            "v3" => {
                let inner = raw.trim().trim_start_matches('{').trim_end_matches('}');
                v3 = parse_sub(inner);
            }
            "v1_v2_delta" => v1_v2_delta = raw.parse().ok(),
            "v2_v3_delta" => v2_v3_delta = raw.parse().ok(),
            "v3_v1_delta" => v3_v1_delta = raw.parse().ok(),
            "ranked_winner" => ranked_winner = Some(raw.trim_matches('"').to_string()),
            _ => return None,
        }
    }
    Some(ParsedCompare3 {
        hands_per_pair: hands_per_pair?,
        blind: blind?,
        v1: v1?,
        v2: v2?,
        v3: v3?,
        v1_v2_delta: v1_v2_delta?,
        v2_v3_delta: v2_v3_delta?,
        v3_v1_delta: v3_v1_delta?,
        ranked_winner: ranked_winner?,
    })
}

/// Asserts that `trainer --compare3` exits 0,
/// prints a parseable single-line JSON
/// `Compare3Report` on stdout, and that the
/// headline accounting is internally
/// consistent (the per-config mbb/100 sum to
/// within float-rounding tolerance of zero by
/// the per-pair zero-sum + per-config aggregate
/// invariant, `ranked_winner` ∈
/// `{"v1", "v2", "v3", "tie"}`, the v1 / v2 / v3
/// sub-reports each have non-zero `hands` and
/// the same `hands` count = 2 * K). The test
/// uses `RBP_COMPARE3_HANDS=8` so a CI worker
/// can run the compare3 end-to-end (3 pairs × 8
/// hands = 24 hands total) in a few seconds; the
/// env-var flip takes effect at the very first
/// call to `bench::compare3_hands()`.
#[test]
fn compare3_run_emits_parseable_json_with_consistent_accounting() {
    if !database_url_set() {
        return;
    }

    // Reset state so the compare3's pre-run
    // blueprint counts are deterministic for this
    // test: a fresh empty blueprint is the
    // documented "untrained" state and the
    // compare3's per-config blueprints are the
    // default untrained `Flagship` /
    // `Flagship2` / `Flagship3`. The compare3
    // itself is valid against empty blueprints
    // (it just measures whatever the untrained
    // `DatabasePlayer` / `DatabasePlayer2` /
    // `DatabasePlayer3` do against each other),
    // so the test asserts the compare3 runs
    // end-to-end, not that any one config wins.
    let (stdout, stderr, code) = run_trainer(&["--reset"], &[]);
    assert_eq!(
        code, 0,
        "trainer --reset must exit 0 before the compare3 run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    let (stdout, stderr, code) = run_trainer(
        &["--compare3"],
        &[("RBP_COMPARE3_HANDS", "8"), ("RBP_COMPARE3_BLIND", "2")],
    );
    assert_eq!(
        code, 0,
        "trainer --compare3 must exit 0 on a successful small run.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // (i) The JSON report must be present on
    // stdout. The compare3 emits exactly one
    // line per run; a future refactor that
    // introduces a second line (e.g. a
    // "warmup" report) should fail this test
    // until the contract is updated.
    let line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{') && l.trim_end().ends_with('}'))
        .unwrap_or_else(|| {
            panic!(
                "trainer --compare3 must print a single-line JSON report on stdout.\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let parsed = parse_compare3_line(line).unwrap_or_else(|| {
        panic!("trainer --compare3 stdout must be a parseable Compare3Report JSON; got: {line:?}")
    });

    // (ii) Headline accounting must be
    // internally consistent: K hands were
    // played per pair; the JSON
    // `hands_per_pair` field must match the
    // env var we passed; the v1 / v2 / v3
    // sub-reports' `hands` field must equal
    // `2 * K` (each config plays both seat
    // 0 once and seat 1 once across the
    // three pairs); mbb/100 and CI are
    // finite numbers (no NaN / Inf leaked
    // out of the summariser).
    assert_eq!(
        parsed.hands_per_pair, 8,
        "compare3: hands_per_pair field must equal RBP_COMPARE3_HANDS; got {}",
        parsed.hands_per_pair
    );
    assert_eq!(
        parsed.blind, 2,
        "compare3: blind field must equal RBP_COMPARE3_BLIND; got {}",
        parsed.blind
    );
    assert_eq!(
        parsed.v1.hands,
        2 * parsed.hands_per_pair,
        "compare3: v1.hands must equal 2 * hands_per_pair; got v1={} hands_per_pair={}",
        parsed.v1.hands,
        parsed.hands_per_pair
    );
    assert_eq!(
        parsed.v2.hands,
        2 * parsed.hands_per_pair,
        "compare3: v2.hands must equal 2 * hands_per_pair; got v2={} hands_per_pair={}",
        parsed.v2.hands,
        parsed.hands_per_pair
    );
    assert_eq!(
        parsed.v3.hands,
        2 * parsed.hands_per_pair,
        "compare3: v3.hands must equal 2 * hands_per_pair; got v3={} hands_per_pair={}",
        parsed.v3.hands,
        parsed.hands_per_pair
    );
    assert!(
        parsed.v1.wins + parsed.v1.losses <= parsed.v1.hands,
        "compare3: v1.wins + v1.losses ({}+{}={}) must be <= v1.hands ({})",
        parsed.v1.wins,
        parsed.v1.losses,
        parsed.v1.wins + parsed.v1.losses,
        parsed.v1.hands
    );
    assert!(
        parsed.v2.wins + parsed.v2.losses <= parsed.v2.hands,
        "compare3: v2.wins + v2.losses ({}+{}={}) must be <= v2.hands ({})",
        parsed.v2.wins,
        parsed.v2.losses,
        parsed.v2.wins + parsed.v2.losses,
        parsed.v2.hands
    );
    assert!(
        parsed.v3.wins + parsed.v3.losses <= parsed.v3.hands,
        "compare3: v3.wins + v3.losses ({}+{}={}) must be <= v3.hands ({})",
        parsed.v3.wins,
        parsed.v3.losses,
        parsed.v3.wins + parsed.v3.losses,
        parsed.v3.hands
    );
    assert!(
        parsed.v1.mbb_per_100.is_finite() && parsed.v1.mbb_ci95.is_finite(),
        "compare3: v1 mbb/100 and CI must be finite; got {} ± {}",
        parsed.v1.mbb_per_100,
        parsed.v1.mbb_ci95
    );
    assert!(
        parsed.v2.mbb_per_100.is_finite() && parsed.v2.mbb_ci95.is_finite(),
        "compare3: v2 mbb/100 and CI must be finite; got {} ± {}",
        parsed.v2.mbb_per_100,
        parsed.v2.mbb_ci95
    );
    assert!(
        parsed.v3.mbb_per_100.is_finite() && parsed.v3.mbb_ci95.is_finite(),
        "compare3: v3 mbb/100 and CI must be finite; got {} ± {}",
        parsed.v3.mbb_per_100,
        parsed.v3.mbb_ci95
    );
    assert!(
        parsed.v1_v2_delta.is_finite()
            && parsed.v2_v3_delta.is_finite()
            && parsed.v3_v1_delta.is_finite(),
        "compare3: all three pairwise deltas must be finite; got v1_v2={} v2_v3={} v3_v1={}",
        parsed.v1_v2_delta,
        parsed.v2_v3_delta,
        parsed.v3_v1_delta
    );
    assert!(
        (0.0..=1.0).contains(&parsed.v1.win_rate),
        "compare3: v1.win_rate must be in [0,1]; got {}",
        parsed.v1.win_rate
    );
    assert!(
        (0.0..=1.0).contains(&parsed.v2.win_rate),
        "compare3: v2.win_rate must be in [0,1]; got {}",
        parsed.v2.win_rate
    );
    assert!(
        (0.0..=1.0).contains(&parsed.v3.win_rate),
        "compare3: v3.win_rate must be in [0,1]; got {}",
        parsed.v3.win_rate
    );

    // (iii) The per-config mbb/100 sum-to-zero
    // invariant: each config's aggregate
    // net_chips is the seat-0 PnL of one
    // pair plus the seat-1 PnL of another.
    // Per the per-pair zero-sum invariant,
    // the three per-config mbb/100 values
    // sum to within float-rounding tolerance
    // of 0. The bench's `to_json` formatter
    // uses `:.4` so the precision loss is
    // bounded by `5e-5`; we use `1e-3` for
    // headroom against the JSON-decoded
    // re-parse.
    let mbb_sum = parsed.v1.mbb_per_100 + parsed.v2.mbb_per_100 + parsed.v3.mbb_per_100;
    assert!(
        mbb_sum.abs() < 1e-3,
        "compare3: v1 + v2 + v3 mbb/100 must sum to within float-rounding tolerance of 0; got {mbb_sum}"
    );
    // (iii.b) The per-config net_chips
    // sum-to-zero integer-axis pin: the
    // three `net_chips` integers (the raw
    // chip aggregate each config earned
    // across its two seats) must sum to
    // exactly zero, because each pairwise
    // heads-up `Room` is a two-seat
    // zero-sum game. The JSON formatter
    // prints net_chips as an integer so
    // the equality is exact (no float
    // rounding can leak in).
    let net_sum = parsed.v1.net_chips + parsed.v2.net_chips + parsed.v3.net_chips;
    assert_eq!(
        net_sum, 0,
        "compare3: v1 + v2 + v3 net_chips must sum to exactly 0 (per-pair zero-sum); got {net_sum}"
    );
    // (iii.c) The per-config win_rate_ci95
    // field must be a non-negative finite
    // value (it is a half-width of a
    // 95% confidence interval, so a
    // negative number would be a
    // summariser bug).
    assert!(
        parsed.v1.win_rate_ci95.is_finite() && parsed.v1.win_rate_ci95 >= 0.0,
        "compare3: v1 win_rate_ci95 must be a non-negative finite value; got {}",
        parsed.v1.win_rate_ci95
    );
    assert!(
        parsed.v2.win_rate_ci95.is_finite() && parsed.v2.win_rate_ci95 >= 0.0,
        "compare3: v2 win_rate_ci95 must be a non-negative finite value; got {}",
        parsed.v2.win_rate_ci95
    );
    assert!(
        parsed.v3.win_rate_ci95.is_finite() && parsed.v3.win_rate_ci95 >= 0.0,
        "compare3: v3 win_rate_ci95 must be a non-negative finite value; got {}",
        parsed.v3.win_rate_ci95
    );

    // (iv) The `ranked_winner` field must be
    // one of the four documented values. A
    // future refactor that adds a new
    // `Compare3Winner` variant (e.g.
    // `V1ByCi`) must update this assertion
    // + the variant's `as_str` literal
    // together.
    assert!(
        matches!(parsed.ranked_winner.as_str(), "v1" | "v2" | "v3" | "tie"),
        "compare3: ranked_winner must be one of {{v1, v2, v3, tie}}; got {:?}",
        parsed.ranked_winner
    );

    // (v) The pre-compare3 blueprints were
    // just reset, so the per-config
    // `blueprint_trained_v1` /
    // `blueprint_trained_v2` /
    // `blueprint_trained_v3` flags are all
    // `false`. The compare3 does NOT carry
    // these flags in the JSON (they are
    // logged but not part of the report
    // contract — the v1 / v2 / v3 sub-reports
    // already carry the per-hand math a
    // downstream scraper needs), so we
    // instead assert the
    // `compare3 complete: ...` log line on
    // stderr reports all three flags as
    // `false`. The bench's existing
    // integration test does the analogous
    // check on the bench's
    // `blueprint_trained` field; the
    // compare3's contract is log-line-only
    // for the per-side flags because the
    // dashboard that consumes a
    // `Compare3Report` only cares about the
    // ranked winner / per-config mbb/100,
    // not whether the bots were trained.
    assert!(
        stderr.contains("blueprint_trained_v1=false"),
        "compare3: log line must report blueprint_trained_v1=false; got stderr: {stderr}"
    );
    assert!(
        stderr.contains("blueprint_trained_v2=false"),
        "compare3: log line must report blueprint_trained_v2=false; got stderr: {stderr}"
    );
    assert!(
        stderr.contains("blueprint_trained_v3=false"),
        "compare3: log line must report blueprint_trained_v3=false; got stderr: {stderr}"
    );
}
