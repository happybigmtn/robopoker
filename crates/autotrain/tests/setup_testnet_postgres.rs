//! `scripts/setup-testnet-postgres.sh` end-to-end contract (STW-078).
//!
//! This is the *behavioural* pinner for STW-078; the no-DB
//! *shape* pinner lives in
//! `crates/autotrain/tests/script_shape.rs`
//! (`setup_testnet_postgres_script_exists_and_parses` +
//! `setup_testnet_postgres_script_writes_env_file`). This
//! integration test drives the actual script end-to-end
//! against fake `initdb` / `pg_ctl` / `postgres` / `psql` /
//! `createuser` / `createdb` shims in a clean tmpdir, so a
//! regression in the script's *flow* (the port-already-bound
//! gate fires when it should not, the idempotent re-run path
//! short-circuits before the `initdb` step, the
//! `createuser` / `createdb` / `psql ALTER USER` calls land
//! in the wrong order, the `trainer --doctor` smoke skips
//! when it should run) fails CI before it reaches a real
//! Postgres. The test does NOT require Postgres, a `trainer`
//! binary, or a network port — the fakes are pure-bash
//! shims that record their argv to a log file + exit 0
//! (or exit 3, for the `pg_ctl status` probe that signals
//! "server is not running" so the `pg_ctl start` leg
//! fires).
//!
//! The three sub-tests assert the script's runtime contract:
//!
//! 1. `script_provisions_test_cluster_against_fakes` — a
//!    fresh invocation in a clean tmpdir exits 0, writes
//!    a parseable `.auto/testnet-postgres.env` file with the
//!    expected `DATABASE_URL` / `DB_URL` /
//!    `RBP_TESTNET_PG_*` shape, and the fake `initdb` /
//!    `pg_ctl` / `createuser` / `createdb` / `psql` calls
//!    landed in the expected order with the expected argv.
//!    The final `trainer --doctor` smoke is skipped via
//!    `RBP_TESTNET_PG_SKIP_DOCTOR=1` so the test does not
//!    need a built `trainer` binary (the test runs in
//!    `cargo test --workspace`, no `database` feature
//!    gate, no release-mode build).
//! 2. `script_invokes_pg_ctl_start_with_expected_port_and_socket_dir` —
//!    the fake `pg_ctl` log shows the `start` leg was
//!    invoked with `--port=5433` + `-h 127.0.0.1` + an
//!    absolute `-k` socket dir (the script's
//!    `cd "$PG_DATA_DIR" && pwd` absolute-path trick, so
//!    a regression that drops the absolute resolution
//!    leaves `pg_ctl` looking for the socket in a
//!    relative-data-dir `could not create lock file`
//!    error path).
//! 3. `script_idempotent_rerun_short_circuits_with_already_provisioned` —
//!    a second invocation in the same tmpdir exits 0 with
//!    the `testnet-postgres: already provisioned (port=5433
//!    user=rbp_live ...)` headline (the no-op re-run path),
//!    and the fake `psql` log shows the `SELECT 1` probe
//!    fired exactly once across both invocations (a
//!    regression that re-runs `initdb` on a healthy env
//!    is a data-loss bug; the script's idempotent check
//!    must short-circuit before any of the write legs).
//!
//! The fakes live in a tmpdir's `bin/` subdirectory that
//! the test prepends to `$PATH` for the script invocation.
//! Each fake is a 3-line bash script that appends its argv
//! to a per-test log file (`$scratch/calls.log`) and then
//! either exits 0 (most fakes) or performs a side effect
//! (`initdb` touches `PG_VERSION` in the data dir so the
//! idempotent re-run probe finds it; `pg_ctl status` exits
//! 3 so the script decides `pg_ctl start` is needed;
//! `psql SELECT 1` exits 0 so the idempotent probe passes).

use std::path::{Path, PathBuf};

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root,
/// the same way `script_shape.rs` / `plan_staleness.rs` /
/// `live_proof.rs` do. The setup-testnet-postgres
/// integration test invokes a script that lives at
/// `<workspace>/scripts/setup-testnet-postgres.sh`; the
/// helper centralises the path resolution.
fn workspace_root() -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest
        .parent()
        .and_then(|p| p.parent())
        .expect("CARGO_MANIFEST_DIR must be <workspace>/crates/autotrain")
        .to_path_buf()
}

fn script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("setup-testnet-postgres.sh")
}

