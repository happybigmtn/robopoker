#!/usr/bin/env bash
# scripts/trainer-observe.sh — STW-045 trainer observability wrapper
#
# Pure-bash wrapper that runs an arbitrary `trainer --<mode> ...`
# invocation and writes a parallel per-step JSONL timeline
# (`<output-jsonl>`) a CI dashboard (or a future operator) can
# `jq -c .` for per-line timing + stream attribution. The wrapper
# is *transparent* to the trainer binary:
#
#   - It preserves the trainer's exit code (a non-zero trainer
#     exit still surfaces to the caller of this script, mirroring
#     the `run_step` helper in `scripts/testnet-live-proof.sh`).
#   - It forwards stdout and stderr to the wrapper's own stdout
#     and stderr unchanged (an operator running the wrapper
#     interactively sees the same trainer output they would see
#     without the wrapper).
#   - It does not touch the trainer binary, the autotrain crate,
#     the room protocol, the bench harness, the `Schema`
#     contracts, the K-means cluster counts, the v1 / v2 / v3 /
#     v4 named baselines, or any `trainer --*` CLI. The trainer
#     binary's own per-step log lines are unchanged; the wrapper
#     just *adds* a parallel machine-readable timeline.
#
# The shape contract this wrapper publishes (the contract a CI
# dashboard scrapes):
#
#   <output-jsonl> is a JSONL file. Every line is a JSON object
#   with exactly three top-level string-or-number fields:
#
#     {"ts": <int milliseconds since epoch>, "stream": <"stderr"|"stdout"|"summary">, "line": <the original line content>}
#
#   where:
#     - `ts`     is `date +%s%3N` at the moment the wrapper
#                observed the line, so a CI worker can compute
#                per-step wall-clock duration with
#                `jq -r '.ts' <output-jsonl> | python -c ...`.
#     - `stream` is the source stream the line came from on the
#                trainer binary: `stderr` (the trainer's primary
#                log surface, where `simplelog::TermLogger` writes
#                `INFO` / `WARN` lines), `stdout` (a non-empty
#                trainer stdout line, e.g. the bench's single-line
#                `BenchReport::to_json` JSON), or `summary` (the
#                single trailer line the wrapper emits on its own
#                after the trainer exits; the trailer's `line` is
#                a fixed-shape `trainer observe complete:
#                exit=<0|1|2> cmd=<argv...>` string the
#                `crates/autotrain/tests/trainer_observe.rs`
#                integration test pins).
#     - `line`   is the literal line content the trainer wrote,
#                with embedded `"` and `\` JSON-escaped so a
#                `jq` round-trip is byte-stable.
#
# Usage:
#   bash scripts/trainer-observe.sh <output-jsonl> <trainer-bin>
#                                <trainer-argv...>
#
#   <output-jsonl>   absolute or CWD-relative path to the JSONL
#                    timeline file. The wrapper refuses to run
#                    with exit 3 if the path is empty or its
#                    parent directory does not exist.
#   <trainer-bin>    absolute or PATH-relative path to the
#                    trainer binary (typically
#                    `<workspace>/target/debug/trainer`).
#   <trainer-argv...>  forwarded to the trainer binary verbatim.
#
# Environment:
#   TRAINER_OBSERVE_QUIET  (unset / 0) when set to 1, the
#                    wrapper suppresses its own per-invocation
#                    progress echo (the `trainer observe:
#                    jsonl=<path> trainer=<bin> argv=...` line
#                    that names the JSONL + the trainer binary +
#                    the argv) so a CI worker scraping stdout
#                    for the trainer's own output does not have
#                    to filter the wrapper's preamble. The
#                    JSONL file is always written.
#
# Exit codes:
#   0  trainer exited 0 end-to-end
#   1  wrapper-internal error (no jq, parent dir missing,
#      trainer binary missing, etc.)
#   3  missing positional arg
#   <trainer exit code>  the trainer binary's own exit code is
#                        preserved verbatim — a CI worker that
#                        `bash scripts/trainer-observe.sh
#                        /tmp/run.step.jsonl trainer --bench`
#                        receives the bench's exit code (typically
#                        0 on success, non-zero on a Postgres
#                        connection failure or a missing
#                        blueprint) at the end of the run.
#
# See `crates/autotrain/tests/script_shape.rs` for the
# shell-shape integration test that pins this script's static
# contract + `crates/autotrain/tests/trainer_observe.rs` for
# the no-DB-shape end-to-end integration test that drives a
# real `trainer --bench` invocation under the wrapper and
# asserts the JSONL timeline is parseable + has the documented
# three-field shape.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- preflight: jq is the only non-stdlib dep ----------------------------
# We use `jq` to JSON-encode every line the wrapper observes
# (the `line` field may contain `"` / `\` / control chars that
# a hand-rolled encoder would mishandle). A CI worker without
# `jq` on PATH would see the wrapper fail with a `command not
# found` error the moment the first stderr line arrives, which
# is too late — we gate on `jq` up front so the failure is
# loud and synchronous.
if ! command -v jq >/dev/null 2>&1; then
    echo "trainer-observe: \`jq\` not found on PATH; install jq (the JSONL encoder is a \`jq\` one-liner)" >&2
    exit 1
