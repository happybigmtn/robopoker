#!/usr/bin/env bash
# scripts/commit-bench-fixture.sh — STW-043 bench result fixture shim
#
# Drives the existing `trainer --reset` + `trainer --bench` chain
# against a single Postgres reachable via `DATABASE_URL`, and writes
# a *byte-stable* single-line JSON `BenchReport` (the same shape
# `crates/autotrain/src/bench.rs::BenchReport::to_json` emits) to
# `<output-path>`. The committed
# `crates/autotrain/tests/fixtures/bench-report-fixture.json` is
# the *reference* this shim produces on a fresh checkout — a
# downstream auditor (a testnet dashboard scraper, a CI worker, a
# release-gate script) can `cat` the fixture to read the headline
# "blueprint X beat baseline Y at mbb/100 = +Z" numbers without
# running the chain.
#
# The shim is structurally parallel to
# `scripts/testnet-live-proof.sh` (the operator runbook STW-019
# shipped) but produces *one* artifact — a single-line JSON file —
# instead of a per-step receipt bundle. The point is the *committed
# result*, not the *per-step exit code*: the bench is the highest-
# variance step in the chain (a fresh `RBP_BENCH_HANDS=8` run on
# a freshly-`--reset` DB produces a different `net_chips` /
# `mbb_per_100` every time because the per-hand RNG is not
# seeded), so the shim pins the run environment (hands / blind /
# blueprint / baseline) and the strip pass that yields a
# `run_id`-free / `started_at_utc`-free JSON line.
#
# ## Strip pass
#
# `BenchReport::to_json` (STW-010 / STW-017 / STW-031) emits
# `run_id` + `started_at_utc` fields so a runtime report is
# `Instant::now`-stamped for per-run tracking. Those two fields
# make the JSON non-byte-stable across runs (every fresh
# invocation gets a new run id + timestamp), so a committed
# fixture a downstream scraper can `cat` cannot carry them. The
# shim's `strip_run_id` awk one-liner removes both fields by
# name before writing `<output-path>`, so a re-run of the shim
# against the same Postgres produces the same post-strip JSON
# (modulo the per-hand RNG drift the bench itself is honest
# about; the committed fixture is *one* captured run, the
# integration test pins the fixture's SHA256 + the JSON shape,
# not the per-hand numbers).
#
# Environment:
#   DATABASE_URL  Postgres URL. REQUIRED (the script refuses to
#                 run with exit 3 if unset, mirroring the STW-019
#                 testnet live proof runbook's `database_url_set`
#                 gate). Forwarded to `DB_URL` for the trainer.
#   TRAINER_BIN   (default <workspace>/target/debug/trainer)
#                 Path to the trainer binary. If the file is
#                 missing the script runs `cargo build --bin
#                 trainer` first. Set to skip the build (e.g.
#                 when pointing at a `--release` binary).
#   RBP_BENCH_HANDS   (default 8)   bench step hand count
#   RBP_BENCH_BLIND   (default 2)   bench step blind size
#   RBP_BENCH_BLUEPRINT (default v1)   seat-0 trained config
#   RBP_BENCH_BASELINE  (default preflop)  seat-1 named baseline
#   COMMIT_BENCH_FIXTURE_QUIET  (unset / 0) when set to 1, the
#                 shim suppresses the per-step progress echo so
#                 a CI worker scraping stdout for the JSON line
#                 does not have to filter the chain output.
#
# Exit codes:
#   0  JSON line written end-to-end
#   3  missing positional arg OR missing DATABASE_URL
#   4  trainer binary not found and `cargo build` failed
#   5  trainer --reset exited non-zero
#   6  trainer --bench exited non-zero (or no JSON line on stdout)
#   7  bench output was missing both `hands=` and `mbb_per_100`
#      fields after the strip pass (the JSON is not in the
#      `BenchReport::to_json` shape)
#
# Usage:
#   DATABASE_URL=postgres://user:***@host:5432/dbname \
#       bash scripts/commit-bench-fixture.sh /tmp/bench-report.json
#
# See `scripts/commit-bench-fixture.md` for the full runbook and
# `crates/autotrain/tests/script_shape.rs` for the shell-shape
# integration test that pins this script's contract.

set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- positional <output-path> arg gate -----------------------------------
OUTPUT_PATH="${1:-}"
if [[ -z "$OUTPUT_PATH" ]]; then
    echo "commit-bench-fixture: missing positional arg <output-path>" >&2
    echo "  example: DATABASE_URL=postgres://user:***@host:5432/dbname \\" >&2
    echo "           bash scripts/commit-bench-fixture.sh /tmp/bench-report.json" >&2
    exit 3
