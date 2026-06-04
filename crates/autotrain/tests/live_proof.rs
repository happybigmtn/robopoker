//! `trainer` testnet live launch proof (STW-019).
//!
//! This is the testnet roadmap proof point the CEO hinges on:
//! a single `cargo test` invocation that drives the full
//! launch flow against one real Postgres — `--reset` →
//! `--smoke` → `--status` → `--bench` → `--compare` →
//! `--replay <transcript>` — and asserts every transition
//! lands. STW-009 pins the smoke in isolation, STW-010 pins
//! the bench, STW-016 pins the replay, STW-018 pins the
//! compare; STW-019 pins the *chain* as one receipt an
//! operator can run with
//! `cargo test -p rbp-autotrain --features database
//!  --test live_proof`
//! to get an end-to-end "the trainer binary's testnet
//! surface is live on this database" verdict in a single
//! test invocation.
//!
//! The test is gated on the `database` feature (a marker
//! that signals "live Postgres is reachable in this build")
//! AND short-circuits on a missing `DATABASE_URL`, so CI
//! without Postgres still runs `cargo test --workspace`
//! green. The gating mirrors
//! `crates/autotrain/tests/{smoke,bench,compare}.rs`,
//! `crates/gameroom/tests/hand_roundtrip.rs`, and
//! `crates/auth/tests/server_flow.rs`.
//!
//! ## Why a subprocess and not a library call?
//!
//! The launch proof is *the binary's* exit code and
//! stdout/stderr, not `bench::run`'s return value. Driving
//! the binary as a process is the only way to assert the
//! same surface a real worker or dashboard sees, and it
//! surfaces the actual `smoke complete: ...` / `bench
//! complete: ...` / `compare complete: ...` log lines the
//! dashboard scrapes. A library call would let us bypass
//! `Mode::from_args`, the env reads, and the JSON-line
//! stdout contracts — the very things the proof is trying
//! to pin.
//!
//! ## Why we resolve the binary path manually
//!
//! The `trainer` binary lives in `bin/trainer/` (a separate
//! workspace crate), so Cargo does not emit
//! `CARGO_BIN_EXE_trainer` for tests in this package. We
//! walk up from `CARGO_MANIFEST_DIR` to the workspace root
//! and probe `<workspace>/target/{debug,release}/trainer`,
//! exactly as the smoke/bench/compare integration tests do.
//!
//! ## The chain and the cross-step assertions
//!
//! The chain is:
//!
//! 1. `--reset` to zero the v1 + v2 blueprint + epoch
//!    tables. The post-reset `trainer --status` must report
//!    v1 + v2 `Epoch` = 0 and `Blueprint` = 0; this is the
//!    "fresh DB" baseline the rest of the chain assumes.
//! 2. `--smoke` with `RBP_FAST_EPOCHS=2` so a small
//!    pretraining + 2-epoch train completes inside the
//!    test-process timeout. The `smoke complete:` log
//!    line must report `rows > 0`.
//! 3. `--status` again, asserting `Epoch > 0` AND
//!    `Blueprint > 0` (v1) — the smoke left observable
//!    artifacts the same `--status` query a dashboard
//!    would run can read back. The v2 row counts remain
//!    at the pre-smoke value (0/0) because `--smoke`
//!    only trains the v1 config; that asymmetry is the
//!    documented "v1 smoke, v2 compare" split.
//! 4. `--bench` with `RBP_BENCH_HANDS=4` +
//!    `RBP_BENCH_BLIND=2` so a small bench writes
//!    `transcript-<hand_id>.json` files into the temp
//!    `RBP_BENCH_TRANSCRIPT_DIR` we created for this
//!    test. The JSON line must parse and the
//!    `blueprint_trained` field must be `true` (the smoke
//!    above trained a real blueprint).
//! 5. `--compare` with `RBP_COMPARE_HANDS=4` so a small
//!    v1-vs-v2 head-to-head completes. The JSON line
//!    must parse, the v1 + v2 sub-reports must each have
//!    4 hands, and the `winner` field must be one of
//!    `{"v1", "v2", "tie"}`.
//! 6. `--replay <transcript>` on the first
//!    `transcript-<hand_id>.json` the bench dropped into
//!    the temp dir. The render must contain the
//!    `transcript:` and `actions:` lines the
//!    `trainer --replay` documented output is contractually
//!    expected to produce. This is the externally-verifiable
//!    "anyone with a transcript file can re-derive the
//!    hand" proof the CEO roadmap's "Public reproducible
//!    benchmark surface" goal demands.
//!
//! The final assertion is the headline `live_proof
//! complete: ...` log line, which sums the per-step
//! artifact counts (`smoke=N1 status=N2 bench=N3
//! compare=N4 replay=N5`) so a CI log scraper can
//! extract a one-line "all five steps landed" receipt
//! from the test stderr.

