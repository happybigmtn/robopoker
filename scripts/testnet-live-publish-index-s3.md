# scripts/testnet-live-publish-index-s3.sh — STW-035 runbook

The **publish-index-remote** runbook is the v9 follow-on the
`scripts/testnet-live-publish-index.sh` (STW-034) doc's
scope-boundary section defers to: a deterministic
`index_remote_plan.json` + `index_remote_receipt.json` pair a
CI worker can `aws s3 cp` to push the STW-034 `INDEX.json`
aggregator to a dashboard bucket, AND a no-DB no-rebuild
re-verify path that re-hashes the local `INDEX.json` +
compares to the post-upload `index_remote_receipt.json`'s
`index_sha256` field.

The runbook is the operator-facing entry point for the
STW-035 v9 follow-on chain. It chains
`trainer --verify-index <index-dir>` (the pre-upload
refuse-to-upload-red-index gate) →
`trainer --publish-index-remote <publish-root> --bucket
<s3://...> [--prefix <prefix/>] [--no-dry-run]` (the plan +
post-upload-receipt writer) →
`trainer --verify-index-remote <remote-dir>` (the
post-upload re-verifier) as a sequence of subprocesses and
writes a `SUMMARY.txt` headline a CI worker can `cat` to
confirm the index-remote step landed end-to-end.

## Why a separate upload step (not a refactor of STW-034)

The STW-034 chain is the *aggregator* side: turn a publish
root into one `INDEX.json`. The STW-035 index-remote step
is the *uploader* side: turn that one `INDEX.json` into a
remote-upload plan + a post-upload
`index_remote_receipt.json`. The two are split because the
error surfaces are different: a regression in a single
per-entry `remote_receipt.s3_objects[].sha256` does not
change the `index_remote_plan.json` upload plan's
`s3_objects[]` field, and vice versa. A typed
`PublishIndexRemoteError` enum lives next to the
index-remote so the integration test can assert on the
failure kind a dashboard scraper greps.

## Output layout (drop into `<publish_root>/index_remote/`)

```
<publish_root>/index_remote/
  index_remote_plan.json       # per-file INDEX.json -> s3_uri mapping
  index_remote_receipt.json    # post-upload per-file sha256 + bytes
  SUMMARY.txt                  # single-line headline a CI worker `cat`s
```

The `index_remote/` subdir is **read-only** with respect
to the publish root + the `INDEX.json` (the index-remote
step writes a fresh `index_remote/` subdir, so a
`trainer --publish-index-remote` invocation cannot mutate
the underlying `INDEX.json` or the per-entry
`remote_receipt.json` files even on partial-failure paths).

## The `index_remote_plan.json` shape

```json
{
  "bucket": "s3://robopoker-testnet-dashboard",
  "prefix": "publish-20260604T050000Z/index/",
  "s3_objects": [
    {
      "local_path": "/abs/path/to/publish-root/index/INDEX.json",
      "sha256": "0123...64hex",
      "bytes": 2048,
      "s3_uri": "s3://robopoker-testnet-dashboard/publish-20260604T050000Z/index/INDEX.json"
    }
  ],
  "index_sha256": "0123...64hex",
  "index_bytes": 2048,
  "publish_root_basename": "publish-20260604T050000Z",
  "runbook_version": "STW-035 v1",
  "created_at_utc": "2026-06-04T00:00:00Z",
  "dry_run": true
}
```

The `s3_objects[]` array is sorted by `s3_uri` for
determinism (a CI worker that re-runs the runbook on an
unchanged publish root produces a byte-identical plan).
The upload step pushes ONE file: the `INDEX.json` the
STW-034 chain wrote (the per-entry `remote_receipt.json`
files are already in the bucket, the STW-033 runbook
pushed them).

The top-level `created_at_utc` is the
`RBP_PUBLISH_INDEX_REMOTE_UTC` env knob (or the
`<unknown>` sentinel when unset, so the lib test +
integration test are byte-stable on a CI runner that
does not stamp the env knob).

## The "refuse to paper-over a red index" gate

The index-remote step re-verifies the `INDEX.json` with
`PublishIndex::verify` AS A PRE-UPLOAD GATE. A red
`INDEX.json` returns
`Err(PublishIndexRemoteError::IndexRed(...))` before the
plan is written. The bash runbook converts the non-zero
exit to a runbook exit 5. This is the "refuse to
paper-over a red index" invariant the STW-034 index
verifier already enforces.

## Scope boundary

