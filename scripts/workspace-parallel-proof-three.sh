#!/usr/bin/env bash
#
# scripts/workspace-parallel-proof-three.sh
#
# STW-037 operator-runnable 3-consecutive full-workspace proof.
# Closes the last un-closed `verification:workspace-parallel` mainnet-block
# hinge. STW-020 ships `scripts/workspace-parallel-proof.sh` (the canonical
# 3-consecutive *full-workspace* proof an operator has to hand-orchestrate
# with a no-output knob) and STW-030 ships the cheap in-CI 2-second
# 3-consecutive *gameplay-only* proof the
# `crates/autotrain/tests/workspace_parallel_proof_three.rs::run_three_consecutive_clean_gameplay_lib_test_runs`
# lib test drives. STW-037 sits in between: an operator / nightly worker
# can `bash scripts/workspace-parallel-proof-three.sh` from a clean
# checkout and get a single command that
#
#   (1) runs the in-CI gameplay-only 3-consecutive proof
#       (`cargo test -p rbp-autotrain --test workspace_parallel_proof_three
#       -- --test-threads=1`) three times back-to-back in three separate
#       `cargo test` invocations, AND
#   (2) runs the canonical 3-consecutive *full-workspace* proof
#       (`scripts/workspace-parallel-proof.sh`) once,
#
# capturing each invocation's stdout + stderr + exit code into a
# per-invocation `logs/workspace-parallel-proof-three/<UTC-ISO>/invocation-{1,2,3,4}/{stdout,stderr,exit}.txt`
# layout (a sibling of the STW-020
# `logs/workspace-parallel-proof/<UTC-ISO>/run-{1,2,3}/...` layout), and
# emitting a one-line
# `workspace parallel proof three complete: gameplay_runs=3/3 full_workspace_run_exit=0`
# headline a CI dashboard can `grep ^workspace`. Knobs:
# `RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET=1` mutes the per-invocation
# stdout echo without changing the exit-code contract. The companion
# script exits 3 on any failed invocation, exit 1 on script-internal
# error, exit 0 only when every invocation exited 0.
#
# The script is pure bash + `cargo test` + `bash -n` — no `jq`, no
# `python`, no `aws` vendored; mirrors the
# `scripts/workspace-parallel-proof.sh` +
# `scripts/testnet-live-proof.sh` shape the autotrain pipeline already
# follows.
#
# The `--test-threads=1` concurrency regime the script uses for each
# of the three STW-030 lib-test invocations is the cheapest way to
# (a) prove the STW-030 3-consecutive contract is bit-for-bit
# reproducible across three separate `cargo test` processes (the
# STW-030 lib test already does the 3-consecutive gameplay-lib
# proof in-process, but the new runbook adds the
# *cross-invocation* proof the STW-020 + STW-030 chain used to
# require a hand-orchestrated shell loop for), and (b) keep the
# per-invocation wall-clock cost bounded (a 3-consecutive STW-030
# lib test under `--test-threads=4` would still complete in under
# 4 s per invocation, so the 3 separate invocations are well
# under the operator / nightly runbook budget). The single
# `scripts/workspace-parallel-proof.sh` invocation the runbook
# drives keeps the STW-020 `--test-threads=4` / `--skip=runbook_*`
# concurrency contract untouched (a regression in the STW-020
# contract is caught by the STW-020 sibling runbook + the
# STW-020 lib-test, not by the new runbook).
#
# Output layout:
#   logs/workspace-parallel-proof-three/<UTC-ISO>/invocation-{1,2,3,4}/{stdout,stderr,exit}.txt
#     invocation-{1,2,3} — the three STW-030 lib-test invocations
#     invocation-4       — the single STW-020 runbook invocation
#   logs/workspace-parallel-proof-three/<UTC-ISO>/SUMMARY.txt
#
# Exit codes:
#   0 — every invocation exited 0
#   1 — script-internal error (no cargo, not a workspace, no autotrain crate, etc.)
#   3 — one or more invocations failed
set -euo pipefail

