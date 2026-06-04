#!/usr/bin/env bash
# scripts/testnet-live-publish-index.sh — STW-034 testnet live launch
# publish-index runbook (the testnet dashboard aggregator the
# STW-033 publish-remote runbook doc names as the "next slice
# (`testnet-live-publish-index`)").
#
# Reads a `<publish-root>/` directory the STW-033
# `testnet-live-publish-s3.sh` runbook produced (a tree of
# `publish/<basename>/remote/remote_receipt.json` files, one per
# receipt the runbook published-remote'd) and turns it into a
# deterministic aggregator: a single `INDEX.json` + `SUMMARY.txt`
# pair a testnet dashboard can scrape instead of listing the
# bucket + fetching N manifests.
#
# Output layout (drop into
# `<publish_root>/index/`):
#
#   <publish_root>/index/
#     INDEX.json    # sorted-by-receipt_basename aggregator
#     SUMMARY.txt   # single-line headline a CI worker `cat`s
#
# The index step is **read-only** with respect to the publish
# root: it reads + re-verifies the `remote_receipt.json` files
# in place, then writes its own `index/` subdir under the
# publish root, so a `trainer --publish-index` invocation
# cannot mutate the underlying `remote_receipt.json` files
# even on partial-failure paths.
#
# The script refuses to index a *red* `remote_receipt.json` —
# the STW-034 indexer runs the per-entry
# `PublishedRemoteReceipt::verify` as a pre-index gate; a red
# `remote_receipt.json` short-circuits the index step with
# `PublishIndexError::RemoteReceiptRed(...)` and the runbook
# exits 5. This is the "no paper-over" gate the STW-033
# remote-receipt verifier is the source of truth for: an
# aggregator over a red remote receipt is a hard error, not a
# warning.
#
# After the index step, the script re-verifies the freshly
# written `INDEX.json` via `trainer --verify-index <index-path>`
# (the no-DB no-rebuild re-verifier the STW-034 chain ships)
# so a CI worker that shells out to the runbook can confirm
# the on-disk `INDEX.json` is internally consistent without
# re-running the STW-019 / STW-032 / STW-033 chain.
#
# Environment:
#   PUBLISH_ROOT    Path to a `<publish-root>/` directory the
#                   STW-033 `testnet-live-publish-s3.sh`
#                   runbook produced. REQUIRED. The script
#                   refuses to run with exit 3 if the path is
#                   missing or not a directory.
#   TRAINER_BIN     (default <workspace>/target/debug/trainer)
#                   Path to the trainer binary. If the file is
#                   missing the script runs `cargo build --bin
#                   trainer` first. Set to skip the build (e.g.
#                   when pointing at a `--release` binary).
#   RBP_PUBLISH_INDEX_UTC  Set automatically to the current
#                   `date -u +%Y-%m-%dT%H:%M:%SZ` if unset.
#                   The `INDEX.json`'s `created_at_utc` field
#                   reflects this knob so a downstream auditor
#                   can re-fetch the index + assert it was
#                   written in the expected UTC window. The
#                   fallback `<unknown>` sentinel keeps the
#                   `INDEX.json` byte-stable when the env knob
#                   is unset (the lib test + the integration
#                   test use this sentinel).
#
# Exit codes:
#   0  index written end-to-end; `INDEX.json` + `SUMMARY.txt`
#      landed under `<publish_root>/index/`; the
#      `trainer --verify-index` re-verify exited 0
#   3  PUBLISH_ROOT missing or not a directory
#      (refuse-to-run gate)
#   4  trainer binary not found and `cargo build` failed
#   5  `trainer --publish-index <root>` exited non-zero
#      (red `remote_receipt.json` detected, or other
#      aggregator error: refuse to write a paper-over index)
#   6  `trainer --verify-index <index-path>` exited non-zero
#      (the post-index re-verify failed)
#
# Usage:
#   bash scripts/testnet-live-publish-index.sh <publish-root>
#
# See `scripts/testnet-live-publish-index.md` for the index
# runbook and `crates/autotrain/tests/script_shape.rs` for the
# shell-shape integration test that pins this script's
# contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- argv / env-knob parsing --------------------------------------------
# Positional: PUBLISH_ROOT (REQUIRED).
# Env knobs: TRAINER_BIN, RBP_PUBLISH_INDEX_UTC.
if [[ -z "${PUBLISH_ROOT:-}" && $# -ge 1 ]]; then
    PUBLISH_ROOT="$1"
    shift
fi
if [[ -z "${PUBLISH_ROOT:-}" ]]; then
    echo "testnet-live-publish-index: PUBLISH_ROOT must be set" >&2
    echo "  example: bash scripts/testnet-live-publish-index.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT" ]]; then
    echo "testnet-live-publish-index: publish root $PUBLISH_ROOT does not exist or is not a directory" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-live-publish-index: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "testnet-live-publish-index: cargo build failed" >&2
        exit 4
    fi
fi

