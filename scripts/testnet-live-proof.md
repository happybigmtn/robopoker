# Testnet live launch proof runbook (STW-019)

The `testnet-live-proof` hinge in `steward/HINGES.md` is the
operator-visible counterpart to the `cargo test --workspace` receipts
`STW-009` (smoke), `STW-010` (bench), `STW-016` (replay), and
`STW-018` (compare) already pin individually. Component tests prove
each mode in isolation; this runbook drives the four modes back-to-back
against a single Postgres and writes a per-step receipt bundle a
testnet dashboard can scrape.

## What it does

The runbook `scripts/testnet-live-proof.sh` is a pure-bash driver
that, when given a `DATABASE_URL`, runs the chain

```
trainer --cluster    # 1. bootstrap pretraining + schema (idempotent)
trainer --reset      # 2. zero v1 + v2 blueprint + epoch tables
trainer --smoke      # 3. pretraining + 2-epoch train + sync
trainer --status     # 4. dashboard read: must show Epoch>0, Blueprint>0
trainer --bench      # 5. heads-up DatabasePlayer (v1) vs Fish
trainer --compare    # 6. heads-up v1 DatabasePlayer vs v2 DatabasePlayer2
trainer --replay <t> # 7. re-derive the (Position, Action) sequence
                     #    from the first transcript-*.json the bench
                     #    dropped into the receipt dir
```

as a sequence of subprocesses, captures each step's stdout + stderr +
exit code into a per-step sub-directory, and writes a one-line
`testnet live_proof complete: smoke=N status=N bench=N compare=N replay=BYTES`
headline to `SUMMARY.txt`. The headline line mirrors the
`crates/autotrain/tests/live_proof.rs` integration test's
`live_proof complete: ...` log line so a single dashboard scraper
can grep either the `cargo test` log or the `SUMMARY.txt` file
with the same regex.

## Receipt layout

After `bash scripts/testnet-live-proof.sh` completes against a live
Postgres, the runbook drops a directory tree under a `_v3`-suffixed
basename (STW-095) so a fresh green receipt never collides with the
archived failure-history set the same runbook moved into
`receipts/_archive/failed/`:

```
receipts/testnet-live-proof-20260604T050000Z_v3/
├── SUMMARY.txt                    # headline + per-step exit codes
├── ENV.txt                        # env the chain ran with (secrets redacted)
├── recipe.json                    # machine-readable manifest (STW-023)
├── cluster/{stdout,stderr,exit}.txt
├── reset/{stdout,stderr,exit}.txt
├── smoke/{stdout,stderr,exit}.txt
├── status/{stdout,stderr,exit}.txt
├── bench/
│   ├── {stdout,stderr,exit}.txt   # bench JSON report lives on stdout
│   └── transcripts/
│       └── transcript-<hand_id>.json   # what the --replay leg reads
├── compare/{stdout,stderr,exit}.txt
└── replay/{stdout,stderr,exit}.txt
```

Each `exit.txt` contains a single integer (the trainer's exit code
for that step). The dashboard can grep `SUMMARY.txt` for the
`testnet live_proof complete:` line and then read the matching
`*/stdout.txt` to parse the per-step artifact (e.g. the
`BenchReport` JSON for `--bench`, the rendered seat/action text for
`--replay`).

The `recipe.json` manifest is the single source of truth for the
chain step order + per-step exit codes. Its JSON shape mirrors
the `crates/autotrain::LiveProofRecipe` struct one-for-one:

```json
{
  "trainer_bin": "/srv/dev/repos/robopoker/target/debug/trainer",
  "database_url": "<redacted: 49 chars>",
  "steps": [
    { "name": "cluster", "exit": 0, "stdout_bytes": 123, "stderr_bytes": 0 },
    { "name": "reset",   "exit": 0, "stdout_bytes":  45, "stderr_bytes": 0 },
    { "name": "smoke",   "exit": 0, "stdout_bytes": 678, "stderr_bytes": 0 },
    { "name": "status",  "exit": 0, "stdout_bytes":  90, "stderr_bytes": 0 },
    { "name": "bench",   "exit": 0, "stdout_bytes": 234, "stderr_bytes": 0 },
    { "name": "compare", "exit": 0, "stdout_bytes": 210, "stderr_bytes": 0 },
    { "name": "replay",  "exit": 0, "stdout_bytes": 256, "stderr_bytes": 0 }
  ]
}
```

The seven `steps[i].name` strings are pinned in the
`crates/autotrain::STW023_CHAIN_STEPS` constant, in this order —
a future chain refactor that re-orders or drops a step must
update both the runbook's `write_recipe` heredoc and the
constant in the same change. The `database_url` field stores the
redacted `<redacted: N chars>` form (mirroring `ENV.txt`); a
`cat recipe.json` does not leak a secret into a CI artifact.