The index-remote step does NOT push via a vendored AWS /
GCS SDK (the live `aws s3 cp` shell-out is the bash
runbook's job — adding a 50-MB SDK to a no-system-deps
trainer binary is the inverse of the "pure bash + cargo +
trainer" shape the rest of the autotrain pipeline already
follows); does NOT shell out to `aws` in the default
`trainer --publish-index-remote` path (the
`cargo test --workspace` integration test runs in dry-run
so a regression in the CLI surface fails CI without an
`aws` credential or a live bucket); does NOT touch the
STW-019 `testnet-live-proof.sh` or STW-032
`testnet-live-publish.sh` or STW-033
`testnet-live-publish-s3.sh` or STW-034
`testnet-live-publish-index.sh` runbook (the
index-remote is a follow-on *consumer* of the `INDEX.json`
the STW-034 chain produces, not a refactor); does NOT
change the STW-034 `PublishIndex` / `IndexedEntry` /
`PublishIndexError` JSON shape (a manifest drift fails
the index-remote step's pre-upload `trainer --verify-index`
call); does NOT change the STW-033 `PublishRemotePlan` /
`PublishedRemoteReceipt` / `S3Object` JSON shape (a
`remote_receipt.json` drift fails the index-remote step's
per-entry `PublishedRemoteReceipt::verify` call); does
NOT introduce a Python / `jq` dependency (the runbook is
pure bash + `find` + `sha256sum`).

## CLI surface (the two arms the runbook shells out to)

```
trainer --publish-index-remote <publish-root> --bucket <s3://...> [--prefix <prefix/>] [--no-dry-run]
trainer --verify-index-remote <remote-dir>
```

A bare `--publish-index-remote` with no path returns a
one-line usage + exit 2. A bare `--publish-index-remote`
with no `--bucket` returns a one-line usage + exit 2. A
bare `--verify-index-remote` with no path returns a
one-line usage + exit 2. The `--publish-index-remote` arm
prints a one-line
`live_proof publish_index_remote complete: ...` headline
a dashboard scraper greps. The `--verify-index-remote`
arm prints a one-line
`live_proof index_remote verification passed: ...` /
`live_proof index_remote verification failed: ...`
headline a dashboard scraper greps.

## Environment

| Env knob                          | Default                              | Notes |
|-----------------------------------|--------------------------------------|-------|
| `PUBLISH_ROOT`                    | (none)                               | The `<publish-root>/` directory the STW-034 runbook produced. The script refuses to run with exit 3 if the path is missing or not a directory. |
| `PUBLISH_BUCKET`                  | (none)                               | Bucket URI (`s3://<name>/`) or bare bucket name. REQUIRED. The script refuses to run with exit 3 if the bucket is missing. |
| `PUBLISH_PREFIX`                  | `<root-basename>/index/`             | Key prefix inside the bucket. |
| `TRAINER_BIN`                     | `<workspace>/target/debug/trainer`   | Falls back to `cargo build --bin trainer` when the file is missing. |
| `PUBLISH_DRY_RUN`                 | `1`                                  | Set to `0` to shell out to `aws s3 cp` per file (requires the `aws` CLI on `$PATH` + `AWS_ACCESS_KEY_ID` / `AWS_SECRET_ACCESS_KEY` env knobs; a missing `aws` returns `PublishIndexRemoteError::AwsCli` and the arm exits 2). |
| `RBP_PUBLISH_INDEX_REMOTE_UTC`    | `date -u +%Y-%m-%dT%H:%M:%SZ`        | Stamped on `index_remote_receipt.json`'s `created_at_utc` / `uploaded_at_utc` field. Falls back to `<unknown>` when unset. |

## Exit codes

| Exit | Meaning |
|------|---------|
| 0    | Index-remote written end-to-end; `index_remote_plan.json` + `index_remote_receipt.json` landed under `<publish_root>/index_remote/`; the `trainer --verify-index-remote` re-verify exited 0. |
| 3    | `PUBLISH_ROOT` missing or not a directory, or `PUBLISH_BUCKET` missing. |
| 4    | trainer binary not found and `cargo build` failed. |
| 5    | `trainer --verify-index <index-dir>` exited non-zero (red `INDEX.json` detected, or other index error: refuse to upload a red index). |
| 6    | `trainer --publish-index-remote <root>` exited non-zero (the plan + post-upload-receipt writer failed). |
| 7    | `trainer --verify-index-remote <remote-dir>` exited non-zero (the post-upload re-verify failed). |

## Usage

```bash
bash scripts/testnet-live-publish-index-s3.sh <publish-root> <s3://bucket>
```

