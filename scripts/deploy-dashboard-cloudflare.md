# `scripts/deploy-dashboard-cloudflare.sh` — STW-054 Cloudflare-Pages deploy runbook

The *deploy* leg of the testnet public-surface north star
the prior CEO reviews named but did not row up. The
`scripts/testnet-live-publish-dashboard.sh` (STW-036)
runbook ships, but it shells out to `aws s3 sync` against
a bucket that does not exist on disk; the README's
`## Public dashboard` link is a
`<https://robopoker-testnet-dashboard.pages.dev/>`
placeholder; no `wrangler` config / no `cloudflared` / no
Terraform is committed. A stranger clicking the README
link gets a 404.

STW-054 lands `scripts/deploy-dashboard-cloudflare.sh`
— a pure-bash runbook that mirrors the
`scripts/testnet-live-publish-dashboard.sh` (STW-036)
S3/CloudFront chain shape, but uses `wrangler pages
deploy` instead of `aws s3 sync` so an operator with a
Cloudflare account (but no AWS account) can still bring
up the testnet dashboard. The two runbooks coexist:
STW-036 remains the S3/CloudFront path; STW-054 is the
Cloudflare Pages path.

The runbook reads a `<publish-root>/index/INDEX.json`
the STW-034 `testnet-live-publish-index.sh` runbook
produced (the dashboard data feed), verifies the index
(the pre-deploy refuse-to-deploy-red-index gate the
STW-034 `trainer --verify-index <index-dir>` CLI is
the source of truth for), and `wrangler pages deploy`s
the `<publish-root>/index/` directory to a Cloudflare
Pages project the runbook's
`RBP_DASHBOARD_PAGES_PROJECT` env knob names (default
`robopoker-testnet-dashboard`).

The dashboard's `IndexClient` reads the bucket-hosted
`INDEX.json` via the `RBP_DASHBOARD_INDEX_URL` env knob
(default `http://localhost:8080/api/index` in tests, the
Cloudflare Pages URL in production). The typed read
re-uses `rbp_autotrain::PublishIndex` (the same Rust
type the STW-034 chain writes), so a shape drift in
`INDEX.json` fails BOTH the dashboard's typed read AND
the `trainer --verify-index` re-verify at the same CI
step.

## Usage

```bash
export RBP_DASHBOARD_CF_API_TOKEN=<cloud...n>
export RBP_DASHBOARD_CF_ACCOUNT_ID=<cloudflare-account-id>
bash scripts/deploy-dashboard-cloudflare.sh \
    receipts/publish-20260604T050000Z/
```

The runbook chains `trainer --verify-index
<index-dir>` (the pre-deploy refuse-to-deploy-red-index
gate) → `wrangler pages project create <name>` (the
idempotent first-time project-create, skipped on
subsequent runs) → `wrangler pages deploy <index-dir>
--project-name <name> --commit-dirty=true` (the actual
Pages push) → a one-line README reconciliation step
that updates the README's `## Public dashboard` URL
line from the `<https://robopoker-testnet-dashboard.pages.dev/>`
placeholder to the real URL `wrangler` printed.

## Environment

| Knob | Default | Purpose |
|------|---------|---------|
| `PUBLISH_ROOT` | (positional) | `<publish-root>/` the STW-034 chain produced. REQUIRED. |
| `RBP_DASHBOARD_CF_API_TOKEN` | (none) | Cloudflare API token the runbook exports as `CLOUDFLARE_API_TOKEN` for `wrangler`. REQUIRED. |
| `RBP_DASHBOARD_CF_ACCOUNT_ID` | (none) | Cloudflare account ID the runbook uses for the first-time `wrangler pages project create`. REQUIRED. |
| `RBP_DASHBOARD_PAGES_PROJECT` | `robopoker-testnet-dashboard` | Pages project name; the `wrangler.toml` at the repo root pins the same default. |
| `RBP_DASHBOARD_DEPLOYED_URL` | `https://<project>.pages.dev/` | The post-deploy public URL the runbook reconciles the README's `## Public dashboard` line to. |
| `WRANGLER_BIN` | `wrangler` | `wrangler` CLI path. No on-demand install. |
| `TRAINER_BIN` | `<workspace>/target/debug/trainer` | `trainer` binary path; auto-builds if missing. |

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Deploy succeeded; `live_proof dashboard deploy complete: pages_url=<url> files=<N> bytes=<B>` line in `SUMMARY.txt`; `deploy.json` written next to `SUMMARY.txt` with the `pages_url` / `files` / `bytes` fields; README `## Public dashboard` line updated to the real URL. |
| `1` | Script-internal error (e.g. `cargo build --bin trainer` failed). |
| `3` | Missing `RBP_DASHBOARD_CF_API_TOKEN` / missing `wrangler` on `$PATH` / missing `RBP_DASHBOARD_CF_ACCOUNT_ID` / missing `PUBLISH_ROOT` (or not a directory) / missing `INDEX.json` under `<publish-root>/index/` (refuse-to-run gate); failed `trainer --verify-index <index-dir>` (red `INDEX.json` detected, refuse to deploy a red index); failed `wrangler pages deploy` (the Cloudflare Pages push failed). |

