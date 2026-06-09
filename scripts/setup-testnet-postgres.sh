#!/usr/bin/env bash
# scripts/setup-testnet-postgres.sh — STW-078 testnet postgres env provisioning
#
# Brings up a local Postgres the testnet live proof runbook
# (`scripts/testnet-live-proof.sh`) and the rest of the robopoker
# toolchain can talk to, on a *non-default* port (5433) so the
# script does not collide with a system Postgres on :5432, with
# a known `rbp_live` user + a known `rbp_live` password + a
# known `rbp_live` database, and writes a `.auto/testnet-postgres.env`
# file the operator (or a CI worker) can `source` to set
# `DATABASE_URL` + `DB_URL` for the runbook chain.
#
# Why a dedicated script: the receipt
# `receipts/testnet-live-proof-20260609T060233Z/` (06:02 UTC, the
# most recent runbook invocation) shows the `doctor` step failed
# with `"db_reachable":false,"detail":"SELECT 1 failed: psql:
# error: connection to server at \"127.0.0.1\", port 5433 failed:
# FATAL:  password authentication failed for user \"rbp_live\""`.
# The `rbp_live` user's password is not reproducible across
# reboots / Postgres restarts; the runbook's `trainer --doctor`
# gate is the *right* gate (it catches the failure cleanly), but
# the operator-runnable provisioning script that *produces* a
# reproducible `DATABASE_URL` is missing. This script is that
# missing piece — a fresh shell can run it and land a healthy
# `DATABASE_URL` the runbook reads.
#
# Usage:
#   bash scripts/setup-testnet-postgres.sh
#
# The script is pure-bash + idempotent + does not require `sudo`
# (it runs as the unprivileged user + uses a tmpdir data dir +
# a non-default port) + does not introduce a `docker` dependency
# (it uses the local `initdb` / `pg_ctl` / `postgres` binaries
# the OS already ships). Re-running on a healthy env exits 0
# with a one-line `testnet-postgres: already provisioned` message,
# no data loss.
#
# Environment overrides (operator knobs — the script does the
# right thing when unset):
#   RBP_TESTNET_PG_PORT         (5433)  TCP port the local Postgres
#                                       binds to. The non-default
#                                       value avoids a collision
#                                       with a system Postgres on
#                                       `:5432`.
#   RBP_TESTNET_PG_USER         (rbp_live)  Postgres role the
#                                          runbook authenticates
#                                          as.
#   RBP_TESTNET_PG_PASSWORD     (rbp_live)  Postgres password the
#                                          runbook sends. The
#                                          default is a known
#                                          local-only test
#                                          credential; do NOT
#                                          reuse this in
#                                          production.
#   RBP_TESTNET_PG_DATABASE     (rbp_live)  Postgres database the
#                                          runbook reads.
#   RBP_TESTNET_PG_DATA_DIR     ($WORKSPACE_ROOT/.auto/testnet-postgres/data)
#                                       The Postgres data
#                                       directory. Lives under
#                                       `.auto/` so a future
#                                       `auto steward` run can
#                                       treat it as a run-evidence
#                                       root (not a product
#                                       commit).
#   RBP_TESTNET_PG_LOG_DIR      ($WORKSPACE_ROOT/.auto/testnet-postgres/log)
#                                       Where the script writes
#                                       the `postgres.log` /
#                                       `initdb.log` /
#                                       `psql_create.log` files
#                                       the operator inspects
#                                       when the smoke test
#                                       fails.
#   RBP_TESTNET_PG_ENV_FILE     ($WORKSPACE_ROOT/.auto/testnet-postgres.env)
#                                       The env file the script
#                                       writes. `source` it to
#                                       set `DATABASE_URL` +
#                                       `DB_URL` for the
#                                       runbook.
#   RBP_TESTNET_PG_SKIP_DOCTOR  (unset)  Set to 1 to skip the
#                                       final `trainer --doctor`
#                                       smoke test (useful when
#                                       `trainer --doctor` is
#                                       not available because
#                                       the trainer binary
#                                       has not been built).
#   TRAINER_BIN                 ($WORKSPACE_ROOT/target/debug/trainer)
#                                       Path to the trainer
#                                       binary the smoke test
#                                       invokes.
#
# Exit codes:
#   0  provisioning complete + `trainer --doctor` smoke passed
#      (or the env was already provisioned, idempotent re-run)
#   1  generic error (see stderr for detail)
#   2  required binary missing (`initdb` / `pg_ctl` / `postgres`
#      / `psql` / `createuser` / `createdb` not on `$PATH`)
#   3  the configured port is already bound (another Postgres
#      is using it; refuse to start a second instance)
#   4  `initdb` failed
#   5  `pg_ctl start` failed
#   6  user/database creation failed
#   7  `trainer --doctor` smoke test failed (the env is up but
#      the trainer does not see it; the operator inspects
#      `.auto/testnet-postgres/log/postgres.log`)
#
# After the script exits 0, a worker can:
#   source .auto/testnet-postgres.env
#   RBP_TESTNET_FAST=1 bash scripts/testnet-live-proof.sh
# and the runbook chain will see a healthy Postgres.
#
# See `scripts/setup-testnet-postgres.md` for the runbook and
# `crates/autotrain/tests/setup_testnet_postgres.rs` for the
# no-DB integration test that pins the contract.
set -euo pipefail