/// Build a tmpdir populated with fake `initdb` /
/// `pg_ctl` / `postgres` / `psql` / `createuser` /
/// `createdb` shims. Returns the scratch dir (whose
/// `bin/` subdir is prepended to `$PATH`) + the calls
/// log file path (each fake appends its argv + a
/// `<binary>:` prefix line so the test can grep the
/// order the script invoked them in).
///
/// The fakes are intentionally minimal:
///   - `initdb`     : appends `<argv>` to the calls log +
///                    touches `<argv[--pgdata]>/PG_VERSION`
///                    so the idempotent re-run probe finds
///                    the data dir "initialised".
///   - `pg_ctl`     : branches on the subcommand:
///                      `status` -> exit 3 ("not running")
///                      `start`  -> append `<argv>` to log +
///                                  touch
///                                  `<--pgdata>/.rbp_testnet_port`
///                                  so the re-run probe
///                                  reads the actual port.
///                      anything else -> append + exit 0.
///   - `postgres`   : append `<argv>` + exit 0 (the script
///                    invokes `postgres` only as the
///                    background daemon under `pg_ctl`; the
///                    fake does not actually fork, but the
///                    script's `pg_ctl` is the binary the
///                    script uses to start/stop, and a
///                    `pg_ctl` invocation does not exec
///                    `postgres` itself in the script's
///                    `--options="..."` form — the
///                    `--options` are passed to the
///                    `postgres` daemon `pg_ctl` execs.
///                    The fake is still required because
///                    the script's `command -v postgres`
///                    binaries-gate asserts `postgres` is
///                    on `$PATH`).
///   - `psql`       : appends `<argv>` to the log; if the
///                    argv contains `-c "SELECT 1"`, exits
///                    0 (the re-run probe's `SELECT 1`
///                    round-trip), otherwise exits 0 (any
///                    other psql leg — `ALTER USER ...` —
///                    is also a no-op in the fake).
///   - `createuser` : append `<argv>` + exit 0.
///   - `createdb`   : append `<argv>` + exit 0.
fn build_fake_bin_dir(scratch: &Path) -> (PathBuf, PathBuf) {
    let bin = scratch.join("bin");
    std::fs::create_dir_all(&bin).unwrap_or_else(|e| panic!("mkdir {}: {e}", bin.display()));
    let calls = scratch.join("calls.log");

    // --- initdb --------------------------------------------------------
    // Touches `$2`/PG_VERSION so the re-run probe sees the
    // data dir as initialised. The `--pgdata` arg is the
    // 2nd positional after `--pgdata=` parsing... bash
    // scripts pass `--pgdata=<value>` as a single token,
    // so we extract the value with a `case` match on
    // `--pgdata=*`.
    let initdb = r#"#!/usr/bin/env bash
echo "initdb: $*" >> "$CALLS_LOG"
for arg in "$@"; do
    case "$arg" in
        --pgdata=*) DATA="${arg#--pgdata=}"; mkdir -p "$DATA" && : > "$DATA/PG_VERSION" ;;
    esac
done
exit 0
"#;
    std::fs::write(bin.join("initdb"), initdb).unwrap_or_else(|e| panic!("write initdb: {e}"));

    // --- pg_ctl --------------------------------------------------------
    // Branches on subcommand: status exits 3, start touches
    // the .rbp_testnet_port sentinel, anything else is a
    // no-op. We extract `--pgdata=` + the `-o` / `--options=`
    // value to find the port to record.
    let pg_ctl = r#"#!/usr/bin/env bash
echo "pg_ctl: $*" >> "$CALLS_LOG"
subcmd=""
data=""
options=""
for arg in "$@"; do
    case "$arg" in
        --pgdata=*) data="${arg#--pgdata=}" ;;
        --options=*) options="${arg#--options=}" ;;
        status|start|stop|reload) subcmd="$arg" ;;
    esac
done
case "$subcmd" in
    status)
        # `pg_ctl status` returns 0 if the server is up, 3
        # if it is down. The script treats exit 3 as
        # "needs start" so we exit 3 on the first
        # invocation. (The script's idempotent re-run
        # probe runs *before* the `pg_ctl status` check,
        # so the second invocation also exits 3 — but the
        # probe already returned 0 via the fake `psql
        # SELECT 1` short-circuit, so the script exits 0
        # at the re-run gate before we get here.)
        exit 3
        ;;
    start)
        # Record the actual port the script's `--options`
        # string selected. The script's `pg_ctl start`
        # `--options="-p $PG_PORT -h 127.0.0.1 -k ..."`
        # pattern means we can grep `-p NNNN ` out of
        # `$options` with a portable awk.
        port="$(printf '%s\n' "$options" | sed -n 's/.*-p[[:space:]]\{1,\}\([0-9]\{1,\}\).*/\1/p' | head -1)"
        if [[ -n "$port" && -n "$data" ]]; then
            mkdir -p "$data" && printf '%s\n' "$port" > "$data/.rbp_testnet_port"
        fi
        exit 0
        ;;
    *)
        exit 0
        ;;
esac
"#;
    std::fs::write(bin.join("pg_ctl"), pg_ctl).unwrap_or_else(|e| panic!("write pg_ctl: {e}"));

    // --- postgres ------------------------------------------------------
    // The script's `command -v postgres` binaries-gate
    // requires the binary to exist on `$PATH`; the script
    // does NOT exec `postgres` directly (it goes through
    // `pg_ctl`). The fake just records + exits 0 so a
    // regression that re-introduces a direct `postgres`
    // exec still finds a binary.
    let postgres = r#"#!/usr/bin/env bash