Example:

```bash
bash scripts/testnet-live-publish-index-s3.sh \
    receipts/publish-20260604T050000Z/ \
    s3://robopoker-testnet-dashboard
```

## What it does (end-to-end)

1. Refuse to run with exit 3 if `PUBLISH_ROOT` is missing
   or not a directory, or `PUBLISH_BUCKET` is missing, or
   the publish root has no `index/INDEX.json` (the
   STW-034 runbook must run first).
2. Refuse to run with exit 4 if the `trainer` binary is
   missing and `cargo build --bin trainer` fails.
3. Default the `RBP_PUBLISH_INDEX_REMOTE_UTC` env knob to
   `date -u +%Y-%m-%dT%H:%M:%SZ` (or `<unknown>` if
   `date` is missing).
4. Default `PUBLISH_DRY_RUN` to `1` (dry-run).
5. Shell out to `trainer --verify-index <index-dir>`:
   - Re-hash every local file the `INDEX.json` claims to
     have inlined.
   - Assert every digest matches.
   - Assert every `s3_uri` in the index appears in the
     inlined plan (a phantom `s3_uri` is a hard
     `PublishIndexError::MissingObject`).
6. Shell out to `trainer --publish-index-remote
   <publish-root> --bucket <s3://...> [--prefix
   <prefix/>] [--no-dry-run]`:
   - Re-verify the `INDEX.json` with
     `PublishIndex::verify` AS A PRE-UPLOAD GATE (refuse
     to paper-over a red `INDEX.json`).
   - Re-validate every per-entry
     `remote_receipt.json` the STW-034 chain inlined
     (the STW-033 `PublishedRemoteReceipt` is the source
     of truth for the per-file upload plan the STW-033
     runbook already pushed to the bucket — the
     index-remote step only pushes the new `INDEX.json`
     file).
   - Build the per-file `s3_objects[]` array
     (`INDEX.json -> s3://<bucket>/<prefix>/INDEX.json`).
   - In live mode, shell out to `aws s3 cp` per file.
   - Write a deterministic
     `<publish-root>/index_remote/index_remote_plan.json`
     + `index_remote_receipt.json` + `SUMMARY.txt` trio
     (entries sorted by `s3_uri`).
7. Shell out to `trainer --verify-index-remote
   <remote-dir>`:
   - Re-hash the local `INDEX.json` the
     `index_remote_receipt.json` claims to have uploaded.
   - Assert every digest matches.
   - Assert every `s3_uri` in the receipt appears in the
     inlined plan (a phantom `s3_uri` is a hard
     `PublishIndexRemoteError::MissingObject`).
8. Append the launch provenance (publish root + bucket +
   prefix + created-at timestamp + trainer binary) to
   `<publish_root>/index_remote/SUMMARY.txt` so a single
   `cat` confirms the whole chain.
9. Print the `SUMMARY.txt` headline to stdout a CI worker
   scrapes.

## See also

- `scripts/testnet-live-proof.sh` (STW-019) — the
  source `live_proof` runbook that drops the
  `receipts/testnet-live-proof-<UTC-ISO>/` receipt
  directory.
- `scripts/testnet-live-publish.sh` (STW-032) — the
  publish runbook that drops the
  `<publish>/<basename>/` publish bundle directory.
- `scripts/testnet-live-publish-s3.sh` (STW-033) — the
  publish-remote runbook that drops the
  `<publish>/<basename>/remote/remote_receipt.json`
  post-upload manifest.
- `scripts/testnet-live-publish-index.sh` (STW-034) —
  the publish-index runbook that drops the
  `<publish-root>/index/INDEX.json` aggregator the
  STW-035 index-remote step uploads.
- `crates/autotrain/src/publish_index_remote.rs` — the
  `trainer --publish-index-remote` + `trainer
  --verify-index-remote` Rust source (the no-DB
  no-network uploader + the no-DB no-rebuild
  re-verifier).
- `crates/autotrain/tests/publish_index_remote.rs` — the
  integration test that pins the CLI surface
  end-to-end (round-trip + red-index + missing-bucket).
- `crates/autotrain/tests/script_shape.rs` — the
  shell-shape integration test that pins the runbook
  script (exists + executable + parses with `bash -n` +
  shells out to `--verify-index` BEFORE
  `--publish-index-remote` + references the
  `--publish-index-remote` / `--bucket` CLI
  subcommand + the `testnet-live-publish-index.md` doc
  references the `--verify-index-remote` re-verify
  subcommand).
