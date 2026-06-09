# Testnet Postgres env provisioning runbook (STW-078)

The `scripts/testnet-live-proof.sh` runbook assumes a reachable
`DATABASE_URL` (or `DB_URL`) the rest of the `trainer` toolchain
can talk to, and the receipt history shows that assumption is
*not* trivial to satisfy from a clean shell:

- `receipts/testnet-live-proof-20260609T060233Z/` (the most
  recent runbook invocation as of 2026-06-09) shows the
  `doctor` step failed with `"db_reachable":false,"detail":"SELECT
  1 failed: psql: error: connection to server at \"127.0.0.1\",
  port 5433 failed: FATAL:  password authentication failed for
  user \"rbp_live\""`. The `rbp_live` user's password is not
  reproducible across reboots / Postgres restarts.
- Every other failed/partial `receipts/testnet-live-proof-*/`
  directory in the archive tells the same story: the
  *environment*, not the trainer chain, is the block.

`scripts/setup-testnet-postgres.sh` (this slice, STW-078) is
the missing piece — a pure-bash, idempotent, no-`docker` script
that brings up a local Postgres on a non-default port
(`127.0.0.1:5433`) with a known `rbp_live` user + a known
`rbp_live` password + a known `rbp_live` database, and writes
a `.auto/testnet-postgres.env` file the operator (or a CI
worker) can `source` to set `DATABASE_URL` + `DB_URL` for the
runbook chain. After this script exits 0, a worker can:

```sh
source .auto/testnet-postgres.env
RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh
```

and the runbook's `doctor` step will see a healthy Postgres
(post-STW-078 shape) instead of a `db_reachable: false` auth
failure (pre-STW-078 shape).

## What it does

The script `scripts/setup-testnet-postgres.sh` is a pure-bash
driver that, when given nothing, performs the following chain:

1. **Required-binaries gate** — refuses to run if
   `initdb` / `pg_ctl` / `postgres` / `psql` / `createuser` /
   `createdb` are missing on `$PATH` (exit 2 with a one-line
   `testnet-postgres: required binary missing: ...` message
   plus a per-distro install hint).
2. **Idempotent re-run probe** — if the data dir is already
   initialised AND a local `psql SELECT 1` against the data
   dir's Unix socket succeeds, the script exits 0 with the
   pinned `testnet-postgres: already provisioned (port=... user=...
   database=... data_dir=...)` headline. Re-running on a
   healthy env is a no-op (no data loss).
3. **Port-already-bound gate** — refuses to start a second
   Postgres on the configured port (exit 3 with a one-line
   `testnet-postgres: port 5433 already in use` message + a
   hint to set `RBP_TESTNET_PG_PORT` to a free port).
4. **Env file emission** — writes a parseable
   `.auto/testnet-postgres.env` file the operator can
   `source`. The file's `DATABASE_URL` / `DB_URL` use the
   known `rbp_live` / `rbp_live` / `rbp_live` / `5433` /
   `rbp_live` defaults the runbook expects. The file is
   `chmod 0600` so a multi-user host does not leak the test
   credential off the operator's disk.
5. **initdb** — initialises a fresh Postgres cluster under
   `.auto/testnet-postgres/data/` with `--auth=trust` (the
   documented escape valve for local test environments) and
   `--username=rbp_live --pwfile=<(printf rbp_live)` so the
   superuser matches the role the runbook authenticates as.
6. **pg_ctl start** — starts the server on
   `127.0.0.1:5433` (localhost-only by design) with the data
   dir as the Unix-socket directory. Records the actual port
   the cluster was started with in a small sentinel file
   under the data dir so a re-run with a changed
   `RBP_TESTNET_PG_PORT` still finds the running instance.
7. **createuser + createdb** — creates the `rbp_live` role
   (idempotent) and the `rbp_live` database (idempotent,
   `rbp_live`-owned).
8. **ALTER USER ... PASSWORD** — pins `rbp_live`'s password
   to the value the runbook's `DATABASE_URL` carries. Defensive
   against a Postgres restart that reverted `pg_authid` (e.g.
   a `pg_ctl reload` after a `--pwfile`-less `initdb`).
9. **Final smoke test** — invokes `trainer --doctor` against
   the resulting `DATABASE_URL` and asserts (a) the doctor
   exits 0 and (b) the doctor's JSON line includes
   `"db_reachable":true`. A failure here is exit 7 with a
   one-line `testnet-postgres: trainer --doctor failed` (or
   `trainer --doctor reported db_reachable: false`) message +
   a per-log-file pointer the operator inspects.

The script is **idempotent** — a re-run on a healthy env
exits 0 with the `already provisioned` headline, no data
loss. The `trainer --doctor` smoke step is skipped (and the
`complete:` headline still emitted) when the trainer binary
is missing or `RBP_TESTNET_PG_SKIP_DOCTOR=1` is set.

