# scripts/testnet-live-publish-index.sh — STW-034 runbook

The **publish-index** runbook is the v8 follow-on the
`scripts/testnet-live-publish-s3.sh` (STW-033) doc names as
the next slice: a deterministic aggregator over every
`publish/<basename>/remote/remote_receipt.json` the
STW-033 chain produced on a single machine. The aggregator
is the testnet dashboard's `cat INDEX.json` away — a
single `INDEX.json` + `SUMMARY.txt` pair a dashboard can
scrape instead of listing the bucket + fetching N
manifests.

The runbook is the operator-facing entry point for the
STW-034 v8 follow-on chain. It chains
`trainer --publish-index <publish-root>` (the index
writer) + `trainer --verify-index <index-path>` (the
index re-verifier) as a sequence of subprocesses and
writes a `SUMMARY.txt` headline a CI worker can `cat` to
confirm the index step landed end-to-end.

## Why a separate aggregator (not a refactor of STW-033)

The STW-033 chain is the *producer* side: turn one
`testnet-live-proof-<UTC-ISO>/` receipt into one upload
plan + one `remote_receipt.json` per receipt. The
STW-034 index step is the *aggregator* side: scan a
publish root, collect every `remote_receipt.json`, write
a single `INDEX.json` an auditor can scrape. The two
are split because the error surfaces are different: a
regression in a single `remote_receipt.json`'s
`s3_uri` field does not change the `INDEX.json`
aggregator's `entry_count` / `total_bytes` field, and
vice versa. A typed `PublishIndexError` enum lives next
to the indexer so the integration test can assert on
the failure kind a dashboard scraper greps.

## Output layout (drop into `<publish_root>/index/`)

```
<publish_root>/index/
  INDEX.json    # sorted-by-receipt_basename aggregator
  SUMMARY.txt   # single-line headline a CI worker `cat`s
```

The `INDEX.json` is **read-only** with respect to the
publish root: the indexer writes its output under
`<publish_root>/index/`, so a
`trainer --publish-index` invocation cannot mutate the
underlying `remote_receipt.json` files even on
partial-failure paths.

## The `INDEX.json` shape

```json
{
  "publish_root": "/abs/path/to/publish-root",
  "runbook_version": "STW-034 v1",
  "created_at_utc": "2026-06-04T00:00:00Z",
  "entry_count": 3,
  "total_bytes": 24576,
  "entries": [
    {
      "receipt_basename": "testnet-live-proof-20260604T050000Z",
      "receipt_dir": "/abs/path/to/publish-root/publish/testnet-live-proof-20260604T050000Z",
      "remote_receipt_path": "/abs/path/to/publish-root/publish/testnet-live-proof-20260604T050000Z/remote/remote_receipt.json",
      "remote_receipt": { ... }
    },
    ...
  ]
}
```

The `entries[]` array is sorted by `receipt_basename`
so re-running the index step on an unchanged publish
root produces a byte-identical `INDEX.json`. Each
entry inlines the `PublishedRemoteReceipt` the
STW-033 runbook wrote (bucket + prefix + `s3_objects[]`
+ `bundle_sha256` + `total_bytes` + `uploaded_at_utc` +
`runbook_version`), so a dashboard scraper can read
the full upload plan + per-file `s3_uri`s from the index
without re-fetching the per-receipt
`remote_receipt.json`.

The top-level `created_at_utc` is the
`RBP_PUBLISH_INDEX_UTC` env knob (or the
`<unknown>` sentinel when unset, so the lib test +
integration test are byte-stable on a CI runner that
does not stamp the env knob).

## The "refuse to paper-over a red remote receipt" gate

The indexer re-verifies every `remote_receipt.json`
with `PublishedRemoteReceipt::verify` AS A PER-ENTRY
PRE-INDEX GATE. A red `remote_receipt.json` returns
`Err(PublishIndexError::RemoteReceiptRed(...))` before
the `INDEX.json` is written. The bash runbook converts
the non-zero exit to a runbook exit 5. This is the
"refuse to paper-over a red remote receipt" invariant
the STW-028 receipt verifier + STW-032 bundle verifier
+ STW-033 remote-receipt verifier already enforce.

## Scope boundary

The index step does NOT push to S3 / GCS / git-tag (a
CI worker can `aws s3 cp` the local
`publish/<root>/index/` directory in a follow-on
slice); does NOT change the STW-019
`testnet-live-proof.sh` or STW-032
`testnet-live-publish.sh` or STW-033
`testnet-live-publish-s3.sh` runbook (the index is a
follow-on *consumer* of the `remote_receipt.json`
files the STW-033 chain produces, not a refactor);
does NOT change the STW-033 `PublishRemotePlan` /
`PublishedRemoteReceipt` / `S3Object` JSON shape (a
manifest drift fails the index step's per-entry
pre-index `trainer --verify-remote` check); does NOT
introduce a Python / `jq` dependency (the runbook is
pure bash + `find` + `sha256sum`).

