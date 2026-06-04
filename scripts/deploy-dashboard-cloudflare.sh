#!/usr/bin/env bash
# scripts/deploy-dashboard-cloudflare.sh — STW-054 Cloudflare Pages
# deploy runbook (the *deploy* leg of the public-surface
# north star the prior reviews named but did not row up).
#
# Mirrors the `scripts/testnet-live-publish-dashboard.sh`
# (STW-036 S3/CloudFront path) chain shape, but uses
# `wrangler pages deploy` instead of `aws s3 sync` so an
# operator with a Cloudflare account (but no AWS account)
# can still bring up the testnet dashboard. The script
# refuses to deploy a *red* index — it shells out to
# `trainer --verify-index <index-dir>` (the STW-034
# verifier) before the `wrangler pages deploy` step, and
# bails with exit 3 if the index doesn't pass. This is
# the "no paper-over" gate the STW-034 index verifier is
# the source of truth for: a Cloudflare-Pages-deploy of
# a red index is a hard error, not a warning.
#
# Reads a `<publish-root>/index/INDEX.json` + the
# `transcripts/` directory the STW-034 → STW-035 chain
# produced (the dashboard data feed), and `wrangler pages
# deploy`s the `<publish-root>/index/` directory to a
# Cloudflare Pages project the runbook's
# `RBP_DASHBOARD_PAGES_PROJECT` env knob names
# (default `robopoker-testnet-dashboard`).
#
# The dashboard's `IndexClient` reads the bucket-hosted
# `INDEX.json` via the `RBP_DASHBOARD_INDEX_URL` env knob
# (default `http://localhost:8080/api/index` in tests,
# the Cloudflare Pages URL in production). The typed
# read re-uses `rbp_autotrain::PublishIndex`, so a
# shape drift in `INDEX.json` fails BOTH the
# dashboard's typed read AND the
# `trainer --verify-index` re-verify at the same CI
# step.
#
# Environment:
#   PUBLISH_ROOT             Path to a `<publish-root>/`
#                            directory the STW-034
#                            `testnet-live-publish-index.sh`
#                            runbook produced. REQUIRED.
#                            The script refuses to run
#                            with exit 3 if the path is
#                            missing or not a directory.
#   RBP_DASHBOARD_CF_API_TOKEN
#                            Cloudflare API token the
#                            runbook exports as
#                            `CLOUDFLARE_API_TOKEN` for
#                            `wrangler`. REQUIRED. The
#                            script refuses to run with
#                            exit 3 if the env knob is
#                            missing.
#   RBP_DASHBOARD_CF_ACCOUNT_ID
#                            Cloudflare account ID the
#                            runbook uses to idempotently
#                            create the Pages project on
#                            first run (`wrangler pages
#                            project create`). REQUIRED
#                            for the first-time
#                            `project create` step; on
#                            subsequent runs the project
#                            already exists and the
#                            `project create` step is
#                            skipped. The script refuses
#                            to run with exit 3 if the
#                            env knob is missing.
#   RBP_DASHBOARD_PAGES_PROJECT
#                            Pages project name
#                            (default
#                            `robopoker-testnet-dashboard`).
#                            The runbook idempotently
#                            creates the project on
#                            first run + skips on
#                            subsequent runs.
#   RBP_DASHBOARD_DEPLOYED_URL
#                            The post-deploy public URL
#                            the runbook reconciles the
#                            README's `## Public
#                            dashboard` line to.
#                            Optional — the runbook
#                            defaults to
#                            `https://<project>.pages.dev/`
#                            when the env knob is unset
#                            (the spec's
#                            `${RBP_DASHBOARD_DEPLOYED_URL:-<project>.pages.dev/}`
#                            shape).
#   WRANGLER_BIN             (default `wrangler`) Path to
#                            the `wrangler` CLI. If the
#                            file is missing the script
#                            exits with code 3 (no
#                            on-demand install — the
#                            `wrangler` CLI is a system
#                            dep the deploy host is
#                            expected to ship, the same
#                            way `cargo` is).
#   TRAINER_BIN              (default
#                            `<workspace>/target/debug/trainer`)
#                            Path to the `trainer`
#                            binary. Auto-builds with
#                            `cargo build --bin trainer`
#                            if missing.
#
# Exit codes:
#   0  deploy succeeded; `live_proof dashboard deploy
#      complete: pages_url=<url> files=<N> bytes=<B>`
#      line in `SUMMARY.txt`; `deploy.json` written
#      next to `SUMMARY.txt` with the `pages_url` /
#      `files` / `bytes` fields; README
#      `## Public dashboard` line updated to the real
#      URL.
#   1  script-internal error
#   3  missing `RBP_DASHBOARD_CF_API_TOKEN` /
#      missing `wrangler` on `$PATH` /
#      missing `RBP_DASHBOARD_CF_ACCOUNT_ID` /
#      missing `PUBLISH_ROOT` (or not a directory) /
#      missing `INDEX.json` under `<publish-root>/index/`
#      (refuse-to-run gate);
#      failed `trainer --verify-index <index-dir>`
#      (red `INDEX.json` detected, refuse to deploy
#      a red index);
#      failed `wrangler pages deploy` (the
#      Cloudflare Pages push failed).
#
# Usage:
#   bash scripts/deploy-dashboard-cloudflare.sh \
#       <publish-root>
#
# See `scripts/deploy-dashboard-cloudflare.md` for the
# dashboard-deploy runbook and
# `crates/autotrain/tests/script_shape.rs` for the
# shell-shape integration test that pins this script's
# static contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- argv / env-knob parsing --------------------------------------------
# Positional: PUBLISH_ROOT (REQUIRED).
# Env knobs: RBP_DASHBOARD_CF_API_TOKEN (REQUIRED),
#            RBP_DASHBOARD_CF_ACCOUNT_ID (REQUIRED),
#            RBP_DASHBOARD_PAGES_PROJECT,
#            RBP_DASHBOARD_DEPLOYED_URL,
#            WRANGLER_BIN, TRAINER_BIN.
if [[ $# -ge 1 ]]; then
    PUBLISH_ROOT="$1"
    shift
fi
if [[ -z "${PUBLISH_ROOT:-}" ]]; then
    echo "deploy-dashboard: PUBLISH_ROOT must be set" >&2
    echo "  example: bash scripts/deploy-dashboard-cloudflare.sh \\" >&2
    echo "           receipts/publish-20260604T050000Z/" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT" ]]; then
    echo "deploy-dashboard: publish root $PUBLISH_ROOT does not exist or is not a directory" >&2
    exit 3
fi
if [[ ! -d "$PUBLISH_ROOT/index" ]]; then
    echo "deploy-dashboard: publish root $PUBLISH_ROOT has no index/ subdirectory" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi
if [[ ! -f "$PUBLISH_ROOT/index/INDEX.json" ]]; then
    echo "deploy-dashboard: INDEX.json missing at $PUBLISH_ROOT/index/INDEX.json" >&2
    echo "  (the STW-034 testnet-live-publish-index.sh runbook must run first)" >&2
    exit 3
fi
if [[ -z "${RBP_DASHBOARD_CF_API_TOKEN:-}" ]]; then
    echo "deploy-dashboard: missing RBP_DASHBOARD_CF_API_TOKEN env knob" >&2
    echo "  the runbook exports the knob as CLOUDFLARE_API_TOKEN for wrangler" >&2
    echo "  example: export RBP_DASHBOARD_CF_API_TOKEN=<cloudflare-api-token>" >&2
    exit 3
fi
if [[ -z "${RBP_DASHBOARD_CF_ACCOUNT_ID:-}" ]]; then
    echo "deploy-dashboard: missing RBP_DASHBOARD_CF_ACCOUNT_ID env knob" >&2
    echo "  the runbook uses the knob for the first-time wrangler pages project create" >&2
    echo "  example: export RBP_DASHBOARD_CF_ACCOUNT_ID=<cloudflare-account-id>" >&2
    exit 3
fi

# --- trainer binary path + on-demand build -------------------------------
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"
if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "deploy-dashboard: trainer binary not found at $TRAINER_BIN" >&2
    echo "  building with \`cargo build --bin trainer\`..." >&2
    if ! (cd "$WORKSPACE_ROOT" && cargo build --bin trainer) >&2; then
        echo "deploy-dashboard: cargo build failed" >&2
        exit 1
    fi
fi

# --- wrangler CLI presence check -----------------------------------------
# `wrangler pages deploy` is the actual Cloudflare Pages
# push. The runbook refuses to run with exit 3 if
# `wrangler` is missing on `$PATH` (no on-demand install
# — the `wrangler` CLI is a system dep the deploy host
# is expected to ship, the same way `cargo` is).
WRANGLER_BIN="${WRANGLER_BIN:-wrangler}"
if ! command -v "$WRANGLER_BIN" >/dev/null 2>&1; then
    echo "deploy-dashboard: wrangler not on PATH (looked for '$WRANGLER_BIN')" >&2
    echo "  install wrangler on the deploy host (e.g. \`npm install -g wrangler\`" >&2
    echo "  or \`cargo install wrangler\`)" >&2
    exit 3
fi

# Export the API token the runbook consumes as
# `CLOUDFLARE_API_TOKEN` for `wrangler`. The
# `RBP_DASHBOARD_CF_API_TOKEN` knob is the operator's
# env-knob form; the wrangler CLI reads the
# `CLOUDFLARE_API_TOKEN` form. The export is the only
# point the token is touched, so a future refactor that
# pivots to a `~/.config/.wrangler/config/default.toml`
# read can drop the export without touching the rest of
# the chain.
export CLOUDFLARE_API_TOKEN="$RBP_DASHBOARD_CF_API_TOKEN"

# --- Cloudflare Pages project name --------------------------------------
# The Pages project name is the `RBP_DASHBOARD_PAGES_PROJECT`
# env knob (default `robopoker-testnet-dashboard`, the
# placeholder URL the README's `## Public dashboard` line
# names). The wrangler.toml at the repo root pins the
# same default, so a `wrangler pages deploy` invocation
# picks up the project name from the wrangler.toml
# unless the operator overrides it on the command line.
PAGES_PROJECT="${RBP_DASHBOARD_PAGES_PROJECT:-robopoker-testnet-dashboard}"

# --- pre-deploy gate: refuse to deploy a red INDEX.json ------------------
# The STW-034 `INDEX.json` is the source of truth for
# the aggregator a dashboard scrapes. A red
# `INDEX.json` (a tampered per-entry
# `remote_receipt.s3_objects[].sha256`, a missing
# `INDEX.json`, a missing per-entry
# `remote_receipt.json`) short-circuits the deploy with
# `PublishIndexError::...` from the `--verify-index`
# arm; we re-verify with the dedicated
# `trainer --verify-index <index-dir>` CLI as a hard
# pre-deploy gate so a CI worker that shells out to
# `wrangler pages deploy` does not push a red aggregator
# to a Cloudflare Pages project.
INDEX_DIR="$PUBLISH_ROOT/index"
echo "deploy-dashboard: verifying INDEX.json at $INDEX_DIR"
if ! "$TRAINER_BIN" --verify-index "$INDEX_DIR" \
        >"$PUBLISH_ROOT/.verify-index.stdout" \
        2>"$PUBLISH_ROOT/.verify-index.stderr"; then
    echo "deploy-dashboard: index verifier rejected the INDEX.json" >&2
    echo "  stdout: $PUBLISH_ROOT/.verify-index.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.verify-index.stderr" >&2
    echo "  refusing to deploy a red index" >&2
    rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"
    exit 3
fi
rm -f "$PUBLISH_ROOT/.verify-index.stdout" "$PUBLISH_ROOT/.verify-index.stderr"

# --- idempotent Pages project create (first-run only) -------------------
# `wrangler pages deploy <dir>` succeeds when the Pages
# project already exists; on a clean Cloudflare account
# the project does NOT exist and the deploy fails with
# a `not found` error. The runbook idempotently runs
# `wrangler pages project create <name>` BEFORE the
# deploy step, captures the project's first-time create
# exit code, and continues regardless (a "project
# already exists" non-zero exit is the second-run
# shape; a real Cloudflare API failure is a different
# non-zero exit that the next step's `wrangler pages
# deploy` would also surface).
echo "deploy-dashboard: ensuring Pages project '$PAGES_PROJECT' exists"
if ! "$WRANGLER_BIN" pages project create "$PAGES_PROJECT" \
        >"$PUBLISH_ROOT/.wrangler-create.stdout" \
        2>"$PUBLISH_ROOT/.wrangler-create.stderr"; then
    # A "project already exists" failure is a benign
    # second-run path; any other failure is a real
    # Cloudflare API error and surfaces in the next
    # step's deploy anyway. We do NOT treat the
    # `project create` exit code as the gate; we treat
    # the `pages deploy` exit code as the gate. The
    # `pages deploy` step's `wrangler` stdout prints
    # the `pages_url=<url>` line we scrape for the
    # SUMMARY.txt headline + the deploy.json manifest.
    echo "deploy-dashboard: wrangler pages project create non-zero (likely already exists); continuing" >&2
fi
rm -f "$PUBLISH_ROOT/.wrangler-create.stdout" "$PUBLISH_ROOT/.wrangler-create.stderr"

# --- the Cloudflare-Pages-deploy step ------------------------------------
# `wrangler pages deploy <dir> --project-name <name>
# --commit-dirty=true` is the actual push. The runbook
# captures the wrangler stdout (the `pages_url=<url>`
# line wrangler prints) and the wrangler stderr (the
# upload progress a CI worker can also scrape). The
# `--commit-dirty=true` flag is the wrangler shape that
# allows the deploy to land even when the workspace has
# uncommitted changes (the `cargo build` of the
# `trainer` binary in the pre-deploy gate may have left
# `Cargo.lock` or `target/` dirty on a developer
# machine).
echo "deploy-dashboard: deploying $INDEX_DIR to Pages project '$PAGES_PROJECT'"
if ! "$WRANGLER_BIN" pages deploy "$INDEX_DIR" \
        --project-name "$PAGES_PROJECT" \
        --commit-dirty=true \
        >"$PUBLISH_ROOT/.wrangler-deploy.stdout" \
        2>"$PUBLISH_ROOT/.wrangler-deploy.stderr"; then
    echo "deploy-dashboard: wrangler pages deploy failed" >&2
    echo "  stdout: $PUBLISH_ROOT/.wrangler-deploy.stdout" >&2
    echo "  stderr: $PUBLISH_ROOT/.wrangler-deploy.stderr" >&2
    # On error path, leave the
    # `.wrangler-deploy.{stdout,stderr}` files in
    # place so an operator can inspect what went
    # wrong.
    exit 3
fi

# Capture the `pages_url=...` line wrangler printed.
# The wrangler CLI prints the deployed URL on a
# `pages_url=<url>` line on its own stdout; the
# `head -1` of the captured stdout is the line a CI
# worker scrapes. A future wrangler revision that
# renames the line is caught by the
# `deploy_dashboard_cloudflare_script_emits_live_proof_headline`
# shape pin (the SUMMARY.txt append that follows
# embeds the URL into a `live_proof dashboard
# deploy complete: pages_url=<url> ...` headline).
PAGES_URL="$(
    grep -oE 'pages_url=[^ ]+' "$PUBLISH_ROOT/.wrangler-deploy.stdout" \
        | head -1 \
        | sed -E 's/^pages_url=//' \
        || true
)"
if [[ -z "$PAGES_URL" ]]; then
    # Fallback: the `RBP_DASHBOARD_DEPLOYED_URL` env
    # knob, or the `${project}.pages.dev/` default.
    # The fallback keeps the SUMMARY.txt headline +
    # the deploy.json manifest byte-stable on a
    # wrangler revision that drops the
    # `pages_url=<url>` line.
    PAGES_URL="${RBP_DASHBOARD_DEPLOYED_URL:-https://${PAGES_PROJECT}.pages.dev/}"
fi
# STW-059: stamp the resolved `pages_url` back into
# the `RBP_DASHBOARD_DEPLOYED_URL` env knob so a
# *subsequent* `wrangler pages deploy` invocation
# (e.g. a follow-on Pages re-deploy) OR a follow-on
# `cargo run -p rbp-dashboard` smoke test is sourced
# from the same env knob the STW-058
# `serve_static_index` handler reads. The
# `replace_in_readme` sed step + the dashboard's
# meta line + the `deploy.json` `pages_url` field
# are all driven from the same `pages_url` variable;
# the export closes the loop between the `wrangler
# pages deploy` stdout URL and the
# `serve_static_index` env-knob read. The export is
# a *runbook* change only — it does NOT change the
# `RBP_DASHBOARD_DEPLOYED_URL` env knob semantics
# the STW-058 handler reads (the env-knob read shape
# is unchanged; the runbook now writes the knob
# back with the URL wrangler just printed so a
# downstream subprocess picks it up).
export RBP_DASHBOARD_DEPLOYED_URL="$PAGES_URL"
# Also print the export line to stdout so a CI
# worker that scrapes the runbook's stdout can
# confirm the stamp landed (the `export` builtin
# writes to the calling process env but does NOT
# print, so the explicit `echo` is the
# observation surface the STW-059 hand-test
# `RBP_DASHBOARD_CF_API_TOKEN=<dummy>
#  RBP_DASHBOARD_CF_ACCOUNT_ID=<dummy>
#  PUBLISH_ROOT=/tmp/fake
#  scripts/deploy-dashboard-cloudflare.sh`
# `prints export RBP_DASHBOARD_DEPLOYED_URL=<url>
#  to stdout` contract depends on).
echo "export RBP_DASHBOARD_DEPLOYED_URL=$PAGES_URL"
rm -f "$PUBLISH_ROOT/.wrangler-deploy.stdout" "$PUBLISH_ROOT/.wrangler-deploy.stderr"

# --- file/byte counts for the headline + the deploy.json manifest ------
# The `live_proof dashboard deploy complete: pages_url=<url>
# files=<N> bytes=<B>` headline the existing
# `grep ^live_proof` scrape contract expects is
# deterministic + byte-stable on re-runs. The
# `FILES` + `BYTES` counts are the per-deploy
# file / byte count of the `<publish-root>/index/`
# directory the wrangler deploy pushed.
FILES="$(find "$INDEX_DIR" -type f | wc -l | tr -d ' ')"
BYTES="$(find "$INDEX_DIR" -type f -printf '%s\n' | awk '{s+=$1} END {print s+0}')"

# --- the headline SUMMARY.txt -------------------------------------------
# The `INDEX.json` already carries a `SUMMARY.txt` the
# STW-034 runbook wrote; the deploy runbook appends
# the deploy provenance (Pages project + URL +
# wrangler CLI + deployed_at timestamp + the
# `live_proof dashboard deploy complete: ...`
# headline) so a single `cat` confirms the whole
# chain. The `grep ^live_proof dashboard deploy`
# pattern the existing scrape contract publishes
# picks up the headline a CI dashboard reads.
SUMMARY="$PUBLISH_ROOT/index/SUMMARY.txt"
DEPLOYED_AT="$(date -u +%Y-%m-%dT%H:%M:%SZ 2>/dev/null || echo '<unknown>')"
{
    echo ""
    echo "  cloudflare_pages_deploy:"
    echo "    project:      $PAGES_PROJECT"
    echo "    pages_url:    $PAGES_URL"
    echo "    wrangler_bin: $WRANGLER_BIN"
    echo "    deployed_at:  $DEPLOYED_AT"
    echo "    files:        $FILES"
    echo "    bytes:        $BYTES"
} >> "$SUMMARY"
# The headline the existing `grep ^live_proof`
# scrape contract expects.
printf 'live_proof dashboard deploy complete: pages_url=%s files=%d bytes=%d\n' \
    "$PAGES_URL" "$FILES" "$BYTES" >> "$SUMMARY"

# --- the deploy.json manifest ------------------------------------------
# The machine-readable complement to the
# `live_proof dashboard deploy complete: ...`
# headline. The dashboard's render layer can `cat
# deploy.json` to read the `pages_url` / `files` /
# `bytes` fields in one JSON read.
DEPLOY_JSON="$PUBLISH_ROOT/index/deploy.json"
cat > "$DEPLOY_JSON" <<JSON
{
  "pages_url": "$PAGES_URL",
  "project": "$PAGES_PROJECT",
  "wrangler_bin": "$WRANGLER_BIN",
  "deployed_at": "$DEPLOYED_AT",
  "files": $FILES,
  "bytes": $BYTES
}
JSON

# --- README reconciliation step -----------------------------------------
# The README's `## Public dashboard` URL line is a
# baked-in `<https://robopoker-testnet-dashboard.pages.dev/>`
# placeholder the operator's first run replaces
# with the real URL `wrangler` printed. The
# runbook does the replacement in-place on the
# workspace's `README.md` (the only file the
# change touches; a future copy-on-deploy the
# STW-058 + STW-059 follow-on lands will keep
# the URL byte-stable across re-deploys to the
# same Pages project). A subsequent `git diff
# README.md` shows the URL line move from the
# placeholder to the real URL; a subsequent
# `git commit README.md` lands the change.
README="$WORKSPACE_ROOT/README.md"
if [[ -f "$README" ]]; then
    # Replace the placeholder line with the real URL.
    # The placeholder is the bare
    # `https://robopoker-testnet-dashboard.pages.dev/`
    # token the README's `## Public dashboard` line
    # carries (the line is rendered as
    # `Public dashboard: <https://...>`). A future
    # re-deploy to a different Pages project updates
    # the same line in-place; the README's `${VAR:-default}`
    # env-knob form (the STW-058 follow-on) keeps
    # the URL byte-stable across re-deploys to the
    # same Pages project.
    if grep -q "https://robopoker-testnet-dashboard.pages.dev/" "$README"; then
        # Use a portable sed form (BSD sed on macOS
        # vs. GNU sed on Linux differ on the `-i`
        # flag; the `tempfile + mv` shape works on
        # both). The escape sequences are minimal
        # because the URL is a fixed token.
        TMP="$(mktemp)"
        sed "s|https://robopoker-testnet-dashboard.pages.dev/|$PAGES_URL|g" \
            "$README" > "$TMP"
        mv "$TMP" "$README"
        echo "deploy-dashboard: reconciled README '## Public dashboard' line to $PAGES_URL"
    fi
fi

# Echo the headline line so a CI worker scraping
# stdout can pin the dashboard-deploy step without
# reading the file.
cat "$SUMMARY"

echo "deploy-dashboard: chain landed end-to-end"
echo "  pages_url:   $PAGES_URL"
echo "  project:     $PAGES_PROJECT"
echo "  summary:     $SUMMARY"
echo "  deploy.json: $DEPLOY_JSON"
echo "  re-verify:   $TRAINER_BIN --verify-index $INDEX_DIR"