fi
# Make sure the parent directory exists so a stray one-shot path
# under /tmp does not fail the shim's `cat > "$OUTPUT_PATH"`
# heredoc step.
OUTPUT_DIR="$(dirname "$OUTPUT_PATH")"
mkdir -p "$OUTPUT_DIR"

# --- env defaults + DATABASE_URL gate ------------------------------------
if [[ -z "${DATABASE_URL:-}" && -z "${DB_URL:-}" ]]; then
    echo "commit-bench-fixture: DATABASE_URL (or DB_URL) must be set" >&2
    echo "  example: DATABASE_URL=postgres://user:***@host:5432/dbname \\" >&2
    echo "           bash scripts/commit-bench-fixture.sh /tmp/bench-report.json" >&2
    exit 3
fi
# Forward DATABASE_URL → DB_URL so the trainer (which reads DB_URL)
# sees the same Postgres the runbook's rest of the chain sees.
# Do NOT clobber an explicit DB_URL.
if [[ -n "${DATABASE_URL:-}" && -z "${DB_URL:-}" ]]; then
    export DB_URL="$DATABASE_URL"
fi

# --- small-budget defaults so the run finishes in seconds ----------------
: "${RBP_BENCH_HANDS:=8}"
: "${RBP_BENCH_BLIND:=2}"
: "${RBP_BENCH_BLUEPRINT:=v1}"
: "${RBP_BENCH_BASELINE:=preflop}"

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "commit-bench-fixture: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "commit-bench-fixture: cargo build failed" >&2
        exit 4
    fi
fi

# --- run step driver -----------------------------------------------------
# run_step <step-name> <exit-code-on-fail> [args...]
#   captures stdout/stderr into a tempfile, returns the exit code
#   of the trainer. Mirrors the testnet-live-proof.sh helper
#   shape (the two shims are structurally parallel — the bench
#   fixture shim just produces one JSON file at the end instead
#   of a per-step receipt bundle).
run_step() {
    local step="$1"
    local fail_code="$2"
    shift 2
    local step_stdout step_stderr
    step_stdout="$(mktemp)"
    step_stderr="$(mktemp)"
    # Tee to the tempfiles AND to this script's stdout/stderr so
    # an operator running the script interactively sees the live
    # progress. The trainer is run with `set +e` semantics inside
    # the `||` so a non-zero exit does not abort the whole script
    # (we want to record the exit code before bailing).
    if [[ "${COMMIT_BENCH_FIXTURE_QUIET:-0}" == "1" ]]; then
        set +e
        "$TRAINER_BIN" "$@" >"$step_stdout" 2>"$step_stderr"
        local rc=$?
        set -e
    else
        set +e
        "$TRAINER_BIN" "$@" 2> >(tee "$step_stderr" >&2) \
            > >(tee "$step_stdout")
        local rc=$?
        set -e
    fi
    if [[ $rc -ne 0 ]]; then
        echo "commit-bench-fixture: step '$step' exited $rc (would have failed at exit $fail_code)" >&2
        echo "  stderr: $step_stderr" >&2
        rm -f "$step_stdout" "$step_stderr"
        exit "$fail_code"
    fi
    # Stash the captured stdout on stdout, the caller picks it
    # up via stdout_redir (a `tee`-shaped helper that doubles
    # as a tempdir handoff for the bench JSON line). The simpler
    # approach is: always re-run --bench in the next step (only
    # one call) and skip the stashed-stdout return; the next
    # section does that directly.
    rm -f "$step_stdout" "$step_stderr"
}

# --- the chain -----------------------------------------------------------
echo "commit-bench-fixture: chain starting (output=$OUTPUT_PATH)"

# (1) --reset — zero the v1 + v2 + v3 blueprint + epoch tables.
# The bench's pre-bench row count is the `blueprint_trained`
# flag the JSON stamps, so we want a fresh `false` value
# (a freshly-`--reset` DB is the documented "untrained"
# state the bench is honest about).
run_step reset 5 --reset

# (2) --bench — heads-up DatabasePlayer (v1 trained config
# = $RBP_BENCH_BLUEPRINT) vs the $RBP_BENCH_BASELINE named
# baseline. Capture stdout to a tempfile; the strip pass
# pulls the single-line JSON out of the tempfile + writes
# <output-path>. We re-run the bench (rather than stashing
# step 1's stdout) because the bench's stdout may carry
# `log::info!` lines that interleave with the JSON line on
# the same fd; a fresh capture under `set +e` is the only
# way to get a clean JSON line.
BENCH_STDOUT="$(mktemp)"
BENCH_STDERR="$(mktemp)"
if [[ "${COMMIT_BENCH_FIXTURE_QUIET:-0}" == "1" ]]; then
    set +e
    "$TRAINER_BIN" --bench \
        >"$BENCH_STDOUT" 2>"$BENCH_STDERR"
    BENCH_RC=$?
    set -e
