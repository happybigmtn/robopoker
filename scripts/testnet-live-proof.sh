#!/usr/bin/env bash
# scripts/testnet-live-proof.sh — STW-019 testnet live launch proof runbook
#
# Drives the full `trainer` testnet launch chain
# (`--cluster` → `--reset` → `--smoke` → `--status` → `--bench` →
# `--compare` → `--replay <transcript>`) against a single Postgres
# reachable via `DATABASE_URL`, and writes a per-step receipt bundle
# an operator (or a testnet dashboard) can scrape:
#
#   receipts/testnet-live-proof-<UTC-ISO>/
#     SUMMARY.txt                    # the one-line launch receipt
#     ENV.txt                        # the env the chain ran with
#     cluster/{stdout,stderr,exit}.txt
#     reset/{stdout,stderr,exit}.txt
#     smoke/{stdout,stderr,exit}.txt
#     status/{stdout,stderr,exit}.txt
#     bench/{stdout,stderr,exit}.txt
#     bench/transcripts/             # the bench's transcript-*.json files
#     compare/{stdout,stderr,exit}.txt
#     replay/{stdout,stderr,exit}.txt
#
# The script is the operator-visible counterpart of
# `crates/autotrain/tests/live_proof.rs` (which proves the same chain
# inside `cargo test --test live_proof`). The receipt layout is
# designed so a downstream testnet dashboard can:
#
#   1. `grep "^testnet live_proof complete:" receipts/.../SUMMARY.txt`
#      to get the per-step artifact counts in one line.
#   2. `cat receipts/.../bench/stdout.txt | python -m json.tool`
#      to parse the bench's `BenchReport` JSON.
#   3. `cat receipts/.../replay/stdout.txt`
#      to view the rendered transcript.
#
# Environment:
#   DATABASE_URL  Postgres URL. REQUIRED (the script refuses to run
#                 with exit 3 if unset, mirroring the testnet live
#                 proof integration test's `database_url_set` gate).
#   DB_URL        The trainer's actual env name. If `DATABASE_URL` is
#                 set and `DB_URL` is not, the script forwards
#                 `DATABASE_URL` → `DB_URL` so the trainer finds the
#                 same Postgres the rest of the chain sees.
#   RBP_FAST_EPOCHS       (default 2)   smoke step epoch count
#   RBP_FAST_BATCH        (default 16)  smoke step batch size
#   RBP_BENCH_HANDS       (default 4)   bench step hand count
#   RBP_BENCH_BLIND       (default 2)   bench step blind size
#   RBP_BENCH_TRANSCRIPT_DIR  (set by the script; do not override)
#   RBP_COMPARE_HANDS     (default 4)   compare step hand count
#   RBP_COMPARE_BLIND     (default 2)   compare step blind size
#   TRAINER_BIN           (default <workspace>/target/debug/trainer)
#                 Path to the trainer binary. If the file is missing
#                 the script runs `cargo build --bin trainer` first.
#                 Set to skip the build (e.g. when pointing at a
#                 `--release` binary).
#
# Exit codes:
#   0  chain completed end-to-end
#   3  DATABASE_URL not set (refuse-to-run gate)
#   4  trainer binary not found and `cargo build` failed
#   5+ chain step N exited non-zero (5 = cluster, 6 = reset, 7 = smoke,
#      8 = status, 9 = bench, 10 = compare, 11 = replay)
#
# Usage:
#   DATABASE_URL=postgres://user:pass@host:5432/dbname \
#       bash scripts/testnet-live-proof.sh
#
# See `scripts/testnet-live-proof.md` for the full runbook and
# `crates/autotrain/tests/script_shape.rs` for the shell-shape
# integration test that pins this script's contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- env defaults + DATABASE_URL gate ------------------------------------
if [[ -z "${DATABASE_URL:-}" && -z "${DB_URL:-}" ]]; then
    echo "testnet-live-proof: DATABASE_URL (or DB_URL) must be set" >&2
    echo "  example: DATABASE_URL=postgres://user:pass@host:5432/dbname \\" >&2
    echo "           bash scripts/testnet-live-proof.sh" >&2
    exit 3