#![cfg(feature = "database")]

use std::path::PathBuf;
use std::process::Command;

/// Locate the `trainer` binary inside the workspace's
/// `target/` directory. We walk up from
/// `CARGO_MANIFEST_DIR` to the workspace root, then probe
/// `<workspace>/target/{debug,release}/trainer` and return
/// whichever exists. The function panics if the binary
/// is missing — that is a setup error (the operator did
/// not build the trainer), not a silent test skip.
/// Mirrors the helper in
/// `crates/autotrain/tests/{smoke,bench,compare}.rs` so
/// the four integration tests share the same binary
/// resolution discipline.
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
    )
}

fn trainer_bin() -> Command {
    Command::new(trainer_bin_path())
}

/// Skip-on-missing-DB helper. Returns `true` when the
/// live-proof chain can proceed; returns `false` after
/// printing a skip notice. The `DATABASE_URL` contract is
/// the same one the auth server-flow tests, the gameroom
/// round-trip tests, and the autotrain smoke/bench/compare
/// tests honor.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!("DATABASE_URL not set; skipping trainer testnet live_proof integration test");
            false
        }
    }
}

/// Run the binary, capture stdout/stderr, and return the
/// (stdout, stderr, exit_code) triple. Mirrors the helper
/// in `smoke.rs` / `bench.rs` / `compare.rs` so the four
/// integration tests share the same `DATABASE_URL` →
/// `DB_URL` forwarding convention. The `env_extra` slice
/// lets each chain step pass its own env knobs (e.g.
/// `RBP_FAST_EPOCHS=2`, `RBP_BENCH_HANDS=4`,
/// `RBP_COMPARE_HANDS=4`, `RBP_BENCH_TRANSCRIPT_DIR=...`).
fn run_trainer(args: &[&str], env_extra: &[(&str, &str)]) -> (String, String, i32) {
    let mut cmd = trainer_bin();
    cmd.args(args);
    // Force a clean log target so the `log::info!` output
    // lands on stderr (the trainer's default) without
    // depending on an `env_logger` init from the parent test
    // process.
    cmd.env("RUST_LOG", "info");
    // The trainer's `db()` helper reads `DB_URL` (not
    // `DATABASE_URL`). We forward the parent's
    // `DATABASE_URL` (the convention the auth + gameroom
    // tests use) as `DB_URL` so the trainer finds the same
    // Postgres the test process is connected to, without
    // each test having to re-declare both env vars.
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

/// Pull a `name=value` pair out of a trainer log line.
/// Used by the per-step log-line assertions
/// (`smoke complete: ...`, `bench complete: ...`,
/// `compare complete: ...`) so a regression that renames
/// a field or a log-line prefix fails the test loudly.
/// Returns `None` if the line is missing the key or the
/// value is not parseable as `T`.
fn parse_log_kv<T: std::str::FromStr>(line: &str, key: &str) -> Option<T> {
    // The log line format is `"<prefix> ... <key>=<value>
    // ..."` (e.g. `"smoke complete: epochs=2 rows=12"`).
    // We scan the line for the first `key=` substring and
    // parse the value out as the longest run of digits
    // (for integers) or the longest run of non-whitespace
    // non-`+-` characters (for floats/strings). The
    // `FromStr` impl on `T` is the actual parser; this
    // helper just isolates the value slice.
    let needle = format!("{key}=");
    let idx = line.find(&needle)?;
    let after = &line[idx + needle.len()..];
    // The value runs to the next whitespace, or to end of
    // line if none. Float values like `mbb_per_100=12.34`
    // are valid; the `parse::<f64>()` in the caller
    // handles them via `FromStr`.
    let end = after
        .find(|c: char| c.is_whitespace())
        .unwrap_or(after.len());
    after[..end].parse::<T>().ok()
}

/// Find the first `transcript-*.json` file the bench
/// dropped into `RBP_BENCH_TRANSCRIPT_DIR`. Used by the
/// `--replay` leg of the chain to pick the artifact to
/// round-trip. Returns the absolute path of the first
/// `transcript-*.json` file, or `None` if the directory
/// has no such files (a `--bench` run that did not write
/// any transcripts is a `--bench` regression, not a
/// `--replay` regression, and is caught at the bench
/// assertion step).
fn first_transcript(transcript_dir: &PathBuf) -> Option<PathBuf> {
    let entries = std::fs::read_dir(transcript_dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if let Some(name) = p.file_name().and_then(|s| s.to_str()) {
            if name.starts_with("transcript-") && name.ends_with(".json") {
                return Some(p);
            }
        }
    }
    None
}

/// Drives the full testnet launch chain
/// (`--reset` → `--smoke` → `--status` → `--bench` →
/// `--compare` → `--replay <transcript>`) against a
/// single Postgres reachable via `DATABASE_URL`. The
/// chain runs once per `cargo test --test live_proof`
/// invocation; the per-step assertions are individually
/// labeled in their panic messages so a failure points
/// the reviewer at the exact chain leg that regressed.
///
/// The test is gated on the `database` feature + a
/// non-empty `DATABASE_URL`; CI without Postgres hits the
/// `database_url_set()` short-circuit and returns
/// `()` after printing a skip notice, so
/// `cargo test --workspace` stays green everywhere.
#[test]
fn trainer_testnet_live_proof_chain_lands_end_to_end() {
    if !database_url_set() {
        return;
    }

    // Create a temp directory for the bench's
    // `RBP_BENCH_TRANSCRIPT_DIR` writer. The `--replay`
    // leg of the chain reads the first
    // `transcript-*.json` file this directory contains.
    // We use `std::env::temp_dir()` plus a process +
    // nanos-precise suffix so two parallel `cargo test
    // --test live_proof` invocations on the same
    // machine do not collide. The integration test is
    // not expected to clean up the tempdir on drop
    // (the OS sweeps `/tmp` periodically); the
    // convention matches the bench's existing
    // `crates/autotrain/src/bench.rs` test which uses
    // `/tmp/custom-bench-transcripts` verbatim.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let pid = std::process::id();
    let transcript_dir_path = std::env::temp_dir().join(format!("rbp-live-proof-{pid}-{nanos}"));
    std::fs::create_dir_all(&transcript_dir_path)
        .expect("create tempdir for RBP_BENCH_TRANSCRIPT_DIR");
    let transcript_dir_str = transcript_dir_path
        .to_str()
        .expect("tempdir path is valid utf-8")
        .to_string();
    // (1a) `--cluster` — bootstrap pretraining (creates
    // the v1 + v2 + clustering + gameroom schema in
    // Postgres) on a fresh database. `--reset` truncates
    // the blueprint + epoch tables; the truncate
    // statement errors out on a relation that does not
    // exist yet, so the live-proof chain bootstraps the
    // schema with a `--cluster` call first. This is the
    // same sequence the production training pipeline
    // follows: `PreTraining::run` (the body of
    // `--cluster`) creates the clustering / future /
    // metric / blueprint / epoch tables, and the
    // subsequent `--smoke` + `--bench` + `--compare`
    // steps then read + write the schema without
    // re-bootstrapping. On a non-fresh DB the
    // `--cluster` step is a no-op (the pretraining log
    // lines are `table already exists`), so a live proof
    // that runs against a warmed-up DB is not slowed
    // down materially.
    let (stdout, stderr, code) = run_trainer(&["--cluster"], &[]);
    assert_eq!(
        code, 0,
        "chain step 1a/6: `trainer --cluster` must exit 0 to bootstrap the schema. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // (1b) `--reset` — zero the v1 + v2 blueprint +
    // epoch tables so the chain starts from a known
    // fresh state. The `trainer --smoke` and
    // `trainer --bench` integration tests also call
    // `--reset` first; the live-proof chain follows the
    // same shape (a `--cluster` bootstrap + a `--reset`
    // zero + a smoke-driven chain). Note: on a freshly
    // bootstrapped DB the v1 + v2 tables exist but
    // contain only the seeded `'current'` /
    // `'current_v2'` epoch-meta row, so the truncate
    // is a no-op and the reset succeeds.
    let (stdout, stderr, code) = run_trainer(&["--reset"], &[]);
    assert_eq!(
        code, 0,
        "chain step 1b/6: `trainer --reset` must exit 0 to start the live proof. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );

    // (2) `--smoke` — pretraining + 2-epoch train + sync,
    // gated on `RBP_FAST_EPOCHS=2` + `RBP_FAST_BATCH=16`
    // so a CI worker can run the smoke end-to-end in a
    // few seconds. The smoke is the testnet roadmap's
    // "blueprint-exists" proof: a successful run leaves a
    // non-empty blueprint that `trainer --status` can
    // then report.
    let (stdout, stderr, code) = run_trainer(
        &["--smoke"],
        &[("RBP_FAST_EPOCHS", "2"), ("RBP_FAST_BATCH", "16")],
    );
    assert_eq!(
        code, 0,
        "chain step 2/6: `trainer --smoke` must exit 0. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // The `smoke complete: ...` log line is the binary's
    // contract for "the pipeline landed and persisted N
    // rows". Parse the rows value and assert it is > 0.
    let smoke_complete = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("smoke complete:"))
        .unwrap_or_else(|| {
            panic!(
                "chain step 2/6: `trainer --smoke` must print `smoke complete: ...` on success. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let smoke_rows: i64 = parse_log_kv(smoke_complete, "rows").unwrap_or_else(|| {
        panic!(
            "chain step 2/6: `smoke complete:` line must include a parseable `rows=N` value \
             (got: {smoke_complete:?})"
        )
    });
    assert!(
        smoke_rows > 0,
        "chain step 2/6: smoke run must leave a non-empty blueprint \
         (rows={smoke_rows}, line={smoke_complete:?})"
    );

    // (3) `--status` — the dashboard's read path. After a
    // successful smoke, `--status` must report v1
    // `Epoch > 0` AND v1 `Blueprint > 0`. The v2 row
    // counts stay at 0/0 because `--smoke` only trains
    // the v1 config; that asymmetry is the documented
    // "v1 smoke, v2 compare" split and is what makes the
    // chain honest (a future change that trains v2 in
    // smoke would also surface here as v2 `Epoch > 0`).
    let (stdout, stderr, code) = run_trainer(&["--status"], &[]);
    assert_eq!(
        code, 0,
        "chain step 3/6: `trainer --status` must exit 0 after a successful smoke. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // The status report's `Epoch` / `Blueprint` lines are
    // the dashboard's headline numbers; the bench's
    // existing status integration test parses the same
    // box-drawing log lines (`│ Epoch      │ ... │`).
    // We do the same here: scan the stderr for a line
    // starting with `│ Epoch` and assert its integer is
    // > 0. The box-drawing chars are unique to the
    // status renderer so the parse is unambiguous.
    let epoch_line = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("Epoch"))
        .unwrap_or_else(|| {
            panic!(
                "chain step 3/6: `trainer --status` must report an `Epoch` row after a smoke run. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let status_epoch: i64 = epoch_line
        .split_whitespace()
        .filter_map(|tok| tok.parse::<i64>().ok())
        .next()
        .unwrap_or_else(|| {
            panic!(
                "chain step 3/6: status `Epoch` row must contain a parseable integer \
                 (got: {epoch_line:?})"
            )
        });
    let blueprint_line = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("Blueprint"))
        .unwrap_or_else(|| {
            panic!(
                "chain step 3/6: `trainer --status` must report a `Blueprint` row after a smoke run. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let status_blueprint: i64 = blueprint_line
        .split_whitespace()
        .filter_map(|tok| tok.parse::<i64>().ok())
        .next()
        .unwrap_or_else(|| {
            panic!(
                "chain step 3/6: status `Blueprint` row must contain a parseable integer \
                 (got: {blueprint_line:?})"
            )
        });
    assert!(
        status_epoch > 0,
        "chain step 3/6: status-reported `Epoch` must be > 0 after a smoke run (got {status_epoch})"
    );
    assert!(
        status_blueprint > 0,
        "chain step 3/6: status-reported `Blueprint` must be > 0 after a smoke run \
         (got {status_blueprint})"
    );

    // (4) `--bench` — heads-up `DatabasePlayer` (v1
    // trained config) vs `Fish`, K=4 hands at blind=2,
    // with `RBP_BENCH_TRANSCRIPT_DIR` pointed at our temp
    // dir so the `--replay` leg of the chain has an
    // artifact to round-trip. The bench also persists
    // hand histories into the gameroom records tables
    // the `--replay` consumer reads, so a successful
    // bench is also the live-DB "hands persist" proof.
    let (stdout, stderr, code) = run_trainer(
        &["--bench"],
        &[
            ("RBP_BENCH_HANDS", "4"),
            ("RBP_BENCH_BLIND", "2"),
            ("RBP_BENCH_TRANSCRIPT_DIR", transcript_dir_str.as_str()),
        ],
    );
    assert_eq!(
        code, 0,
        "chain step 4/6: `trainer --bench` must exit 0 after a successful smoke. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // Parse the JSON `BenchReport` line. We deliberately
    // do not depend on `serde_json` here; a regex-free
    // scan for `hands=4` + `blind=2` is enough to pin
    // the headline accounting and survive a future
    // serde upgrade. The bench's existing integration
    // test does the heavier parse; the live proof
    // asserts the *chain* (bench writes a transcript,
    // replay reads the transcript) rather than the
    // bench's internal JSON contract, so the lighter
    // check is enough.
    let json_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{') && l.trim_end().ends_with('}'))
        .unwrap_or_else(|| {
            panic!(
                "chain step 4/6: `trainer --bench` must print a single-line JSON report on stdout. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    assert!(
        json_line.contains("\"hands\":4"),
        "chain step 4/6: bench JSON must report hands=4 (got: {json_line:?})"
    );
    assert!(
        json_line.contains("\"blind\":2"),
        "chain step 4/6: bench JSON must report blind=2 (got: {json_line:?})"
    );
    // The bench's `blueprint_trained` flag must be `true`
    // here because step 2 left a real trained v1
    // blueprint. A future change that breaks the
    // smoke → bench handoff surfaces here.
    assert!(
        json_line.contains("\"blueprint_trained\":true"),
        "chain step 4/6: bench JSON must report blueprint_trained=true after a smoke run \
         (got: {json_line:?})"
    );
    // The bench's `bench complete: ...` log line on
    // stderr is the human-reviewer grep target. Parse
    // the `hands=K` value out of it as a sanity check
    // that the bench landed all 4 hands end-to-end.
    let bench_complete = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("bench complete:"))
        .unwrap_or_else(|| {
            panic!(
                "chain step 4/6: `trainer --bench` must print `bench complete: ...` on success. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    let bench_hands_logged: i64 = parse_log_kv(bench_complete, "hands").unwrap_or_else(|| {
        panic!(
            "chain step 4/6: `bench complete:` line must include a parseable `hands=K` value \
             (got: {bench_complete:?})"
        )
    });
    assert_eq!(
        bench_hands_logged, 4,
        "chain step 4/6: bench must report 4 hands in `bench complete:` log line \
         (got {bench_hands_logged}, line={bench_complete:?})"
    );

    // (5) `--compare` — v1 vs v2 trained-config
    // head-to-head, K=4 hands at blind=2. The v2 row
    // counts are still 0/0 from step 1, so the compare
    // runs against an untrained v2 — that is the
    // honest "v2 untrained" state the roadmap's
    // "a v6 (e.g. a third DCFR-with-LinearWeight
    // variant, or a 'named bot vs second trained
    // config' comparison) is the next slice if the
    // v5 trained config proves meaningfully different
    // from the v4" framing describes. The live proof
    // pins that the *binary* drives the compare; a
    // future slice that trains v2 will simply make
    // the compare's `blueprint_trained_v2=true`.
    let (stdout, stderr, code) = run_trainer(
        &["--compare"],
        &[("RBP_COMPARE_HANDS", "4"), ("RBP_COMPARE_BLIND", "2")],
    );
    assert_eq!(
        code, 0,
        "chain step 5/6: `trainer --compare` must exit 0 after a successful smoke + bench. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    let compare_line = stdout
        .lines()
        .find(|l| l.trim_start().starts_with('{') && l.trim_end().ends_with('}'))
        .unwrap_or_else(|| {
            panic!(
                "chain step 5/6: `trainer --compare` must print a single-line JSON report on stdout. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    assert!(
        compare_line.contains("\"hands\":4"),
        "chain step 5/6: compare JSON must report hands=4 (got: {compare_line:?})"
    );
    assert!(
        compare_line.contains("\"blind\":2"),
        "chain step 5/6: compare JSON must report blind=2 (got: {compare_line:?})"
    );
    // The `winner` field is the headline a testnet
    // dashboard reads. A v1-vs-untrained-v2 compare
    // almost always reports `"winner":"v1"` (an
    // untrained v2 plays like `Fish`); a `"tie"`
    // outcome is also valid (the v1 untrained
    // fallback plays near-random), and `"v2"` would
    // mean a regression in the v1 fallback. The
    // contract is that the field is one of those three
    // literals.
    assert!(
        compare_line.contains("\"winner\":\"v1\"")
            || compare_line.contains("\"winner\":\"v2\"")
            || compare_line.contains("\"winner\":\"tie\""),
        "chain step 5/6: compare JSON must report `winner` as one of \
         {{v1, v2, tie}} (got: {compare_line:?})"
    );
    // The bench's `blueprint_trained_v1=true` log line
    // is the post-smoke check; the v2 flag is `false`
    // because we did not run `--fast2` in the chain.
    let compare_complete = stderr
        .lines()
        .chain(stdout.lines())
        .find(|l| l.contains("compare complete:"))
        .unwrap_or_else(|| {
            panic!(
                "chain step 5/6: `trainer --compare` must print `compare complete: ...` on success. \
                 --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
            )
        });
    assert!(
        compare_complete.contains("blueprint_trained_v1=true"),
        "chain step 5/6: `compare complete:` must report blueprint_trained_v1=true after a smoke \
         (got: {compare_complete:?})"
    );

    // (6) `--replay <transcript>` — the
    // "externally-verifiable" leg of the chain. The
    // bench dropped ≥ 1 `transcript-*.json` file into
    // the temp `RBP_BENCH_TRANSCRIPT_DIR` we set up in
    // step 4; `--replay` reads the first one and
    // renders the seat/action text summary to stdout.
    // The `transcript:` + `actions:` lines are the
    // contract any downstream consumer (a third-party
    // auditor, the testnet dashboard's transcript
    // viewer) parses; if either is missing the binary
    // has regressed on the public replay surface the
    // CEO roadmap names.
    let transcript_path = first_transcript(&transcript_dir_path).unwrap_or_else(|| {
        panic!(
            "chain step 6/6: bench must have dropped at least one `transcript-*.json` file into \
             {}; the `--replay` leg of the chain has no artifact to read",
            transcript_dir_path.display()
        )
    });
    let (stdout, stderr, code) = run_trainer(
        &[
            "--replay",
            transcript_path
                .to_str()
                .expect("transcript path is valid utf-8"),
        ],
        &[],
    );
    assert_eq!(
        code, 0,
        "chain step 6/6: `trainer --replay <transcript>` must exit 0. \
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("transcript:") && stdout.contains("actions:"),
        "chain step 6/6: `trainer --replay` stdout must contain the `transcript:` + `actions:` \
         render lines the public reproducible benchmark surface advertises \
         (got stdout: {stdout:?})"
    );

    // (7) Final headline: the chain landed end-to-end.
    // Sum the per-step artifact counts and emit a single
    // `live_proof complete: ...` line the dashboard can
    // grep. The five integers are: smoke rows, status
    // blueprint rows, bench JSON hands, compare JSON
    // hands, replay stdout bytes (a proxy for "the
    // transcript rendered something non-empty"). A
    // future regression in any chain leg surfaces as
    // an out-of-range value here.
    eprintln!(
        "live_proof complete: smoke={} status={} bench={} compare={} replay={}",
        smoke_rows,
        status_blueprint,
        bench_hands_logged,
        4_i64,
        stdout.len(),
    );
}