fi

# --- positional <output-jsonl> <trainer-bin> [argv...] arg gate ---------
OUTPUT_JSONL="${1:-}"
if [[ -z "$OUTPUT_JSONL" ]]; then
    echo "trainer-observe: missing positional arg <output-jsonl>" >&2
    echo "  example: bash scripts/trainer-observe.sh /tmp/run.step.jsonl \\" >&2
    echo "           $WORKSPACE_ROOT/target/debug/trainer --bench" >&2
    exit 3
fi
TRAINER_BIN="${2:-}"
if [[ -z "$TRAINER_BIN" ]]; then
    echo "trainer-observe: missing positional arg <trainer-bin>" >&2
    echo "  example: bash scripts/trainer-observe.sh /tmp/run.step.jsonl \\" >&2
    echo "           $WORKSPACE_ROOT/target/debug/trainer --bench" >&2
    exit 3
fi
# The remaining args are the trainer's argv; the wrapper
# forwards them verbatim to the trainer binary.
shift 2
TRAINER_ARGV=("$@")

# Make sure the JSONL's parent directory exists so a
# one-shot path under /tmp does not fail the wrapper's
# `tee` step.
OUTPUT_DIR="$(dirname "$OUTPUT_JSONL")"
if [[ ! -d "$OUTPUT_DIR" ]]; then
    echo "trainer-observe: parent directory of <output-jsonl> does not exist: $OUTPUT_DIR" >&2
    echo "  create the directory first (e.g. \`mkdir -p \"\$OUTPUT_DIR\"\`) or pick a different path" >&2
    exit 1
fi
# Truncate the JSONL so a re-run of the wrapper against the
# same file does not append to a stale timeline (a CI worker
# that points two consecutive runs at the same path gets
# exactly the second run's events, not a concatenation).
: >"$OUTPUT_JSONL"

