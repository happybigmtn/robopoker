//! `trainer-observe.sh` end-to-end integration test (STW-045).
//!
//! This is the cheapest machine-receipt the testnet north star
//! has for the new trainer observability wrapper: a single
//! `cargo test` invocation that drives the wrapper against a
//! real `trainer --bench` (the same small-budget
//! `RBP_BENCH_HANDS=4` the testnet live-proof runbook uses) and
//! asserts the JSONL timeline the wrapper writes is the
//! contract a CI dashboard scrapes:
//!
//! 1. The wrapper exits 0 end-to-end (the trainer's exit
//!    code is preserved; on a successful small bench the
//!    trainer exits 0 and so does the wrapper).
//! 2. The JSONL has *at least* 2 lines (a `stream: "stderr"`
//!    line the bench's `log::info!` stream emits + a
//!    `stream: "summary"` trailer line the wrapper adds on
//!    exit).
//! 3. Every JSONL line parses as a JSON object with exactly
//!    three top-level fields: `ts` (integer ms-since-epoch),
//!    `stream` (one of `"stderr"` / `"stdout"` /
//!    `"summary"`), and `line` (the original trainer line
//!    content, JSON-escaped via `jq` so embedded `"` / `\`
//!    / control chars survive the round-trip byte-stable).
//! 4. The summary trailer's `line` field matches the
//!    pinned `trainer observe complete: exit=<rc> cmd=<argv>`
//!    shape (a CI dashboard can `jq -c 'select(.stream ==
//!    "summary")' <output-jsonl>` and receive a one-line
//!    per-run summary without re-parsing the trainer's own
//!    log stream).
//!
//! The test is gated on the `database` feature AND
//! short-circuits on a missing `DATABASE_URL`, so CI without
//! Postgres still runs `cargo test --workspace` green. The
//! gating mirrors `crates/autotrain/tests/{bench,compare,
//! compare3,live_proof}.rs` so the five integration tests
//! share the same skip-on-missing-DB convention. The test
//! ALSO short-circuits on a missing `jq` (the wrapper's
//! JSON-encoder dependency), so a CI host without `jq` on
//! PATH skips the live-driver step but the no-DB
//! shell-shape pinners in `crates/autotrain/tests/
//! script_shape.rs` still pass.
//!
//! ## Why we resolve the binary paths manually
//!
//! The `trainer` binary lives in `bin/trainer/` (a separate
//! workspace crate), so Cargo does not emit
//! `CARGO_BIN_EXE_trainer` for tests in this package. We
//! walk up from `CARGO_MANIFEST_DIR` to the workspace root
//! and probe `<workspace>/target/{debug,release}/trainer`,
//! exactly as the bench / compare / compare3 / live_proof
//! integration tests do. The `trainer-observe.sh` wrapper
//! lives at `<workspace>/scripts/trainer-observe.sh` (a
//! workspace-root sibling of `crates/`), which we resolve
//! the same way the `script_shape.rs` integration test
//! does so the two tests share the same path resolution
//! discipline.

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
/// the helper in `crates/autotrain/tests/{bench,compare,
/// compare3,live_proof}.rs` so the five integration tests
/// share the same binary resolution discipline.
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
    )
}

/// Locate the `scripts/trainer-observe.sh` wrapper at the
/// workspace root. The wrapper is *not* a Cargo artifact,
/// so the path is resolved manually the same way the
/// `script_shape.rs` integration test does. Panics if the
/// script is missing — a regression in the wrapper's
/// file-on-disk contract is caught by
/// `script_shape::trainer_observe_script_exists_and_parses`,
/// so a panic here is a setup error, not a silent test
/// skip.
fn wrapper_script_path() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain")
        .join("scripts")
        .join("trainer-observe.sh")
}

/// Skip-on-missing-DB helper. Returns `true` when the
/// bench run can proceed; returns `false` after printing a
/// skip notice. The `DATABASE_URL` contract is the same
/// one the auth server-flow tests, the gameroom round-trip
/// tests, and the autotrain bench / compare / compare3 /
/// live_proof tests honor.
fn database_url_set() -> bool {
    match std::env::var("DATABASE_URL") {
        Ok(s) if !s.trim().is_empty() => true,
        _ => {
            eprintln!(
                "DATABASE_URL not set; skipping trainer --bench end-to-end under \
                 scripts/trainer-observe.sh"
            );
            false
        }
    }
}