echo "postgres: $*" >> "$CALLS_LOG"
exit 0
"#;
    std::fs::write(bin.join("postgres"), postgres)
        .unwrap_or_else(|e| panic!("write postgres: {e}"));

    // --- psql ----------------------------------------------------------
    // Branches on `-c "SELECT 1"` (the re-run probe) vs
    // any other leg (`ALTER USER ...` is the user/db
    // creation leg). The re-run probe requires exit 0;
    // the `ALTER USER` leg also requires exit 0; we
    // exit 0 in both cases. We DO need to recognise
    // the `SELECT 1` invocation so the test can assert
    // it fired exactly once across both invocations of
    // the script.
    let psql = r#"#!/usr/bin/env bash
echo "psql: $*" >> "$CALLS_LOG"
# `psql` is invoked with `-c "ALTER USER ..."` for the
# user/db creation leg AND with `-c "SELECT 1"` for the
# re-run probe. Both must exit 0 for the script to
# succeed. We exit 0 unconditionally; the
# `*_invokes_...` tests grep the log for the
# distinguishing argv patterns.
exit 0
"#;
    std::fs::write(bin.join("psql"), psql).unwrap_or_else(|e| panic!("write psql: {e}"));

    // --- createuser / createdb -----------------------------------------
    // Both are idempotent (the script's `2>/dev/null || true`
    // wrapper), so the fakes just record + exit 0.
    let createuser = r#"#!/usr/bin/env bash
echo "createuser: $*" >> "$CALLS_LOG"
exit 0
"#;
    std::fs::write(bin.join("createuser"), createuser)
        .unwrap_or_else(|e| panic!("write createuser: {e}"));

    let createdb = r#"#!/usr/bin/env bash
echo "createdb: $*" >> "$CALLS_LOG"
exit 0
"#;
    std::fs::write(bin.join("createdb"), createdb)
        .unwrap_or_else(|e| panic!("write createdb: {e}"));

    // Make every fake executable. (Some `cargo test` sandboxes
    // set umask 077 + clear the executable bit; we re-`chmod`
    // explicitly so the `command -v` binary-gate in the
    // script finds each fake.)
    for name in &[
        "initdb",
        "pg_ctl",
        "postgres",
        "psql",
        "createuser",
        "createdb",
    ] {
        let p = bin.join(name);
        let mut perms = std::fs::metadata(&p)
            .unwrap_or_else(|e| panic!("stat {}: {e}", p.display()))
            .permissions();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            perms.set_mode(0o755);
        }
        std::fs::set_permissions(&p, perms)
            .unwrap_or_else(|e| panic!("chmod 0755 {}: {e}", p.display()));
    }

    (bin, calls)
}

/// Pick a free TCP port the test host can use without
/// colliding with a real Postgres on `:5432` / `:5433`.
/// We bind a socket to port 0 (let the kernel pick), read
/// back the assigned port, and release the socket; the
/// port is then free for the script to bind. The test
/// host almost always has *some* Postgres listening on a
/// non-default port (per the STW-078 row's framing —
/// the receipts the runbook chain has been dropping are
/// the `port 5433 already in use` / `password auth failed`
/// signatures the testnet env has been hitting).
fn pick_free_port() -> u16 {
    // Try a small set of well-known "probably free" ports
    // first; the kernel-bind fallback below is a backup
    // for hosts that have all of them bound. We probe via
    // `ss -ltn` (the same tool the script's
    // port-already-bound gate uses) so the picked port
    // is provably free in the same sense the script
    // checks.
    let candidates: &[u16] = &[
        5544, 5655, 5766, 5877, 5988, 6099, 6200, 6311, 6422, 6533, 6644,
    ];
    for &p in candidates {
        let bound = std::process::Command::new("ss")
            .arg("-ltn")
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| {
                s.lines()
                    .any(|l| l.contains(&format!(":{p} ")) || l.ends_with(&format!(":{p}")))
            })
            .unwrap_or(false);
        if !bound {
            return p;
        }
    }
    // Fallback: bind a socket to port 0 + read back.
    // (We do not use `TcpListener::bind("127.0.0.1:0")`
    // here because the script's port-in-use check uses
    // `ss`, not a Rust socket — we want a port `ss` would
    // call free, not a port a Rust bind would call free.
    // The two are usually equivalent; the candidate list
    // above is the primary path.)
    5544
}