# Resolve the trainer binary to an absolute path. The trainer
# binary is typically a target/debug/trainer that may not be
# on the wrapper caller's PATH; resolving up front means a
# `which` / `type` lookup at the end (for the trailer line)
# returns the same path the wrapper actually invoked.
if [[ "$TRAINER_BIN" != /* ]]; then
    # Either a relative path (e.g. `./target/debug/trainer`)
    # or a bare name (e.g. `trainer`) the caller expects to
    # resolve via PATH. We try `command -v` first, falling
    # back to `cd`-and-resolve for the relative case.
    if command -v "$TRAINER_BIN" >/dev/null 2>&1; then
        TRAINER_BIN="$(command -v "$TRAINER_BIN")"
    else
        echo "trainer-observe: trainer binary not found on PATH or filesystem: $TRAINER_BIN" >&2
        exit 1
    fi
fi
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "trainer-observe: trainer binary not executable: $TRAINER_BIN" >&2
    exit 1
fi

# --- header line (suppressed under TRAINER_OBSERVE_QUIET=1) --------------
if [[ "${TRAINER_OBSERVE_QUIET:-0}" != "1" ]]; then
    echo "trainer-observe: jsonl=$OUTPUT_JSONL trainer=$TRAINER_BIN argv=${TRAINER_ARGV[*]}" >&2
fi

# --- the per-line JSONL encoder -----------------------------------------
# emit_step <stream> <line>
#   Writes one JSONL line to $OUTPUT_JSONL. The `<line>` is
#   JSON-encoded via `jq -Rs .` (a "raw input, single JSON
#   string output" filter), which handles every escaping
#   edge case (embedded `"`, `\`, control chars, NUL bytes
#   that `tr` would mangle). The result is wrapped in a
#   small object with `ts` (the millisecond Unix epoch from
#   `date +%s%3N`) + `stream` (the literal string the
#   caller passed: `stderr` or `stdout`) + `line` (the
#   JSON-encoded line content). The `jq` call is the
#   bottleneck on a high-volume trainer run; the bench's
#   log stream is bounded (a 4-handed bench emits O(10)
#   `INFO` lines + 1 `WARN` line + 1 `JSON` line), so the
#   `jq` overhead is sub-millisecond per observed line.
emit_step() {
    local stream="$1"
    local line="$2"
    local ts
    ts="$(date +%s%3N)"
    # The `jq -cn --arg ts "$ts" --arg stream "$stream"
    #   --arg line "$line" '{ts: ($ts|tonumber),
    #   stream: $stream, line: $line}'` filter is a
    #   null-input object build: we pass the three
    #   already-substituted strings via `--arg` (which
    #   handles the escaping for us) and let `jq` build
    #   the object. The `($ts|tonumber)` cast converts
    #   the string `ts` to an integer so the JSONL
    #   consumer can do arithmetic on it without an
    #   extra `tonumber` round-trip.
    jq -cn --arg ts "$ts" --arg stream "$stream" --arg line "$line" \
        '{ts: ($ts|tonumber), stream: $stream, line: $line}' \
        >>"$OUTPUT_JSONL"
}

# --- the per-stream pipe ------------------------------------------------
# `tee` would be simpler, but `tee` does not give us a per-line
# callback the way bash's `while IFS= read -r` does. We use
# two background process substitutions: one to drain stderr
# into a `while read` loop that calls `emit_step stderr`,
# another to drain stdout into a `while read` loop that
# calls `emit_step stdout`. The two loops run in parallel
# and the trainer's exit code is captured via `wait $PID`.
# The original stdout / stderr are *also* written back to
# this script's own stdout / stderr (via `tee /dev/stderr` /
# `tee /dev/stdout` inside the loops) so an operator running
# the wrapper interactively sees the trainer's output live
# without having to read the JSONL.
STDERR_PIPE_FD="$(mktemp -u)"
STDOUT_PIPE_FD="$(mktemp -u)"
mkfifo "$STDERR_PIPE_FD" "$STDOUT_PIPE_FD"

# Start a background drainer for stderr. The drainer
# reads each line, mirrors it back to the wrapper's own
# stderr (so the operator sees the trainer's log stream
# live), and emits a JSONL line tagged `stream: "stderr"`.
# The `tee /dev/stderr` inside the subshell is the
# mirror; the `emit_step` call is the JSONL append.
(
    # shellcheck disable=SC2162
    while IFS= read -r line; do
        printf '%s\n' "$line" >&2
        emit_step stderr "$line"
    done <"$STDERR_PIPE_FD"
) &
STDERR_DRAIN_PID=$!

# Same shape for stdout.
(
    # shellcheck disable=SC2162
    while IFS= read -r line; do
        printf '%s\n' "$line"
        emit_step stdout "$line"
    done <"$STDOUT_PIPE_FD"
) &
STDOUT_DRAIN_PID=$!

# Now invoke the trainer with the two pipe paths swapped
# onto its stderr / stdout. We open the FIFO write ends
# in the current shell and exec the trainer; the
# `set +e` / `set -e` dance is so a non-zero trainer
# exit code does not abort the wrapper (we want to
# capture the exit code in `$TRAINER_RC` first, emit the
# summary line, and then return the exit code to the
# caller).
set +e
"$TRAINER_BIN" "${TRAINER_ARGV[@]}" \
    2>"$STDERR_PIPE_FD" \
    >"$STDOUT_PIPE_FD"
TRAINER_RC=$?
set -e

# The trainer's exit closes its write end of each FIFO,
# which makes the background drainers' `read` calls see
# EOF and exit. `wait` reaps them.
wait "$STDERR_DRAIN_PID" || true
wait "$STDOUT_DRAIN_PID" || true

# --- summary trailer line -----------------------------------------------
# The summary line is a single `stream: "summary"` JSONL
# row whose `line` field is a fixed-shape
# `trainer observe complete: exit=<rc> cmd=<argv...>`
# string. The format is pinned by
# `crates/autotrain/tests/trainer_observe.rs`; a CI
# dashboard can `jq -c 'select(.stream == "summary")'
# <output-jsonl>` and receive a one-line per-run summary
# without re-parsing the trainer's own log stream.
SUMMARY_LINE="trainer observe complete: exit=$TRAINER_RC cmd=${TRAINER_ARGV[*]}"
emit_step summary "$SUMMARY_LINE"

# --- cleanup tempfiles ---------------------------------------------------
rm -f "$STDERR_PIPE_FD" "$STDOUT_PIPE_FD"

# --- wrapper-progress echo (suppressed under TRAINER_OBSERVE_QUIET=1) --
if [[ "${TRAINER_OBSERVE_QUIET:-0}" != "1" ]]; then
    echo "trainer-observe: chain landed (exit=$TRAINER_RC, jsonl=$OUTPUT_JSONL)" >&2
fi

# Preserve the trainer's exit code. A CI worker that drives
# `bash scripts/trainer-observe.sh <jsonl> <bin> --bench`
# via `set -e` and asserts exit 0 sees the bench's actual
# exit code here, not a wrapper-internal success.
exit "$TRAINER_RC"
