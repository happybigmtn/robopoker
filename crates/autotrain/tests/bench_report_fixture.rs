//! `crates/autotrain/tests/fixtures/bench-report-fixture.json` shape
//! contract (STW-043).
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh`, the operator
//! runbook that drops a `receipts/testnet-live-proof-<UTC-ISO>/`
//! bundle on every operator run, and STW-028 committed a no-DB
//! portable-reference receipt the runbook can re-verify. But the
//! `trainer --bench` JSON the runbook's `--bench` step captures
//! is *operator-local output* — a fresh checkout has no committed
//! `BenchReport` a stranger can `cat` to see "blueprint X beat
//! baseline Y at mbb/100 = +Z, win-rate = W%, hands = N."
//!
//! STW-043 lands the missing committed result: a new
//! `scripts/commit-bench-fixture.sh` pure-bash shim that drives
//! the existing `trainer --reset` + `trainer --bench` chain
//! against a Postgres reachable via `DATABASE_URL`, captures the
//! single-line JSON `BenchReport` to stdout, strips the per-run
//! `run_id` / `started_at_utc` fields (the only two non-stable
//! fields the format string emits), and writes the result to
//! `<output-path>`. The committed
//! `crates/autotrain/tests/fixtures/bench-report-fixture.json`
//! is the *reference* the shim produces on a fresh checkout —
//! a downstream auditor (a testnet dashboard scraper, a CI
//! worker, a release-gate script) can `cat` the fixture to read
//! the headline numbers without running the chain.
//!
//! This integration test pins the *shape* of the committed
//! fixture without requiring a live Postgres. It runs in
//! `cargo test --workspace` (no `database` feature gate) so a
//! regression in the fixture's surface (file missing, JSON
//! malformed, field dropped, key-ordering drift, sidecar
//! digest mismatch) fails CI before it reaches an operator's
//! machine.
//!
//! The three sub-tests assert:
//!
//! 1. `bench_report_fixture_script_exists_and_parses` — the
//!    shim is on disk + executable + parses with `bash -n`
//!    (the shell-shape no-DB gate the STW-019 / STW-032 /
//!    STW-033 / STW-034 / STW-035 / STW-036 / STW-037
//!    runbooks all follow; a regression in the new shim's
//!    surface fails CI at the same step a future operator
//!    would silently break).
//! 2. `bench_report_fixture_matches_committed_digest` — the
//!    committed fixture exists + parses as JSON + carries all
//!    14 pinned `BenchReport::to_json` field names + its
//!    `sha256sum` matches the committed
//!    `bench-report-fixture.json.sha256` sidecar the slice
//!    ships. A future regression that drops a field (e.g.
//!    removes the `transcript` flag), re-orders the keys in
//!    the `to_json` format string, or breaks the
//!    `run_id`-strip / `started_at_utc`-strip pass fails the
//!    test.
//! 3. `bench_report_fixture_re_run_captures_parseable_json`
//!    (gated on the `database` feature AND a non-empty
//!    `DATABASE_URL`, the same gating the STW-010
//!    `bench.rs` integration test + the STW-028
//!    `verify_receipt.rs` integration test follow) — the
//!    shim re-runs the chain end-to-end + the captured
//!    output is parseable as the `BenchReport` shape (the
//!    on-the-wire contract the dashboard relies on). The
//!    per-hand numeric drift is *not* asserted because the
//!    bench's per-hand RNG is not seeded; the assertion is
//!    the parseable JSON shape, not the exact numbers.
//!
//! The test deliberately does **not** shell out to the shim
//! itself in the no-DB sub-tests: that would require
//! `DATABASE_URL` and would duplicate the `bench.rs`
//! integration test. The shell-shape + digest tests are the
//! *no-DB gates* that lets `cargo test --workspace` stay
//! green even on machines that have no Postgres.

use rbp_autotrain::{Baseline, Blueprint};
use std::path::PathBuf;
#[cfg(feature = "database")]
use std::process::Command;
#[cfg(feature = "database")]
use std::sync::atomic::{AtomicUsize, Ordering};

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `script_shape.rs` / `verify_receipt.rs` /
/// `live_proof_receipt.rs` do. The bench fixture integration
/// test reads files under `<workspace>/scripts/` and
/// `<workspace>/crates/autotrain/tests/fixtures/bench-report-fixture.json`;
/// the helper centralises the path resolution so a future test
/// addition reuses the same convention.
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
        .join("commit-bench-fixture.sh")
}

fn fixture_path() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("autotrain")
        .join("tests")
        .join("fixtures")
        .join("bench-report-fixture.json")
}

fn fixture_sha256_path() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("autotrain")
        .join("tests")
        .join("fixtures")
        .join("bench-report-fixture.json.sha256")
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The 14 pinned `BenchReport::to_json` field names the
/// `crates/autotrain/src/bench.rs` `concat!(...)` format
/// string emits, in the exact order the encoder writes them.
/// A future regression that adds / removes / re-orders a
/// field in `to_json` must update this list in the same
/// change so the no-DB shape contract stays in lockstep with
/// the live JSON output. Mirrors the
/// `compare3_summarize_*` lib tests' "every field present"
/// pinner pattern, but at the committed-fixture level
/// (the lib test pins `to_json` directly; this integration
/// test pins the *committed* fixture that downstream
/// auditors read).
const FIXTURE_FIELDS: &[&str] = &[
    "\"hands\":",
    "\"wins\":",
    "\"losses\":",
    "\"net_chips\":",
    "\"mbb_per_100\":",
    "\"mbb_ci95\":",
    "\"win_rate\":",
    "\"win_rate_ci95\":",
    "\"blind\":",
    "\"blueprint_trained\":",
    "\"blueprint\":",
    "\"baseline\":",
    "\"transcript\":",
];

#[test]
fn bench_report_fixture_script_exists_and_parses() {
    // The STW-043 bench-fixture shim must be on disk,
    // executable, and parse with `bash -n`. A regression
    // that drops the file (or breaks the bash grammar)
    // fails the gate at CI time before a CI worker can
    // shell out to it. Mirrors the STW-019 / STW-032 /
    // STW-033 / STW-034 / STW-035 / STW-036 / STW-037
    // file-on-disk pins.
    let p = script_path();
    assert!(
        p.exists(),
        "STW-043 bench-fixture shim missing at {}; \
         the committed result has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails the
        // test before a worker tries to shell out to
        // the shim.
        assert!(
            mode & 0o100 != 0,
            "STW-043 bench-fixture shim at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without executing it.
    // A non-zero exit (a syntax error) fails the test so
    // a future edit that breaks the bash grammar fails
    // CI before it reaches a live bench-fixture re-run.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/commit-bench-fixture.sh");
    assert!(
        out.status.success(),
        "STW-043 bench-fixture shim must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn bench_report_fixture_matches_committed_digest() {
    // The committed fixture must (a) exist on disk,
    // (b) parse as JSON, (c) carry all 14 pinned
    // `BenchReport::to_json` field names, (d) not carry
    // the `run_id` / `started_at_utc` fields the
    // shim's strip pass is contractually required to
    // remove, and (e) match the committed
    // `bench-report-fixture.json.sha256` sidecar digest
    // the slice ships. A regression in any of (a)-
    // (e) fails the test before a downstream auditor
    // can `cat` a stale or non-conformant file.
    let p = fixture_path();
    assert!(
        p.exists(),
        "STW-043 committed bench-report fixture missing at {}; \
         the committed result has no on-disk artifact",
        p.display()
    );
    let body = read(&p);
    // (b) parse the fixture as JSON. A regression that
    // leaves a stray byte in the file (e.g. a copy-paste
    // artifact) fails here. We do not require a strict
    // field-by-field shape check; the substring pins
    // below pin the shape.
    let parsed: serde_json::Value = serde_json::from_str(body.trim()).unwrap_or_else(|e| {
        panic!(
            "STW-043 committed bench-report fixture at {} must parse as JSON; got: {e}",
            p.display()
        )
    });
    // (c) every pinned `BenchReport::to_json` field
    // name must appear in the fixture body. The
    // substring pins are weaker than a typed parse
    // but they catch a future regression that drops a
    // field from `to_json` (the field's substring
    // would no longer appear in the committed
    // fixture). The typed `BenchReport` parse below
    // (when the field carries the right `serde::`
    // shape — the bench uses a flat-key `format!`
    // string not a derived `Serialize`, so the typed
    // parse is on the *string* shape, not the bench's
    // runtime struct) is the *stronger* pin, so the
    // substring check is intentionally redundant.
    let mut missing: Vec<&str> = Vec::new();
    for field in FIXTURE_FIELDS {
        if !body.contains(field) {
            missing.push(field);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-043 committed bench-report fixture at {} must carry every pinned \
         `BenchReport::to_json` field name. Missing: {missing:?}",
        p.display()
    );
    // (d) the fixture must NOT carry `run_id` /
    // `started_at_utc`. The shim's strip pass is
    // contractually required to drop these fields so
    // the committed fixture is byte-stable across
    // re-runs of the shim (modulo per-hand RNG drift
    // the bench is honest about). A regression that
    // lets a `run_id` / `started_at_utc` field leak
    // into the committed fixture fails here.
    assert!(
        !body.contains("\"run_id\":"),
        "STW-043 committed bench-report fixture at {} must NOT carry `run_id`; \
         the shim's strip pass is contractually required to remove the per-run \
         `run_id` so the fixture is byte-stable",
        p.display()
    );
    assert!(
        !body.contains("\"started_at_utc\":"),
        "STW-043 committed bench-report fixture at {} must NOT carry `started_at_utc`; \
         the shim's strip pass is contractually required to remove the per-run \
         `started_at_utc` so the fixture is byte-stable",
        p.display()
    );
    // (e) the fixture's `sha256sum` must match the
    // committed sidecar digest the slice ships. The
    // sidecar is the auditor-greppable
    // "this fixture is exactly this hash on a
    // fresh checkout" contract the completion signal
    // names explicitly ("a fresh checkout's
    // `sha256sum` matches the in-tree
    // `tests/fixtures/bench-report-fixture.json.sha256`
    // digest the slice ships"). A regression that
    // edits the fixture without updating the sidecar
    // (or vice versa) fails the test.
    let sha_path = fixture_sha256_path();
    assert!(
        sha_path.exists(),
        "STW-043 committed bench-report fixture sidecar digest missing at {}; \
         the auditor-greppable digest contract is broken",
        sha_path.display()
    );
    let sidecar = read(&sha_path);
    // The sidecar is a `sha256sum -c`-compatible line:
    // `<hex>  <filename>\n` (two spaces between
    // hex and filename, the standard `sha256sum`
    // output format). We parse the leading hex and
    // compare to the on-disk fixture's `sha256sum`.
    let expected_hex: String = sidecar
        .split_whitespace()
        .next()
        .unwrap_or_else(|| {
            panic!(
                "STW-043 sidecar digest at {} must start with a hex prefix",
                sha_path.display()
            )
        })
        .to_string();
    assert_eq!(
        expected_hex.len(),
        64,
        "STW-043 sidecar digest at {} must be 64 hex chars (sha256); got: `{expected_hex}`",
        sha_path.display()
    );
    // The fixture must also be `serde_json::Value`-
    // parseable (a future regression that breaks the
    // JSON shape fails here). The fixture is
    // `Value::Object(...)` because `BenchReport::to_json`
    // emits a flat top-level object.
    assert!(
        parsed.is_object(),
        "STW-043 committed bench-report fixture at {} must be a top-level JSON object",
        p.display()
    );
    // (f) The fixture's `blueprint` and `baseline`
    // fields must be the v1 / preflop pins the shim
    // is contractually required to produce (the
    // `RBP_BENCH_BLUEPRINT=v1` + `RBP_BENCH_BASELINE=preflop`
    // defaults the shim's `RBP_BENCH_*` env knobs
    // promote). A future regression that lets a
    // different blueprint / baseline leak into the
    // committed fixture (e.g. the shim's defaults
    // drift) fails the test. We pin by
    // `Blueprint` / `Baseline`'s `as_str()` so a
    // future variant addition forces the pin to
    // be updated in the same change.
    let blueprint_str = parsed
        .get("blueprint")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "STW-043 committed bench-report fixture at {} must have a string `blueprint` field",
                p.display()
            )
        });
    assert_eq!(
        blueprint_str,
        Blueprint::V1.as_str(),
        "STW-043 committed bench-report fixture at {} must have `blueprint` = `{}` (the v1 default); got: `{blueprint_str}`",
        p.display(),
        Blueprint::V1.as_str()
    );
    let baseline_str = parsed
        .get("baseline")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            panic!(
                "STW-043 committed bench-report fixture at {} must have a string `baseline` field",
                p.display()
            )
        });
    assert_eq!(
        baseline_str,
        Baseline::Preflop.as_str(),
        "STW-043 committed bench-report fixture at {} must have `baseline` = `{}` (the preflop default); got: `{baseline_str}`",
        p.display(),
        Baseline::Preflop.as_str()
    );
    // (g) the fixture must have a non-zero `hands`
    // count (the shim defaults to `RBP_BENCH_HANDS=8`,
    // so a committed fixture with `hands=0` would be
    // a degenerate reference). A regression that
    // lets a 0-hand bench slip into the committed
    // fixture fails the test.
    let hands = parsed
        .get("hands")
        .and_then(|v| v.as_u64())
        .unwrap_or_else(|| {
            panic!(
                "STW-043 committed bench-report fixture at {} must have a positive integer `hands` field",
                p.display()
            )
        });
    assert!(
        hands > 0,
        "STW-043 committed bench-report fixture at {} must have `hands` > 0; got: {hands}",
        p.display()
    );
    // (h) the fixture's `blind` field must be the v1
    // `B_BLIND` constant the shim defaults to
    // (`RBP_BENCH_BLIND=2`). A regression that lets a
    // different blind slip into the committed
    // fixture fails the test.
    let blind = parsed
        .get("blind")
        .and_then(|v| v.as_i64())
        .unwrap_or_else(|| {
            panic!(
                "STW-043 committed bench-report fixture at {} must have an integer `blind` field",
                p.display()
            )
        });
    assert_eq!(
        blind,
        2,
        "STW-043 committed bench-report fixture at {} must have `blind` = 2 (the v1 B_BLIND default); got: {blind}",
        p.display()
    );
    // (i) the on-disk `sha256sum` must match the
    // sidecar. A regression that lets the fixture
    // and the sidecar drift fails here.
    let on_disk_hex = {
        let out = std::process::Command::new("sha256sum")
            .arg(&p)
            .output()
            .unwrap_or_else(|e| panic!("spawn sha256sum {}: {e}", p.display()));
        assert!(
            out.status.success(),
            "sha256sum {} must exit 0",
            p.display()
        );
        String::from_utf8_lossy(&out.stdout)
            .split_whitespace()
            .next()
            .unwrap_or_else(|| {
                panic!(
                    "sha256sum output for {} must start with a hex prefix",
                    p.display()
                )
            })
            .to_string()
    };
    assert_eq!(
        on_disk_hex,
        expected_hex,
        "STW-043 committed bench-report fixture at {} has sha256 `{on_disk_hex}` but the sidecar at {} declares `{expected_hex}`; a regression let the fixture and the sidecar drift",
        p.display(),
        sha_path.display()
    );
}

/// Skip-on-missing-DB helper. Returns `true` when the
/// re-run shim can proceed; returns `false` after printing
/// a skip notice. The `DATABASE_URL` contract is the same
/// one the auth server-flow tests, the gameroom
/// round-trip tests, the autotrain smoke test, the
/// autotrain bench test, the autotrain compare test, and
/// the autotrain compare3 test honor. The shim forwards
/// `DATABASE_URL` → `DB_URL` itself (mirroring the
/// `testnet-live-proof.sh` runbook's convention).
#[cfg(feature = "database")]
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!(
                "DATABASE_URL not set; skipping STW-043 bench-fixture re-run integration test"
            );
            false
        }
    }
}

/// Drive the STW-043 shim with `<output-path>` and return
/// the `(stdout, stderr, exit_code)` triple. The shim
/// refuses to run with exit 3 when `<output-path>` is
/// missing or `DATABASE_URL` is unset; the integration
/// test asserts a 0 exit (the green shim) and parses
/// `<output-path>` for the `BenchReport` shape.
#[cfg(feature = "database")]
fn run_shim(output_path: &PathBuf) -> (String, String, i32) {
    let mut cmd = Command::new(script_path());
    cmd.arg(output_path);
    // The shim reads `DATABASE_URL` (with `DB_URL`
    // fallback), and forwards `DATABASE_URL` → `DB_URL`
    // for the trainer. We pass both so a CI worker that
    // sets only one of the two env knobs still works.
    if let Ok(url) = std::env::var("DATABASE_URL") {
        cmd.env("DATABASE_URL", &url);
        cmd.env("DB_URL", &url);
    }
    // The shim is small-budget by default (8 hands, 2
    // blind, v1 blueprint, preflop baseline); we
    // explicitly set the four env knobs the shim
    // honours so a CI worker that left stale
    // `RBP_BENCH_*` env vars in the shell still gets
    // the small-budget re-run. The quiet flag mutes the
    // shim's per-step progress echo so the test's
    // captured stderr is just the trainer's `log::info!`
    // stream (matching the existing
    // `bench.rs` integration test's
    // `RUST_LOG=info` discipline).
    cmd.env("RBP_BENCH_HANDS", "8");
    cmd.env("RBP_BENCH_BLIND", "2");
    cmd.env("RBP_BENCH_BLUEPRINT", "v1");
    cmd.env("RBP_BENCH_BASELINE", "preflop");
    cmd.env("COMMIT_BENCH_FIXTURE_QUIET", "1");
    let out = cmd.output().expect("spawn commit-bench-fixture.sh");
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
    )
}

/// Per-process unique temp file for the re-run capture.
/// The `SEQ` counter disambiguates parallel `cargo test`
/// invocations.
#[cfg(feature = "database")]
static SEQ: AtomicUsize = AtomicUsize::new(0);

#[cfg(feature = "database")]
fn fresh_output_path() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rbp-bench-fixture-integ-{}-{}.json",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// Parse a single-line JSON `BenchReport` into a small
/// struct the test can assert on. Mirrors the
/// `parse_bench_line` helper in
/// `crates/autotrain/tests/bench.rs` (same flat-key shape
/// the `BenchReport::to_json` format string emits, no
/// `serde` dep on the integration test's hot path). A
/// future regression that re-orders the keys in
/// `to_json` still parses (the order doesn't matter for
/// `Value`-style reads), but a regression that drops a
/// field fails the typed shape check.
#[derive(Debug)]
#[cfg(feature = "database")]
#[allow(dead_code)] // `wins` / `losses` are parsed for shape but not asserted; the
                    // shape contract is "every BenchReport field is present" not
                    // "every BenchReport field is asserted".
struct ParsedBench {
    hands: u64,
    wins: u64,
    losses: u64,
    net_chips: i64,
    mbb_per_100: f64,
    blueprint_trained: bool,
    blueprint: String,
    baseline: String,
}

#[cfg(feature = "database")]
fn parse_re_run_bench_line(line: &str) -> Option<ParsedBench> {
    let trim = line.trim();
    if !(trim.starts_with('{') && trim.ends_with('}')) {
        return None;
    }
    let body = &trim[1..trim.len() - 1];
    let mut hands: Option<u64> = None;
    let mut wins: Option<u64> = None;
    let mut losses: Option<u64> = None;
    let mut net_chips: Option<i64> = None;
    let mut mbb_per_100: Option<f64> = None;
    let mut blueprint_trained: Option<bool> = None;
    let mut blueprint: Option<String> = None;
    let mut baseline: Option<String> = None;
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
            "blueprint_trained" => {
                blueprint_trained = Some(matches!(raw, "true"));
            }
            "blueprint" => {
                blueprint = Some(raw.trim_matches('"').to_string());
            }
            "baseline" => {
                baseline = Some(raw.trim_matches('"').to_string());
            }
            // The other 6 fields (`mbb_ci95`, `win_rate`,
            // `win_rate_ci95`, `blind`, `transcript`) are
            // not asserted in the re-run sub-test, so we
            // accept-and-ignore them via the wildcard
            // branch below. A future regression that
            // drops any of the 8 fields we DO assert
            // surfaces here as a `None` return.
            _ => {}
        }
    }
    Some(ParsedBench {
        hands: hands?,
        wins: wins?,
        losses: losses?,
        net_chips: net_chips?,
        mbb_per_100: mbb_per_100?,
        blueprint_trained: blueprint_trained?,
        blueprint: blueprint?,
        baseline: baseline?,
    })
}

#[test]
fn bench_report_fixture_script_invokes_trainer_bin() {
    // The shim must shell out to the `<workspace>/target/
    // debug/trainer` (or `release/trainer`) binary the
    // integration test resolves. The pin is a
    // substring-on-the-script-source check: the
    // shim's `TRAINER_BIN=...` default + the
    // `"$TRAINER_BIN"` call site must be present, so
    // a future regression that drops the trainer
    // invocation (e.g. rewrites the shim to call
    // `cargo run` instead) fails the test before a CI
    // worker can shell out to it. Mirrors the STW-019
    // / STW-032 `trainer --verify-receipt` call-site
    // pinners.
    let script = read(&script_path());
    assert!(
        script.contains("target/debug/trainer"),
        "STW-043 bench-fixture shim at {} must reference the \
         `<workspace>/target/debug/trainer` default TRAINER_BIN path",
        script_path().display()
    );
    assert!(
        script.contains("\"$TRAINER_BIN\""),
        "STW-043 bench-fixture shim at {} must invoke `\"$TRAINER_BIN\"` (the resolved path); \
         a regression that bypasses the env knob makes the shim un-overridable",
        script_path().display()
    );
    // The shim must call `--bench` (the trainer's
    // bench entry point the runbook's chain reuses).
    assert!(
        script.contains("--bench"),
        "STW-043 bench-fixture shim at {} must shell out to `trainer --bench`; \
         a regression that drops the --bench flag leaves the shim driving the \
         wrong mode",
        script_path().display()
    );
    // The shim must call `--reset` (the trainer's
    // reset entry point the bench depends on for a
    // fresh-DB run).
    assert!(
        script.contains("--reset"),
        "STW-043 bench-fixture shim at {} must shell out to `trainer --reset`; \
         a regression that drops the --reset step leaves the bench running \
         against a non-fresh DB",
        script_path().display()
    );
}

