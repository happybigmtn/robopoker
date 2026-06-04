# `publish-fixture/` — STW-032 committed no-DB portable reference bundle

A byte-stable green publish bundle the repo ships as a portable
reference for downstream auditors (a testnet dashboard's "verify"
button, a CI auditor, a release-gate script).

## Contents

- `bundle.tar.gz` — deterministic `tar.gz` of the receipt the
  publish step tarred. Built with `tar --sort=name --mtime=@0
  --owner=0 --group=0` and `flate2::Compression::none()` so a
  byte-identical receipt produces a byte-identical tarball.
- `bundle.sha256` — single-line `sha256  bundle.tar.gz` (the
  `sha256sum -c` format).
- `manifest.json` — machine-readable per-file digests + metadata
  the `trainer --verify-bundle <path>` CLI re-verifies against.
- `README.md` — this file.

## Re-verify

A downstream auditor can re-verify the bundle on any machine
that has a `trainer` binary (no Postgres required):

```bash
$ trainer --verify-bundle crates/autotrain/tests/fixtures/publish-fixture/
live_proof bundle verification passed: bundle=bundle.tar.gz files=25 bytes=20503 sha256=cff28a13f2471bd15324b69f65e6ffa869a4ecd84748dc0e78719a7ffef11313
$ echo $?
0
```

A green exit 0 + a `live_proof bundle verification passed: ...`
line means the bundle is verifier-compatible.

## How the fixture is generated

The fixture is generated from a synthetic receipt the
`LiveProofReceipt::write_to` lib helper writes (a minimal
green-receipt shape: `SUMMARY.txt` + `recipe.json` + the seven
step directories with `exit=0` / `stdout` / `stderr`). The
publish step (`trainer --publish`) turns the receipt into the
tarball + sha256 + manifest you see here.

A future regression that breaks the publisher (a `tar --sort`
option drift, a gzip-compression drift) or the verifier
(an `extract_tar_gz` regression, a `PublishedBundle::from_bundle_path`
regression) fails the lib test
`verify_bundle::tests::run_verifies_committed_publish_fixture` +
the integration test
`crates/autotrain/tests/publish.rs::verify_bundle_round_trips_through_real_trainer_binary`
simultaneously.

## What it is NOT

- It is **not** a real testnet launch proof. The receipt the
  fixture was generated from is a synthetic green-receipt the
  lib helper writes (no Postgres, no `--cluster` / `--smoke` /
  `--bench` chain ran). The fixture's role is the
  *bundle* contract, not the *receipt* contract — the
  `testnet-live-proof-fixture/` sibling covers the receipt
  side.
- It is **not** regenerable from source on every `cargo test`
  run. The fixture is committed as a byte-stable artifact; a
  future maintainer who needs to regenerate it (after a
  `publish_receipt` change that intentionally drifts the
  bundle format) can re-run `trainer --publish` on a
  synthetic receipt and overwrite the committed bundle +
  sha256 + manifest.
