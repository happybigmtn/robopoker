//! `scripts/plan-staleness-gate.sh` shape contract (STW-022).
//!
//! This integration test pins the *shape* of the STW-022
//! planning-surface gate without requiring a full roadmap
//! edit. It runs in `cargo test --workspace` (no `database`
//! feature gate) so a regression in the gate's surface (file
//! missing, syntax broken, executable bit cleared, claim-map
//! drift, headline format drift, exit-code contract) fails CI
//! before a worker can ship a planning edit that re-introduces
//! a ghost P0 row.
//!
//! The five sub-tests assert the gate's static contract:
//!
//! 1. `script_exists_and_is_executable` — the gate is on disk
//!    and has its executable bit set.
//! 2. `script_parses_with_bash_n` — `bash -n` parses the script
//!    without error (a syntax regression fails the gate at CI).
//! 3. `gate_claim_map_covers_every_ghost_p0_row` — the static
//!    `P0_TO_STW` table inside the script (one row per shipped
//!    P0 duplicate) names every STW id the `steward/DRIFT.md`
//!    GHOST table flags as `genesis:P0-*`. A future refactor
//!    that drops a mapping (e.g. someone deletes the
//!    `P0-smoke|STW-009` line) fails CI before the gate
//!    silently stops checking the P0 smoke path.
//! 4. `gate_headline_format_is_pinned` — the script's stdout
//!    ends with the literal `plan staleness gate complete:
//!    checked=N ghosts=0` prefix a CI dashboard greps, and the
//!    `checked=` / `ghosts=` keys appear in that order.
//! 5. `gate_runs_end_to_end_with_clean_and_ghost_roadmaps` —
//!    the gate is driven against two fabricated roadmaps (one
//!    with the 5 ghost rows + a matching shipped STW in the
//!    plan, one with the 5 ghost rows already retired) and
//!    the exit code + headline format are asserted. This is
//!    the only test in the file that actually executes the
//!    gate; the other four are shape contracts.

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `script_shape.rs` / `workspace_parallel_proof.rs`
/// do. The plan-staleness integration test reads files under
/// `<workspace>/scripts/`, `<workspace>/genesis/plans/`, and
/// `<workspace>/IMPLEMENTATION_PLAN.md`; the helper centralises
/// the path resolution so a future test addition reuses the
/// same convention.
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
        .join("plan-staleness-gate.sh")
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn script_exists_and_is_executable() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-022 gate script missing at {}; \
         the plan-vs-reality staleness gate has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        assert!(
            mode & 0o100 != 0,
            "STW-022 gate script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
}