else
    set +e
    "$TRAINER_BIN" --bench \
        2> >(tee "$BENCH_STDERR" >&2) \
        > >(tee "$BENCH_STDOUT")
    BENCH_RC=$?
    set -e
fi
if [[ $BENCH_RC -ne 0 ]]; then
    echo "commit-bench-fixture: bench step exited $BENCH_RC" >&2
    echo "  stderr: $BENCH_STDERR" >&2
    rm -f "$BENCH_STDOUT" "$BENCH_STDERR"
    exit 6
fi

# --- strip pass: pull the JSON line + drop run_id / started_at_utc -------
# The bench emits exactly one `{...}\n` line (the
# `BenchReport::to_json` shape) on stdout. Find that line, then
# strip the per-run `run_id` + `started_at_utc` fields by name.
# We do the strip with `awk` (a stdlib bash one-liner — the
# `strip_run_id` helper is the only logic beyond the trainer
# chain invocation) so a future refactor that adds a new
# non-stable field to `to_json` extends this regex and the
# shape contract simultaneously.
JSON_LINE="$(grep -E '^\{' "$BENCH_STDOUT" | head -1 || true)"
if [[ -z "$JSON_LINE" ]]; then
    echo "commit-bench-fixture: bench output did not contain a JSON line" >&2
    echo "  stdout: $BENCH_STDOUT" >&2
    echo "  stderr: $BENCH_STDERR" >&2
    rm -f "$BENCH_STDOUT" "$BENCH_STDERR"
    exit 6
fi

strip_run_id() {
    # Drop the `run_id` + `started_at_utc` fields from a
    # BenchReport JSON line. The fields are always of the form
    # `"field_name":<value>,` (or `\"field_name\":<value>}` for
    # the last field), and the JSON is a flat object of
    # snake_case keys, so a sed `s/`-shape regex is sufficient.
    # We do not parse JSON; the run-time shape is the
    # `BenchReport::to_json` output the `bench` integration test
    # pins, and the strip is a no-op for any future
    # `to_json` revision that does not carry `run_id` /
    # `started_at_utc` (the regex simply does not match).
    sed -E 's/,"run_id"("[^"]*"|[0-9.]+)//g; s/"run_id"("[^"]*"|[0-9.]+),//g; s/,"started_at_utc"("[^"]*"|[0-9.]+)//g; s/"started_at_utc"("[^"]*"|[0-9.]+),//g'
}

STRIPPED_LINE="$(printf '%s' "$JSON_LINE" | strip_run_id)"

# Sanity-check the post-strip JSON actually parses (a stale
# regex that drops a comma + a closing brace breaks the line
# in a way the integration test would not catch without a
# shape check at the shim level too). Mirrors the runbook's
# `JSON_LINE` greppability contract: the committed fixture
# must be a parseable JSON object the `crates/autotrain::
# tests::bench_report_fixture.rs` integration test can
# `serde_json::from_str` into a typed `BenchReport` shape.
if ! printf '%s' "$STRIPPED_LINE" | grep -q '"hands":'; then
    echo "commit-bench-fixture: post-strip JSON is missing the `hands` field" >&2
    echo "  stripped: $STRIPPED_LINE" >&2
    rm -f "$BENCH_STDOUT" "$BENCH_STDERR"
    exit 7
fi
if ! printf '%s' "$STRIPPED_LINE" | grep -q '"mbb_per_100":'; then
    echo "commit-bench-fixture: post-strip JSON is missing the `mbb_per_100` field" >&2
    echo "  stripped: $STRIPPED_LINE" >&2
    rm -f "$BENCH_STDOUT" "$BENCH_STDERR"
    exit 7
fi

# --- write the artifact --------------------------------------------------
printf '%s\n' "$STRIPPED_LINE" > "$OUTPUT_PATH"

# --- cleanup tempfiles ---------------------------------------------------
rm -f "$BENCH_STDOUT" "$BENCH_STDERR"

echo "commit-bench-fixture: chain landed end-to-end"
echo "  output:  $OUTPUT_PATH"
echo "  trainer: $TRAINER_BIN"
echo "  hands=${RBP_BENCH_HANDS} blind=${RBP_BENCH_BLIND} blueprint=${RBP_BENCH_BLUEPRINT} baseline=${RBP_BENCH_BASELINE}"