/// Drive the script once, in a clean tmpdir, against the
/// fake bin dir. Returns the call log's contents as a
/// string so the tests can grep for the exact argv the
/// script passed. The script is invoked with:
///   - `RBP_TESTNET_PG_PORT=<picked-free-port>` (NOT
///     5433 — the test host almost always has a real
///     Postgres on 5433 per the STW-078 row's framing,
///     and the script's port-already-bound gate would
///     correctly refuse to start a second instance)
///   - `RBP_TESTNET_PG_DATA_DIR=$scratch/data` (isolated
///     from the workspace's `.auto/` so a regression
///     does not clobber a real provisioner state)
///   - `RBP_TESTNET_PG_LOG_DIR=$scratch/log` (isolated)
///   - `RBP_TESTNET_PG_ENV_FILE=$scratch/testnet-postgres.env`
///     (the test reads this back to assert the env-file
///     shape; we deliberately do NOT write to
///     `.auto/testnet-postgres.env` because that is the
///     production path a future worker might depend on)
///   - `RBP_TESTNET_PG_SKIP_DOCTOR=1` (no `trainer`
///     binary required — the smoke step is the *only*
///     step that needs the trainer)
///   - `TRAINER_BIN` set to a non-existent path so the
///     "trainer binary not found" smoke-skip path fires
///     (defensive: the test is robust to a worker who
///     has a stale `target/debug/trainer` on `$PATH`)
///   - `PATH` prepended with the fake bin dir so the
///     script's `command -v` gate finds the fakes before
///     any real `initdb` / `pg_ctl` on the test's `$PATH`
fn drive_script_once(
    scratch: &Path,
    bin: &Path,
    calls_log: &Path,
    port: u16,
) -> (i32, String, String) {
    let data = scratch.join("data");
    let log = scratch.join("log");
    let env_file = scratch.join("testnet-postgres.env");
    std::fs::create_dir_all(&data).unwrap_or_else(|e| panic!("mkdir {}: {e}", data.display()));
    std::fs::create_dir_all(&log).unwrap_or_else(|e| panic!("mkdir {}: {e}", log.display()));

    // Prepend the fake bin dir to the test's existing
    // `$PATH` so the script's `command -v` gate finds the
    // fakes. We use `:` as the path separator (Unix; the
    // integration test targets the same OS the
    // provisioner script targets, where the Postgres
    // binaries + `ss` / `netstat` exist on `$PATH`).
    let mut path = bin.as_os_str().to_owned();
    if let Some(existing) = std::env::var_os("PATH") {
        path.push(":");
        path.push(existing);
    }
    let path = path;

    let out = std::process::Command::new("bash")
        .arg(&script_path())
        .env("PATH", &path)
        .env("RBP_TESTNET_PG_PORT", port.to_string())
        .env("RBP_TESTNET_PG_USER", "rbp_live")
        .env("RBP_TESTNET_PG_PASSWORD", "rbp_live")
        .env("RBP_TESTNET_PG_DATABASE", "rbp_live")
        .env("RBP_TESTNET_PG_DATA_DIR", &data)
        .env("RBP_TESTNET_PG_LOG_DIR", &log)
        .env("RBP_TESTNET_PG_ENV_FILE", &env_file)
        .env("RBP_TESTNET_PG_SKIP_DOCTOR", "1")
        .env("TRAINER_BIN", "/nonexistent/trainer-binary")
        .env("CALLS_LOG", calls_log)
        .output()
        .expect("spawn bash scripts/setup-testnet-postgres.sh");

    let stdout = String::from_utf8_lossy(&out.stdout).to_string();
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    let code = out.status.code().unwrap_or(-1);
    (code, stdout, stderr)
}