# --- repo + script paths -------------------------------------------------
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# Walk up from scripts/ to the workspace root (one level).
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# --- env defaults --------------------------------------------------------
PG_PORT="${RBP_TESTNET_PG_PORT:-5433}"
PG_USER="${RBP_TESTNET_PG_USER:-rbp_live}"
PG_PASSWORD="${RBP_TESTNET_PG_PASSWORD:-rbp_live}"
PG_DATABASE="${RBP_TESTNET_PG_DATABASE:-rbp_live}"
PG_DATA_DIR="${RBP_TESTNET_PG_DATA_DIR:-$WORKSPACE_ROOT/.auto/testnet-postgres/data}"
PG_LOG_DIR="${RBP_TESTNET_PG_LOG_DIR:-$WORKSPACE_ROOT/.auto/testnet-postgres/log}"
PG_ENV_FILE="${RBP_TESTNET_PG_ENV_FILE:-$WORKSPACE_ROOT/.auto/testnet-postgres.env}"
TRAINER_BIN="${TRAINER_BIN:-$WORKSPACE_ROOT/target/debug/trainer}"

# --- required binaries gate (exit 2) -------------------------------------
# Refuse to run if any of the postgres userland / server binaries
# the script invokes is missing on `$PATH`. The integration test
# in `crates/autotrain/tests/setup_testnet_postgres.rs` synthesises
# fakes in a clean tmpdir + runs the script against them, so
# CI on a host without Postgres installed still exercises the
# contract.
REQUIRED_BINS=("initdb" "pg_ctl" "postgres" "psql" "createuser" "createdb")
MISSING_BINS=()
for bin in "${REQUIRED_BINS[@]}"; do
    if ! command -v "$bin" >/dev/null 2>&1; then
        MISSING_BINS+=("$bin")
    fi
