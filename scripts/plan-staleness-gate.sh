#!/usr/bin/env bash
#
# scripts/plan-staleness-gate.sh
#
# STW-022 plan-vs-reality staleness gate. The CEO testnet roadmap
# (`genesis/plans/000-ceo-testnet-roadmap.md`) maintains a
# `## Immediate P0 — testnet proof points (dispatch now)` checklist.
# Every P0 row in that list duplicates a shipped STW item on `main`
# (the `steward/DRIFT.md` GHOST table makes the duplication
# mechanical: `genesis:P0-schema` = STW-006 + STW-008,
# `genesis:P0-hand-roundtrip` = STW-008, `genesis:P0-smoke` =
# STW-009, `genesis:P0-bench` = STW-010, `genesis:P0-auth` =
# STW-004). When a worker opens the kanban board and reads
# `IMPLEMENTATION_PLAN.md` they see the P0 row as the
# "current TOP OPEN PRIORITY"; without a mechanical gate, the
# only signal the P0 row is a GHOST claim is the prose on
# lines 1284-1297 of the plan, which a fast worker can miss.
#
# This script closes the loop. It:
#   1. Extracts every `[ ] [P0] <claim>` row from
#      `genesis/plans/000-ceo-testnet-roadmap.md`.
#   2. Maps each P0 row to the STW it duplicates via a static
#      claim-text -> STW-id table the script owns (mirrored
#      against the `steward/DRIFT.md` GHOST table).
#   3. For each `[ ]` P0 row, grep `IMPLEMENTATION_PLAN.md` for
#      `- [x] \`STW-NNN\`` (the shipped marker). If the STW is
#      shipped, the P0 row is GHOST.
#   4. Exits 0 only when no GHOST row is present. Exits 3 on a
#      GHOST row, printing the precise file:line:claim + the
#      shipped STW proof. Exits 1 on a script-internal error
#      (e.g. a planning file is missing).
#
# Knobs (all optional):
#   RBP_PLAN_STALENESS_QUIET — set to 1 to suppress the per-row
#       green output (the script still prints the headline line a
#       CI dashboard greps; it just doesn't list every checked row).
#   RBP_PLAN_STALENESS_ROADMAP — override the roadmap path
#       (default `<workspace>/genesis/plans/000-ceo-testnet-roadmap.md`).
#   RBP_PLAN_STALENESS_PLAN — override the plan path
#       (default `<workspace>/IMPLEMENTATION_PLAN.md`).
#
# Output layout:
#   stdout — per-row status (unless QUIET=1) + the
#       `plan staleness gate complete: checked=N ghosts=0` headline.
#   stderr — script-internal errors and the precise ghost line(s)
#       a CI worker reads when the gate fails.
#
# Exit codes:
#   0 — no GHOST P0 rows; the planning surface matches reality
#   1 — script-internal error (missing planning file, no cargo, etc.)
#   3 — one or more GHOST P0 rows; the gate fails; the precise
#       list of ghost rows is on stderr.

set -euo pipefail

WORKSPACE_ROOT="${WORKSPACE_ROOT:-$(pwd)}"
ROADMAP="${RBP_PLAN_STALENESS_ROADMAP:-${WORKSPACE_ROOT}/genesis/plans/000-ceo-testnet-roadmap.md}"
PLAN="${RBP_PLAN_STALENESS_PLAN:-${WORKSPACE_ROOT}/IMPLEMENTATION_PLAN.md}"
QUIET="${RBP_PLAN_STALENESS_QUIET:-0}"

if ! [ -f "${ROADMAP}" ]; then
  echo "plan_staleness_gate error: roadmap not found at ${ROADMAP}" >&2
  exit 1
fi
if ! [ -f "${PLAN}" ]; then
  echo "plan_staleness_gate error: plan not found at ${PLAN}" >&2
  exit 1
fi

