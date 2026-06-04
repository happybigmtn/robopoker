# Testnet live launch publish runbook (STW-032)

The `scripts/testnet-live-proof.sh` runbook (STW-019) writes a
local `receipts/testnet-live-proof-<UTC-ISO>/` directory the
operator (or a testnet dashboard) can scrape. STW-032 lands the
*publish* step the runbook doc names as the "next slice
(`testnet-live-publish`)" — a deterministic, content-addressed
portable publish bundle a third party (a testnet dashboard
bucket, a CI auditor, a release-gate script) can fetch + re-verify
without re-running the chain.

## What it does

The runbook `scripts/testnet-live-publish.sh` is a pure-bash
driver that, when given a `receipts/testnet-live-proof-<UTC-ISO>/`
directory, runs the publish chain

```
trainer --verify-receipt <receipt-dir>   # 1. refuse to publish a red receipt
trainer --publish      <receipt-dir>    # 2. write the portable bundle
```

as a sequence of subprocesses, captures each step's stdout /
stderr into a sibling `.verify-receipt.*` / `.publish.*` log, and
emits a one-line `testnet live_proof publish complete: ...`
headline to `SUMMARY.txt`. The bash script also writes a
`publish/testnet-live-proof-<UTC-ISO>/` directory the operator
(or a CI worker) can `aws s3 cp` / `gsutil cp` into a dashboard
bucket in a single step.

The publish step is **read-only** with respect to the receipt.
`trainer --publish` copies the receipt into a fresh
`staging/` tempdir, walks the copy, and never opens the original
receipt for write. A partial-failure path leaves the receipt
untouched and the staging copy partially written.

## Bundle layout

After `bash scripts/testnet-live-publish.sh
receipts/testnet-live-proof-20260604T050000Z/` completes, the
runbook drops a directory tree:

```
publish/testnet-live-proof-20260604T050000Z/
  bundle.tar.gz                          # deterministic tar.gz of the receipt
  bundle.sha256                          # single-line `sha256  bundle.tar.gz` (sha256sum -c format)
  manifest.json                          # machine-readable per-file digests + metadata
  SUMMARY.txt                            # the one-line publish headline
```

The `bundle.tar.gz` is built with `tar --sort=name --mtime=@0
--owner=0 --group=0` and `flate2::Compression::none()` (the
publisher is `crates/autotrain/src/publish.rs::write_tar_gz`) so
a byte-identical receipt produces a byte-identical tarball. The
`manifest.json` is a JSON object with the following shape
(mirrors the `crate::publish::PublishedBundle` struct
one-for-one):

```json
{
  "bundle_filename": "bundle.tar.gz",
  "bundle_sha256": "<64-hex sha256 of the tarball>",
  "total_bytes": 12345,
  "files": [
    { "path": "SUMMARY.txt", "sha256": "<...>", "bytes": 256 },
    { "path": "recipe.json", "sha256": "<...>", "bytes": 512 },
    { "path": "cluster/stdout.txt", "sha256": "<...>", "bytes": 32 },
    ...
  ],
  "receipt_dir": "<absolute path to the source receipt>",
  "runbook_version": "STW-032 v1",
  "trainer_git_sha": "<git rev-parse HEAD of the workspace>"
}
```

The `files[]` array is sorted by `path` for determinism, and the
`bytes` / `sha256` fields match the on-disk `sha256sum -c` of
the extracted tree (a `tar -xzf bundle.tar.gz && sha256sum -c
bundle.sha256` round-trips identically across machines).

## Verifying the bundle

A downstream auditor can re-verify the bundle with a single
static `trainer` binary (the `trainer --verify-bundle` arm
STW-032 also ships):

```bash
$ trainer --verify-bundle publish/testnet-live-proof-20260604T050000Z/
live_proof bundle verification passed: bundle=bundle.tar.gz files=23 bytes=12345 sha256=<...>
$ echo $?
0
```

A green exit 0 + a `live_proof bundle verification passed: ...`
line means the bundle is verifier-compatible. A non-zero exit +
a `live_proof bundle verification failed: <kind>: ...` line
names the failure mode (`manifest_shape` /
`bundle_hash_mismatch` / `missing_file` / `file_unreadable`) and
the precise detail (the missing path, the file that mismatched,
the unreadable file).

The same CLI also accepts the **committed no-DB fixture** the
repo ships at
`crates/autotrain/tests/fixtures/publish-fixture/` so a
downstream auditor can re-verify the canonical green-bundle
shape on any machine without a Postgres. The fixture is the
portable reference a `cargo test --workspace` invocation
re-verifies on every commit; a drift in either the fixture or
the verifier fails the lib test
`verify_bundle::tests::run_verifies_committed_publish_fixture`
and the integration test
`crates/autotrain/tests/publish.rs::verify_bundle_round_trips_through_real_trainer_binary`
simultaneously.