# Default the index UTC timestamp. The `RBP_PUBLISH_INDEX_UTC`
# env knob is the timestamp the indexer stamps on the
# `INDEX.json`'s `created_at_utc` field; a future auditor can
# re-fetch + assert the index was written in the expected
# window. The fallback `<unknown>` sentinel keeps the
# `INDEX.json` byte-stable when the env knob is unset (the
# lib test + the integration test use this sentinel).
if [[ -z "${RBP_PUBLISH_INDEX_UTC:-}" ]]; then
    RBP_PUBLISH_INDEX_UTC="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo '<unknown>')"
fi
export RBP_PUBLISH_INDEX_UTC

# --- the publish-index step ---------------------------------------------
# `trainer --publish-index <publish-root>` reads the
# `remote_receipt.json` files under
# `<publish-root>/publish/<basename>/remote/`, re-verifies
# every one as a per-entry pre-index gate, and writes a
# deterministic `INDEX.json` + `SUMMARY.txt` pair under
# `<publish-root>/index/`. The aggregator is **always
# no-network**: the dashboard-scraper surface is a
# `cat INDEX.json` away, not a `find <bucket>` away. A red
# `remote_receipt.json` short-circuits the index with
# `PublishIndexError::RemoteReceiptRed(...)` and the arm
# exits 2 — the runbook converts that to exit 5.
echo "testnet-live-publish-index: indexing publish root $PUBLISH_ROOT"
if ! "$TRAINER_BIN" --publish-index "$PUBLISH_ROOT" \
        >"$PUBLISH_ROOT/.publish-index.stdout" \
        2>"$PUBLISH_ROOT/.publish-index.stderr"; then
    echo "testnet-live-publish-index: trainer --publish-index failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.publish-index.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.publish-index.stderr" >&2
    # On error path, leave the .publish-index.{stdout,stderr}
    # files in place so an operator can inspect what went
    # wrong.
    exit 5
fi
# Capture the headline line a CI worker scrapes (the
# `live_proof publish_index complete: ...` line) before
# removing the stdout redirect.
INDEX_HEADLINE="$(cat "$PUBLISH_ROOT/.publish-index.stdout")"
rm -f "$PUBLISH_ROOT/.publish-index.stdout" "$PUBLISH_ROOT/.publish-index.stderr"

# --- post-index re-verify (the verifier the bash runbook
#     also runs so a CI worker can confirm the on-disk
#     `INDEX.json` is internally consistent) -------------
# `trainer --verify-index <index-path>` re-hashes every
# local file the `INDEX.json` claims to have inlined (each
# entry's `s3_objects[].local_path` is read + re-sha256'd +
# compared to the entry's `sha256`), asserts every digest
# matches, asserts every `s3_uri` in the index appears in
# the inlined plan (a phantom `s3_uri` is a hard
# `PublishIndexError::MissingObject` error), and prints
# a one-line `live_proof index verification passed: ...` /
# `live_proof index verification failed: ...` headline.
INDEX_DIR="$PUBLISH_ROOT/index"
echo "testnet-live-publish-index: re-verifying index at $INDEX_DIR"
if ! "$TRAINER_BIN" --verify-index "$INDEX_DIR" \
        >"$PUBLISH_ROOT/.verify-index.stdout" \
        2>"$PUBLISH_ROOT/.verify-index.stderr"; then
    echo "testnet-live-publish-index: trainer --verify-index failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.verify-index.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.verify-index.stderr" >&2
    # On error path, leave the .verify-index.{stdout,stderr}
    # files in place so an operator can inspect what went
    # wrong.
    exit 6
fi
VERIFY_HEADLINE="$(cat "$PUBLISH_ROOT/.verify-index.stdout")"
rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"

# --- the headline SUMMARY.txt -------------------------------------------
# The `trainer --publish-index` arm already writes a
# `SUMMARY.txt` (the single-line headline a CI worker
# `cat`s to confirm the index step landed end-to-end); the
# runbook appends the post-index re-verify line + the
# launch provenance (publish root + index path + created-at
# timestamp + trainer binary) so a single `cat` confirms
# the whole chain.
INDEX_SUMMARY="$INDEX_DIR/SUMMARY.txt"
{
    echo "testnet live_proof publish_index complete: root=$PUBLISH_ROOT index=$INDEX_DIR created_at_utc=$RBP_PUBLISH_INDEX_UTC"
    echo ""
    echo "  publish_root:    $PUBLISH_ROOT"
    echo "  index_dir:       $INDEX_DIR"
    echo "  index_headline:  $INDEX_HEADLINE"
    echo "  verify_headline: $VERIFY_HEADLINE"
    echo "  trainer:         $TRAINER_BIN"
    echo "  created_at:      $RBP_PUBLISH_INDEX_UTC"
} > "$INDEX_SUMMARY"

# Echo the headline line so a CI worker scraping stdout can
# pin the publish-index step without reading the file. The
# format mirrors the
# `crates/autotrain/tests/publish_index.rs` integration
# test's `live_proof publish_index complete: ...` line a
# future dashboard scraper greps the log for.
cat "$INDEX_SUMMARY"

echo "testnet-live-publish-index: chain landed end-to-end"
echo "  index:    $INDEX_DIR/INDEX.json"
echo "  summary:  $INDEX_SUMMARY"
echo "  re-verify: $TRAINER_BIN --verify-index $INDEX_DIR"