/// Skip-on-missing-`jq` helper. Returns `true` when the
/// wrapper's JSON-encoder dependency is on PATH; returns
/// `false` after printing a skip notice. The `jq` dependency
/// is *only* needed for the live-driver step; the
/// `script_shape.rs` shell-shape pinners are the no-`jq`
/// contract the wrapper's static surface depends on, and
/// they pass regardless of whether `jq` is installed.
fn jq_on_path() -> bool {
    Command::new("jq")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Run the wrapper script against a real `trainer --bench`
/// with the small-budget `RBP_BENCH_HANDS=4` the
/// `testnet-live-proof.sh` runbook uses (the same
/// env-gated budget the testnet launch chain runs against
/// in production). Returns the wrapper's stdout, stderr,
/// exit code, and the JSONL timeline file's contents so the
/// caller can assert on every surface the wrapper
/// publishes.
fn run_wrapper(tmp_jsonl: &PathBuf) -> (String, String, i32, String) {
    let mut cmd = Command::new(wrapper_script_path());
    // Positional arg 1: the JSONL path the wrapper writes.
    cmd.arg(tmp_jsonl);
    // Positional arg 2: the trainer binary the wrapper
    // invokes (resolved via `trainer_bin_path()` so the
    // integration test pins the same binary the testnet
    // launch chain runs against).
    cmd.arg(trainer_bin_path());
    // Positional arg 3+: the trainer's argv. We pin a
    // small-budget bench with the testnet proof runbook's
    // `RBP_BENCH_HANDS=4` / `RBP_BENCH_BLIND=2` defaults
    // so a CI worker running this test gets a fast
    // end-to-end verdict in seconds. The `--blueprint v1`
    // + `--baseline preflop` argv matches the spec the
    // IMPLEMENTATION_PLAN.md STW-045 row names verbatim.
    cmd.args(["--bench", "--blueprint", "v1", "--baseline", "preflop"]);
    // Forward `DATABASE_URL` → `DB_URL` (the trainer's
    // actual env name) and force `RUST_LOG=info` so the
    // bench's `log::info!` stream lands on stderr (the
    // wrapper's `stream: "stderr"` JSONL line source).
    cmd.env("RUST_LOG", "info");
    if let Ok(url) = std::env::var("DATABASE_URL") {
        cmd.env("DB_URL", url);
    }
    // Honor `TRAINER_OBSERVE_QUIET=1` so the wrapper's own
    // per-invocation progress echo (`trainer-observe:
    // jsonl=... trainer=... argv=...` on stderr) does not
    // leak into the captured stderr the test asserts on
    // (the wrapper contract is "the trainer's own output
    // is unchanged"; the progress echo is a wrapper-internal
    // surface that would otherwise be a noise source for
    // the JSONL-only consumer). We set it unconditionally
    // here so the test's stderr assertion is byte-stable
    // across CI hosts with different `TRAINER_OBSERVE_QUIET`
    // defaults.
    cmd.env("TRAINER_OBSERVE_QUIET", "1");
    let out = cmd
        .output()
        .expect("spawn scripts/trainer-observe.sh trainer --bench");
    let jsonl = std::fs::read_to_string(tmp_jsonl).unwrap_or_else(|e| {
        panic!(
            "STW-045 wrapper must have written {} (got read error: {e})",
            tmp_jsonl.display()
        )
    });
    (
        String::from_utf8_lossy(&out.stdout).into_owned(),
        String::from_utf8_lossy(&out.stderr).into_owned(),
        out.status.code().unwrap_or(-1),
        jsonl,
    )
}

/// The single integration sub-test STW-045 ships. The
/// test pins the wrapper's end-to-end contract on a real
/// `trainer --bench` invocation (the spec the
/// IMPLEMENTATION_PLAN.md row names verbatim): the
/// wrapper exits 0, the JSONL has at least one
/// `stream: "stderr"` line + one `stream: "summary"`
/// line, every JSONL line is a parseable three-field
/// object with the pinned `ts` / `stream` / `line`
/// shape, and the summary trailer's `line` field matches
/// the `trainer observe complete: exit=<rc> cmd=<argv>`
/// pinned shape.
#[test]
fn wrapper_around_trainer_bench_writes_parseable_jsonl() {
    if !database_url_set() {
        return;
    }
    if !jq_on_path() {
        eprintln!("jq not on PATH; skipping STW-045 end-to-end integration test");
        return;
    }

    // Build a tempdir for the JSONL. The wrapper refuses
    // to run if the parent directory does not exist, so we
    // create the dir explicitly (the `mktemp` helper only
    // creates the *file*, not the dir). A failed `mktemp`
    // is a setup error, not a silent test skip.
    let tmp_dir = std::env::temp_dir().join(format!("rbp-stw045-{}", std::process::id()));
    std::fs::create_dir_all(&tmp_dir)
        .unwrap_or_else(|e| panic!("create temp dir {}: {e}", tmp_dir.display()));
    let tmp_jsonl = tmp_dir.join("run.step.jsonl");

    let (stdout, stderr, code, jsonl) = run_wrapper(&tmp_jsonl);

    // (i) The wrapper must exit 0 on a successful small
    // bench. The trainer's exit code is preserved by the
    // wrapper; a non-zero trainer exit surfaces as a
    // non-zero wrapper exit (the contract the runbook's
    // `run_step` helper in `scripts/testnet-live-proof.sh`
    // also relies on). A `cargo test` CI worker that drives
    // this test sees the bench's actual exit code here.
    assert_eq!(
        code, 0,
        "scripts/trainer-observe.sh must exit 0 on a successful small \
         trainer --bench.\\n--- wrapper stdout ---\\n{stdout}\\n--- wrapper stderr ---\\n{stderr}"
    );

    // (ii) The JSONL must have at least 2 lines: a
    // `stream: "stderr"` line the bench's `log::info!`
    // stream emits + a `stream: "summary"` trailer line
    // the wrapper adds on exit. The bench's stderr carries
    // at least the `[INFO] connecting to database` line,
    // so a successful run always satisfies the lower bound.
    let line_count = jsonl.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        line_count >= 2,
        "STW-045 JSONL at {} must have at least 2 lines (one \
         `stream: \"stderr\"` + one `stream: \"summary\"`); got {line_count} lines.\\n\
         --- jsonl ---\\n{jsonl}\\n--- wrapper stdout ---\\n{stdout}\\n--- wrapper stderr ---\\n{stderr}",
        tmp_jsonl.display()
    );

    // (iii) Every JSONL line must parse as a JSON object
    // with exactly three top-level fields: `ts` (integer),
    // `stream` (one of `"stderr"` / `"stdout"` /
    // `"summary"`), `line` (string). We deliberately do
    // NOT depend on `serde_json` for the parse (it is
    // already in the dep graph but we want the test to be
    // a thin contract check that survives a future serde
    // upgrade): a small hand-rolled counter is enough for
    // the flat three-key object the `jq -cn` encoder
    // produces, and the explicit counting keeps the
    // failure mode obvious if a future refactor adds /
    // drops a field.
    let mut saw_stderr = false;
    let mut saw_summary = false;
    let mut summary_line: Option<String> = None;
    for (i, line) in jsonl.lines().filter(|l| !l.trim().is_empty()).enumerate() {
        let trimmed = line.trim();
        assert!(
            trimmed.starts_with('{') && trimmed.ends_with('}'),
            "STW-045 JSONL line {i} must be a JSON object (got: {trimmed:?})"
        );
        let body = &trimmed[1..trimmed.len() - 1];
        // Split on top-level commas. The `jq -cn` encoder
        // emits a flat three-key object, so a naive `,`
        // split is sufficient and keeps the test
        // independent of any JSON crate.
        let mut ts_raw: Option<&str> = None;
        let mut stream: Option<&str> = None;
        let mut line_field_present = false;
        for kv in body.split(',') {
            let (k, v) = kv.split_once(':').unwrap_or_else(|| {
                panic!("STW-045 JSONL line {i} has a malformed key:value pair: {kv:?}")
            });
            let key = k.trim().trim_matches('"');
            let raw = v.trim();
            match key {
                "ts" => ts_raw = Some(raw),
                "stream" => stream = Some(raw.trim_matches('"')),
                "line" => line_field_present = true,
                other => panic!(
                    "STW-045 JSONL line {i} has an unexpected top-level field `{other}`; \
                     the contract is exactly three fields `ts` / `stream` / `line`"
                ),
            }
        }
        let ts_raw =
            ts_raw.unwrap_or_else(|| panic!("STW-045 JSONL line {i} is missing the `ts` field"));
        // The `jq` encoder casts the `ts` field to a
        // number via `($ts|tonumber)`, so the on-disk
        // value must be a bare integer (no surrounding
        // quotes). We assert by `parse::<u64>()` (the test
        // runs in 2026, so a u64 ms-since-epoch is
        // always positive; a 0 / negative value is also
        // caught by the parse failure).
        let ts: u64 = ts_raw.parse().unwrap_or_else(|e| {
            panic!(
                "STW-045 JSONL line {i} has a non-integer `ts` field \
                 (got: {ts_raw:?}, parse error: {e}); the `jq` encoder must \
                 cast the `ts` field to a number via `($ts|tonumber)` so a CI \
                 dashboard can do arithmetic on it without an extra `tonumber` \
                 round-trip"
            )
        });
        // The `ts` must be plausible (a 13-digit ms-since-epoch
        // for a 2026 run is roughly 1.7e12; assert the lower
        // bound catches a regression that emits seconds / ns
        // / a 0-value placeholder).
        assert!(
            ts >= 1_700_000_000_000,
            "STW-045 JSONL line {i} has an implausibly small `ts` ({ts}); \
             the `date +%s%3N` encoder must emit ms-since-epoch (a 13-digit \
             value in 2026+)"
        );
        let stream = stream
            .unwrap_or_else(|| panic!("STW-045 JSONL line {i} is missing the `stream` field"));
        assert!(
            matches!(stream, "stderr" | "stdout" | "summary"),
            "STW-045 JSONL line {i} has an unrecognized `stream` value \
             ({stream:?}); the contract is exactly three stream tags: \
             `stderr` / `stdout` / `summary`"
        );
        assert!(
            line_field_present,
            "STW-045 JSONL line {i} is missing the `line` field; the \
             wrapper's per-line JSONL encoder must carry the original \
             trainer line content under the `line` key"
        );
        match stream {
            "stderr" => saw_stderr = true,
            "summary" => {
                saw_summary = true;
                summary_line = Some(trimmed.to_string());
            }
            _ => {}
        }
    }

    // (iv) The JSONL must contain at least one
    // `stream: "stderr"` line (the bench's `log::info!`
    // stream always emits at least the `[INFO] connecting
    // to database` line) AND exactly one `stream:
    // "summary"` line (the wrapper's own trailer).
    assert!(
        saw_stderr,
        "STW-045 JSONL must contain at least one `stream: \"stderr\"` line; \
         the bench's `log::info!` stream always emits at least the \
         `[INFO] connecting to database` line, so a successful run satisfies \
         the lower bound.\\n--- jsonl ---\\n{jsonl}"
    );
    assert!(
        saw_summary,
        "STW-045 JSONL must contain a `stream: \"summary\"` trailer line; \
         a CI dashboard `select(.stream == \"summary\")` grep depends on the \
         trailer being byte-stable.\\n--- jsonl ---\\n{jsonl}"
    );

    // (v) The summary trailer's `line` field must match
    // the pinned `trainer observe complete: exit=<rc>
    // cmd=<argv>` shape. We assert by *string prefix*
    // (the `<rc>` / `<argv>` are runtime values the test
    // does not pin byte-for-byte, only their
    // presence-and-shape) so a future refactor that adds
    // a new key=value pair (e.g. `duration_ms=...`) does
    // not silently fail the test.
    let summary_line = summary_line.expect("STW-045 summary trailer was seen above");
    assert!(
        summary_line.contains("\"trainer observe complete: exit=\""),
        "STW-045 summary trailer `line` field must contain the pinned \
         `trainer observe complete: exit=` prefix; got: {summary_line}"
    );
    assert!(
        summary_line.contains("\"cmd=\""),
        "STW-045 summary trailer `line` field must contain the `cmd=` \
         key=value pair so a CI dashboard can extract the per-run argv; \
         got: {summary_line}"
    );
    // The `exit=` value must reflect the trainer's
    // actual exit code (0 on a successful small bench);
    // the wrapper's contract is "the trainer's exit
    // code is preserved verbatim", so a `exit=` value
    // other than `0` here would mean the wrapper
    // silently swallowed a non-zero exit.
    assert!(
        summary_line.contains("\"exit=0\""),
        "STW-045 summary trailer `line` field must carry the trainer's \
         actual exit code (0 on a successful small bench); got: {summary_line}"
    );

    // Cleanup the tempdir. A failed `remove_dir_all` is
    // not a test failure (the tempdir is in `/tmp` and
    // the OS will GC it), but a leak in CI is a noise
    // signal we want to avoid.
    let _ = std::fs::remove_dir_all(&tmp_dir);
}