## How to run it

Prerequisites:

- `initdb` / `pg_ctl` / `postgres` / `psql` / `createuser` /
  `createdb` on `$PATH` (the standard `postgresql` apt / yum /
  brew package). The script refuses to run without them
  (exit 2 + per-distro install hint).
- A `trainer` binary at `<workspace>/target/debug/trainer`
  *or* a `RBP_TESTNET_PG_SKIP_DOCTOR=1` set (the smoke test
  is the only step that needs the trainer). The script
  invokes the trainer as the final smoke; when the trainer
  is missing the script exits 0 with the `complete:` headline
  plus a one-line `(smoke test skipped: trainer binary not
  found at ...)` note.

```sh
# One-shot, debug build, smoke test enabled:
bash scripts/setup-testnet-postgres.sh

# One-shot, smoke test skipped (no trainer binary):
RBP_TESTNET_PG_SKIP_DOCTOR=1 bash scripts/setup-testnet-postgres.sh

# Custom port (e.g. when :5433 is already bound):
RBP_TESTNET_PG_PORT=5544 bash scripts/setup-testnet-postgres.sh

# Custom env file location (e.g. a CI worker wants the env
# file at a known scratch path, not under `.auto/`):
RBP_TESTNET_PG_ENV_FILE=/tmp/my-runbook.env \
    bash scripts/setup-testnet-postgres.sh
```

After the script exits 0:

```sh
# Pick up the env the script just wrote.
source .auto/testnet-postgres.env

# Run the testnet live proof against the freshly-provisioned
# Postgres. The runbook's `doctor` step will now see
# `db_reachable: true` (post-STW-078 shape) instead of the
# pre-STW-078 `db_reachable: false` + password-auth error.
RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh
```

The script runs in **seconds** on a clean shell (`initdb` on
an empty data dir is a few hundred ms, `pg_ctl start` is
sub-second on a warm host, the role + db creation is a single
round-trip each). The Postgres process stays running until
the operator explicitly stops it (`pg_ctl --pgdata
.auto/testnet-postgres/data stop`) or reboots the host.

## Environment knobs honoured

| env | default | purpose |
|---|---|---|
| `RBP_TESTNET_PG_PORT` | `5433` | TCP port the local Postgres binds to. The non-default value avoids a collision with a system Postgres on `:5432`. |
| `RBP_TESTNET_PG_USER` | `rbp_live` | Postgres role the runbook authenticates as. |
| `RBP_TESTNET_PG_PASSWORD` | `rbp_live` | Postgres password the runbook sends. The default is a known local-only test credential; do **NOT** reuse this value in production. |
| `RBP_TESTNET_PG_DATABASE` | `rbp_live` | Postgres database the runbook reads. |
| `RBP_TESTNET_PG_DATA_DIR` | `<workspace>/.auto/testnet-postgres/data` | Postgres data directory. Lives under `.auto/` so a future `auto steward` run treats it as run-evidence (not a product commit). |
| `RBP_TESTNET_PG_LOG_DIR` | `<workspace>/.auto/testnet-postgres/log` | Where the script writes the `postgres.log` / `initdb.log` / `psql_alter.log` / `pg_ctl.log` / `doctor.stdout` / `doctor.stderr` files the operator inspects on smoke-test failure. |
| `RBP_TESTNET_PG_ENV_FILE` | `<workspace>/.auto/testnet-postgres.env` | The env file the script writes. `source` it to set `DATABASE_URL` + `DB_URL` for the runbook. |
| `RBP_TESTNET_PG_SKIP_DOCTOR` | (unset) | Set to `1` to skip the final `trainer --doctor` smoke test (useful when `trainer --doctor` is not available because the trainer binary has not been built). |
| `TRAINER_BIN` | `<workspace>/target/debug/trainer` | Path to the trainer binary the smoke test invokes. |

## Exit codes

| code | meaning |
|---:|---|
| 0 | provisioning complete + `trainer --doctor` smoke passed (or the env was already provisioned, idempotent re-run) |
| 1 | generic error (see stderr for detail) |
| 2 | required binary missing (`initdb` / `pg_ctl` / `postgres` / `psql` / `createuser` / `createdb` not on `$PATH`) |
| 3 | the configured port is already bound (another Postgres is using it; refuse to start a second instance) |
| 4 | `initdb` failed (see `.auto/testnet-postgres/log/initdb.log`) |
| 5 | `pg_ctl start` failed (see `.auto/testnet-postgres/log/pg_ctl.log` + `postgres.log`) |
| 6 | user/database creation or `ALTER USER ... PASSWORD` failed (see `.auto/testnet-postgres/log/psql_alter.log`) |
| 7 | `trainer --doctor` smoke test failed (the env is up but the trainer does not see it; the operator inspects `.auto/testnet-postgres/log/postgres.log` + `doctor.stdout` + `doctor.stderr`) |

