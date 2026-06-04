#!/usr/bin/env bash
# scripts/testnet-live-publish-dashboard.sh — STW-036 testnet live
# launch dashboard-deploy runbook (the v10 follow-on the
# STW-035 publish-index-remote runbook doc defers to).
#
# Reads a `<publish-root>/index/INDEX.json` the
# STW-035 publish-index-remote step produced (the
# dashboard data feed), and `aws s3 sync`s the
# `<publish-root>/index/` directory to a public S3 /
# Cloudflare Pages bucket the dashboard's
# `RBP_DASHBOARD_INDEX_URL` env knob points at.
#
# This runbook is the *deploy* half of the v10
# follow-on: a CI worker that produced an
# `INDEX.json` (via the STW-034 → STW-035 chain) and
# the `transcripts/` directory the bench wrote can
# bring up a public dashboard in one `aws s3 sync`
# step.
#
# Remote layout (drop into
# `<bucket>/<prefix>/`):
#
#   <bucket>/<prefix>/
#     INDEX.json      # the STW-034 aggregator
#     SUMMARY.txt     # the STW-034 headline
#
# The dashboard's `IndexClient` reads the
# bucket-hosted `INDEX.json` via the
# `RBP_DASHBOARD_INDEX_URL` env knob (default
# `http://localhost:8080/api/index` in tests,
# `<bucket-url>/INDEX.json` in production). The
# typed read re-uses `rbp_autotrain::PublishIndex`,
# so a shape drift in `INDEX.json` fails BOTH the
# dashboard's typed read AND the
# `trainer --verify-index` re-verify at the same CI
# step.
#
# The runbook refuses to deploy a *red* index — it
# shells out to `trainer --verify-index <index-dir>`
# (the STW-034 verifier) before the `aws s3 sync`
# step, and bails with exit 5 if the index doesn't
# pass. This is the "no paper-over" gate the
# STW-034 index verifier is the source of truth
# for: a dashboard-deploy of a red index is a hard
# error, not a warning.
#
# Environment:
#   PUBLISH_ROOT    Path to a `<publish-root>/`
#                   directory the STW-034
#                   `testnet-live-publish-index.sh`
#                   runbook produced. REQUIRED.
#                   The script refuses to run with
#                   exit 3 if the path is missing or
#                   not a directory.
#   PUBLISH_BUCKET  Bucket URI (`s3://<name>/`) or
#                   bare bucket name (`<name>`).
#                   REQUIRED. The script refuses to
#                   run with exit 3 if the bucket is
#                   missing.
#   PUBLISH_PREFIX  Key prefix inside the bucket
#                   (default `<root-basename>/index/`).
#   AWS_BIN         (default `aws`) Path to the
#                   `aws` CLI. If the file is missing
#                   the script exits with code 4
#                   (no on-demand install — the
#                   `aws` CLI is a system dep the
#                   deploy host is expected to ship,
#                   the same way `cargo` is).
#
# Exit codes:
#   0  index deployed end-to-end; `INDEX.json` +
#      `SUMMARY.txt` landed under
#      `<bucket>/<prefix>/`; the `aws s3 sync`
#      exited 0
#   3  PUBLISH_ROOT missing or not a directory, or
#      PUBLISH_BUCKET missing (refuse-to-run gate)
#   4  `aws` CLI not found (no on-demand install)
#   5  `trainer --verify-index <index-dir>` exited
#      non-zero (red `INDEX.json` detected, or
#      other index error: refuse to deploy a red
#      index)
#   6  `aws s3 sync` exited non-zero
#
# Usage:
#   bash scripts/testnet-live-publish-dashboard.sh \
#       <publish-root> <s3://bucket[/prefix]>
#
# See `scripts/testnet-live-publish-dashboard.md`
# for the dashboard-deploy runbook and
# `crates/autotrain/tests/script_shape.rs` for
# the shell-shape integration test that pins this
# script's contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- argv / env-knob parsing --------------------------------------------
# Positional: PUBLISH_ROOT (REQUIRED), PUBLISH_BUCKET (REQUIRED).
# Env knobs: PUBLISH_PREFIX, AWS_BIN, TRAINER_BIN.
if [[ $# -ge 1 ]]; then
    PUBLISH_ROOT="$1"
    shift
fi
if [[ $# -ge 1 ]]; then
    PUBLISH_BUCKET="$1"
    shift
fi
if [[ -z "${PUBLISH_ROOT:-}" ]]; then
    echo "testnet-live-publish-dashboard: PUBLISH_ROOT must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-dashboard.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z/ s3://robopoker-testnet-dashboard" >&2
    exit 3
fi
if [[ -z "${PUBLISH_BUCKET:-}" ]]; then
    echo "testnet-live-publish-dashboard: PUBLISH_BUCKET must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-dashboard.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z/ s3://robopoker-testnet-dashboard" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT" ]]; then
    echo "testnet-live-publish-dashboard: publish root $PUBLISH_ROOT does not exist or is not a directory" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT/index" ]]; then
    echo "testnet-live-publish-dashboard: publish root $PUBLISH_ROOT has no index/ subdirectory" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi
if [[ ! -f "$PUBLISH_ROOT/index/INDEX.json" ]]; then
    echo "testnet-live-publish-dashboard: INDEX.json missing at $PUBLISH_ROOT/index/INDEX.json" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-publish-dashboard: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-publish-dashboard: cargo build failed" >&2
        exit 4
    fi
fi

# --- aws cli presence check ---------------------------------------------
AWS_BIN="${AWS_BIN:-aws}"
if ! command -v "$AWS_BIN" >/dev/null 2>&1; then
    echo "testnet-live-publish-dashboard: aws CLI not found on PATH (looked for '$AWS_BIN')" >&2
    echo "  install the aws CLI on the deploy host (e.g. \`pip install awscli\` or \`apt-get install -y awscli\`)" >&2
    exit 4
fi

# --- pre-deploy gate: refuse to deploy a red INDEX.json ------------------
# The STW-034 `INDEX.json` is the source of truth for the
# aggregator a dashboard scrapes. A red `INDEX.json` (a
# tampered per-entry `remote_receipt.s3_objects[].sha256`, a
# missing `INDEX.json`, a missing per-entry
# `remote_receipt.json`) short-circuits the deploy with
# `PublishIndexError::...` from the `--verify-index` arm; we
# re-verify with the dedicated `trainer --verify-index
# <index-dir>` CLI as a hard pre-deploy gate so a CI worker
# that shells out to `aws s3 sync` does not push a red
# aggregator to a dashboard bucket.
INDEX_DIR="$PUBLISH_ROOT/index"
echo "testnet-live-publish-dashboard: verifying INDEX.json at $INDEX_DIR"
if ! "$TRAINER_BIN" --verify-index "$INDEX_DIR" \
        >"$PUBLISH_ROOT/.verify-index.stdout" \
        2>"$PUBLISH_ROOT/.verify-index.stderr"; then
    echo "testnet-live-publish-dashboard: index verifier rejected the INDEX.json" >&2
    echo "  stdout: $PUBLISH_ROOT/.verify-index.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.verify-index.stderr" >&2
    echo "  refusing to deploy a red index" >&2
    rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"
    exit 5
fi
rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"

# --- the dashboard-deploy step -----------------------------------------
# `aws s3 sync <local> <bucket>/<prefix>/` is the deploy
# surface the dashboard's `RBP_DASHBOARD_INDEX_URL` env
# knob points at. The `--delete` flag removes stale files
# (a removed receipt's `INDEX.json` row is no longer
# reflected in the dashboard), and the
# `--cache-control max-age=60` flag keeps the dashboard's
# browser-fetched `INDEX.json` fresh on a 1-minute
# `Cache-Control` window.
PREFIX="${PUBLISH_PREFIX:-}"
# Default the prefix to `<root-basename>/index/` so a
# CI worker that runs the STW-035 chain + this runbook
# in sequence lands the dashboard data feed under a
# per-run key prefix (a separate deploy never
# overwrites a previous deploy's bucket layout).
if [[ -z "$PREFIX" ]]; then
    ROOT_BASENAME="$(basename "$PUBLISH_ROOT")"
    PREFIX="${ROOT_BASENAME}/index/"
fi
S3_URI="${PUBLISH_BUCKET%/}/${PREFIX}"
echo "testnet-live-publish-dashboard: deploying $INDEX_DIR to $S3_URI"
if ! "$AWS_BIN" s3 sync "$INDEX_DIR/" "$S3_URI" \
        --delete \
        --cache-control "max-age=60" \
        >"$PUBLISH_ROOT/.aws-sync.stdout" \
        2>"$PUBLISH_ROOT/.aws-sync.stderr"; then
    echo "testnet-live-publish-dashboard: aws s3 sync failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.aws-sync.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.aws-sync.stderr" >&2
    # On error path, leave the .aws-sync.{stdout,stderr}
    # files in place so an operator can inspect what went
    # wrong.
    exit 6
fi
rm -f "$PUBLISH_ROOT/.aws-sync.stdout" "$PUBLISH_ROOT/.aws-sync.stderr"

# --- the headline SUMMARY.txt ------------------------------------------
# The `INDEX.json` already carries a `SUMMARY.txt` the
# STW-034 runbook wrote; the deploy runbook appends
# the deploy provenance (bucket + prefix + aws CLI
# version + deployed_at timestamp) so a single `cat`
# confirms the whole chain.
SUMMARY="$PUBLISH_ROOT/index/SUMMARY.txt"
DEPLOYED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo '<unknown>')"
{
    echo ""
    echo "  deploy:"
    echo "    bucket:       $PUBLISH_BUCKET"
    echo "    prefix:       $PREFIX"
    echo "    s3_uri:       $S3_URI"
    echo "    aws_bin:      $AWS_BIN"
    echo "    deployed_at:  $DEPLOYED_AT"
} >> "$SUMMARY"

# Echo the headline line so a CI worker scraping
# stdout can pin the dashboard-deploy step without
# reading the file.
cat "$SUMMARY"

echo "testnet-live-publish-dashboard: chain landed end-to-end"
echo "  s3_uri:    $S3_URI"
echo "  summary:   $SUMMARY"
echo "  re-verify: $TRAINER_BIN --verify-index $INDEX_DIR"