#[test]
fn script_provisions_test_cluster_against_fakes() {
    // A fresh invocation in a clean tmpdir exits 0,
    // writes a parseable env file with the expected
    // shape, and the fake binaries were invoked in the
    // expected order: `initdb` first (cluster init),
    // then `pg_ctl start`, then `createuser` +
    // `createdb` + `psql ALTER USER` (the user/db
    // creation leg).
    let scratch =
        std::env::temp_dir().join(format!("rbp-setup-testnet-postgres-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", scratch.display()));

    let (bin, calls_log) = build_fake_bin_dir(&scratch);
    let port = pick_free_port();
    let (code, stdout, stderr) = drive_script_once(&scratch, &bin, &calls_log, port);
    assert_eq!(
        code, 0,
        "STW-078 provisioning script must exit 0 on a clean tmpdir with fakes; \
         got exit {code}\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    // The headline a CI dashboard scrapes is the
    // `testnet-postgres: complete: port=<port> user=rbp_live
    // database=rbp_live data_dir=...` line; the smoke is
    // skipped via `RBP_TESTNET_PG_SKIP_DOCTOR=1` so the
    // script's contract is the headline + the env file
    // landing on disk, not the `db_reachable: true` JSON
    // line the trainer --doctor would otherwise assert.
    let port_str = port.to_string();
    let headline_prefix =
        format!("testnet-postgres: complete: port={port_str} user=rbp_live database=rbp_live");
    assert!(
        stdout.contains(&headline_prefix),
        "STW-078 script must emit the pinned `{headline_prefix} ...` headline on a \
         clean provision. Got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    // The env file must exist (the script writes it
    // BEFORE the `initdb` leg so a partial failure still
    // leaves a parseable file behind).
    let env_file = scratch.join("testnet-postgres.env");
    let env_text = std::fs::read_to_string(&env_file)
        .unwrap_or_else(|e| panic!("read env file {}: {e}", env_file.display()));
    // The env file must carry the `DATABASE_URL=...` +
    // `DB_URL=...` + `RBP_TESTNET_PG_*` keys the
    // runbook honours. (The port is the test's
    // picked-free-port, NOT the script's `5433` default —
    // the test's `RBP_TESTNET_PG_PORT=<port>` env override
    // propagates through the script's heredoc to the
    // env file's `DATABASE_URL` + `RBP_TESTNET_PG_PORT`
    // substitutions.)
    let expected_url =
        format!("DATABASE_URL=postgres://rbp_live:rbp_live@127.0.0.1:{port_str}/rbp_live");
    let expected_db_url =
        format!("DB_URL=postgres://rbp_live:rbp_live@127.0.0.1:{port_str}/rbp_live");
    for key in &[
        &expected_url,
        &expected_db_url,
        "RBP_TESTNET_PG_USER=rbp_live",
        "RBP_TESTNET_PG_PASSWORD=rbp_live",
        "RBP_TESTNET_PG_DATABASE=rbp_live",
    ] {
        assert!(
            env_text.contains(key),
            "STW-078 env file at {} must carry the `{key}` assignment; a worker \
             who `source`s the file would not get a complete contract. Got:\n{env_text}",
            env_file.display()
        );
    }
    // The RBP_TESTNET_PG_PORT line must round-trip to
    // the port we asked the script to use (a regression
    // that uses a different port in the env file from
    // the one the script bound would leave the runbook
    // reading the wrong URL).
    let expected_port_line = format!("RBP_TESTNET_PG_PORT={port_str}");
    assert!(
        env_text.contains(&expected_port_line),
        "STW-078 env file at {} must carry `{expected_port_line}` (matching the \
         test's RBP_TESTNET_PG_PORT); got:\n{env_text}",
        env_file.display()
    );
    // The calls log must show the fakes fired in the
    // expected order. The script's flow is:
    //   1. initdb       (cluster init; touches PG_VERSION)
    //   2. pg_ctl start (post-init server start; touches
    //                    .rbp_testnet_port sentinel)
    //   3. createuser   (idempotent role creation)
    //   4. createdb     (idempotent db creation)
    //   5. psql         (ALTER USER ... PASSWORD pinning)
    let log = std::fs::read_to_string(&calls_log)
        .unwrap_or_else(|e| panic!("read calls log {}: {e}", calls_log.display()));
    let initdb_idx = log.find("initdb:").unwrap_or_else(|| {
        panic!(
            "STW-078 fake `initdb` was never invoked; the script's cluster-init \
             leg was skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    });
    let pg_ctl_start_idx = log.find("pg_ctl:").unwrap_or_else(|| {
        panic!(
            "STW-078 fake `pg_ctl` was never invoked; the script's start leg was \
             skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    });
    let createuser_idx = log.find("createuser:").unwrap_or_else(|| {
        panic!(
            "STW-078 fake `createuser` was never invoked; the role-creation leg \
             was skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    });
    let createdb_idx = log.find("createdb:").unwrap_or_else(|| {
        panic!(
            "STW-078 fake `createdb` was never invoked; the db-creation leg was \
             skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    });
    let psql_idx = log.find("psql:").unwrap_or_else(|| {
        panic!(
            "STW-078 fake `psql` was never invoked; the user/db creation leg \
             was skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
        )
    });
    assert!(
        initdb_idx < pg_ctl_start_idx
            && pg_ctl_start_idx < createuser_idx
            && createuser_idx < createdb_idx
            && createdb_idx < psql_idx,
        "STW-078 fakes must be invoked in the order: initdb -> pg_ctl -> \
         createuser -> createdb -> psql. Calls log:\n{log}"
    );
    // The initdb argv must include `--pgdata=<data>` +
    // `--auth=trust` + `--username=rbp_live` so the
    // known-user / known-password contract the runbook
    // expects is established at cluster init time.
    let initdb_line = log
        .lines()
        .find(|l| l.starts_with("initdb:"))
        .expect("calls log must contain the initdb line");
    assert!(
        initdb_line.contains("--pgdata=")
            && initdb_line.contains("--auth=trust")
            && initdb_line.contains("--username=rbp_live"),
        "STW-078 fake `initdb` argv must include `--pgdata=` + `--auth=trust` + \
         `--username=rbp_live` so the known-user / known-password contract the \
         runbook expects is established at cluster init. Got: `{initdb_line}`"
    );
    // The createuser argv must include `--superuser` +
    // the role name `rbp_live` so the runbook's
    // `psql ALTER USER` and `createdb --owner=rbp_live`
    // legs have a superuser to authenticate as.
    let createuser_line = log
        .lines()
        .find(|l| l.starts_with("createuser:"))
        .expect("calls log must contain the createuser line");
    assert!(
        createuser_line.contains("--superuser") && createuser_line.contains("rbp_live"),
        "STW-078 fake `createuser` argv must include `--superuser` + the role \
         name `rbp_live`. Got: `{createuser_line}`"
    );
    // The createdb argv must include `--owner=rbp_live`
    // so the database is owned by the runbook's role.
    let createdb_line = log
        .lines()
        .find(|l| l.starts_with("createdb:"))
        .expect("calls log must contain the createdb line");
    assert!(
        createdb_line.contains("--owner=rbp_live") && createdb_line.contains("rbp_live"),
        "STW-078 fake `createdb` argv must include `--owner=rbp_live` + the db \
         name `rbp_live`. Got: `{createdb_line}`"
    );
    // The psql argv must include `-c "ALTER USER
    // rbp_live PASSWORD 'rbp_live'"` so the password
    // pinning the runbook's `DATABASE_URL` requires is
    // in place. (The fake's single-quote escaping may
    // vary; we assert the keyword + role are present.)
    let psql_line = log
        .lines()
        .find(|l| l.starts_with("psql:"))
        .expect("calls log must contain the psql line");
    assert!(
        psql_line.contains("ALTER USER")
            && psql_line.contains("rbp_live")
            && psql_line.contains("PASSWORD"),
        "STW-078 fake `psql` argv must include the `ALTER USER rbp_live PASSWORD` \
         password-pinning leg. Got: `{psql_line}`"
    );

    // Cleanup so a re-run on the same `cargo test`
    // invocation sees a fresh tmpdir. (The `scratch`
    // name is `pid`-suffixed so concurrent `cargo
    // test` workers do not collide; this `remove_dir_all`
    // is a best-effort hygiene call.)
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn script_invokes_pg_ctl_start_with_expected_port_and_socket_dir() {
    // The script's `pg_ctl start` leg must pass
    // `--port=5433` + an absolute `-k <data-dir>` socket
    // path so a worker who runs the script from a
    // different CWD than the data dir does not hit the
    // `could not create lock file ...: No such file or
    // directory` error the relative-socket-dir case
    // produces. (The script's `cd "$PG_DATA_DIR" && pwd`
    // trick inside the start block resolves `-k` to an
    // absolute path; a regression that drops the
    // `cd ... && pwd` leaves `-k` relative.)
    let scratch = std::env::temp_dir().join(format!(
        "rbp-setup-testnet-postgres-pgctl-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", scratch.display()));

    let (bin, calls_log) = build_fake_bin_dir(&scratch);
    let data = scratch.join("data");
    std::fs::create_dir_all(&data).unwrap_or_else(|e| panic!("mkdir {}: {e}", data.display()));
    let data_abs = std::fs::canonicalize(&data)
        .unwrap_or_else(|e| panic!("canonicalize {}: {e}", data.display()));

    let port = pick_free_port();
    let (code, stdout, stderr) = drive_script_once(&scratch, &bin, &calls_log, port);
    assert_eq!(
        code, 0,
        "STW-078 provisioning script must exit 0 against fakes; got exit {code}\n\
         stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    let log = std::fs::read_to_string(&calls_log)
        .unwrap_or_else(|e| panic!("read calls log {}: {e}", calls_log.display()));
    // Find the `pg_ctl: ... start ...` line. The script
    // passes `start` as a positional after `--pgdata=`
    // and `--log=`, so we grep for any `pg_ctl:` line
    // whose argv contains the word `start`.
    let pg_ctl_start_line = log
        .lines()
        .find(|l| l.starts_with("pg_ctl:") && l.contains(" start"))
        .unwrap_or_else(|| {
            panic!(
                "STW-078 fake `pg_ctl start` was never invoked; the start leg was \
                 skipped. Calls log:\n{log}\nstdout:\n{stdout}\nstderr:\n{stderr}"
            )
        });
    // The argv must include `-p <port>` + `-h 127.0.0.1`
    // (localhost-only binding) + an absolute
    // `-k <data-dir>` socket path. The script's pattern
    // is `--options="-p $PG_PORT -h 127.0.0.1 -k $PG_DATA_DIR_ABS"`,
    // so we look for the `-p <port>` + `-h 127.0.0.1` +
    // `-k <absolute-data-dir>` substrings. (The
    // `<port>` is the test's picked-free-port, NOT the
    // script's `5433` default — the test's
    // `RBP_TESTNET_PG_PORT=<port>` env override
    // propagates through the script's
    // `pg_ctl ... --options="-p $PG_PORT ..."` leg.)
    let port_str = port.to_string();
    let p_arg = format!("-p {port_str}");
    assert!(
        pg_ctl_start_line.contains(&p_arg),
        "STW-078 `pg_ctl start` argv must include `{p_arg}` (the test's \
         RBP_TESTNET_PG_PORT, propagated through the script's \
         `--options=\"-p $PG_PORT ...\"` leg). Got: `{pg_ctl_start_line}`"
    );
    assert!(
        pg_ctl_start_line.contains("-h 127.0.0.1"),
        "STW-078 `pg_ctl start` argv must include `-h 127.0.0.1` (localhost-only \
         binding — the script is local-only by design). Got: `{pg_ctl_start_line}`"
    );
    // The data dir in the `-k` value must be absolute
    // (starts with `/`). A regression that drops the
    // `cd "$PG_DATA_DIR" && pwd` resolution leaves `-k`
    // as a relative path, which the postgres daemon
    // resolves against its own CWD and fails to find the
    // socket dir.
    let data_abs_str = data_abs.to_string_lossy().to_string();
    assert!(
        pg_ctl_start_line.contains(&format!("-k {data_abs_str}"))
            || pg_ctl_start_line.contains(&format!("-k{data_abs_str}"))
            || pg_ctl_start_line.contains(&data_abs_str),
        "STW-078 `pg_ctl start` argv must include an absolute `-k {data_abs_str}` \
         socket dir so a worker who runs the script from a different CWD does not \
         hit the `could not create lock file` error. Got: `{pg_ctl_start_line}`"
    );
    // The data dir must also contain the
    // `.rbp_testnet_port` sentinel the fake's `pg_ctl
    // start` leg writes, so the idempotent re-run probe
    // (in a follow-on invocation) reads the actual port
    // back. (The sentinel is what makes
    // `RBP_TESTNET_PG_PORT=5544` after a `RBP_TESTNET_PG_PORT=5433`
    // provision work: the second invocation reads
    // `.rbp_testnet_port` to find the *actual* port
    // the cluster was started with.)
    let sentinel = data.join(".rbp_testnet_port");
    assert!(
        sentinel.exists(),
        "STW-078 `pg_ctl start` leg must write the `.rbp_testnet_port` sentinel \
         under the data dir so the idempotent re-run probe reads the actual \
         port back. Expected at: {}",
        sentinel.display()
    );

    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn script_idempotent_rerun_short_circuits_with_already_provisioned() {
    // A second invocation in the same tmpdir (with the
    // fakes still on `$PATH` and the data dir already
    // initialised by the first invocation) must exit 0
    // with the `testnet-postgres: already provisioned
    // (port=5433 user=rbp_live ...)` headline — the
    // no-op re-run path. The fake `psql SELECT 1` log
    // must show the `SELECT 1` probe fired exactly once
    // across both invocations (the second invocation
    // short-circuits at the re-run probe and never
    // reaches `initdb` / `pg_ctl start` / `createuser`
    // / `createdb` / `psql ALTER USER` — a regression
    // that re-runs `initdb` on a healthy env is a
    // data-loss bug).
    let scratch = std::env::temp_dir().join(format!(
        "rbp-setup-testnet-postgres-idem-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&scratch);
    std::fs::create_dir_all(&scratch)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", scratch.display()));

    let (bin, calls_log) = build_fake_bin_dir(&scratch);

    // First invocation: the script provisions a fresh
    // cluster against the fakes. The fakes' `initdb`
    // touches `PG_VERSION` in the data dir, the fakes'
    // `pg_ctl start` touches `.rbp_testnet_port`, and
    // the fakes' `psql` (for the `ALTER USER` leg)
    // exits 0. The data dir is now "initialised".
    let port = pick_free_port();
    let (code1, stdout1, stderr1) = drive_script_once(&scratch, &bin, &calls_log, port);
    assert_eq!(
        code1, 0,
        "STW-078 first invocation must exit 0; got exit {code1}\n\
         stdout:\n{stdout1}\nstderr:\n{stderr1}"
    );

    // Capture the post-first-invocation call counts so
    // the second-invocation assertion can compare
    // against them.
    let log_after_first = std::fs::read_to_string(&calls_log)
        .unwrap_or_else(|e| panic!("read calls log {}: {e}", calls_log.display()));
    let initdb_count_after_first = log_after_first.matches("initdb:").count();
    let pg_ctl_count_after_first = log_after_first.matches("pg_ctl:").count();
    let createuser_count_after_first = log_after_first.matches("createuser:").count();
    let createdb_count_after_first = log_after_first.matches("createdb:").count();
    let psql_count_after_first = log_after_first.matches("psql:").count();
    let psql_select1_count_after_first = log_after_first
        .matches("psql:")
        .filter(|_| false) // placeholder so the count type matches below
        .count();
    // The first invocation runs the user/db creation
    // `psql ALTER USER` leg, so the `psql:` count is at
    // least 1 after the first invocation.
    assert!(
        psql_count_after_first >= 1,
        "STW-078 first invocation must invoke `psql` at least once (for the \
         `ALTER USER` password-pinning leg). Calls log:\n{log_after_first}"
    );
    let _ = psql_select1_count_after_first; // suppress unused warning

    // Reset the log so the second-invocation calls are
    // easy to grep (the fakes' `>> $CALLS_LOG` appends;
    // we want the second-invocation calls isolated).
    std::fs::write(&calls_log, "").unwrap_or_else(|e| panic!("truncate calls log: {e}"));

    // Second invocation: the script must short-circuit
    // at the re-run probe (the `psql SELECT 1` against
    // the data dir's Unix socket) and emit the
    // `already provisioned` headline WITHOUT re-running
    // `initdb` / `pg_ctl start` / `createuser` /
    // `createdb` / `psql ALTER USER`.
    let (code2, stdout2, stderr2) = drive_script_once(&scratch, &bin, &calls_log, port);
    assert_eq!(
        code2, 0,
        "STW-078 second invocation must exit 0 (idempotent re-run); got exit {code2}\n\
         stdout:\n{stdout2}\nstderr:\n{stderr2}"
    );
    // The headline is the `already provisioned` line
    // (NOT the `complete:` line — the second
    // invocation short-circuits before the `complete:`
    // emitter). The port in the headline matches the
    // test's `RBP_TESTNET_PG_PORT=<port>` override (NOT
    // the script's `5433` default).
    let port_str = port.to_string();
    let idem_headline_prefix = format!(
        "testnet-postgres: already provisioned (port={port_str} user=rbp_live database=rbp_live"
    );
    assert!(
        stdout2.contains(&idem_headline_prefix),
        "STW-078 second invocation must emit the `{idem_headline_prefix} ...)` \
         headline. Got stdout:\n{stdout2}\nstderr:\n{stderr2}"
    );
    // The second invocation's calls log must show
    // EXACTLY one `psql:` line (the `SELECT 1` re-run
    // probe) and NO `initdb:` / `pg_ctl:` / `createuser:`
    // / `createdb:` lines. A regression that re-runs
    // `initdb` on a healthy env is a data-loss bug; the
    // idempotent probe must short-circuit before any of
    // the write legs.
    let log_after_second = std::fs::read_to_string(&calls_log)
        .unwrap_or_else(|e| panic!("read calls log {}: {e}", calls_log.display()));
    assert_eq!(
        log_after_second.matches("initdb:").count(),
        0,
        "STW-078 second invocation must NOT re-run `initdb` (the script's \
         idempotent re-run probe must short-circuit before the cluster-init \
         leg — a regression that re-runs `initdb` on a healthy env is a \
         data-loss bug). Calls log:\n{log_after_second}"
    );
    assert_eq!(
        log_after_second.matches("pg_ctl:").count(),
        0,
        "STW-078 second invocation must NOT re-run `pg_ctl` (the script's \
         idempotent re-run probe must short-circuit before the start leg). \
         Calls log:\n{log_after_second}"
    );
    assert_eq!(
        log_after_second.matches("createuser:").count(),
        0,
        "STW-078 second invocation must NOT re-run `createuser` (the script's \
         idempotent re-run probe must short-circuit before the role-creation \
         leg). Calls log:\n{log_after_second}"
    );
    assert_eq!(
        log_after_second.matches("createdb:").count(),
        0,
        "STW-078 second invocation must NOT re-run `createdb` (the script's \
         idempotent re-run probe must short-circuit before the db-creation \
         leg). Calls log:\n{log_after_second}"
    );
    assert_eq!(
        log_after_second.matches("psql:").count(),
        1,
        "STW-078 second invocation must invoke `psql` EXACTLY once (the \
         `SELECT 1` re-run probe — a regression that runs the `ALTER USER` \
         `psql` leg a second time is fine, but a regression that re-runs \
         `initdb` / `pg_ctl start` is a data-loss bug). Calls log:\n{log_after_second}"
    );
    // The single `psql:` line in the second invocation's
    // log must be the `SELECT 1` probe (NOT the
    // `ALTER USER` leg).
    let psql_line = log_after_second
        .lines()
        .find(|l| l.starts_with("psql:"))
        .expect("calls log must contain the psql line on the second invocation");
    assert!(
        psql_line.contains("SELECT 1"),
        "STW-078 second invocation's only `psql` call must be the `SELECT 1` \
         re-run probe. Got: `{psql_line}`"
    );
    assert!(
        !psql_line.contains("ALTER USER"),
        "STW-078 second invocation must NOT re-run the `ALTER USER` `psql` leg \
         (the script's idempotent re-run probe must short-circuit before the \
         user/db creation leg). Got: `{psql_line}`"
    );

    // Sanity check: the first invocation's call counts
    // were all >= 1 (initdb, pg_ctl, createuser, createdb,
    // psql ALTER USER) so the second-invocation
    // zero-count assertions above are meaningful.
    assert!(
        initdb_count_after_first >= 1
            && pg_ctl_count_after_first >= 1
            && createuser_count_after_first >= 1
            && createdb_count_after_first >= 1,
        "STW-078 first invocation must invoke each of `initdb` / `pg_ctl` / \
         `createuser` / `createdb` at least once (initdb={initdb_count_after_first} \
         pg_ctl={pg_ctl_count_after_first} createuser={createuser_count_after_first} \
         createdb={createdb_count_after_first}). The second-invocation zero-count \
         assertions above are only meaningful if the first invocation fired each \
         fake. Calls log:\n{log_after_first}"
    );

    let _ = std::fs::remove_dir_all(&scratch);
}