## How the dashboard scrapes a provision event

```sh
# Get the headline line in one shot (the same regex the
# `testnet live_proof complete: ...` scraper uses; the
# `testnet-postgres: complete: ...` prefix is a sibling
# line a dashboard scraper can grep the same way).
grep '^testnet-postgres: complete:' \
    <workspace>/.auto/testnet-postgres.log 2>/dev/null \
    || bash scripts/setup-testnet-postgres.sh \
        | grep '^testnet-postgres: complete:'

# Read the env the script just wrote (the source of truth
# for `DATABASE_URL` / `DB_URL` / the role / the port).
cat .auto/testnet-postgres.env
```

The env file the script writes is the single source of truth
for the `DATABASE_URL` the runbook reads. The headline line
is the operator-visible proof the env is healthy. A dashboard
that scrapes both has a closed loop: a worker runs the
provisioner, the dashboard reads the headline + the env, and
the runbook (run with `source .auto/testnet-postgres.env`)
inherits the same contract.

## What the runbook does NOT do

- It does **not** change `trainer --doctor`. The doctor is
  the runbook's pre-flight gate; if it is green, the rest
  of the chain (`--cluster` → `--reset` → `--smoke` → ...)
  will see the same healthy Postgres. STW-078 ships the
  provisioning script that *produces* a reproducible env;
  the doctor itself is unchanged.
- It does **not** change the `testnet-live-proof.sh` runbook.
  The runbook reads `DATABASE_URL` / `DB_URL` from the env
  exactly as it does today; the new script's only job is to
  *produce* a reproducible env.
- It does **not** require `sudo` or `docker`. The script
  runs as the unprivileged user + uses a tmpdir data dir +
  a non-default port (`5433`) + the local `initdb` /
  `pg_ctl` / `postgres` binaries the OS already ships.
- It does **not** push the env anywhere remote. The data
  dir + the env file are local. A CI worker that needs a
  remote Postgres can `ssh` into a box and run the script
  there, or a future follow-on slice can wrap the
  `RBP_TESTNET_PG_*` knobs in a `testnet-live-postgres`
  runbook that runs the script on a remote host.
- It does **not** delete the data dir on a failure. A
  partial-failure leaves the data dir + log files behind so
  the operator can `cat .auto/testnet-postgres/log/initdb.log`
  to see what went wrong. The next run is idempotent: if
  the data dir is partially initialised but the cluster is
  not running, the script re-runs `initdb` (which refuses
  to clobber an existing `PG_VERSION`) or, if the data
  dir is fully initialised, the script's idempotent probe
  short-circuits with `already provisioned`.

## Pinning the script's shape

The shell-shape integration test
`crates/autotrain/tests/script_shape.rs` runs without a
database and asserts the script's static contract:

1. `setup_testnet_postgres_script_exists_and_parses`
   (STW-078) — `scripts/setup-testnet-postgres.sh` is on
   disk, has its owner-executable bit set, and parses
   with `bash -n`.
2. `setup_testnet_postgres_script_writes_env_file` (STW-078)
   — the script sources a `cat > "$PG_ENV_FILE" <<ENV ... ENV`
   heredoc whose body, after bash interpolation tokens are
   substituted, parses as the expected
   `DATABASE_URL=postgres://user:***@host:port/dbname` +
   `DB_URL=...` + `RBP_TESTNET_PG_*` env-file shape. The
   integration test
   `crates/autotrain/tests/setup_testnet_postgres.rs`
   additionally drives the script end-to-end against fake
   `initdb` / `pg_ctl` / `postgres` / `psql` /
   `createuser` / `createdb` shims in a clean tmpdir
   (with `RBP_TESTNET_PG_SKIP_DOCTOR=1` so the test does
   not need a built `trainer` binary) and asserts the
   env file lands at the configured path with the
   expected shape.

This means a future refactor that, say, drops the `ENV`
heredoc terminator or renames a `RBP_TESTNET_PG_*` knob
fails the shell-shape test even before it reaches a live
Postgres.

## See also

- `scripts/testnet-live-proof.sh` — the receipt runbook
  the `DATABASE_URL` this script provisions feeds.
- `crates/autotrain/tests/setup_testnet_postgres.rs` —
  the no-DB integration test that drives the script
  end-to-end against fake Postgres binaries in a clean
  tmpdir.
- `crates/autotrain/tests/script_shape.rs` — the
  shell-shape pinner (no DB required; runs in
  `cargo test --workspace`).
- `IMPLEMENTATION_PLAN.md` — STW-078 row, the RE-PLAN-004
  framing that ships this slice.
- `steward/HINGES.md` — the `testnet-live-proof` hinge
  STW-070 (evidence only) + STW-078 (env provisioning)
  jointly close.
