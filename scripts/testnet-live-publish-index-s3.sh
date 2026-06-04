#!/usr/bin/env bash
# scripts/testnet-live-publish-index-s3.sh — STW-035 testnet live launch
# publish-index-remote runbook (the v9 follow-on the STW-034
# `testnet-live-publish-index.sh` runbook doc's scope-boundary
# section defers to: a CI worker that has run the STW-034
# publish-index step wants to `aws s3 cp` the local
# `INDEX.json` to a dashboard bucket).
#
# Reads a `<publish-root>/index/INDEX.json` the STW-034
# `testnet-live-publish-index.sh` runbook produced and (a) writes
# a deterministic upload plan
# (`<publish-root>/index_remote/index_remote_plan.json`,
# `s3_objects[]` sorted by `s3_uri`) + a post-upload
# `index_remote_receipt.json` (mirrors the plan with
# `uploaded_at_utc` + the `INDEX.json` sha256 + bytes) to
# `<publish-root>/index_remote/` (dry-run, the default) or (b)
# shells out to `aws s3 cp` per `s3_objects[]` and writes the
# post-upload `index_remote_receipt.json` (live, gated by
# `--no-dry-run`).
#
# Remote layout (drop into
# `<publish_root>/index_remote/`):
#
#   <publish_root>/index_remote/
#     index_remote_plan.json       # per-file INDEX.json -> s3_uri mapping
#     index_remote_receipt.json    # post-upload per-file sha256 + bytes
#     SUMMARY.txt                  # single-line headline a CI worker `cat`s
#
# The publish-index-remote step is **read-only** with respect to
# the publish root + the `INDEX.json`. The script invokes the
# `trainer` CLI's publish-index-remote arm (which re-verifies the
# STW-034 `INDEX.json` as a pre-upload gate, builds an upload
# plan, and either writes the plan + a stub
# `index_remote_receipt.json` in dry-run or shells out to
# `aws s3 cp` in live mode) and never opens the original
# `INDEX.json` for write. A partial-failure path leaves the
# `INDEX.json` untouched.
#
# The script refuses to upload a *red* `INDEX.json` — it shells
# out to `trainer --verify-index <index-dir>` (the STW-034
# verifier) before the publish-index-remote step, and bails with
# exit 5 if the index doesn't pass. This is the "no paper-over"
# gate the STW-034 index verifier is the source of truth for:
# a publish-index-remote of a red index is a hard error, not a
# warning.
#
# Environment:
#   PUBLISH_ROOT    Path to a `<publish-root>/` directory the
#                   STW-034 `testnet-live-publish-index.sh`
#                   runbook produced. REQUIRED. The script
#                   refuses to run with exit 3 if the path is
#                   missing or not a directory.
#   PUBLISH_BUCKET  Bucket URI (`s3://<name>/`) or bare bucket
#                   name (`<name>`). REQUIRED. The script refuses
#                   to run with exit 3 if the bucket is missing.
#   PUBLISH_PREFIX  Key prefix inside the bucket
#                   (default `<root-basename>/index/`).
#   TRAINER_BIN     (default <workspace>/target/debug/trainer)
#                   Path to the trainer binary. If the file is
#                   missing the script runs `cargo build --bin
#                   trainer` first. Set to skip the build (e.g.
#                   when pointing at a `--release` binary).
#   PUBLISH_DRY_RUN (default `1`) Whether the
#                   publish-index-remote trainer arm runs in
#                   dry-run. Set to `0` to shell out to
#                   `aws s3 cp` per file (requires the `aws`
#                   CLI to be on `$PATH` and the shell to have
#                   `AWS_ACCESS_KEY_ID` /
#                   `AWS_SECRET_ACCESS_KEY` env knobs set; a
#                   missing `aws` returns
#                   `PublishIndexRemoteError::AwsCli` and the
#                   arm exits 2).
#   RBP_PUBLISH_INDEX_REMOTE_UTC  Set automatically to the current
#                   `date -u +%Y-%m-%dT%H:%M:%SZ` if unset.
#                   The index-remote-plan + index-remote-receipt's
#                   `created_at_utc` /
#                   `uploaded_at_utc` field reflect this knob
#                   so a downstream auditor can re-fetch +
#                   assert the upload happened in the expected
#                   UTC window. The fallback `<unknown>`
#                   sentinel keeps the manifest byte-stable
#                   when the env knob is unset (the lib test +
#                   the integration test use this sentinel).
#
# Exit codes:
#   0  index-remote written end-to-end; upload plan + post-
#      upload receipt landed under `<publish>/index_remote/`
#   3  PUBLISH_ROOT missing or not a directory, or PUBLISH_BUCKET
#      missing (refuse-to-run gate)
#   4  trainer binary not found and `cargo build` failed
#   5  `trainer --verify-index <index-dir>` exited non-zero
#      (red `INDEX.json` detected, or other index error:
#      refuse to upload a red index)
#   6  `trainer --publish-index-remote <root>` exited non-zero
#      (the plan + post-upload-receipt writer failed)
#   7  `trainer --verify-index-remote <remote-dir>` exited
#      non-zero (the post-upload re-verify failed)
#
# Usage:
#   bash scripts/testnet-live-publish-index-s3.sh <publish-root> <s3://bucket>
#
# See `scripts/testnet-live-publish-index-s3.md` for the
# index-remote runbook and `crates/autotrain/tests/script_shape.rs`
# for the shell-shape integration test that pins this script's
# contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- argv / env-knob parsing --------------------------------------------
# Positional: PUBLISH_ROOT (REQUIRED), PUBLISH_BUCKET (REQUIRED).
# Env knobs: PUBLISH_PREFIX, TRAINER_BIN, PUBLISH_DRY_RUN,
#            RBP_PUBLISH_INDEX_REMOTE_UTC.
if [[ $# -ge 1 ]]; then
    PUBLISH_ROOT="$1"
    shift
fi
if [[ $# -ge 1 ]]; then
    PUBLISH_BUCKET="$1"
    shift
fi
if [[ -z "${PUBLISH_ROOT:-}" ]]; then
    echo "testnet-live-publish-index-s3: PUBLISH_ROOT must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-index-s3.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z/ s3://robopoker-testnet-dashboard" >&2
    exit 3
fi
if [[ -z "${PUBLISH_BUCKET:-}" ]]; then
    echo "testnet-live-publish-index-s3: PUBLISH_BUCKET must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-index-s3.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z/ s3://robopoker-testnet-dashboard" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT" ]]; then
    echo "testnet-live-publish-index-s3: publish root $PUBLISH_ROOT does not exist or is not a directory" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT/index" ]]; then
    echo "testnet-live-publish-index-s3: publish root $PUBLISH_ROOT has no index/ subdirectory" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi
if [[ ! -f "$PUBLISH_ROOT/index/INDEX.json" ]]; then
    echo "testnet-live-publish-index-s3: INDEX.json missing at $PUBLISH_ROOT/index/INDEX.json" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-publish-index-s3: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-publish-index-s3: cargo build failed" >&2
        exit 4
    fi
fi

# Default the index-remote UTC timestamp. The
# `RBP_PUBLISH_INDEX_REMOTE_UTC` env knob is the timestamp the
# index-remote stamps on the
# `index_remote_receipt.json`'s `created_at_utc` /
# `uploaded_at_utc` field; a future auditor can re-fetch +
# assert the index-remote was written in the expected window.
# The fallback `<unknown>` sentinel keeps the manifest
# byte-stable when the env knob is unset (the lib test + the
# integration test use this sentinel).
if [[ -z "${RBP_PUBLISH_INDEX_REMOTE_UTC:-}" ]]; then
    RBP_PUBLISH_INDEX_REMOTE_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo '<unknown>')"
fi
export RBP_PUBLISH_INDEX_REMOTE_UTC

# Default the dry-run knob to `1` so a `cargo test --workspace`
# invocation runs the runbook without shelling out to `aws`.
: "${PUBLISH_DRY_RUN:=1}"
export PUBLISH_DRY_RUN

# --- pre-upload gate: refuse to upload a red INDEX.json ------------------
# The STW-034 `INDEX.json` is the source of truth for the
# aggregator a dashboard scrapes. A red `INDEX.json` (a tampered
# per-entry `remote_receipt.s3_objects[].sha256`, a missing
# `INDEX.json`, a missing per-entry `remote_receipt.json`)
# short-circuits the upload with
# `PublishIndexRemoteError::IndexRed(...)` from the
# publish-index-remote arm; we re-verify with the dedicated
# `trainer --verify-index <index-dir>` CLI as a hard pre-upload
# gate so a CI worker that shells out to `aws s3 cp` does not push
# a red aggregator to a dashboard bucket.
INDEX_DIR="$PUBLISH_ROOT/index"
echo "testnet-live-publish-index-s3: verifying INDEX.json at $INDEX_DIR"
if ! "$TRAINER_BIN" --verify-index "$INDEX_DIR" \
        >"$PUBLISH_ROOT/.verify-index.stdout" \
        2>"$PUBLISH_ROOT/.verify-index.stderr"; then
    echo "testnet-live-publish-index-s3: index verifier rejected the INDEX.json" >&2
    echo "  stdout: $PUBLISH_ROOT/.verify-index.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.verify-index.stderr" >&2
    echo "  refusing to plan an upload for a red index" >&2
    rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"
    exit 5
fi
rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"

# --- the publish-index-remote step --------------------------------------
# `trainer --publish-index-remote <publish-root> --bucket <s3://...>
# [--prefix <prefix/>]` reads the `INDEX.json` the STW-034
# chain wrote, re-verifies it as a pre-upload gate, and writes a
# deterministic upload plan + a post-upload
# `index_remote_receipt.json` to
# `<publish-root>/index_remote/`. In dry-run (the default), the
# arm does NOT shell out to `aws`; in live mode
# (PUBLISH_DRY_RUN=0), the arm shells out to `aws s3 cp` per
# file in the plan.
echo "testnet-live-publish-index-s3: writing index-remote-receipt to $PUBLISH_ROOT/index_remote/"
REMOTE_ARGS=(
    --publish-index-remote "$PUBLISH_ROOT"
    --bucket "$PUBLISH_BUCKET"
    --prefix "${PUBLISH_PREFIX:-}"
)
if [[ "$PUBLISH_DRY_RUN" != "1" ]]; then
    REMOTE_ARGS+=(--no-dry-run)
fi
if ! "$TRAINER_BIN" "${REMOTE_ARGS[@]}" \
        >"$PUBLISH_ROOT/.publish-index-remote.stdout" \
        2>"$PUBLISH_ROOT/.publish-index-remote.stderr"; then
    echo "testnet-live-publish-index-s3: trainer --publish-index-remote failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.publish-index-remote.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.publish-index-remote.stderr" >&2
    # On error path, leave the .publish-index-remote.{stdout,stderr}
    # files in place so an operator can inspect what went wrong.
    exit 6
fi
rm -f "$PUBLISH_ROOT/.publish-index-remote.stdout" "$PUBLISH_ROOT/.publish-index-remote.stderr"

# --- post-upload re-verify (the verifier the bash runbook
#     also runs so a CI worker can confirm the on-disk
#     `index_remote_receipt.json` is internally consistent) ----
echo "testnet-live-publish-index-s3: re-verifying index-remote-receipt at $PUBLISH_ROOT/index_remote/"
if ! "$TRAINER_BIN" --verify-index-remote "$PUBLISH_ROOT/index_remote" \
        >"$PUBLISH_ROOT/.verify-index-remote.stdout" \
        2>"$PUBLISH_ROOT/.verify-index-remote.stderr"; then
    echo "testnet-live-publish-index-s3: trainer --verify-index-remote failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.verify-index-remote.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.verify-index-remote.stderr" >&2
    # On error path, leave the .verify-index-remote.{stdout,stderr}
    # files in place so an operator can inspect what went wrong.
    exit 7
fi
rm -f "$PUBLISH_ROOT/.verify-index-remote.stdout" "$PUBLISH_ROOT/.verify-index-remote.stderr"

# --- the headline SUMMARY.txt -------------------------------------------
SUMMARY="$PUBLISH_ROOT/index_remote/SUMMARY.txt"
{
    echo "testnet live_proof publish_index_remote complete: root=$PUBLISH_ROOT bucket=$PUBLISH_BUCKET prefix=${PUBLISH_PREFIX:-}"
    echo ""
    echo "  publish_root: $PUBLISH_ROOT"
    echo "  index_dir:    $PUBLISH_ROOT/index"
    echo "  remote_dir:   $PUBLISH_ROOT/index_remote"
    echo "  bucket:       $PUBLISH_BUCKET"
    echo "  prefix:       ${PUBLISH_PREFIX:-}"
    echo "  trainer:      $TRAINER_BIN"
    echo "  uploaded_at:  $RBP_PUBLISH_INDEX_REMOTE_UTC"
    echo "  dry_run:      $PUBLISH_DRY_RUN"
    echo "  files:"
    if [[ -f "$PUBLISH_ROOT/index_remote/index_remote_plan.json" ]]; then
        echo "    index_remote_plan.json    $(wc -c < "$PUBLISH_ROOT/index_remote/index_remote_plan.json" | tr -d '[:space:]') bytes"
    fi
    if [[ -f "$PUBLISH_ROOT/index_remote/index_remote_receipt.json" ]]; then
        echo "    index_remote_receipt.json $(wc -c < "$PUBLISH_ROOT/index_remote/index_remote_receipt.json" | tr -d '[:space:]') bytes"
    fi
} > "$SUMMARY"

# Echo the headline line so a CI worker scraping stdout can
# pin the publish-index-remote step without reading the file.
# The format mirrors the
# `crates/autotrain/tests/publish_index_remote.rs` integration
# test's `live_proof publish_index_remote complete: ...` line a
# future dashboard scraper greps the log for.
cat "$SUMMARY"

echo "testnet-live-publish-index-s3: chain landed end-to-end"
echo "  summary:    $SUMMARY"
echo "  re-verify:  $TRAINER_BIN --verify-index-remote $PUBLISH_ROOT/index_remote"