#[test]
fn script_parses_with_bash_n() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-022 gate script missing at {} (cannot bash -n a missing file)",
        p.display()
    );
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/plan-staleness-gate.sh");
    assert!(
        out.status.success(),
        "STW-022 gate script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn gate_claim_map_covers_every_ghost_p0_row() {
    // Every STW id the `steward/DRIFT.md` GHOST table flags as
    // duplicating a [P0] row in the CEO testnet roadmap must
    // also be present in the gate's static `P0_TO_STW` claim
    // map. The map uses `|` separators and a heredoc-style
    // string, so the assertion is a simple `contains` for each
    // expected STW id. A future refactor that drops a mapping
    // (e.g. someone edits the heredoc to remove STW-009) fails
    // CI before the gate silently stops checking the P0 smoke
    // path.
    let script = read(&script_path());
    // The five STW ids the GHOST table attributes to ghost
    // P0 rows. Mirrors `steward/DRIFT.md` rows `genesis:P0-schema`
    // (STW-006), `genesis:P0-hand-roundtrip` (STW-008),
    // `genesis:P0-smoke` (STW-009), `genesis:P0-bench`
    // (STW-010), `genesis:P0-auth` (STW-004).
    let required_stw_ids = [
        ("STW-006", "P0-schema"),
        ("STW-008", "P0-hand-roundtrip"),
        ("STW-009", "P0-smoke"),
        ("STW-010", "P0-bench"),
        ("STW-004", "P0-auth"),
    ];
    let mut missing: Vec<&str> = Vec::new();
    for (stw, p0_token) in &required_stw_ids {
        if !script.contains(stw) {
            missing.push(p0_token);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-022 gate script at {} must reference every ghost-P0 STW id \
         (mirrors steward/DRIFT.md GHOST table). Missing mappings: {missing:?}",
        script_path().display()
    );
}

#[test]
fn gate_headline_format_is_pinned() {
    // The headline line a CI dashboard greps. The script writes
    // `plan staleness gate complete: checked=N ghosts=M` to
    // stdout on every run; both `checked=` and `ghosts=` keys
    // must appear, in that order, so a regex dashboard
    // extraction stays stable.
    let script = read(&script_path());
    assert!(
        script.contains("plan staleness gate complete: checked="),
        "STW-022 gate must print a `plan staleness gate complete: checked=...` \
         headline line; the dashboard scraper relies on this exact prefix"
    );
    let required_pairs = ["checked=${checked}", "ghosts=${ghosts}"];
    let mut last_idx = 0usize;
    for pair in &required_pairs {
        let idx = script.find(pair).unwrap_or_else(|| {
            panic!(
                "STW-022 gate headline printf string must include `{pair}`; \
                 a dashboard scraper relies on every key=N pair being present"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-022 gate headline key=N pairs must appear in order \
             checked, ghosts (got `{pair}` before its predecessor)"
        );
        last_idx = idx;
    }
    // The exit-code contract: ghost=0 -> exit 0; ghost>0 -> exit 3.
    // Pin the literal `exit 3` and `exit 0` tokens so a refactor
    // that silently changes the failure exit code fails CI.
    assert!(
        script.contains("exit 3"),
        "STW-022 gate must exit 3 on a GHOST row (CI dashboards scrape the exit code)"
    );
    assert!(
        script.contains("exit 0"),
        "STW-022 gate must exit 0 on a clean run (CI dashboards scrape the exit code)"
    );
}

#[test]
fn gate_runs_end_to_end_with_clean_and_ghost_roadmaps() {
    // The only test in the file that actually executes the
    // gate. Builds two fabricated planning surfaces in a
    // tempdir:
    //
    //   - ghost roadmap  : contains the five [P0] rows the
    //                      `steward/DRIFT.md` GHOST table names
    //   - ghost plan     : marks all five STW rows as
    //                      `- [x] `STW-NNN`` (shipped)
    //   - clean roadmap  : same five [P0] rows BUT all marked
    //                      `- [x]` (the post-retirement state)
    //
    // Drives the gate against each pair and asserts:
    //
    //   - ghost run  -> exit 3, headline `ghosts=5`
    //   - clean run  -> exit 0, headline `ghosts=0`
    //
    // The gate is fed via the `RBP_PLAN_STALENESS_ROADMAP` /
    // `RBP_PLAN_STALENESS_PLAN` knobs, so the real planning
    // files are never touched.
    let scratch = std::env::temp_dir().join(format!("rbp-plan-staleness-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", scratch.display()));

    let ghost_roadmap = scratch.join("ghost-roadmap.md");
    let ghost_plan = scratch.join("ghost-plan.md");
    let clean_roadmap = scratch.join("clean-roadmap.md");
    let clean_plan = scratch.join("clean-plan.md");

    // Ghost roadmap: the five unchecked [P0] rows the current
    // `genesis/plans/000-ceo-testnet-roadmap.md` ships with.
    std::fs::write(
        &ghost_roadmap,
        "# fabricated ghost roadmap\n\n\
         ## Immediate P0 — testnet proof points (dispatch now)\n\
         - [ ] [P0] Implement the `Schema` (copy/columns/indices/freeze) contracts.\n\
         - [ ] [P0] Add an end-to-end test in `crates/gameroom` that plays a full hand.\n\
         - [ ] [P0] Implement a `trainer` smoke path with `--fast` config.\n\
         - [ ] [P0] Build a `bin/bench` (or `trainer --bench`) head-to-head harness.\n\
         - [ ] [P0] Land STW-004 auth hardening for `Crypto::from_env`.\n",
    )
    .expect("write ghost roadmap");

    // Ghost plan: all five STWs are marked shipped.
    std::fs::write(
        &ghost_plan,
        "# fabricated plan\n\n\
         - [x] `STW-006` shipped.\n\
         - [x] `STW-008` shipped.\n\
         - [x] `STW-009` shipped.\n\
         - [x] `STW-010` shipped.\n\
         - [x] `STW-004` shipped.\n",
    )
    .expect("write ghost plan");

    // Clean roadmap: same five rows but flipped to [x] (the
    // post-retirement state the STW-022 retirement lands in
    // `genesis/plans/000-ceo-testnet-roadmap.md`).
    std::fs::write(
        &clean_roadmap,
        "# fabricated clean roadmap\n\n\
         ## Immediate P0 — testnet proof points (dispatch now)\n\
         - [x] [P0] Implement the `Schema` (copy/columns/indices/freeze) contracts.\n\
         - [x] [P0] Add an end-to-end test in `crates/gameroom` that plays a full hand.\n\
         - [x] [P0] Implement a `trainer` smoke path with `--fast` config.\n\
         - [x] [P0] Build a `bin/bench` (or `trainer --bench`) head-to-head harness.\n\
         - [x] [P0] Land STW-004 auth hardening for `Crypto::from_env`.\n",
    )
    .expect("write clean roadmap");

    // Clean plan: same shipped markers (the gate doesn't care
    // whether the plan is shipped, only whether the [P0] row
    // is still unchecked in the roadmap).
    std::fs::write(
        &clean_plan,
        "# fabricated plan\n\n\
         - [x] `STW-006` shipped.\n\
         - [x] `STW-008` shipped.\n\
         - [x] `STW-009` shipped.\n\
         - [x] `STW-010` shipped.\n\
         - [x] `STW-004` shipped.\n",
    )
    .expect("write clean plan");

    let gate = script_path();
    assert!(
        gate.exists(),
        "STW-022 gate script missing at {} (cannot drive it from this test)",
        gate.display()
    );

    // Ghost run: the gate must exit 3 and report ghosts=5.
    let ghost_out = std::process::Command::new("bash")
        .arg(&gate)
        .env("RBP_PLAN_STALENESS_ROADMAP", &ghost_roadmap)
        .env("RBP_PLAN_STALENESS_PLAN", &ghost_plan)
        .env("RBP_PLAN_STALENESS_QUIET", "1")
        .output()
        .expect("spawn bash scripts/plan-staleness-gate.sh (ghost)");
    let ghost_stdout = String::from_utf8_lossy(&ghost_out.stdout);
    let ghost_stderr = String::from_utf8_lossy(&ghost_out.stderr);
    assert_eq!(
        ghost_out.status.code(),
        Some(3),
        "STW-022 gate must exit 3 on a fabricated ghost roadmap \
         (5 unchecked [P0] rows duplicating 5 shipped STWs)\n\
         --- stdout ---\n{ghost_stdout}\n--- stderr ---\n{ghost_stderr}"
    );
    assert!(
        ghost_stdout.contains("plan staleness gate complete: checked=5 ghosts=5"),
        "STW-022 gate must report `ghosts=5` on the fabricated ghost roadmap; \
         got stdout:\n{ghost_stdout}\nstderr:\n{ghost_stderr}"
    );
    // The stderr must name every ghost row so a worker can
    // jump straight to the offender.
    for stw in ["STW-006", "STW-008", "STW-009", "STW-010", "STW-004"] {
        assert!(
            ghost_stderr.contains(stw),
            "STW-022 gate stderr must name the ghosted STW {stw} on failure; \
             got stderr:\n{ghost_stderr}"
        );
    }

    // Clean run: the gate must exit 0 and report ghosts=0.
    let clean_out = std::process::Command::new("bash")
        .arg(&gate)
        .env("RBP_PLAN_STALENESS_ROADMAP", &clean_roadmap)
        .env("RBP_PLAN_STALENESS_PLAN", &clean_plan)
        .env("RBP_PLAN_STALENESS_QUIET", "1")
        .output()
        .expect("spawn bash scripts/plan-staleness-gate.sh (clean)");
    let clean_stdout = String::from_utf8_lossy(&clean_out.stdout);
    let clean_stderr = String::from_utf8_lossy(&clean_out.stderr);
    assert_eq!(
        clean_out.status.code(),
        Some(0),
        "STW-022 gate must exit 0 on a fabricated clean roadmap \
         (5 [x] [P0] rows, no ghost duplication)\n\
         --- stdout ---\n{clean_stdout}\n--- stderr ---\n{clean_stderr}"
    );
    assert!(
        clean_stdout.contains("plan staleness gate complete: checked=0 ghosts=0"),
        "STW-022 gate must report `ghosts=0` on the fabricated clean roadmap; \
         (the clean roadmap has no `[ ] [P0]` rows so the headline shows \
         `checked=0`; the script's early-exit path is the correct green signal). \
         Got stdout:\n{clean_stdout}\nstderr:\n{clean_stderr}"
    );

    // Cleanup the tempdir.
    let _ = std::fs::remove_dir_all(&scratch);
}

#[test]
fn plan_staleness_gate_catches_p1_ghosts() {
    // STW-065: the gate's new P1 pass catches unchecked [P1]
    // rows in both the roadmap and the plan that duplicate a
    // shipped STW. A P1 row is *not* a ghost when it carries
    // the `RESCOPED 2026-06-05` marker.
    let scratch =
        std::env::temp_dir().join(format!("rbp-plan-staleness-p1-{}", std::process::id()));
    std::fs::create_dir_all(&scratch)
        .unwrap_or_else(|e| panic!("mkdir {}: {e}", scratch.display()));

    let roadmap = scratch.join("roadmap.md");
    let plan = scratch.join("plan.md");

    // Fabricated roadmap: one unchecked [P1] row with a STW id.
    std::fs::write(
        &roadmap,
        "# fabricated roadmap\n\n\
         - [ ] [P1] `STW-099` A ghost P1 row in the roadmap.\n",
    )
    .expect("write roadmap");

    // Fabricated plan: the same STW is marked shipped, so the
    // [P1] row in the roadmap is a ghost.
    std::fs::write(
        &plan,
        "# fabricated plan\n\n\
         - [x] `STW-099` shipped.\n\
         - [ ] [P1] `STW-099` A ghost P1 row in the plan too.\n",
    )
    .expect("write plan");

    let gate = script_path();
    assert!(
        gate.exists(),
        "STW-065 gate script missing at {} (cannot drive P1 test)",
        gate.display()
    );

    // Ghost run: the gate must exit 3 because both the roadmap
    // and the plan have unchecked [P1] rows for a shipped STW.
    let out = std::process::Command::new("bash")
        .arg(&gate)
        .env("RBP_PLAN_STALENESS_ROADMAP", &roadmap)
        .env("RBP_PLAN_STALENESS_PLAN", &plan)
        .env("RBP_PLAN_STALENESS_QUIET", "1")
        .output()
        .expect("spawn bash scripts/plan-staleness-gate.sh (p1 ghost)");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert_eq!(
        out.status.code(),
        Some(3),
        "STW-065 P1 pass must exit 3 on a fabricated P1 ghost \
         (unchecked [P1] rows duplicating a shipped STW)\n\
         --- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
    );
    assert!(
        stdout.contains("plan staleness gate complete: checked=2 ghosts=2"),
        "STW-065 P1 pass must report `ghosts=2` (one ghost in roadmap + one in plan); \
         got stdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("STW-099"),
        "STW-065 P1 pass stderr must name the ghosted STW-099 on failure; \
         got stderr:\n{stderr}"
    );

    // RESCOPED run: the same plan but with a RESCOPED marker
    // on the P1 rows — the gate must exit 0.
    std::fs::write(
        &roadmap,
        "# fabricated roadmap\n\n\
         - [ ] [P1] `STW-099` A ghost P1 row RESCOPED 2026-06-05.\n",
    )
    .expect("write rescoped roadmap");
    std::fs::write(
        &plan,
        "# fabricated plan\n\n\
         - [x] `STW-099` shipped.\n\
         - [ ] [P1] `STW-099` A ghost P1 row RESCOPED 2026-06-05.\n",
    )
    .expect("write rescoped plan");

    let rescoped_out = std::process::Command::new("bash")
        .arg(&gate)
        .env("RBP_PLAN_STALENESS_ROADMAP", &roadmap)
        .env("RBP_PLAN_STALENESS_PLAN", &plan)
        .env("RBP_PLAN_STALENESS_QUIET", "1")
        .output()
        .expect("spawn bash scripts/plan-staleness-gate.sh (p1 rescoped)");
    let rescoped_stdout = String::from_utf8_lossy(&rescoped_out.stdout);
    let rescoped_stderr = String::from_utf8_lossy(&rescoped_out.stderr);
    assert_eq!(
        rescoped_out.status.code(),
        Some(0),
        "STW-065 P1 pass must exit 0 when the P1 rows carry a RESCOPED 2026-06-05 marker\n\
         --- stdout ---\n{rescoped_stdout}\n--- stderr ---\n{rescoped_stderr}"
    );
    assert!(
        rescoped_stdout.contains("plan staleness gate complete: checked=2 ghosts=0"),
        "STW-065 P1 pass must report `ghosts=0` when RESCOPED; \
         got stdout:\n{rescoped_stdout}\nstderr:\n{rescoped_stderr}"
    );

    // Cleanup the tempdir.
    let _ = std::fs::remove_dir_all(&scratch);
}