# The P0-row -> STW-id claim map. Each row pairs a stable
# substring of the [P0] roadmap claim with the STW id the row
# duplicates when shipped. The substring must be a unique
# match against the *current* roadmap text (a future roadmap
# edit that retires or rewrites a row must either remove the
# row or update the substring here, or the gate will start
# failing for a different reason).
#
# Format: P0_CLAIM_SUBSTRING|STW_ID
P0_TO_STW=$'Implement the `Schema`|STW-006\nAdd an end-to-end test in `crates/gameroom`|STW-008\nImplement a `trainer` smoke path|STW-009\nBuild a `bin/bench`|STW-010\nLand STW-004 auth hardening|STW-004'

# Extract every unchecked P0 row from the roadmap with its
# line number. A row is `- [ ] [P0] <claim text>` and may
# span multiple physical lines? No: in the current roadmap
# every P0 row is a single line, so a `grep -n` per-line
# scan is exact.
P0_LINES=$(grep -nE '^- \[ \] \[P0\]' "${ROADMAP}" || true)

if [ -z "${P0_LINES}" ]; then
  # The roadmap has no unchecked P0 rows: the gate is trivially
  # green (the retirement landed). Print the headline and exit.
  echo "plan staleness gate: 0 unchecked [P0] rows found in ${ROADMAP}"
  echo "plan staleness gate complete: checked=0 ghosts=0"
  exit 0
fi

# Walk the claim map and test each P0 row individually. The
# roadmap prose (and the worker-readable hint on line 1288) is
# the source of truth for the token-to-STW mapping; this script
# mechanically mirrors it.
checked=0
ghosts=0
ghost_log=""

while IFS='|' read -r CLAIM_SUB STW_ID; do
  # Skip blank lines (the heredoc terminator produces one).
  [ -z "${CLAIM_SUB}" ] && continue
  checked=$((checked + 1))
  # The P0 row is still `[ ]`. Is the STW it duplicates shipped?
  # Shipped = `IMPLEMENTATION_PLAN.md` has `- [x] \`STW-006\``
  # somewhere. The grep anchors on the `\`-quoted STW id and
  # the `- [x] ` checkbox prefix to avoid matching prose that
  # *mentions* a shipped STW id without being the shipped row
  # itself.
  if ! grep -qF "${CLAIM_SUB}" <<<"${P0_LINES}"; then
    # The P0 row was rewritten / retired (e.g. claim text
    # drifted off the published substring, or the row was
    # removed entirely). Treat as a no-op so the gate doesn't
    # spuriously fail when a refactor retires a row by
    # rewriting its claim.
    if [ "${QUIET}" != "1" ]; then
      echo "plan staleness gate: <rewritten or retired P0 row matching '${CLAIM_SUB}'> — no current [ ] match in roadmap"
    fi
    continue
  fi
  if grep -qE "^- \[x\] \`${STW_ID}\`" "${PLAN}"; then
    ghosts=$((ghosts + 1))
    # Capture the exact roadmap line that is GHOST so a worker
    # reading the stderr can jump straight to the offender.
    GHOST_LINE=$(grep -nF "${CLAIM_SUB}" <<<"${P0_LINES}" | head -n1)
    ghost_log="${ghost_log}
  GHOST: ${GHOST_LINE}
    duplicates shipped ${STW_ID} (see \`- [x] \`${STW_ID}\`\` in IMPLEMENTATION_PLAN.md)"
  else
    if [ "${QUIET}" != "1" ]; then
      echo "plan staleness gate: <P0 row matching '${CLAIM_SUB}'> -> ${STW_ID} — not yet shipped (P0 is real)"
    fi
  fi
done <<<"${P0_TO_STW}"

# Headline line a CI dashboard greps.
echo "plan staleness gate complete: checked=${checked} ghosts=${ghosts}"

if [ "${ghosts}" -gt 0 ]; then
  echo "plan staleness_gate FAIL: ${ghosts} ghost P0 row(s) in ${ROADMAP} duplicate shipped STW items.${ghost_log}" >&2
  echo "plan staleness_gate hint: retire the [P0] row in ${ROADMAP} (e.g. flip to [x] or remove); the GHOST P0 list above mirrors steward/DRIFT.md." >&2
  exit 3
fi

exit 0
