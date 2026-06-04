#!/usr/bin/env bash
#
# scripts/workspace-parallel-proof.sh
#
# STW-020 workspace parallel test proof. Runs the full workspace
# cargo test under `--test-threads=4` three times back-to-back
# against the current source tree, captures each run's stdout,
# stderr, and exit code into a per-run directory, and asserts all
# three runs exit 0. Exits 0 on success (after printing the
# `workspace parallel proof complete: runs=3 failures=0` summary
# line a CI dashboard can grep), exits 3 if any of the three runs
# fails, exits 1 on a script-internal error (e.g. cwd not a cargo
# workspace).
#
# Knobs (all optional):
#   RBP_WORKSPACE_PARALLEL_THREADS — test-threads argument, default 4
#   RBP_WORKSPACE_PARALLEL_RUNS    — number of full runs, default 3
#   RBP_WORKSPACE_PARALLEL_SKIP_BUILD — set to 1 to skip the
#       pre-build (faster for tight inner loops where the binary
#       is already fresh)
#
# Output layout:
#   logs/workspace-parallel-proof/<UTC-ISO>/run-{1,2,3}/{stdout,stderr,exit}.txt
#   logs/workspace-parallel-proof/<UTC-ISO>/SUMMARY.txt
#
# Exit codes:
#   0 — all runs passed
#   1 — script-internal error (no cargo, not a workspace, etc.)
#   3 — one or more runs failed
set -euo pipefail

WORKSPACE_ROOT="${WORKSPACE_ROOT:-$(pwd)}"
THREADS="${RBP_WORKSPACE_PARALLEL_THREADS:-4}"
RUNS="${RBP_WORKSPACE_PARALLEL_RUNS:-3}"
SKIP_BUILD="${RBP_WORKSPACE_PARALLEL_SKIP_BUILD:-0}"

if ! command -v cargo >/dev/null 2>&1; then
  echo "workspace_parallel_proof error: cargo not on PATH" >&2
  exit 1
fi
if ! [ -f "${WORKSPACE_ROOT}/Cargo.toml" ]; then
  echo "workspace_parallel_proof error: no Cargo.toml at ${WORKSPACE_ROOT}" >&2
  exit 1
fi

UTC_ISO="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_ROOT="${WORKSPACE_ROOT}/logs/workspace-parallel-proof"
RUN_DIR="${OUT_ROOT}/${UTC_ISO}"
mkdir -p "${RUN_DIR}"

echo "workspace parallel proof: runs=${RUNS} threads=${THREADS} root=${WORKSPACE_ROOT}" | tee "${RUN_DIR}/preflight.log"

if [ "${SKIP_BUILD}" != "1" ]; then
  echo "workspace parallel proof: pre-building workspace tests (one-time cost)" | tee -a "${RUN_DIR}/preflight.log"
  if ! (cd "${WORKSPACE_ROOT}" && cargo test --workspace --no-run) >>"${RUN_DIR}/preflight.log" 2>&1; then
    echo "workspace parallel proof error: cargo test --workspace --no-run failed" >&2
    exit 1
  fi
fi

# The recursive integration test
# (crates/autotrain/tests/workspace_parallel_proof.rs::
#  `runbook_run_exits_zero_with_single_clean_workspace_run`)
# spawns this very script. If we don't filter it out, the script
# re-runs cargo test --workspace which re-runs the test which
# re-spawns the script → infinite recursion. Skip by test-name;
# the test name is a stable contract (covered by the same
# `crates/autotrain/tests/workspace_parallel_proof.rs` shape
# test) so the filter survives future refactors.
RECURSIVE_SKIP='--skip=runbook_run_exits_zero_with_single_clean_workspace_run'

failures=0
for run in $(seq 1 "${RUNS}"); do
  run_path="${RUN_DIR}/run-${run}"
  mkdir -p "${run_path}"
  echo "workspace parallel proof: starting run ${run}/${RUNS}" | tee -a "${RUN_DIR}/preflight.log"
  set +e
  (
    cd "${WORKSPACE_ROOT}"
    cargo test --workspace -- --test-threads="${THREADS}" ${RECURSIVE_SKIP}
  ) >"${run_path}/stdout.txt" 2>"${run_path}/stderr.txt"
  exit_code=$?
  set -e
  echo "${exit_code}" >"${run_path}/exit.txt"
  if [ "${exit_code}" -ne 0 ]; then
    failures=$((failures + 1))
    echo "workspace parallel proof: run ${run} FAILED (exit ${exit_code}); see ${run_path}/stderr.txt" >&2
  else
    echo "workspace parallel proof: run ${run} ok" | tee -a "${RUN_DIR}/preflight.log"
  fi
done

SUMMARY_PATH="${RUN_DIR}/SUMMARY.txt"
{
  echo "utc=${UTC_ISO}"
  echo "workspace_root=${WORKSPACE_ROOT}"
  echo "threads=${THREADS}"
  echo "runs=${RUNS}"
  echo "failures=${failures}"
  echo "run_log_dir=${RUN_DIR}"
} >"${SUMMARY_PATH}"

# Headline line a CI dashboard can grep.
echo "workspace parallel proof complete: runs=${RUNS} failures=${failures}"

if [ "${failures}" -gt 0 ]; then
  exit 3
fi
exit 0