## How to run it

Prerequisites:
- A reachable Postgres (any version that supports `gen_random_uuid()`
  via `pgcrypto` and the `ON DELETE CASCADE` + `fillfactor` options
  the schema uses). The schema bootstrap is part of the chain.
- A built `trainer` binary. The runbook invokes
  `cargo build --bin trainer` automatically if
  `target/debug/trainer` is missing; point `TRAINER_BIN` at a
  `target/release/trainer` to skip the debug build.

```sh
# One-shot, debug build, fast mode (minutes):
DATABASE_URL=postgres://user:***@host:5432/dbname \
    RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh

# One-shot, debug build, full budget (hours):
DATABASE_URL=postgres://user:***@host:5432/dbname \
    bash scripts/testnet-live-proof.sh

# Release build, custom receipt location:
TRAINER_BIN=$PWD/target/release/trainer \
DATABASE_URL=postgres://user:***@host:5432/dbname \
    bash scripts/testnet-live-proof.sh
```

The script runs in **fast mode** when `RBP_TESTNET_FAST=1` is set,
using a small-budget chain (2 smoke epochs, 4 bench hands,
4 compare hands) so a complete run finishes in minutes rather than
hours. The `--cluster` step's per-street kmeans pass is also capped
in fast mode (STW-077: 1024-row input sample + 8 Lloyd iterations
per street, vs. the production 1.3M-row / 14M-row input + 20 / 24
Lloyd iterations) so a fresh-DB cluster step completes in under
5 minutes per street — under 30 minutes total for all 4 streets.
STW-091 adds a third cap on top of STW-077: the per-street
`Layer::lookup` construction (which runs immediately after kmeans)
is also prefix-capped in fast mode (1024 rows by default, mirroring
the kmeans cap; the production lookup iterates the full
`N = N_FLOP = 1_286_792` flop isomorphism space / `N = N_TURN =
13_960_050` for turn — a 2026-06-10 11:20 receipt captured a hang
at `calculating lookup flop` AFTER kmeans completed, because
STW-077's kmeans cap was the right *kind* of fix but the wrong
*layer*). With all three caps the cluster step completes in
under 5 minutes per street; the lookup is the no-longer-blocking
step in the chain.
Without the flag the trainer uses its own defaults (larger
budgets). Override individual env knobs to scale up or down; the
chain is structurally identical to the production launch path so a
large budget is just "more hands, more epochs".

## Environment knobs honoured

The runbook honours the same env discipline the four integration
tests use, so a `DATABASE_URL` set for the runbook is also valid
for `cargo test -p rbp-autotrain --features database --test live_proof`:

| env | default | purpose |
|---|---|---|
| `DATABASE_URL` | (required) | Postgres URL. Forwarded as `DB_URL` (the trainer's actual env name). |
| `DB_URL` | (inherits from `DATABASE_URL`) | Direct override. |
| `RBP_TESTNET_FAST` | (unset) | Set to `1` to auto-select minimal epochs/hands/batch for a fast validation run. |
| `RBP_FAST_EPOCHS` | 2 (when `RBP_TESTNET_FAST=1`) | smoke step epoch count |
| `RBP_FAST_BATCH` | 16 (when `RBP_TESTNET_FAST=1`) | smoke step batch size |
| `RBP_BENCH_HANDS` | 4 (when `RBP_TESTNET_FAST=1`) | bench step hand count |
| `RBP_BENCH_BLIND` | 2 (when `RBP_TESTNET_FAST=1`) | bench step blind size |
| `RBP_COMPARE_HANDS` | 4 (when `RBP_TESTNET_FAST=1`) | compare step hand count |
| `RBP_COMPARE_BLIND` | 2 (when `RBP_TESTNET_FAST=1`) | compare step blind size |
| `RBP_FAST_KMEANS_SAMPLE` | 1024 (when `RBP_TESTNET_FAST=1`) | STW-077: per-street kmeans input point cap (the production 1.3M-row flop / 14M-row turn pool is sub-sampled to this many rows; operator-overridable) |
| `RBP_FAST_KMEANS_ITERATIONS` | 8 (when `RBP_TESTNET_FAST=1`) | STW-077: per-street kmeans Lloyd-iteration cap (the production 20 flop / 24 turn iterations are replaced with this cap; operator-overridable) |
| `RBP_FAST_LOOKUP_SAMPLE` | 1024 (when `RBP_TESTNET_FAST=1`) | STW-091: per-street lookup input prefix cap (the production `Layer::lookup` iterates the full `N = N_FLOP = 1_286_792` flop isomorphism space / `N = N_TURN = 13_960_050` for turn; the fast-mode cap truncates the prefix to this many rows so the per-street lookup completes in < 1 s instead of hanging on the full N; operator-overridable) |
| `RBP_BENCH_TRANSCRIPT_DIR` | (set by runbook) | bench's transcript writer location |
| `TRAINER_BIN` | `<workspace>/target/debug/trainer` | trainer binary path |

## Exit codes

| code | meaning |
|---:|---|
| 0 | chain landed end-to-end |
| 3 | `DATABASE_URL` (or `DB_URL`) not set — refuse-to-run gate |
| 4 | trainer binary not found and `cargo build --bin trainer` failed |
| 5 | `trainer --cluster` exited non-zero |
| 6 | `trainer --reset` exited non-zero |
| 7 | `trainer --smoke` exited non-zero |
| 8 | `trainer --status` exited non-zero |
| 9 | `trainer --bench` exited non-zero |
| 10 | `trainer --compare` exited non-zero |
| 11 | `trainer --replay` exited non-zero (or no transcript was produced) |

## How the dashboard scrapes a receipt

```sh
# Get the headline line in one shot.
grep '^testnet live_proof complete:' \
    receipts/testnet-live-proof-*/SUMMARY.txt

# Parse the bench's JSON `BenchReport` (stdout.txt is single-line JSON).
python -c "import json,sys; print(json.load(open(sys.argv[1])))" \
    receipts/testnet-live-proof-*/bench/stdout.txt

# Render the bench's first transcript (the public reproducible surface).
cat receipts/testnet-live-proof-*/replay/stdout.txt

# Re-verify a receipt bundle with the shared Rust verifier (STW-023).
# The verifier asserts every step exited 0, the `recipe.json` manifest
# is shape-valid, and the SUMMARY.txt headline matches the pinned
# `testnet live_proof complete: smoke=...` format.
cargo test -p rbp-autotrain --test live_proof_receipt
```

A third-party auditor that has the `recipe.json` file (and not
the `SUMMARY.txt` headline) can re-verify the chain with the
shared `LiveProofReceipt::read_and_verify` API directly:

```rust
use rbp_autotrain::LiveProofReceipt;
LiveProofReceipt::read_and_verify(receipt_root)
    .expect("operator receipt is green");
```

## Receipts archive (`receipts/_archive/failed/`) — STW-095

The `receipts/_archive/failed/` directory is the committed audit
trail of pre-`STW-095` failed/partial runbook runs. After STW-095
landed, the runbook moved 33 failed/partial
`receipts/testnet-live-proof-2026060{4..10}T*` directories into
`receipts/_archive/failed/` (the move preserves the receipts
on-disk; only the git-tracked surface changes — see
`.gitignore`'s `!receipts/_archive/` exception). A planner
scanning the `receipts/` dir sees only one fresh
`testnet-live-proof-<UTC>_v3/` green receipt + the
`receipts/_archive/failed/` summary (with the
`INDEX.md` per-receipt failure-mode audit trail), not 33
confusing stale dirs + 0 green ones.

The `INDEX.md` is the failure-history a future planner reads
to understand *why* the prior 33 runs failed without re-running
them: one bullet per archived receipt + the failure mode + the
slice that fixed it. The summary entries are organized
chronologically (oldest first) and tagged with the
`STW-NNN` slice that closed each failure family (e.g.
`STW-075` deterministic `Check::clustered` + `STW-077`
fast-mode kmeans cap + `STW-086` kmeans++ empty-histogram
pre-filter + `STW-087` empty-cluster guard + `STW-091`/`STW-094`
lookup fast-mode cap).

The STW-095 receipt chain is *evidence only*: the dependency
chain is `STW-075` + `STW-077` + `STW-078` + `STW-086` + `STW-087` +
`STW-094` (the v1→v10 testnet chain is structurally complete
*and* kmeans-panic-blocked *and* lookup-hang-blocked — the
green receipt is the operator-visible proof all six layers
hold in a real Postgres run). If a fresh runbook run fails
after STW-095, the worker reports the failure as a new
`[P0]`-ranked next slice (or blocks for human input) rather
than silently fixing the runbook in the STW-095 commit.

## Re-verifying a receipt with the trainer binary (STW-028)

A downstream tool (a testnet dashboard's "verify" button, a
CI check, a release-gate script) that already has the static
`trainer` binary can re-verify a receipt bundle the runbook
dropped without re-running `cargo test` or installing the
workspace's library crates. The new no-DB mode is:

```sh
cargo build --bin trainer
./target/debug/trainer --verify-receipt \
    receipts/testnet-live-proof-20260604T050000Z/
```

A green exit 0 + a `live_proof receipt verification passed: ...`
line means the bundle is verifier-compatible. A non-zero
exit + a `live_proof receipt verification failed: <kind>: ...`
line names the failure mode (`recipe_shape` / `step_failed` /
`headline`) and the precise detail (the missing file, the
failing step + exit code, the bad headline prefix).

The same CLI also accepts the **committed no-DB fixture**
the repo ships at
`crates/autotrain/tests/fixtures/testnet-live-proof-fixture/`
so a downstream auditor can re-verify the canonical green-receipt
shape on any machine without a Postgres. The fixture is the
portable reference a `cargo test --workspace` invocation
re-verifies on every commit; a drift in either the fixture
or the verifier fails the lib test
`verify_receipt::tests::run_verifies_committed_testnet_live_proof_fixture`
and the integration test
`crates/autotrain/tests/verify_receipt.rs` simultaneously.

## What the runbook does NOT do

- It does **not** change the trainer's `--smoke` / `--bench` /
  `--compare` / `--replay` behaviour. Those are already shipped and
  pinned by their own integration tests
  (`crates/autotrain/tests/{smoke,bench,compare,live_proof}.rs`).
  STW-019 is the *runbook*, not new trainer functionality.
- It does **not** introduce a Python or `jq` dependency. The
  runbook is pure bash so a Docker image that ships only the
  `trainer` binary + bash can run the proof.
- It does **not** require Docker. A worker that already has
  `cargo` + `bash` + a `DATABASE_URL` can run the proof as-is.
- It does **not** push to a remote registry. The receipt directory
  is local. The v7 follow-on (`testnet-live-publish`) — **shipped
  as STW-032** — turns the receipt into a deterministic,
  content-addressed portable bundle
  (`publish/testnet-live-proof-<UTC-ISO>/{bundle.tar.gz, bundle.sha256, manifest.json, SUMMARY.txt}`)
  a CI worker can `aws s3 cp` / `gsutil cp` into a testnet
  dashboard bucket. See
  [`scripts/testnet-live-publish.md`](testnet-live-publish.md)
  for the publish runbook and
  [`scripts/testnet-live-publish.sh`](testnet-live-publish.sh)
  for the bash driver. STW-033 lands the
  *plan-first* `trainer --publish-remote <receipt-dir>
  --bucket <s3://...>` half of that push (the deterministic
  upload plan + `remote_receipt.json` manifest a CI worker
  can re-verify without re-running the chain); see
  [`scripts/testnet-live-publish-s3.sh`](testnet-live-publish-s3.sh)
  for the companion bash driver.

## Pinning the runbook's shape

The shell-shape integration test
`crates/autotrain/tests/script_shape.rs` runs without a database and
asserts:

1. `scripts/testnet-live-proof.sh` exists and is executable.
2. `bash -n scripts/testnet-live-proof.sh` parses (catches
   a syntax regression at CI time).
3. The runbook doc lists every env knob the script honours
   (catches a doc drift where the script gains a knob but the
   doc forgets to mention it).
4. The runbook doc references every chain step the live proof
   integration test covers (`--cluster`, `--reset`, `--smoke`,
   `--status`, `--bench`, `--compare`, `--replay`).
5. The runbook script sources a `recipe.json` manifest block
   (a `cat > "$RECEIPT_DIR/recipe.json" <<JSON ... JSON`
   heredoc anchored to the `LiveProofRecipe` JSON shape)
   and the doc documents the manifest file in the
   receipt-layout section above. The heredoc terminator
   is unquoted (`<<JSON`, not `<<'JSON'`) so the
   `TRAINER_BIN` / `db_redacted` shell variables expand
   into the manifest body before write.

This means a future refactor that, say, removes the `--status` leg
or renames an env knob fails the shell-shape test even before it
reaches a live Postgres.

## See also

- `crates/autotrain/tests/live_proof.rs` — the cargo-test counterpart
  (asserts the chain lands inside a single `cargo test` run; pins
  the per-step log-line contracts).
- `crates/autotrain/tests/script_shape.rs` — the shell-shape
  pinner (no DB required; runs in `cargo test --workspace`).
- `steward/HINGES.md` — the source of the `testnet-live-proof`
  hinge and its ranking above the `workspace-parallel` /
  `STW-001` / `STW-007` / `STW-011` / `STW-015` decisions.
- `genesis/plans/000-ceo-testnet-roadmap.md` — the CEO-signed
  testnet north star ("A public, reproducible NLHE benchmark
  where a trained robopoker blueprint bot beats a named baseline
  head-to-head, with every match downloadable as a replayable,
  signed transcript") that this runbook operationally closes.