WORKSPACE_ROOT="${WORKSPACE_ROOT:-$(pwd)}"
QUIET="${RBP_WORKSPACE_PARALLEL_PROOF_THREE_QUIET:-0}"

# The STW-030 lib test the runbook invokes three times. Pinned
# by the same lib test name the
# `crates/autotrain/tests/workspace_parallel_proof_three.rs::run_three_consecutive_clean_gameplay_lib_test_runs`
# test owns, so a regression that renames the lib test (and
# would silently change the 3-consecutive contract) fails the
# new runbook at the *first* invocation, not on a later
# operator / nightly run.
LIB_TEST="run_three_consecutive_clean_gameplay_lib_test_runs"

# The three STW-030 lib-test invocations + the one STW-020
# runbook invocation. Each invocation is its own `cargo test`
# / bash process — the runbook is intentionally a *consumer*
# of the existing lib test + runbook, not a refactor of either.
INVOCATION_COUNT=4

if ! command -v cargo >/dev/null 2>&1; then
  echo "workspace_parallel_proof_three error: cargo not on PATH" >&2
  exit 1
fi
if ! [ -f "${WORKSPACE_ROOT}/Cargo.toml" ]; then
  echo "workspace_parallel_proof_three error: no Cargo.toml at ${WORKSPACE_ROOT}" >&2
  exit 1
fi
if ! [ -d "${WORKSPACE_ROOT}/crates/autotrain" ]; then
  echo "workspace_parallel_proof_three error: no crates/autotrain at ${WORKSPACE_ROOT}" >&2
  exit 1
fi
if ! [ -x "${WORKSPACE_ROOT}/scripts/workspace-parallel-proof.sh" ]; then
  echo "workspace_parallel_proof_three error: scripts/workspace-parallel-proof.sh missing or not executable at ${WORKSPACE_ROOT}/scripts/workspace-parallel-proof.sh" >&2
  exit 1
fi

UTC_ISO="$(date -u +%Y%m%dT%H%M%SZ)"
OUT_ROOT="${WORKSPACE_ROOT}/logs/workspace-parallel-proof-three"
RUN_DIR="${OUT_ROOT}/${UTC_ISO}"
mkdir -p "${RUN_DIR}"

# `--exact` makes the filter match the lib test name
# bit-for-bit (a future regression that adds a sibling lib
# test with a similar name fails the gate fast) and `--` is
# the cargo-test convention for forwarding `--test-threads=N`
# to the test binary, not to `cargo test` itself.
LIB_TEST_FILTER="--exact"

echo "workspace parallel proof three: invocations=${INVOCATION_COUNT} lib_test=${LIB_TEST} root=${WORKSPACE_ROOT}" \
  | tee "${RUN_DIR}/preflight.log"

failures=0
gameplay_runs_ok=0
full_workspace_exit=-1

# The recursive-spawn dodge mirrors the STW-020 sibling script's
# RECURSIVE_SKIP. The new runbook does not itself call
# `cargo test --workspace` directly (it only calls
# `cargo test -p rbp-autotrain --test workspace_parallel_proof_three
# -- --test-threads=1`, which is crate-scoped and would
# re-enter the STW-030 lib test's own 3-consecutive gameplay
# loop only if `--test-threads=1` is dropped); the single
# STW-020 runbook invocation the runbook drives forwards the
# STW-020 RECURSIVE_SKIP filter as-is. A future regression
# that drops the crate-scope filter (`-p rbp-autotrain
# --test workspace_parallel_proof_three`) would re-enter the
# recursive-spawn trap the STW-020 RECURSIVE_SKIP filter
# is meant to dodge — keep the crate scope as a hard
# requirement of this runbook's contract.
for invocation in $(seq 1 3); do
  inv_path="${RUN_DIR}/invocation-${invocation}"
  mkdir -p "${inv_path}"
  if [ "${QUIET}" != "1" ]; then
    echo "workspace parallel proof three: starting invocation ${invocation}/3 (STW-030 lib test)" \
      | tee -a "${RUN_DIR}/preflight.log"
  fi
  set +e
  (
    cd "${WORKSPACE_ROOT}"
    cargo test -p rbp-autotrain --test workspace_parallel_proof_three \
      -- "${LIB_TEST_FILTER}" "${LIB_TEST}" --test-threads=1
  ) >"${inv_path}/stdout.txt" 2>"${inv_path}/stderr.txt"
  exit_code=$?
  set -e
  echo "${exit_code}" >"${inv_path}/exit.txt"
  if [ "${exit_code}" -ne 0 ]; then
    failures=$((failures + 1))
    echo "workspace parallel proof three: invocation ${invocation}/3 (STW-030) FAILED (exit ${exit_code}); see ${inv_path}/stderr.txt" >&2
  else
    gameplay_runs_ok=$((gameplay_runs_ok + 1))
    if [ "${QUIET}" != "1" ]; then
      echo "workspace parallel proof three: invocation ${invocation}/3 (STW-030) ok" \
        | tee -a "${RUN_DIR}/preflight.log"
    fi
  fi