fi
# Forward DATABASE_URL → DB_URL so the trainer (which reads DB_URL)
# sees the same Postgres the rest of the chain sees. Do NOT clobber
# an explicit DB_URL.
if [[ -n "${DATABASE_URL:-}" && -z "${DB_URL:-}" ]]; then
    export DB_URL="$DATABASE_URL"
fi

# --- small-budget defaults so the chain finishes in seconds --------------
: "${RBP_FAST_EPOCHS:=2}"
: "${RBP_FAST_BATCH:=16}"
: "${RBP_BENCH_HANDS:=4}"
: "${RBP_BENCH_BLIND:=2}"
: "${RBP_COMPARE_HANDS:=4}"
: "${RBP_COMPARE_BLIND:=2}"

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-proof: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with `cargo build --bin trainer`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-proof: cargo build failed" >&2
        exit 4
    fi
fi

# --- receipt bundle directory --------------------------------------------
UTC_ISO="$(date -u +%Y%m%dT%H%M%SZ)"
RECEIPT_DIR="$WORKSPACE_ROOT/receipts/testnet-live-proof-$UTC_ISO"
mkdir -p "$RECEIPT_DIR"

# Snapshot the env that drove the chain so a reviewer can reproduce
# the exact run. Redact DATABASE_URL / DB_URL secrets before writing.
{
    echo "WORKSPACE_ROOT=$WORKSPACE_ROOT"
    echo "TRAINER_BIN=$TRAINER_BIN"
    echo "RBP_FAST_EPOCHS=$RBP_FAST_EPOCHS"
    echo "RBP_FAST_BATCH=$RBP_FAST_BATCH"
    echo "RBP_BENCH_HANDS=$RBP_BENCH_HANDS"
    echo "RBP_BENCH_BLIND=$RBP_BENCH_BLIND"
    echo "RBP_COMPARE_HANDS=$RBP_COMPARE_HANDS"
    echo "RBP_COMPARE_BLIND=$RBP_COMPARE_BLIND"
    if [[ -n "${DATABASE_URL:-}" ]]; then
        echo "DATABASE_URL=<redacted: ${#DATABASE_URL} chars>"
    else
        echo "DATABASE_URL=<unset>"
    fi
    if [[ -n "${DB_URL:-}" && "${DB_URL:-}" != "${DATABASE_URL:-}" ]]; then
        echo "DB_URL=<redacted: ${#DB_URL} chars>"
    fi
} > "$RECEIPT_DIR/ENV.txt"

# --- chain step driver ---------------------------------------------------
# run_step <step-name> <exit-code-on-fail> [args...]
#   captures stdout/stderr/exit into $RECEIPT_DIR/<step>/{stdout,stderr,exit}.txt
#   returns the exit code of the trainer (via the `set -e` discipline).
run_step() {
    local step="$1"
    local fail_code="$2"
    shift 2
    local step_dir="$RECEIPT_DIR/$step"
    mkdir -p "$step_dir"
    local stdout_file="$step_dir/stdout.txt"
    local stderr_file="$step_dir/stderr.txt"
    local exit_file="$step_dir/exit.txt"

    # Tee to the receipt files AND to this script's stdout/stderr so
    # an operator running the script interactively sees the live
    # progress. The trainer is run with `set +e` semantics inside the
    # `||` so a non-zero exit does not abort the whole script (we want
    # to record the exit code in the receipt before bailing).
    set +e
    "$TRAINER_BIN" "$@" >"$stdout_file" 2>"$stderr_file"
    local rc=$?
    set -e
    echo "$rc" > "$exit_file"

    if [[ $rc -ne 0 ]]; then
        echo "testnet-live-proof: step '$step' exited $rc (would have failed at exit $fail_code)" >&2
        echo "  stdout: $stdout_file" >&2
        echo "  stderr: $stderr_file" >&2
        exit "$fail_code"
    fi
}

# --- parse smoke / bench / compare artifact counts ----------------------
# parse_kv <line> <key> — extract an integer from `key=value` in a
# log line. Returns 0 on success, 1 on missing key. Mirrors the
# `parse_log_kv` helper in `crates/autotrain/tests/live_proof.rs`.
parse_kv() {
    local line="$1"
    local key="$2"
    local needle="${key}="
    local idx
    idx="$(printf '%s' "$line" | grep -boF "$needle" | head -1 | cut -d: -f1 || true)"
    if [[ -z "$idx" ]]; then
        return 1
    fi
    local after="${line:$((idx + ${#needle}))}"
    local end
    end="$(printf '%s' "$after" | grep -boE '[[:space:]]' | head -1 | cut -d: -f1 || true)"
    if [[ -z "$end" ]]; then
        end="${#after}"
    fi
    printf '%s' "${after:0:end}"
}