## Output layout

```
<publish-root>/index/
  INDEX.json                    # the STW-034 aggregator
  SUMMARY.txt                   # the STW-034 headline + the STW-054 deploy provenance
  deploy.json                   # the STW-054 machine-readable deploy manifest
```

The `SUMMARY.txt` is the existing STW-034 headline the
publish-index runbook already wrote; this runbook
appends the deploy provenance (Pages project + URL +
`wrangler` CLI + deploy timestamp + the `live_proof
dashboard deploy complete: ...` headline) so a single
`cat` confirms the whole chain. The `grep ^live_proof
dashboard deploy` pattern the existing scrape contract
publishes picks up the headline a CI dashboard reads.

The `deploy.json` is the machine-readable complement
to the `live_proof dashboard deploy complete: ...`
headline — a `pages_url` / `project` /
`wrangler_bin` / `deployed_at` / `files` / `bytes`
JSON object the dashboard's render layer can `cat` in
one read.

## The `live_proof dashboard deploy complete: ...` headline

A `live_proof dashboard deploy complete: pages_url=<url>
files=<N> bytes=<B>` line is appended to
`<publish-root>/index/SUMMARY.txt` after the
`wrangler pages deploy` step exits 0 + the
`pages_url=<url>` line `wrangler` printed is captured.
The `FILES` + `BYTES` counts are computed by
`find "$INDEX_DIR" -type f -printf '%s\n' | wc -l` +
`find "$INDEX_DIR" -type f -printf '%s\n' | awk '{s+=$1}
END {print s}'` so the headline is deterministic +
byte-stable on re-runs.

## Pinning

The `crates/autotrain/tests/script_shape.rs` shell-shape
pinner
`deploy_dashboard_cloudflare_script_exists_and_parses`
asserts the runbook is on disk + executable + parses
with `bash -n`. A regression in any of these surfaces
as a failing `cargo test -p rbp-autotrain --test
script_shape` invocation at the same CI step a future
operator would silently break.

The companion
`deploy_dashboard_cloudflare_script_emits_live_proof_headline`
pin (STW-057) asserts the runbook's source contains the
literal `live_proof dashboard deploy complete:
pages_url=` string. A regression in the headline
contract is caught at the same CI step a CI dashboard's
`grep ^live_proof dashboard deploy` scrape would
silently miss.

## See also

- `scripts/testnet-live-publish-dashboard.sh` (STW-036) —
  the S3/CloudFront sibling the runbook supersedes for
  the Cloudflare Pages path. The S3 path remains for
  operators who prefer CloudFront over Cloudflare Pages.
- `scripts/testnet-live-publish-index.sh` (STW-034) —
  the publish-index runbook the deploy step consumes
  `INDEX.json` from.
- `wrangler.toml` — the minimum config the
  Cloudflare Pages path needs (the `name =
  "robopoker-testnet-dashboard"` project name only; no
  `account_id` / no `api_token` / no `compatibility_date`
  / no `pages_build_output_dir`).
- `crates/autotrain/tests/script_shape.rs` — the
  shell-shape pinner (no DB required; runs in
  `cargo test --workspace`).
- `crates/dashboard/src/` — the typed `IndexClient` +
  the `axum` router + the static `index.html` the
  dashboard binary ships.
- `crates/dashboard/tests/smoke.rs` — the four-route
  end-to-end smoke test.