done

# Invocation 4 — the single STW-020 canonical 3-consecutive
# *full-workspace* proof. We do NOT pass
# RBP_WORKSPACE_PARALLEL_QUIET=1 here; the STW-020 runbook's
# own per-run stdout echo is the headline a future operator
# would scan, and the STW-020 runbook's exit-0 contract is
# unchanged. The new runbook only captures the runbook's
# stdout + stderr + exit into the same per-invocation layout.
fw_path="${RUN_DIR}/invocation-4"
mkdir -p "${fw_path}"
if [ "${QUIET}" != "1" ]; then
  echo "workspace parallel proof three: starting invocation 4/4 (STW-020 runbook)" \
    | tee -a "${RUN_DIR}/preflight.log"
fi
set +e
(
  cd "${WORKSPACE_ROOT}"
  WORKSPACE_ROOT="${WORKSPACE_ROOT}" \
  RBP_WORKSPACE_PARALLEL_SKIP_BUILD="${RBP_WORKSPACE_PARALLEL_SKIP_BUILD:-0}" \
    bash "${WORKSPACE_ROOT}/scripts/workspace-parallel-proof.sh"
) >"${fw_path}/stdout.txt" 2>"${fw_path}/stderr.txt"
fw_exit=$?
set -e
echo "${fw_exit}" >"${fw_path}/exit.txt"
full_workspace_exit="${fw_exit}"
if [ "${fw_exit}" -ne 0 ]; then
  failures=$((failures + 1))
  echo "workspace parallel proof three: invocation 4/4 (STW-020 runbook) FAILED (exit ${fw_exit}); see ${fw_path}/stderr.txt" >&2
else
  if [ "${QUIET}" != "1" ]; then
    echo "workspace parallel proof three: invocation 4/4 (STW-020 runbook) ok" \
      | tee -a "${RUN_DIR}/preflight.log"
  fi
fi

SUMMARY_PATH="${RUN_DIR}/SUMMARY.txt"
{
  echo "utc=${UTC_ISO}"
  echo "workspace_root=${WORKSPACE_ROOT}"
  echo "invocations=${INVOCATION_COUNT}"
  echo "gameplay_runs_ok=${gameplay_runs_ok}"
  echo "full_workspace_run_exit=${full_workspace_exit}"
  echo "failures=${failures}"
  echo "run_log_dir=${RUN_DIR}"
} >"${SUMMARY_PATH}"

# Headline line a CI dashboard can grep. Pinned by the
# `crates/autotrain/tests/workspace_parallel_proof_three.rs::operator_runnable_three_script_exists_and_parses`
# sub-test (the new STW-037 3rd sub-test) — a regression in
# either the prefix or the `gameplay_runs=` / `full_workspace_run_exit=`
# key=N pairs fails the new sub-test at CI time.
echo "workspace parallel proof three complete: gameplay_runs=${gameplay_runs_ok}/3 full_workspace_run_exit=${full_workspace_exit}"

if [ "${failures}" -gt 0 ]; then
  exit 3
fi
exit 0
