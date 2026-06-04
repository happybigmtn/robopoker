# Testnet live launch proof — committed portable reference

This directory is a **committed, no-DB, canonical fixture**
of the testnet live launch proof receipt bundle shape
that the `scripts/testnet-live-proof.sh` runbook writes
at runtime and the `crates/autotrain/src/receipt.rs`
`LiveProofReceipt::read_and_verify` verifier reads.

## Why a committed fixture

The `testnet-live-proof` HAZARDS hinge (the only
mainnet-block HAZARDS row not yet closed) needs an
operator-visible end-to-end receipt. The bash runbook
needs a live Postgres to drop a real receipt; a
dashboard / CI / release-gate script that wants to
*verify* a receipt the operator dropped should not need
to spin up Postgres. STW-028 ships:

- The committed fixture in this directory (a portable
  reference a downstream auditor can `cat` /
  `trainer --verify-receipt` against without any
  database).
- The `trainer --verify-receipt <path>` CLI subcommand
  (STW-028) that re-verifies any receipt bundle —
  committed, runbook-produced, or hand-rolled — and
  prints a one-line `live_proof receipt verification
  passed:` / `live_proof receipt verification failed:
  <kind>: <detail>` verdict.
- An integration test
  (`crates/autotrain/tests/verify_receipt.rs`) that
  pins the on-the-wire surface the new CLI mode
  exposes (exit 0 on a green receipt, exit 2 + a
  `recipe_shape:` / `step_failed:` / `headline:` /
  one-line-usage error line on the four error paths).

## What this fixture is NOT

This is **not** a real `bash scripts/testnet-live-proof.sh`
output. A real runbook output is gitignored under
`/receipts/` (the `STW-019` runtime output convention);
this directory is the no-DB portable reference an auditor
or a dashboard can re-verify on any machine without a
Postgres. The `trainer_bin` / `database_url` /
`stdout.txt` / `stderr.txt` fields are deliberately
empty (zero bytes) so the fixture is byte-stable across
machines and a `git diff` of this directory only changes
when the receipt shape itself changes.

## Re-verifying this fixture

```sh
cargo build --bin trainer
./target/debug/trainer --verify-receipt \
    crates/autotrain/tests/fixtures/testnet-live-proof-fixture
```

Expected output (exit 0):

```
live_proof receipt verification passed: testnet live_proof complete: smoke=12 status=12 bench=4 compare=4 replay=256
```

## Re-verifying a real runbook receipt

After a `DATABASE_URL=postgres://... bash scripts/testnet-live-proof.sh`
run, point the same CLI at the runbook's
`receipts/testnet-live-proof-<UTC-ISO>/` directory:

```sh
./target/debug/trainer --verify-receipt \
    receipts/testnet-live-proof-<UTC-ISO>/
```

A green exit 0 + a `live_proof receipt verification
passed: ...` line means the runbook dropped a
verifier-compatible receipt; a non-zero exit + a
`live_proof receipt verification failed: <kind>: ...`
line means the receipt shape has drifted from the
pinned `STW023_CHAIN_STEPS` / `STW023_HEADLINE_PREFIX`
contract — see the kind (`recipe_shape` / `step_failed`
/ `headline`) for the precise failure mode.