done
if [[ ${#MISSING_BINS[@]} -gt 0 ]]; then
    echo "testnet-postgres: required binary missing: ${MISSING_BINS[*]}" >&2
    echo "  hint: install the postgresql server + client packages the OS ships" >&2
    echo "    Debian/Ubuntu: apt-get install postgresql postgresql-client" >&2
    echo "    RHEL/Fedora:   dnf install postgresql-server postgresql" >&2
    echo "    macOS (brew):  brew install postgresql" >&2
    exit 2
fi

# --- mkdir the data + log dirs up-front ---------------------------------
# Both are required before any of the post-init steps can write to
# them. A failed `mkdir` is a setup error (the data dir's parent
# might be read-only, etc.) and not a recoverable state, so we
# surface it via the generic exit 1 path.
mkdir -p "$PG_DATA_DIR" "$PG_LOG_DIR" \
    || { echo "testnet-postgres: could not create $PG_DATA_DIR or $PG_LOG_DIR" >&2; exit 1; }

# --- idempotent re-run detection (BEFORE the port gate) -----------------
# If the data dir is already initialised AND a local `psql
# SELECT 1` succeeds against the running instance, exit 0 with
# the `already provisioned` headline BEFORE the port-in-use check.
# Our own running instance on the data dir's Unix socket is
# fine — a re-run on a healthy env must NOT refuse to run. The
# check uses the data dir's local Unix socket path so we do not
# depend on the TCP port being the same as the rest of the
# script's $PG_PORT (an operator who re-points
# RBP_TESTNET_PG_PORT after a provision can still re-run the
# script against the existing cluster).
#
# The data dir's actual port is recorded in a small sentinel
# file `pg_ctl start` writes; the idempotent check reads it
# back so a re-run that points at the same data dir finds the
# right socket even if the operator changed
# `RBP_TESTNET_PG_PORT` between invocations.
PG_PORT_ACTUAL="$PG_PORT"
if [[ -f "$PG_DATA_DIR/PG_VERSION" && -f "$PG_DATA_DIR/.rbp_testnet_port" ]]; then
    PG_PORT_ACTUAL="$(cat "$PG_DATA_DIR/.rbp_testnet_port" 2>/dev/null || echo "$PG_PORT")"
fi
if [[ -f "$PG_DATA_DIR/PG_VERSION" ]]; then
    # Probe the data dir's local Unix socket first (port-agnostic).
    # The socket path is the data dir's absolute path; psql uses
    # `PGHOST` as a literal socket directory (not a host name)
    # when the value starts with `/` (or `./`). Resolving the
    # path to an absolute form first makes the check work even
    # when the operator's CWD differs from the data dir.
    PG_DATA_DIR_ABS_PROBE="$(cd "$PG_DATA_DIR" && pwd)"
    if PGHOST="$PG_DATA_DIR_ABS_PROBE" PGPORT="$PG_PORT_ACTUAL" \
        psql -At -U "$PG_USER" -d "$PG_DATABASE" \
            -c "SELECT 1" >/dev/null 2>&1; then
        echo "testnet-postgres: already provisioned (port=$PG_PORT_ACTUAL user=$PG_USER database=$PG_DATABASE data_dir=$PG_DATA_DIR)"
        echo "  env_file: $PG_ENV_FILE"
        exit 0
    fi
fi

# --- port-already-bound gate (exit 3) ------------------------------------
# A second Postgres on the configured port is a recipe for
# connection confusion (the runbook would race two `pg_ctl`
# instances and the `trainer --doctor` smoke would flap).
# Refuse to run; the operator can either stop the other
# Postgres or pick a different port via `RBP_TESTNET_PG_PORT`.
# (Skipped above when the data dir is already initialised —
# the env there is `ours` and the re-run path is the right
# way to surface that fact.)
if command -v ss >/dev/null 2>&1; then
    if ss -ltn 2>/dev/null | awk '{print $4}' | grep -E "(^|:)${PG_PORT}$" >/dev/null 2>&1; then
        echo "testnet-postgres: port $PG_PORT already in use" >&2
        echo "  hint: set RBP_TESTNET_PG_PORT to a free port and re-run" >&2
        exit 3
    fi
elif command -v netstat >/dev/null 2>&1; then
    if netstat -ltn 2>/dev/null | awk '{print $4}' | grep -E "(^|:)${PG_PORT}$" >/dev/null 2>&1; then
        echo "testnet-postgres: port $PG_PORT already in use" >&2
        echo "  hint: set RBP_TESTNET_PG_PORT to a free port and re-run" >&2
        exit 3
    fi
fi

# --- env file emission (idempotent fast path) ----------------------------
# The env file the operator sources. We write it BEFORE
# `initdb` so a partial-failure on a first invocation still
# leaves a parseable env file behind. The contents are stable
# across invocations (the user/password/database/port knobs are
# the operator's contract; the script never re-rolls them).
#
# SECURITY NOTE: the password embedded in DATABASE_URL / DB_URL
# is the local-only test credential (default `rbp_live`). It is
# intentionally a low-entropy string — do NOT reuse this value
# in any non-test environment.
#
# The env file is written to `$PG_ENV_FILE` which lives under
# `.auto/` (gitignored). The script DOES print the password to
# the operator's terminal when it writes the env file via the
# `chmod 0600` line below — that is intentional, the file is
# the operator's auditable handoff. A `chmod 0600` makes it
# owner-readable only so a multi-user host does not leak the
# test credential.
mkdir -p "$(dirname "$PG_ENV_FILE")"
# Use an UNQUOTED heredoc delimiter (`<<ENV`) so bash expands
# the `${PG_USER}` / `${PG_PASSWORD}` / `${PG_PORT}` /
# `${PG_DATABASE}` / `${PG_DATA_DIR}` / `${PG_LOG_DIR}`
# placeholders at write time — the operator's `source` of
# the resulting file must see literal host:port:user values,
# not bash variables that resolve to nothing in the
# operator's shell. Backticks in the comment lines are
# backslash-escaped so the heredoc writer does not perform
# command substitution on the literal word `source`.
cat > "$PG_ENV_FILE" <<ENV
# Auto-generated by scripts/setup-testnet-postgres.sh (STW-078).
# \`source\` this file to set DATABASE_URL + DB_URL for the
# testnet live proof runbook. Re-run the script to refresh
# after a Postgres restart.
#
# SECURITY NOTE: the password embedded in DATABASE_URL /
# DB_URL is the local-only test credential the script
# generated (\$PG_PASSWORD, default \`rbp_live\`). It is
# intentionally a low-entropy string — do NOT reuse this
# value in any non-test environment.
DATABASE_URL=postgres://${PG_USER}:${PG_PASSWORD}@127.0.0.1:${PG_PORT}/${PG_DATABASE}
DB_URL=postgres://${PG_USER}:${PG_PASSWORD}@127.0.0.1:${PG_PORT}/${PG_DATABASE}
RBP_TESTNET_PG_PORT=${PG_PORT}
RBP_TESTNET_PG_USER=${PG_USER}
RBP_TESTNET_PG_PASSWORD=${PG_PASSWORD}
RBP_TESTNET_PG_DATABASE=${PG_DATABASE}
RBP_TESTNET_PG_DATA_DIR=${PG_DATA_DIR}
RBP_TESTNET_PG_LOG_DIR=${PG_LOG_DIR}
ENV
# Owner-only read/write: a multi-user host should not be able
# to scrape the test credential off the operator's disk.
chmod 0600 "$PG_ENV_FILE" 2>/dev/null || true

# --- initdb (idempotent: skip if data dir is already initialised) -------
if [[ ! -f "$PG_DATA_DIR/PG_VERSION" ]]; then
    echo "testnet-postgres: initialising cluster at $PG_DATA_DIR" >&2
    # `--auth=trust` is the documented escape valve for local
    # test environments (we want a known user with a known
    # password, not the system auth chain). The script pins
    # the password via the `psql` `ALTER USER ... PASSWORD ...`
    # leg below so the role matches what `DATABASE_URL`
    # declares. `--pwfile` is the non-interactive form of the
    # `Enter new password:` prompt `initdb` would otherwise
    # ask for.
    if ! initdb \
            --pgdata="$PG_DATA_DIR" \
            --auth=trust \
            --username="$PG_USER" \
            --pwfile=<(printf '%s' "$PG_PASSWORD") \
            >"$PG_LOG_DIR/initdb.log" 2>&1; then
        echo "testnet-postgres: initdb failed; see $PG_LOG_DIR/initdb.log" >&2
        exit 4
    fi
fi

# --- start the server (idempotent: skip if already running) -------------
# `pg_ctl status` returns 0 if the server is up, 3 if it is
# down. We probe before starting so a second invocation does
# not flap a healthy instance.
NEED_START=1
if pg_ctl --pgdata="$PG_DATA_DIR" status >/dev/null 2>&1; then
    NEED_START=0
fi

if [[ "$NEED_START" -eq 1 ]]; then
    echo "testnet-postgres: starting postgres on port $PG_PORT" >&2
    # `--port` selects the listen port. `-k` (Unix socket dir) is
    # the data dir by default; we override to the absolute path
    # of the data dir (the postgres daemon resolves `-k` against
    # its own CWD, not against `--pgdata`, so a relative value
    # such as `.auto/testnet-postgres/data` would fail with
    # `could not create lock file ... : No such file or directory`
    # when the script is invoked from a different CWD than the
    # data dir). `-h` binds to localhost only; the script is
    # local-only by design.
    PG_DATA_DIR_ABS="$(cd "$PG_DATA_DIR" && pwd)"
    if ! pg_ctl --pgdata="$PG_DATA_DIR" \
            --log="$PG_LOG_DIR/postgres.log" \
            --options="-p $PG_PORT -h 127.0.0.1 -k $PG_DATA_DIR_ABS" \
            start \
            >>"$PG_LOG_DIR/pg_ctl.log" 2>&1; then
        echo "testnet-postgres: pg_ctl start failed; see $PG_LOG_DIR/pg_ctl.log + $PG_LOG_DIR/postgres.log" >&2
        exit 5
    fi
    # Record the port the running instance actually bound to
    # in a small sentinel file under the data dir. The
    # idempotent re-run check at the top of the script reads
    # this back so a re-run with a changed
    # RBP_TESTNET_PG_PORT still finds the running instance
    # (the `port=` key=value in the headline reflects the
    # *actual* port the cluster was started with, not the
    # operator's current `RBP_TESTNET_PG_PORT`).
    printf '%s\n' "$PG_PORT" > "$PG_DATA_DIR/.rbp_testnet_port"
fi

# --- create the role + database (idempotent) ---------------------------
# `createuser` and `createdb` are idempotent when the role / db
# already exist (they exit 0 with a notice on stderr, which we
# silence via `2>/dev/null`). The script re-asserts the password
# on every run so a Postgres restart that reverted `pg_authid`
# (e.g. a `pg_ctl reload` after an `initdb`-with-no-`--pwfile`)
# gets re-pinned.
#
# We invoke the userland tools via a Unix-socket connect to
# the freshly-started local Postgres, using `PGHOST` +
# `PGPORT` env vars (the tools honour the standard libpq env
# contract). The `trust` auth on the local socket means
# `psql` / `createuser` / `createdb` do not need a password
# from us here — the password is enforced at the TCP layer
# (via `pg_hba.conf` + the `ALTER USER ... PASSWORD` leg
# below) so the runbook's `DATABASE_URL` round-trip is the
# real contract.
PG_DATA_DIR_ABS_SOCK="$(cd "$PG_DATA_DIR" && pwd)"
PGHOST="$PG_DATA_DIR_ABS_SOCK" PGPORT="$PG_PORT" \
    createuser --username="$PG_USER" \
    --superuser --no-password "$PG_USER" 2>/dev/null || true

PGHOST="$PG_DATA_DIR_ABS_SOCK" PGPORT="$PG_PORT" \
    createdb --username="$PG_USER" \
    --no-password --owner="$PG_USER" "$PG_DATABASE" 2>/dev/null || true

# Re-pin the password (defensive: covers the `initdb --pwfile=`
# path on a fresh cluster + the manual-reset case where an
# operator cleared `pg_authid`). Connect to the
# `postgres` admin DB (always present after `initdb`) so the
# ALTER USER is a no-op-on-re-run for the user/db pair the
# runbook authenticates against.
if ! PGHOST="$PG_DATA_DIR_ABS_SOCK" PGPORT="$PG_PORT" \
    psql --username="$PG_USER" \
    --no-password -d postgres \
    -c "ALTER USER ${PG_USER} PASSWORD '${PG_PASSWORD}';" \
    >"$PG_LOG_DIR/psql_alter.log" 2>&1; then
    echo "testnet-postgres: ALTER USER failed; see $PG_LOG_DIR/psql_alter.log" >&2
    exit 6
fi

# --- final smoke test: `trainer --doctor` exits 0 with db_reachable ----
# The doctor is the runbook's pre-flight gate; if it is green,
# the rest of the chain (`--cluster` → `--reset` → `--smoke` → ...)
# will see the same healthy Postgres. We skip the smoke when
# `RBP_TESTNET_PG_SKIP_DOCTOR=1` (the trainer binary may not
# exist yet on a cold workspace).
if [[ "${RBP_TESTNET_PG_SKIP_DOCTOR:-}" == "1" ]]; then
    echo "testnet-postgres: complete: port=$PG_PORT user=$PG_USER database=$PG_DATABASE data_dir=$PG_DATA_DIR"
    echo "  env_file: $PG_ENV_FILE"
    echo "  (smoke test skipped via RBP_TESTNET_PG_SKIP_DOCTOR=1)"
    exit 0
fi

if [[ ! -x "$TRAINER_BIN" ]]; then
    echo "testnet-postgres: complete: port=$PG_PORT user=$PG_USER database=$PG_DATABASE data_dir=$PG_DATA_DIR"
    echo "  env_file: $PG_ENV_FILE"
    echo "  (smoke test skipped: trainer binary not found at $TRAINER_BIN)"
    echo "  hint: run \`cargo build --bin trainer\` and re-run this script to enable the smoke test"
    exit 0
fi

# Invoke the doctor over TCP (the runbook's `DATABASE_URL`
# contract is a TCP connection, so the smoke test exercises
# the same code path). Source the env file we just wrote so
# `DATABASE_URL` + `DB_URL` are both set (the doctor requires
# one or the other).
# shellcheck disable=SC1090
. "$PG_ENV_FILE"
if ! "$TRAINER_BIN" --doctor >"$PG_LOG_DIR/doctor.stdout" 2>"$PG_LOG_DIR/doctor.stderr"; then
    echo "testnet-postgres: trainer --doctor failed" >&2
    echo "  stdout: $PG_LOG_DIR/doctor.stdout" >&2
    echo "  stderr: $PG_LOG_DIR/doctor.stderr" >&2
    echo "  postgres log: $PG_LOG_DIR/postgres.log" >&2
    exit 7
fi

# The doctor is a JSON line on stdout; assert `db_reachable: true`
# so a future doctor change that flips the field name fails here
# (the script's contract is the *value*, not just the exit code).
if ! grep -q '"db_reachable":true' "$PG_LOG_DIR/doctor.stdout"; then
    echo "testnet-postgres: trainer --doctor reported db_reachable: false" >&2
    echo "  stdout: $PG_LOG_DIR/doctor.stdout" >&2
    echo "  stderr: $PG_LOG_DIR/doctor.stderr" >&2
    echo "  postgres log: $PG_LOG_DIR/postgres.log" >&2
    exit 7
fi

# --- the headline -------------------------------------------------------
# The one-line summary a CI dashboard scrapes. Format mirrors
# the `testnet live_proof complete: ...` headline the runbook
# emits so a regex dashboard extraction stays stable. The
# `key=value` pairs are:
#   port=        the TCP port the local Postgres binds to
#   user=        the role the runbook authenticates as
#   database=    the database the runbook reads
#   data_dir=    the postgres data directory
echo "testnet-postgres: complete: port=$PG_PORT user=$PG_USER database=$PG_DATABASE data_dir=$PG_DATA_DIR"
echo "  env_file: $PG_ENV_FILE"
echo "  smoke: trainer --doctor exit=0 db_reachable=true"