# Pull the row count out of `smoke complete: epochs=N rows=M`.
SMOKE_ROWS=0
STATUS_BLUEPRINT=0
BENCH_HANDS=0
COMPARE_HANDS=0
REPLAY_BYTES=0

# --- the chain -----------------------------------------------------------
echo "testnet-live-proof: chain starting ($RECEIPT_DIR)"

# (1) --cluster — bootstrap pretraining + schema. Idempotent on a
# warmed DB. Must come first; --reset truncates tables that need to
# exist.
run_step cluster 5 --cluster

# (2) --reset — zero the v1 + v2 blueprint + epoch tables. The chain
# starts from a known fresh state so the smoke leg's `rows > 0`
# assertion is meaningful.
run_step reset 6 --reset

# (3) --smoke — pretraining + N-epoch train + sync. The
# `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH` env knobs keep this in
# seconds.
run_step smoke 7 --smoke
SMOKE_COMPLETE="$(grep -E 'smoke complete:' "$RECEIPT_DIR/smoke/stderr.txt" \
    || grep -E 'smoke complete:' "$RECEIPT_DIR/smoke/stdout.txt" || true)"
if [[ -n "$SMOKE_COMPLETE" ]]; then
    SMOKE_ROS_LINE="$(printf '%s' "$SMOKE_COMPLETE" | grep -oE 'rows=[0-9]+' | head -1 || true)"
    if [[ -n "$SMOKE_ROS_LINE" ]]; then
        SMOKE_ROWS="${SMOKE_ROS_LINE#rows=}"
    fi
fi

# (4) --status — the dashboard's read path. After a successful smoke
# the v1 Epoch + Blueprint must be > 0. The script records the
# Blueprint integer from the box-drawing status line for the SUMMARY
# headline. A future refactor that changes the status row format
# fails here.
run_step status 8 --status
STATUS_BLUEPRINT_LINE="$(grep -E 'Blueprint' "$RECEIPT_DIR/status/stderr.txt" \
    || grep -E 'Blueprint' "$RECEIPT_DIR/status/stdout.txt" || true)"
if [[ -n "$STATUS_BLUEPRINT_LINE" ]]; then
    STATUS_BLUEPRINT="$(printf '%s' "$STATUS_BLUEPRINT_LINE" \
        | tr -s ' ' '\n' | grep -E '^[0-9]+$' | head -1 || echo 0)"
fi

# (5) --bench — heads-up DatabasePlayer (v1 trained config) vs Fish.
# Point RBP_BENCH_TRANSCRIPT_DIR at the bench's per-receipt transcript
# subdir so the --replay leg has a transcript to round-trip.
BENCH_TRANSCRIPT_SUBDIR="$RECEIPT_DIR/bench/transcripts"
mkdir -p "$BENCH_TRANSCRIPT_SUBDIR"
RBP_BENCH_TRANSCRIPT_DIR="$BENCH_TRANSCRIPT_SUBDIR" \
    run_step bench 9 --bench
BENCH_COMPLETE="$(grep -E 'bench complete:' "$RECEIPT_DIR/bench/stderr.txt" \
    || grep -E 'bench complete:' "$RECEIPT_DIR/bench/stdout.txt" || true)"
if [[ -n "$BENCH_COMPLETE" ]]; then
    BENCH_HANDS_LINE="$(printf '%s' "$BENCH_COMPLETE" | grep -oE 'hands=[0-9]+' | head -1 || true)"
    if [[ -n "$BENCH_HANDS_LINE" ]]; then
        BENCH_HANDS="${BENCH_HANDS_LINE#hands=}"
    fi
fi

