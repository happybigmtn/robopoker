#!/usr/bin/env bash
# scripts/testnet-live-publish.sh — STW-032 testnet live launch publish runbook
#
# Reads a `receipts/testnet-live-proof-<UTC-ISO>/` directory the
# STW-019 `testnet-live-proof.sh` runbook produced (or
# `LiveProofReceipt::write_to` synthesised) and writes a
# deterministic, content-addressed portable publish bundle a third
# party (a testnet dashboard bucket, a CI auditor, a release-gate
# script) can fetch + re-verify without re-running the chain.
#
# Bundle layout (drop into `publish/testnet-live-proof-<UTC-ISO>/`):
#
#   publish/testnet-live-proof-<UTC-ISO>/
#     bundle.tar.gz       # deterministic tar.gz of the receipt
#     bundle.sha256       # sha256 of the tarball
#     manifest.json       # machine-readable per-file digests + metadata
#
# The publish step is **read-only** with respect to the receipt
# directory: the script copies the receipt into a fresh
# `staging/` tempdir, tars the copy, and never opens the original
# receipt for write. A partial-failure path leaves the receipt
# untouched and the staging copy partially written.
#
# The script refuses to publish a *red* receipt — it shells out to
# `trainer --verify-receipt <dir>` (the STW-028 verifier) before
# tarring, and bails with exit 7 if the receipt doesn't pass. This
# is the "no paper-over" gate the receipt verifier is the source of
# truth for: a publish of a red receipt is a hard error, not a
# warning.
#
# Environment:
#   RECEIPT_DIR       Path to a
#                     `receipts/testnet-live-proof-<UTC-ISO>/`
#                     directory. REQUIRED. The script refuses to
#                     run with exit 3 if the path is missing or
#                     not a directory.
#   PUBLISH_DIR       Override the parent publish directory. The
#                     default is `<receipt_parent>/publish/<basename>/`
#                     so the script writes next to the receipt
#                     without ever overwriting it.
#   TRAINER_BIN       (default <workspace>/target/debug/trainer)
#                     Path to the trainer binary. If the file is
#                     missing the script runs `cargo build --bin
#                     trainer` first. Set to skip the build (e.g.
#                     when pointing at a `--release` binary).
#   RBP_TRAINER_GIT_SHA  Set automatically from `git rev-parse
#                     HEAD` of the workspace. The manifest's
#                     `trainer_git_sha` field reflects the
#                     workspace at publish time (so a downstream
#                     auditor can re-build the trainer with the
#                     same git SHA + re-verify the receipt).
#                     The fallback `<unknown>` sentinel keeps the
#                     manifest byte-stable when the env knob is
#                     unset (the lib test + the committed
#                     publish-fixture use this sentinel).
#
# Exit codes:
#   0  bundle written end-to-end; tarball + sha256 + manifest
#      landed under `<publish>/<basename>/`
#   3  RECEIPT_DIR missing or not a directory (refuse-to-run gate)
#   4  trainer binary not found and `cargo build` failed
#   5  `trainer --verify-receipt <receipt>` exited non-zero (red
#      receipt: refuse to publish a red receipt)
#   6  `trainer --publish <receipt>` exited non-zero (publish step
#      itself failed — e.g. the output dir is unwritable)
#
# Usage:
#   bash scripts/testnet-live-publish.sh \
#       receipts/testnet-live-proof-20260604T050000Z/
#
# See `scripts/testnet-live-publish.md` for the full runbook and
# `crates/autotrain/tests/script_shape.rs` for the shell-shape
# integration test that pins this script's contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- RECEIPT_DIR gate ----------------------------------------------------
if [[ -z "${RECEIPT_DIR:-}" && $# -ge 1 ]]; then
    RECEIPT_DIR="$1"
fi
if [[ -z "${RECEIPT_DIR:-}" ]]; then
    echo "testnet-live-publish: RECEIPT_DIR must be set" >&2
    echo "  example: bash scripts/testnet-live-publish.sh \\" >&2
    echo "           receipts/testnet-live-proof-20260604T050000Z/" >&2
    exit 3
fi
if [[ ! -d "$RECEIPT_DIR" ]]; then
    echo "testnet-live-publish: receipt dir $RECEIPT_DIR does not exist or is not a directory" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-publish: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-publish: cargo build failed" >&2
        exit 4
    fi
fi

# --- git sha + publish dir -----------------------------------------------
# The `RBP_TRAINER_GIT_SHA` env knob is the manifest's
# `trainer_git_sha` field. We default to `git rev-parse HEAD` of
# the workspace so a downstream auditor can rebuild the trainer
# with the same git SHA + re-verify the receipt. A workspace with
# no `.git/` falls back to `<unknown>` (the lib test fixture uses
# this sentinel for byte-stability).
if [[ -z "${RBP_TRAINER_GIT_SHA:-}" ]]; then
    if [[ -d "$WORKSPACE_ROOT/.git" ]] && command -v git >/dev/null 2>&1; then
        RBP_TRAINER_GIT_SHA="$(cd "$WORKSPACE_ROOT" && git rev-parse HEAD 2>/dev/null || echo '<unknown>')"
    else
        RBP_TRAINER_GIT_SHA="<unknown>"
    fi
fi
export RBP_TRAINER_GIT_SHA

# Compute the publish directory. Default: write the bundle into
# `<receipt_parent>/publish/<basename>/` so the script never
# writes inside the receipt (the receipt is the runbook's
# read-only artifact; the publish is a follow-on consumer of it,
# not a refactor of it). The `PUBLISH_DIR` env knob overrides
# the parent; the basename is always the receipt's basename
# (so a `publish/testnet-live-proof-20260604T050000Z/` directory
# in the bucket corresponds one-for-one with a
# `testnet-live-proof-20260604T050000Z/` directory in
# `receipts/`).
RECEIPT_BASENAME="$(basename "$RECEIPT_DIR")"
RECEIPT_PARENT="$(cd "$(dirname "$RECEIPT_DIR")" && pwd)"
if [[ -z "${PUBLISH_DIR:-}" ]]; then
    PUBLISH_DIR="$RECEIPT_PARENT/publish/$RECEIPT_BASENAME"
fi
mkdir -p "$PUBLISH_DIR"

# --- pre-publish gate: refuse to publish a red receipt -------------------
# The STW-019 runbook's receipt is the source of truth; a
# receipt the runbook produced is green iff the per-step
# `exit.txt` files are all 0. The `trainer --verify-receipt
# <path>` CLI (STW-028) is the typed Rust verifier the lib
# test + the integration test both pin, so a `trainer
# --verify-receipt` shell-out is the canonical "is this
# receipt green?" check.
echo "testnet-live-publish: verifying receipt $RECEIPT_DIR"
if ! "$TRAINER_BIN" --verify-receipt "$RECEIPT_DIR" \
        >"$PUBLISH_DIR/.verify-receipt.stdout" \
        2>"$PUBLISH_DIR/.verify-receipt.stderr"; then
    echo "testnet-live-publish: receipt verifier rejected the receipt" >&2
    echo "  stdout: $PUBLISH_DIR/.verify-receipt.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.verify-receipt.stderr" >&2
    echo "  refusing to publish a red receipt" >&2
    rm -f "$PUBLISH_DIR/.verify-receipt.stdout" "$PUBLISH_DIR/.verify-receipt.stderr"
    exit 5
fi
rm -f "$PUBLISH_DIR/.verify-receipt.stdout" "$PUBLISH_DIR/.verify-receipt.stderr"

# --- the publish step ----------------------------------------------------
# `trainer --publish <receipt-dir>` reads the receipt + writes the
# bundle to `<receipt_parent>/publish/<basename>/` (mirroring the
# bash runbook's `PUBLISH_DIR` choice). The CLI's headline is a
# one-line `live_proof publish complete: bundle=... sha256=...
# bytes=...` line a dashboard scraper can `grep ^live_proof
# publish complete:`.
echo "testnet-live-publish: writing bundle to $PUBLISH_DIR"
if ! "$TRAINER_BIN" --publish "$RECEIPT_DIR" \
        >"$PUBLISH_DIR/.publish.stdout" \
        2>"$PUBLISH_DIR/.publish.stderr"; then
    echo "testnet-live-publish: trainer --publish failed" >&2
    echo "  stdout: $PUBLISH_DIR/.publish.stdout" >&2
    echo "  stderr: $PUBLISH_DIR/.publish.stderr" >&2
    # On error path, leave the .publish.{stdout,stderr} files in
    # place so an operator can inspect what went wrong.
    exit 6
fi
rm -f "$PUBLISH_DIR/.publish.stdout" "$PUBLISH_DIR/.publish.stderr"

# --- the headline SUMMARY.txt -------------------------------------------
SUMMARY="$PUBLISH_DIR/SUMMARY.txt"
{
    echo "testnet live_proof publish complete: receipt=$RECEIPT_BASENAME bundle=$PUBLISH_DIR/bundle.tar.gz"
    echo ""
    echo "  receipt_dir: $RECEIPT_DIR"
    echo "  publish_dir: $PUBLISH_DIR"
    echo "  trainer:     $TRAINER_BIN"
    echo "  git_sha:     $RBP_TRAINER_GIT_SHA"
    echo "  files:"
    if [[ -f "$PUBLISH_DIR/bundle.tar.gz" ]]; then
        echo "    bundle.tar.gz     $(wc -c < "$PUBLISH_DIR/bundle.tar.gz" | tr -d '[:space:]') bytes"
    fi
    if [[ -f "$PUBLISH_DIR/bundle.sha256" ]]; then
        echo "    bundle.sha256     $(wc -c < "$PUBLISH_DIR/bundle.sha256" | tr -d '[:space:]') bytes"
    fi
    if [[ -f "$PUBLISH_DIR/manifest.json" ]]; then
        echo "    manifest.json     $(wc -c < "$PUBLISH_DIR/manifest.json" | tr -d '[:space:]') bytes"
    fi
} > "$SUMMARY"

# Echo the headline line so a CI worker scraping stdout can pin the
# publish without reading the file. The format mirrors the
# `crates/autotrain/tests/publish.rs` integration test's
# `live_proof publish complete: ...` line a future dashboard
# scraper greps the log for.
cat "$SUMMARY"

echo "testnet-live-publish: chain landed end-to-end"
echo "  summary: $SUMMARY"
echo "  re-verify: $TRAINER_BIN --verify-bundle $PUBLISH_DIR"
