# `scripts/testnet-live-publish-dashboard.sh` — STW-036 dashboard-deploy runbook

The v10 follow-on the STW-035 publish-index-remote runbook doc
defers to. A CI worker that produced an `INDEX.json` (via the
STW-034 → STW-035 chain) and the `transcripts/` directory the
bench wrote can bring up a public dashboard in one
`aws s3 sync` step.

The runbook reads a `<publish-root>/index/INDEX.json` the
STW-034 `testnet-live-publish-index.sh` runbook produced,
verifies the index (the pre-deploy refuse-to-deploy-red-index
gate the STW-034 `trainer --verify-index <index-dir>` CLI is
the source of truth for), and `aws s3 sync`s the
`<publish-root>/index/` directory to a public S3 / Cloudflare
Pages bucket the dashboard's `RBP_DASHBOARD_INDEX_URL` env
knob points at.

The dashboard's `IndexClient` reads the bucket-hosted
`INDEX.json` via the `RBP_DASHBOARD_INDEX_URL` env knob
(default `http://localhost:8080/api/index` in tests, a
CloudFront URL in production). The typed read re-uses
`rbp_autotrain::PublishIndex` (the same Rust type the
STW-034 chain writes), so a shape drift in `INDEX.json` fails
BOTH the dashboard's typed read AND the
`trainer --verify-index` re-verify at the same CI step.

## Usage

```bash
bash scripts/testnet-live-publish-dashboard.sh \
    receipts/publish-20260604T050000Z/ \
    s3://robopoker-testnet-dashboard
```

## Environment

| Knob | Default | Purpose |
|------|---------|---------|
| `PUBLISH_ROOT` | (positional) | `<publish-root>/` the STW-034 chain produced. REQUIRED. |
| `PUBLISH_BUCKET` | (positional) | `s3://<bucket>` or bare `<bucket>`. REQUIRED. |
| `PUBLISH_PREFIX` | `<root-basename>/index/` | Bucket key prefix. |
| `TRAINER_BIN` | `<workspace>/target/debug/trainer` | `trainer` binary path; auto-builds if missing. |
| `AWS_BIN` | `aws` | `aws` CLI path. No on-demand install. |

## Exit codes

| Code | Meaning |
|------|---------|
| `0` | Index deployed end-to-end; `INDEX.json` + `SUMMARY.txt` landed under `<bucket>/<prefix>/`; `aws s3 sync` exited 0. |
| `3` | `PUBLISH_ROOT` missing / not a directory, or `PUBLISH_BUCKET` missing, or `INDEX.json` missing under `<publish-root>/index/` (refuse-to-run gate). |
| `4` | `aws` CLI not found, or `cargo build --bin trainer` failed. |
| `5` | `trainer --verify-index <index-dir>` exited non-zero (red `INDEX.json` detected — refuse to deploy a red index). |
| `6` | `aws s3 sync` exited non-zero. |

## Output layout

```
<bucket>/<prefix>/
  INDEX.json      # the STW-034 aggregator
  SUMMARY.txt     # the STW-034 headline
```

The `SUMMARY.txt` is the existing STW-034 headline the
publish-index runbook already wrote; this runbook appends
the deploy provenance (bucket + prefix + `aws` CLI + deploy
timestamp) so a single `cat` confirms the whole chain.

## Pinning

The `crates/autotrain/tests/script_shape.rs` shell-shape
pinners assert the script is on disk + executable + parses
with `bash -n`, the `--verify-index <index-dir>` pre-deploy
gate fires BEFORE the `aws s3 sync` step, and the script
invokes the `aws s3 sync ... --delete --cache-control
max-age=60` form. A regression in any of these surfaces as a
failing `cargo test -p rbp-autotrain --test script_shape`
invocation at the same CI step a future operator would
silently break.

## See also

- `scripts/testnet-live-publish-index-s3.sh` — the STW-035
  publish-index-remote runbook this script consumes.
- `scripts/testnet-live-publish-index.sh` — the STW-034
  publish-index runbook that wrote the
  `<publish-root>/index/INDEX.json` the dashboard serves.
- `crates/dashboard/src/` — the typed `IndexClient` + the
  `axum` router + the static `index.html` the dashboard
  binary ships.
- `crates/dashboard/tests/smoke.rs` — the four-route
  end-to-end smoke test.