## Pushing to a remote bucket (STW-033)

The `testnet-live-publish.sh` runbook writes the bundle into a
local `publish/` directory. A CI worker can then `aws s3 cp`
the three files into a dashboard bucket:

```bash
$ aws s3 cp publish/testnet-live-proof-20260604T050000Z/ \
    s3://robopoker-testnet-dashboard/testnet-live-proof-20260604T050000Z/ \
    --recursive
```

STW-033 lands the deterministic, plan-first `trainer
--publish-remote <receipt-dir> --bucket <s3://...>` half of
that push: a CI worker invokes

```bash
$ bash scripts/testnet-live-publish-s3.sh \
    receipts/testnet-live-proof-20260604T050000Z/ \
    s3://robopoker-testnet-dashboard
```

and the chain (a) re-verifies the receipt with `trainer
--verify-receipt`, (b) re-verifies the STW-032 publish bundle
with `trainer --verify-bundle`, (c) shells out to `trainer
--publish-remote` (which writes a deterministic
`remote_plan.json` + `remote_receipt.json` to
`<publish>/<basename>/remote/`), and (d) re-verifies the
post-upload receipt with `trainer --verify-remote`. The
`PUBLISH_DRY_RUN=0` knob flips the
`trainer --publish-remote` arm into live mode (which
shells out to `aws s3 cp` per file). The dry-run default
keeps `cargo test --workspace` no-network.

## What the runbook does NOT do

- It does **not** change the trainer's `--smoke` / `--bench` /
  `--compare` / `--compare3` / `--replay` / `--verify-receipt`
  behaviour. Those are already shipped and pinned by their own
  integration tests
  (`crates/autotrain/tests/{smoke,bench,compare,live_proof}.rs`).
  STW-032 is the *publish* step, not new trainer functionality
  beyond the two new `--publish` / `--verify-bundle` arms.
- It does **not** introduce a Python or `jq` dependency. The
  runbook is pure bash + `tar` + `sha256sum` so a Docker image
  that ships only the `trainer` binary + bash can run the
  publish.
- It does **not** require Docker. A worker that already has
  `cargo` + `bash` + a `trainer` binary can run the publish
  as-is.
- It does **not** push to a remote registry. The publish step
  writes the bundle to a local `publish/` directory. The S3 /
  GCS / git-tag push is a *consumer* of the bundle, not part
  of the publish step itself — see STW-033 + the
  `scripts/testnet-live-publish-s3.sh` companion runbook for
  the deterministic upload-plan / `remote_receipt.json`
  surface.
- It does **not** change the STW-019
  `scripts/testnet-live-proof.sh` runbook. The publish step
  consumes the receipt the runbook produced — a follow-on
  *consumer* of it, not a refactor of it.
- It does **not** change the STW-023
  `LiveProofReceipt::read_and_verify` / `LiveProofRecipe`
  JSON shape. The publish reads + re-verifies the receipt,
  then writes its own manifest — a `recipe.json` drift fails
  the publish step's pre-tar `trainer --verify-receipt` call.

## Pinning the runbook's shape

The shell-shape integration test
`crates/autotrain/tests/script_shape.rs` runs without a database
and asserts:

1. `scripts/testnet-live-publish.sh` exists and is executable.
2. `bash -n scripts/testnet-live-publish.sh` parses (catches a
   syntax regression at CI time).
3. The runbook doc lists every chain step the publish surface
   honors (`--verify-receipt`, `--publish`).
4. The runbook doc references the
   `trainer --verify-bundle <path>` CLI subcommand.

This means a future refactor that, say, removes the
`--verify-receipt` pre-publish gate or renames the bundle
filename fails the shell-shape test even before it reaches a
live Postgres.

## See also

- `crates/autotrain/src/publish.rs` — the Rust publisher
  (`publish_receipt` + `PublishedBundle::verify` + the typed
  `PublishError` enum).
- `crates/autotrain/src/verify_bundle.rs` — the Rust
  `trainer --verify-bundle` CLI handler.
- `crates/autotrain/tests/publish.rs` — the no-DB integration
  test that drives `trainer --publish` + `trainer --verify-bundle`
  end-to-end through a subprocess.
- `crates/autotrain/tests/script_shape.rs` — the shell-shape
  pinner (no DB required; runs in `cargo test --workspace`).
- `scripts/testnet-live-proof.sh` — the upstream runbook the
  publish step consumes receipts from.
- `genesis/plans/000-ceo-testnet-roadmap.md` — the CEO-signed
  testnet north star ("A public, reproducible NLHE benchmark
  where a trained robopoker blueprint bot beats a named baseline
  head-to-head, with every match downloadable as a replayable,
  signed transcript") that the publish step operationally
  extends (a dashboard can now `aws s3 cp` the bundle).