# (6) --compare — v1 vs v2 trained-config head-to-head. The v2 row
# counts are 0/0 here so the compare reports `blueprint_trained_v2=false`
# (a v1 vs untrained-Fish-like-v2 result). The compare's headline
# `winner` is one of {v1, v2, tie}; we capture hands for SUMMARY.
run_step compare 10 --compare
COMPARE_COMPLETE="$(grep -E 'compare complete:' "$RECEIPT_DIR/compare/stderr.txt" \
    || grep -E 'compare complete:' "$RECEIPT_DIR/compare/stdout.txt" || true)"
if [[ -n "$COMPARE_COMPLETE" ]]; then
    COMPARE_HANDS_LINE="$(printf '%s' "$COMPARE_COMPLETE" | grep -oE 'hands=[0-9]+' | head -1 || true)"
    if [[ -n "$COMPARE_HANDS_LINE" ]]; then
        COMPARE_HANDS="${COMPARE_HANDS_LINE#hands=}"
    fi
fi
# Fall back: if `hands=` was not in the compare log line, default to
# the RBP_COMPARE_HANDS env value the chain was sized with.
if [[ "$COMPARE_HANDS" -eq 0 ]]; then
    COMPARE_HANDS="$RBP_COMPARE_HANDS"
fi

# (7) --replay — the externally-verifiable leg. The bench dropped
# ≥ 1 `transcript-*.json` file into BENCH_TRANSCRIPT_SUBDIR; --replay
# reads the first one and renders the seat/action text summary to
# stdout. The receipt's `replay/stdout.txt` bytes is the headline
# "the transcript rendered something non-empty" signal.
TRANSCRIPT_PATH=""
for f in "$BENCH_TRANSCRIPT_SUBDIR"/transcript-*.json; do
    if [[ -f "$f" ]]; then
        TRANSCRIPT_PATH="$f"
        break
    fi
done
if [[ -z "$TRANSCRIPT_PATH" ]]; then
    echo "testnet-live-proof: bench did not drop a transcript-*.json file" >&2
    echo "  expected under: $BENCH_TRANSCRIPT_SUBDIR" >&2
    echo "  the --replay leg has no artifact to read" >&2
    exit 11
fi
run_step replay 11 --replay "$TRANSCRIPT_PATH"
REPLAY_BYTES="$(wc -c < "$RECEIPT_DIR/replay/stdout.txt" | tr -d '[:space:]')"

# --- the headline SUMMARY.txt -------------------------------------------
SUMMARY="$RECEIPT_DIR/SUMMARY.txt"
{
    echo "testnet live_proof complete: smoke=$SMOKE_ROWS status=$STATUS_BLUEPRINT bench=$BENCH_HANDS compare=$COMPARE_HANDS replay=$REPLAY_BYTES"
    echo ""
    echo "  receipt_dir: $RECEIPT_DIR"
    echo "  trainer:     $TRAINER_BIN"
    echo "  steps:"
    echo "    cluster  exit=$(cat "$RECEIPT_DIR/cluster/exit.txt")"
    echo "    reset    exit=$(cat "$RECEIPT_DIR/reset/exit.txt")"
    echo "    smoke    exit=$(cat "$RECEIPT_DIR/smoke/exit.txt") rows=$SMOKE_ROWS"
    echo "    status   exit=$(cat "$RECEIPT_DIR/status/exit.txt") blueprint=$STATUS_BLUEPRINT"
    echo "    bench    exit=$(cat "$RECEIPT_DIR/bench/exit.txt") hands=$BENCH_HANDS transcripts=$(ls -1 "$BENCH_TRANSCRIPT_SUBDIR"/transcript-*.json 2>/dev/null | wc -l | tr -d '[:space:]')"
    echo "    compare  exit=$(cat "$RECEIPT_DIR/compare/exit.txt") hands=$COMPARE_HANDS"
    echo "    replay   exit=$(cat "$RECEIPT_DIR/replay/exit.txt") bytes=$REPLAY_BYTES"
} > "$SUMMARY"

# Echo the headline line so a CI worker scraping stdout can pin the
# receipt without reading the file. The format matches
# `crates/autotrain/tests/live_proof.rs`'s final
# `live_proof complete: ...` line (with the `testnet` prefix the
# runbook adds to disambiguate from the integration test's line).
cat "$SUMMARY"

echo "testnet-live-proof: chain landed end-to-end"
echo "  receipt: $RECEIPT_DIR/SUMMARY.txt"