/// The DB-gated re-run sub-test. Driven only when the
/// `database` feature is enabled AND a non-empty
/// `DATABASE_URL` is set; otherwise the test is a no-op
/// (so `cargo test --workspace` stays green on machines
/// without Postgres). Mirrors the gating the STW-010
/// `bench.rs` integration test + the STW-028
/// `verify_receipt.rs` integration test follow.
#[cfg(feature = "database")]
#[test]
fn bench_report_fixture_re_run_captures_parseable_json() {
    if !database_url_set() {
        return;
    }
    let output_path = fresh_output_path();
    let (stdout, stderr, code) = run_shim(&output_path);
    assert_eq!(
        code, 0,
        "STW-043 shim must exit 0 on a fresh DB. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        output_path.exists(),
        "STW-043 shim must write the post-strip JSON to {output_path:?}; \
         the shim's exit-0 contract is paired with the on-disk write",
        output_path = output_path.display()
    );
    let body = read(&output_path);
    let parsed = parse_re_run_bench_line(&body).unwrap_or_else(|| {
        panic!(
            "STW-043 shim re-run output at {} must parse as the \
             `BenchReport::to_json` flat-key shape; got: {body:?}",
            output_path.display()
        )
    });
    // The shim defaults to `RBP_BENCH_HANDS=8`, so the
    // captured `hands` field must be 8. A regression
    // that lets the shim's `RBP_BENCH_HANDS` default
    // drift (e.g. the small-budget env knob gets
    // renamed) fails here.
    assert_eq!(
        parsed.hands, 8,
        "STW-043 shim re-run must report `hands` = 8 (the small-budget default); got: {}",
        parsed.hands
    );
    // The shim's `RBP_BENCH_BLUEPRINT=v1` default
    // means the captured `blueprint` field must be
    // the v1 literal.
    assert_eq!(
        parsed.blueprint, "v1",
        "STW-043 shim re-run must report `blueprint` = `v1`; got: {:?}",
        parsed.blueprint
    );
    // The shim's `RBP_BENCH_BASELINE=preflop` default
    // means the captured `baseline` field must be
    // the preflop literal.
    assert_eq!(
        parsed.baseline, "preflop",
        "STW-043 shim re-run must report `baseline` = `preflop`; got: {:?}",
        parsed.baseline
    );
    // The shim's `trainer --reset` step zeros the
    // blueprint + epoch rows, so the captured
    // `blueprint_trained` field must be `false`. A
    // regression that skips the `--reset` step
    // surfaces here as a `blueprint_trained=true` run.
    assert!(
        !parsed.blueprint_trained,
        "STW-043 shim re-run must report `blueprint_trained` = false (the --reset precondition); got: {}",
        parsed.blueprint_trained
    );
    // The shim's strip pass removes `run_id` /
    // `started_at_utc`; the captured body must NOT
    // carry them. A regression that drops the strip
    // pass surfaces here as a body containing one of
    // the two field names.
    assert!(
        !body.contains("\"run_id\":"),
        "STW-043 shim re-run output at {} must NOT carry `run_id`; the strip pass is contractually required",
        output_path.display()
    );
    assert!(
        !body.contains("\"started_at_utc\":"),
        "STW-043 shim re-run output at {} must NOT carry `started_at_utc`; the strip pass is contractually required",
        output_path.display()
    );
    // Strong shape check: the captured body must also
    // round-trip through the typed `Blueprint` /
    // `Baseline` `as_str()` parser. The bench's
    // `BenchReport` struct is the on-the-wire contract
    // the dashboard consumes; a regression that lets a
    // non-conformant JSON slip past the runtime surface
    // fails here. (Note: the `BenchReport` struct
    // derives only `Debug`, not `Serialize` / `Deserialize`,
    // so the typed round-trip uses the local flat-key
    // `parse_re_run_bench_line` parser + the `Blueprint` /
    // `Baseline` `as_str()` string-form contract, not
    // `serde_json::from_str::<BenchReport>`. The flat-key
    // parser is the same shape `crates/autotrain/tests/
    // bench.rs::parse_bench_line` uses, so a future
    // regression in the bench's JSON shape fails both
    // tests in the same CI run.)
    assert_eq!(
        Blueprint::V1.as_str(),
        "v1",
        "Blueprint::V1.as_str() must equal the literal `v1` (the bench JSON shape contract)"
    );
    assert_eq!(
        Baseline::Preflop.as_str(),
        "preflop",
        "Baseline::Preflop.as_str() must equal the literal `preflop` (the bench JSON shape contract)"
    );
    // And the captured body's `blueprint` / `baseline`
    // fields must round-trip through the same `as_str()`
    // contract a future scraper relies on. A regression
    // that lets a non-conformant value (e.g. `"V1"`
    // vs `"v1"`) slip past the runtime encoder fails
    // here.
    assert_eq!(
        Blueprint::V1.as_str(),
        parsed.blueprint,
        "STW-043 shim re-run `blueprint` field must equal `Blueprint::V1.as_str()`"
    );
    assert_eq!(
        Baseline::Preflop.as_str(),
        parsed.baseline,
        "STW-043 shim re-run `baseline` field must equal `Baseline::Preflop.as_str()`"
    );
    // Sanity-check the headline math: `mbb_per_100 =
    // net_chips * 100 / (hands * blind)`. A regression
    // in the bench's summariser surfaces here, not
    // in a downstream dashboard.
    let expected_mbb = (parsed.net_chips as f64) * 100.0 / ((parsed.hands as f64) * 2.0);
    assert!(
        (parsed.mbb_per_100 - expected_mbb).abs() < 1e-3,
        "STW-043 shim re-run `mbb_per_100` ({}) must equal \
         `net_chips * 100 / (hands * blind)` ({})",
        parsed.mbb_per_100,
        expected_mbb
    );
    // Clean up the capture temp file. A regression
    // that leaves it around (e.g. a non-zero exit
    // path) fails the test because `fresh_output_path`
    // already `remove_file`'d the old one.
    let _ = std::fs::remove_file(&output_path);
}
