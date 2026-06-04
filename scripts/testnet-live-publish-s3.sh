#!/usr/bin/env bash
# scripts/testnet-live-publish-s3.sh — STW-033 testnet live launch
# remote-publish runbook (the S3 / GCS / git-tag upload step the
# STW-032 runbook doc names as the "next slice
# (`testnet-live-publish`)").
#
# Reads a `receipts/testnet-live-proof-<UTC-ISO>/` directory the
# STW-019 `testnet-live-proof.sh` runbook produced (or
# `LiveProofReceipt::write_to` synthesised) AND a publish bundle
# directory the STW-032 `testnet-live-publish.sh` runbook produced,
# and either (a) writes a deterministic upload plan + a
# `remote_receipt.json` post-upload manifest to
# `<bundle_dir>/remote/` (dry-run, the default) or (b) shells out
# to `aws s3 cp` per `s3_objects[]` and writes the post-upload
# `remote_receipt.json` (live, gated by `--no-dry-run`).
#
# Remote layout (drop into
# `<bundle_dir>/remote/testnet-live-proof-<UTC-ISO>/`):
#
#   <bundle_dir>/remote/
#     remote_plan.json       # per-file local_path -> s3_uri mapping
#     remote_receipt.json    # post-upload per-file sha256 + bytes
#
# The remote-publish step is **read-only** with respect to the
# receipt + the publish bundle. The script invokes the
# `trainer` CLI's remote-publish arm (which copies the
# receipt + bundle references into a fresh `staging/`
# tempdir for sha256 verification) and never opens the
# original receipt or bundle for write. A partial-failure
# path leaves both the receipt and the bundle untouched.
#
# The script refuses to plan an upload for a *red* receipt — it
# shells out to `trainer --verify-receipt <dir>` (the STW-028
# verifier) before the remote-upload step, and bails with exit 5
# if the receipt doesn't pass. This is the "no paper-over" gate
# the receipt verifier is the source of truth for: a remote
# upload of a red receipt is a hard error, not a warning.
#
# Environment:
#   RECEIPT_DIR       Path to a
#                     `receipts/testnet-live-proof-<UTC-ISO>/`
#                     directory. REQUIRED. The script refuses to
#                     run with exit 3 if the path is missing or
#                     not a directory.
#   PUBLISH_BUCKET    Bucket URI (`s3://<name>/`) or bare
#                     bucket name (`<name>`). REQUIRED. The
#                     script refuses to run with exit 3 if the
#                     bucket is missing.
#   PUBLISH_PREFIX    Key prefix inside the bucket
#                     (default `<basename>/`).
#   PUBLISH_DIR       Path to the STW-032 publish bundle
#                     directory the
#                     `testnet-live-publish.sh` runbook
#                     produced. Default
#                     `<receipt_parent>/publish/<basename>/`.
#   TRAINER_BIN       (default <workspace>/target/debug/trainer)
#                     Path to the trainer binary. If the file is
#                     missing the script runs `cargo build --bin
#                     trainer` first. Set to skip the build (e.g.
#                     when pointing at a `--release` binary).
#   PUBLISH_DRY_RUN   (default `1`) Whether the
#                     remote-upload trainer arm runs in
#                     dry-run. Set to `0` to shell out to
#                     `aws s3 cp` per file (requires the `aws`
#                     CLI to be on `$PATH` and the shell to have
#                     `AWS_ACCESS_KEY_ID` /
#                     `AWS_SECRET_ACCESS_KEY` env knobs set; a
#                     missing `aws` returns
#                     `PublishRemoteError::AwsCli` and the
#                     arm exits 2).
#   RBP_PUBLISH_REMOTE_UTC  Set automatically to the current
#                     `date -u +%Y-%m-%dT%H:%M:%SZ` if unset.
#                     The remote-plan + remote-receipt's
#                     `created_at_utc` /
#                     `uploaded_at_utc` field reflect this knob
#                     so a downstream auditor can re-fetch the
#                     receipt + assert the upload happened in
#                     the expected UTC window. The fallback
#                     `<unknown>` sentinel keeps the manifest
#                     byte-stable when the env knob is unset
#                     (the lib test + the committed
#                     `publish-remote-fixture/` use this
#                     sentinel).
#
# Exit codes:
#   0  remote-receipt written end-to-end; upload plan + post-
#      upload receipt landed under `<publish>/<basename>/remote/`
#   3  RECEIPT_DIR missing or not a directory, or PUBLISH_BUCKET
#      missing (refuse-to-run gate)
#   4  trainer binary not found and `cargo build` failed
#   5  `trainer --verify-receipt <receipt>` exited non-zero
#      (red receipt: refuse to plan an upload)
#   6  `trainer --verify-bundle <bundle>` exited non-zero
#      (red publish bundle: refuse to plan an upload)
#   7  the remote-upload trainer arm exited non-zero
#      (the remote-upload step itself failed)
#   8  `trainer --verify-remote <remote>` exited non-zero
#      (the post-upload re-verify failed)
#
# Usage:
#   bash scripts/testnet-live-publish-s3.sh \
#       receipts/testnet-live-proof-20260604T050000Z/ \
#       s3://robopoker-testnet-dashboard
#
# See `scripts/testnet-live-publish.md` for the publish runbook
# and `crates/autotrain/tests/script_shape.rs` for the shell-shape
# integration test that pins this script's contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- argv / env-knob parsing --------------------------------------------
# Positional: RECEIPT_DIR (REQUIRED) [PUBLISH_BUCKET (REQUIRED)].
# Env knobs: PUBLISH_PREFIX, PUBLISH_DIR, PUBLISH_DRY_RUN, TRAINER_BIN,
# RBP_PUBLISH_REMOTE_UTC.
if [[ -z "${RECEIPT_DIR:-}" && $# -ge 1 ]]; then
    RECEIPT_DIR="$1"
    shift
fi
if [[ -z "${RECEIPT_DIR:-}" ]]; then
    echo "testnet-live-publish-s3: RECEIPT_DIR must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-s3.sh \\" >&2
    echo "           receipts/testnet-live-proof-20260604T050000Z/ \\" >&2
    echo "           s3://robopoker-testnet-dashboard" >&2
    exit 3
fi
if [[ ! -d "$RECEIPT_DIR" ]]; then
    echo "testnet-live-publish-s3: receipt dir $RECEIPT_DIR does not exist or is not a directory" >&2
    exit 3
fi
if [[ -z "${PUBLISH_BUCKET:-}" && $# -ge 1 ]]; then
    PUBLISH_BUCKET="$1"
    shift
fi
if [[ -z "${PUBLISH_BUCKET:-}" ]]; then
    echo "testnet-live-publish-s3: PUBLISH_BUCKET must be set" >&2
    echo "  example: PUBLISH_BUCKET=s3://robopoker-testnet-dashboard \\" >&2
    echo "           bash scripts/testnet-live-publish-s3.sh receipts/.../" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-publish-s3: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-publish-s3: cargo build failed" >&2
        exit 4
    fi
fi

# --- publish dir + prefix -------------------------------------------------
# Compute the publish bundle directory the STW-032
# `testnet-live-publish.sh` runbook produced. Default:
# `<receipt_parent>/publish/<basename>/` so the script writes
# next to the bundle without ever overwriting it.
RECEIPT_BASENAME="$(basename "$RECEIPT_DIR")"
RECEIPT_PARENT="$(cd "$(dirname "$RECEIPT_DIR")" && pwd)"
if [[ -z "${PUBLISH_DIR:-}" ]]; then
    PUBLISH_DIR="$RECEIPT_PARENT/publish/$RECEIPT_BASENAME"
fi
if [[ ! -d "$PUBLISH_DIR" ]]; then
    echo "testnet-live-publish-s3: publish bundle dir $PUBLISH_DIR does not exist or is not a directory" >&2
    echo "  run scripts/testnet-live-publish.sh first to materialise the STW-032 bundle" >&2
    exit 3
fi
# Default the prefix to `<basename>/` if the operator
# passes `--prefix ''` (the same `PUBLISH_PREFIX` choice
# the STW-033 remote-upload trainer arm makes).
if [[ -z "${PUBLISH_PREFIX:-}" ]]; then
    PUBLISH_PREFIX="$RECEIPT_BASENAME/"
fi

# Default the upload UTC timestamp. The `RBP_PUBLISH_REMOTE_UTC`
# env knob is the timestamp the publish-remote step stamps on
# the upload plan + receipt; a future auditor can re-fetch +
# assert the upload happened in the expected window.
if [[ -z "${RBP_PUBLISH_REMOTE_UTC:-}" ]]; then
    RBP_PUBLISH_REMOTE_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo '<unknown>')"
fi
export RBP_PUBLISH_REMOTE_UTC

# Default the dry-run knob to `1` so a `cargo test --workspace`
# invocation runs the runbook without shelling out to `aws`.
: "${PUBLISH_DRY_RUN:=1}"
export PUBLISH_DRY_RUN

# --- pre-publish gate: refuse to publish a red receipt -------------------
# The STW-019 runbook's receipt is the source of truth; a
# receipt the runbook produced is green iff the per-step
# `exit.txt` files are all 0. The `trainer --verify-receipt
# <path>` CLI (STW-028) is the typed Rust verifier the lib
# test + the integration test both pin, so a `trainer
# --verify-receipt` shell-out is the canonical "is this
# receipt green?" check.
echo "testnet-live-publish-s3: verifying receipt $RECEIPT_DIR"
if ! "$TRAINER_BIN" --verify-receipt "$RECEIPT_DIR" \
        >"$PUBLISH_DIR/.verify-receipt.stdout" \
        2>"$PUBLISH_DIR/.verify-receipt.stderr"; then
    echo "testnet-live-publish-s3: receipt verifier rejected the receipt" >&2
    echo "  stdout: $PUBLISH_DIR/.verify-receipt.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.verify-receipt.stderr" >&2
    echo "  refusing to plan an upload for a red receipt" >&2
    rm -f "$PUBLISH_DIR/.verify-receipt.stdout" "$PUBLISH_DIR/.verify-receipt.stderr"
    exit 5
fi
rm -f "$PUBLISH_DIR/.verify-receipt.stdout" "$PUBLISH_DIR/.verify-receipt.stderr"

# --- pre-upload gate: refuse to upload a red publish bundle -------------
# The STW-032 publish bundle is the source of truth for the
# per-file bytes + sha256 the upload plan re-hashes against.
# A red bundle (a tampered file, a missing
# `manifest.json`, etc.) short-circuits the upload with
# `BundleHashMismatch` from the publish-remote arm; we
# re-verify with the dedicated
# `trainer --verify-bundle <dir>` CLI as a hard
# pre-upload gate so a CI worker that shells out to
# `aws s3 cp` does not push a red bundle to a dashboard
# bucket.
echo "testnet-live-publish-s3: verifying publish bundle $PUBLISH_DIR"
if ! "$TRAINER_BIN" --verify-bundle "$PUBLISH_DIR" \
        >"$PUBLISH_DIR/.verify-bundle.stdout" \
        2>"$PUBLISH_DIR/.verify-bundle.stderr"; then
    echo "testnet-live-publish-s3: bundle verifier rejected the publish bundle" >&2
    echo "  stdout: $PUBLISH_DIR/.verify-bundle.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.verify-bundle.stderr" >&2
    echo "  refusing to plan an upload for a red publish bundle" >&2
    rm -f "$PUBLISH_DIR/.verify-bundle.stdout" "$PUBLISH_DIR/.verify-bundle.stderr"
    exit 6
fi
rm -f "$PUBLISH_DIR/.verify-bundle.stdout" "$PUBLISH_DIR/.verify-bundle.stderr"

# --- the publish-remote step --------------------------------------------
# `trainer --publish-remote <receipt-dir> --bucket <s3://...>
# [--prefix <prefix/>]` reads the receipt + the STW-032
# publish bundle, re-verifies both as pre-upload gates, and
# writes a deterministic upload plan + a post-upload
# `remote_receipt.json` to `<publish>/<basename>/remote/`.
# In dry-run (the default), the arm does NOT shell out to
# `aws`; in live mode (PUBLISH_DRY_RUN=0), the arm shells
# out to `aws s3 cp` per file in the plan.
echo "testnet-live-publish-s3: writing remote-receipt to $PUBLISH_DIR/remote/"
REMOTE_ARGS=(
    --publish-remote "$RECEIPT_DIR"
    --bucket "$PUBLISH_BUCKET"
    --prefix "$PUBLISH_PREFIX"
)
if [[ "$PUBLISH_DRY_RUN" != "1" ]]; then
    REMOTE_ARGS+=(--no-dry-run)
fi
if ! "$TRAINER_BIN" "${REMOTE_ARGS[@]}" \
        >"$PUBLISH_DIR/.publish-remote.stdout" \
        2>"$PUBLISH_DIR/.publish-remote.stderr"; then
    echo "testnet-live-publish-s3: trainer --publish-remote failed" >&2
    echo "  stdout: $PUBLISH_DIR/.publish-remote.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.publish-remote.stderr" >&2
    # On error path, leave the .publish-remote.{stdout,stderr}
    # files in place so an operator can inspect what went
    # wrong.
    exit 7
fi
rm -f "$PUBLISH_DIR/.publish-remote.stdout" "$PUBLISH_DIR/.publish-remote.stderr"

# --- post-upload re-verify (the verifier the bash runbook
#     also runs so a CI worker can confirm the on-disk
#     `remote_receipt.json` is internally consistent) ----
echo "testnet-live-publish-s3: re-verifying remote-receipt at $PUBLISH_DIR/remote/"
if ! "$TRAINER_BIN" --verify-remote "$PUBLISH_DIR/remote" \
        >"$PUBLISH_DIR/.verify-remote.stdout" \
        2>"$PUBLISH_DIR/.verify-remote.stderr"; then
    echo "testnet-live-publish-s3: trainer --verify-remote failed" >&2
    echo "  stdout: $PUBLISH_DIR/.verify-remote.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.verify-remote.stderr" >&2
    # On error path, leave the .verify-remote.{stdout,stderr}
    # files in place so an operator can inspect what went
    # wrong.
    exit 8
fi
rm -f "$PUBLISH_DIR/.verify-remote.stdout" "$PUBLISH_DIR/.verify-remote.stderr"

# --- the headline SUMMARY.txt -------------------------------------------
SUMMARY="$PUBLISH_DIR/remote/SUMMARY.txt"
{
    echo "testnet live_proof publish_remote complete: receipt=$RECEIPT_BASENAME bucket=$PUBLISH_BUCKET prefix=$PUBLISH_PREFIX"
    echo ""
    echo "  receipt_dir: $RECEIPT_DIR"
    echo "  publish_dir: $PUBLISH_DIR"
    echo "  remote_dir:  $PUBLISH_DIR/remote"
    echo "  bucket:      $PUBLISH_BUCKET"
    echo "  prefix:      $PUBLISH_PREFIX"
    echo "  trainer:     $TRAINER_BIN"
    echo "  uploaded_at: $RBP_PUBLISH_REMOTE_UTC"
    echo "  dry_run:     $PUBLISH_DRY_RUN"
    echo "  files:"
    if [[ -f "$PUBLISH_DIR/remote/remote_plan.json" ]]; then
        echo "    remote_plan.json    $(wc -c < "$PUBLISH_DIR/remote/remote_plan.json" | tr -d '[:space:]') bytes"
    fi
    if [[ -f "$PUBLISH_DIR/remote/remote_receipt.json" ]]; then
        echo "    remote_receipt.json $(wc -c < "$PUBLISH_DIR/remote/remote_receipt.json" | tr -d '[:space:]') bytes"
    fi
} > "$SUMMARY"

# Echo the headline line so a CI worker scraping stdout can
# pin the publish-remote step without reading the file. The
# format mirrors the
# `crates/autotrain/tests/publish_remote.rs` integration
# test's `live_proof publish_remote complete: ...` line a
# future dashboard scraper greps the log for.
cat "$SUMMARY"

echo "testnet-live-publish-s3: chain landed end-to-end"
echo "  summary:    $SUMMARY"
echo "  re-verify:  $TRAINER_BIN --verify-remote $PUBLISH_DIR/remote"