## CLI surface (the two arms the runbook shells out to)

```
trainer --publish-index <publish-root>
trainer --verify-index <index-path>
```

A bare `--publish-index` with no path returns a
one-line usage + exit 2. A bare `--verify-index`
with no path returns a one-line usage + exit 2. The
`--publish-index` arm prints a one-line
`live_proof publish_index complete: ...` headline a
dashboard scraper greps. The `--verify-index` arm
prints a one-line
`live_proof index verification passed: ...` /
`live_proof index verification failed: ...`
headline a dashboard scraper greps.

## Environment

| Env knob                  | Default                              | Notes |
|---------------------------|--------------------------------------|-------|
| `PUBLISH_ROOT`            | (none)                               | The `<publish-root>/` directory the STW-033 runbook produced. The script refuses to run with exit 3 if the path is missing or not a directory. |
| `TRAINER_BIN`             | `<workspace>/target/debug/trainer`   | Falls back to `cargo build --bin trainer` when the file is missing. |
| `RBP_PUBLISH_INDEX_UTC`   | `date -u +%Y-%m-%dT%H:%M:%SZ`        | Stamped on `INDEX.json`'s `created_at_utc` field. Falls back to `<unknown>` when unset. |

## Exit codes

| Exit | Meaning |
|------|---------|
| 0    | Index written end-to-end; `INDEX.json` + `SUMMARY.txt` landed under `<publish_root>/index/`; the `trainer --verify-index` re-verify exited 0. |
| 3    | `PUBLISH_ROOT` missing or not a directory. |
| 4    | trainer binary not found and `cargo build` failed. |
| 5    | `trainer --publish-index <root>` exited non-zero (red `remote_receipt.json` detected, or other aggregator error: refuse to write a paper-over index). |
| 6    | `trainer --verify-index <index-path>` exited non-zero (the post-index re-verify failed). |

## Usage

```bash
bash scripts/testnet-live-publish-index.sh <publish-root>
```

Example:

```bash
bash scripts/testnet-live-publish-index.sh \
    receipts/publish-20260604T050000Z/
```

## What it does (end-to-end)

1. Refuse to run with exit 3 if `PUBLISH_ROOT` is missing
   or not a directory.
2. Refuse to run with exit 4 if the `trainer` binary is
   missing and `cargo build --bin trainer` fails.
3. Default the `RBP_PUBLISH_INDEX_UTC` env knob to
   `date -u +%Y-%m-%dT%H:%M:%SZ` (or `<unknown>` if `date`
   is missing).
4. Shell out to `trainer --publish-index <publish-root>`:
   - Read every `<publish_root>/publish/<basename>/remote/remote_receipt.json`.
   - Re-verify every `remote_receipt.json` with the
     STW-033 `PublishedRemoteReceipt::verify` as a
     per-entry pre-index gate (refuse to paper-over a red
     `remote_receipt.json`).
   - Write a deterministic
     `<publish_root>/index/INDEX.json` + `SUMMARY.txt`
     pair (entries sorted by `receipt_basename`).
5. Shell out to `trainer --verify-index <index-dir>`:
   - Re-hash every local file the `INDEX.json` claims
     to have inlined.
   - Assert every digest matches.
   - Assert every `s3_uri` in the index appears in
     the inlined plan (a phantom `s3_uri` is a hard
     `PublishIndexError::MissingObject`).
6. Append the launch provenance (publish root + index
   path + created-at timestamp + trainer binary) to
   `<publish_root>/index/SUMMARY.txt` so a single `cat`
   confirms the whole chain.
7. Print the `SUMMARY.txt` headline to stdout a CI
   worker scrapes.

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
  post-upload manifest the STW-034 indexer scans.
- `crates/autotrain/src/publish_index.rs` — the
  `trainer --publish-index` + `trainer --verify-index`
  Rust source (the no-DB no-network aggregator + the
  no-DB no-rebuild re-verifier).
- `crates/autotrain/tests/publish_index.rs` — the
  integration test that pins the CLI surface
  end-to-end (round-trip + red-remote-receipt +
  missing-publish-root + verify-index round-trip).
- `crates/autotrain/tests/script_shape.rs` — the
  shell-shape integration test that pins the runbook
  script (exists + executable + parses with `bash -n` +
  shells out to `--publish-index` + `--verify-index`).
