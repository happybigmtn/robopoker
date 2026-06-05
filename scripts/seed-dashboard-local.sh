#!/usr/bin/env bash
# scripts/seed-dashboard-local.sh — STW-068
# Seed a local dashboard-compatible layout from an
# existing (even incomplete) testnet-live-proof receipt.
#
# Usage:
#   bash scripts/seed-dashboard-local.sh <receipt-directory>
#
# The script produces .auto/dashboard-seed/ with:
#   - receipts/<basename>/bench/stdout.txt   (if present)
#   - receipts/<basename>/compare/stdout.txt (if present)
#   - transcripts/transcript-*.json          (if present)
#   - INDEX.json                             (minimal, 1 entry)
#
# Then run:
#   RBP_DASHBOARD_INDEX_URL=file://$(pwd)/.auto/dashboard-seed/INDEX.json \
#   RBP_DASHBOARD_RECEIPT_DIR=$(pwd)/.auto/dashboard-seed/receipts \
#   RBP_DASHBOARD_TRANSCRIPT_DIR=$(pwd)/.auto/dashboard-seed/transcripts \
#     cargo run -p rbp-dashboard

set -euo pipefail

RECEIPT_DIR="${1:-}"
if [[ -z "${RECEIPT_DIR}" ]]; then
    printf 'Usage: %s <receipt-directory>\n' "$0" >&2
    exit 3
fi

if [[ ! -d "${RECEIPT_DIR}" ]]; then
    printf 'Error: not a directory: %s\n' "${RECEIPT_DIR}" >&2
    exit 3
fi

# Validate at least one step subdir exists
if [[ -z "$(find "${RECEIPT_DIR}" -mindepth 1 -maxdepth 1 -type d 2>/dev/null)" ]]; then
    printf 'Error: receipt directory has no step subdirectories: %s\n' "${RECEIPT_DIR}" >&2
    exit 3
fi

SEED_ROOT=".auto/dashboard-seed"
RECEIPT_BASENAME="$(basename "${RECEIPT_DIR}")"

mkdir -p "${SEED_ROOT}/receipts/${RECEIPT_BASENAME}/bench"
mkdir -p "${SEED_ROOT}/receipts/${RECEIPT_BASENAME}/compare"
mkdir -p "${SEED_ROOT}/transcripts"

# Copy bench stdout if it exists
if [[ -f "${RECEIPT_DIR}/bench/stdout.txt" ]]; then
    cp "${RECEIPT_DIR}/bench/stdout.txt" \
        "${SEED_ROOT}/receipts/${RECEIPT_BASENAME}/bench/stdout.txt"
fi

# Copy compare stdout if it exists
if [[ -f "${RECEIPT_DIR}/compare/stdout.txt" ]]; then
    cp "${RECEIPT_DIR}/compare/stdout.txt" \
        "${SEED_ROOT}/receipts/${RECEIPT_BASENAME}/compare/stdout.txt"
fi

# Copy transcripts from receipt dir or sibling transcripts dir
for t in "${RECEIPT_DIR}"/transcript-*.json; do
    if [[ -f "$t" ]]; then
        cp "$t" "${SEED_ROOT}/transcripts/"
    fi
done
for t in ./transcripts/transcript-*.json; do
    if [[ -f "$t" ]]; then
        cp "$t" "${SEED_ROOT}/transcripts/"
    fi
done

UTC_NOW="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
TOTAL_BYTES=0
if [[ -f "${RECEIPT_DIR}/bench/stdout.txt" ]]; then
    TOTAL_BYTES="$(stat -c%s "${RECEIPT_DIR}/bench/stdout.txt" 2>/dev/null || echo 0)"
fi

ABS_ROOT="$(cd "${SEED_ROOT}" && pwd)"
ABS_RECEIPT="${ABS_ROOT}/receipts/${RECEIPT_BASENAME}"

cat > "${SEED_ROOT}/INDEX.json" <<JSON
{
  "publish_root": "${ABS_ROOT}",
  "runbook_version": "STW-068 v1",
  "created_at_utc": "${UTC_NOW}",
  "entry_count": 1,
  "total_bytes": ${TOTAL_BYTES},
  "entries": [
    {
      "receipt_basename": "${RECEIPT_BASENAME}",
      "receipt_dir": "${ABS_RECEIPT}",
      "remote_receipt_path": "${ABS_RECEIPT}/remote/remote_receipt.json",
      "remote_receipt": {
        "plan": {
          "bucket": "local",
          "prefix": "${RECEIPT_BASENAME}/",
          "region": "us-east-1",
          "s3_objects": [],
          "bundle_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
          "bundle_bytes": 0,
          "receipt_basename": "${RECEIPT_BASENAME}",
          "runbook_version": "STW-068 v1",
          "created_at_utc": "${UTC_NOW}",
          "dry_run": true
        },
        "uploaded_at_utc": "${UTC_NOW}",
        "s3_objects": [],
        "total_bytes": ${TOTAL_BYTES},
        "bundle_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
        "runbook_version": "STW-068 v1"
      },
      "bench": {
        "blueprint": "v1",
        "baseline": "fish",
        "mbb_per_100": 0.0,
        "mbb_ci95": 0.0,
        "win_rate": 0.0
      }
    }
  ]
}
JSON

printf 'dashboard seed layout complete: %s\n' "${SEED_ROOT}"
printf 'Run:\n  RBP_DASHBOARD_INDEX_URL=file://%s/INDEX.json \\\n  RBP_DASHBOARD_RECEIPT_DIR=%s/receipts \\\n  RBP_DASHBOARD_TRANSCRIPT_DIR=%s/transcripts \\\n    cargo run -p rbp-dashboard\n' \
    "${ABS_ROOT}" "${ABS_ROOT}" "${ABS_ROOT}"
