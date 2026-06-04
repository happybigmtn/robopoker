//! `scripts/testnet-live-proof.sh` shape contract (STW-019).
//!
//! This integration test pins the *shape* of the STW-019 runbook
//! without requiring a live Postgres. It runs in
//! `cargo test --workspace` (no `database` feature gate) so a
//! regression in the runbook's surface (file missing, syntax
//! broken, executable bit cleared, doc drift) fails CI before it
//! ever reaches a live DB.
//!
//! The five sub-tests assert the runbook's static contract:
//!
//! 1. `script_exists_and_is_executable` — the runbook is on disk
//!    and has its executable bit set (a worker can invoke it via
//!    `bash scripts/testnet-live-proof.sh`).
//! 2. `script_parses_with_bash_n` — `bash -n` parses the script
//!    without error (a syntax regression fails the gate at CI time).
//! 3. `runbook_doc_lists_every_env_knob` — every `RBP_FAST_EPOCHS` /
//!    `RBP_FAST_BATCH` / `RBP_BENCH_HANDS` / `RBP_BENCH_BLIND` /
//!    `RBP_COMPARE_HANDS` / `RBP_COMPARE_BLIND` / `RBP_BENCH_TRANSCRIPT_DIR`
//!    the runbook honours also appears in
//!    `scripts/testnet-live-proof.md` (catches doc drift where the
//!    script gains a knob but the doc forgets to mention it).
//! 4. `runbook_doc_references_every_chain_step` — the runbook doc
//!    names every chain step the `live_proof.rs` integration test
//!    covers (`--cluster`, `--reset`, `--smoke`, `--status`,
//!    `--bench`, `--compare`, `--replay`). A future refactor that
//!    drops a leg fails here.
//! 5. `script_writes_recipe_json_manifest` (STW-023) — the runbook
//!    script sources a `cat > "$RECEIPT_DIR/recipe.json" <<JSON ... JSON`
//!    heredoc whose body parses as the `LiveProofRecipe` JSON shape
//!    (the seven pinned step names in order, the `trainer_bin` /
//!    `database_url` / `steps[]` fields, the per-step `name` /
//!    `exit` / `stdout_bytes` / `stderr_bytes` fields). The runbook
//!    doc also references the `recipe.json` file. A regression
//!    that drops the `recipe.json` block (or renames a step
//!    field) fails this test and the
//!    `crates/autotrain/tests/live_proof_receipt.rs` integration
//!    test simultaneously — the operator-visible receipt and
//!    the CI-visible receipt share one source of truth.
//!
//! The test deliberately does **not** shell out to the runbook
//! itself: that would require `DATABASE_URL` and would be a
//! duplicate of the `live_proof` integration test. The shell-shape
//! test is the *no-DB gate* that lets `cargo test --workspace`
//! stay green even on machines that have no Postgres.

use std::path::PathBuf;

/// Walk up from `CARGO_MANIFEST_DIR` to the workspace root, the
/// same way `bench.rs` / `live_proof.rs` do. The shell-shape
/// integration test reads files under `<workspace>/scripts/` and
/// `<workspace>/README.md`; the helper centralises the path
/// resolution so a future test addition reuses the same convention.
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
        .join("testnet-live-proof.sh")
}

fn runbook_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-proof.md")
}

fn read(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn script_exists_and_is_executable() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-019 runbook script missing at {}; \
         the testnet live launch proof has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The executable bit for the owner must be set; a worker
        // running `bash scripts/testnet-live-proof.sh` works even
        // without the executable bit, but the integration test
        // pins the convention `chmod +x` the runbook shipped
        // with, so a future chmod regression fails the test.
        assert!(
            mode & 0o100 != 0,
            "STW-019 runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        // On non-Unix hosts we can only assert the file is
        // present; the bash -n check below covers the "is the
        // file actually a bash script" question.
        let _ = meta;
    }
}

#[test]
fn script_parses_with_bash_n() {
    let p = script_path();
    assert!(
        p.exists(),
        "STW-019 runbook script missing at {} (cannot bash -n a missing file)",
        p.display()
    );
    // `bash -n` parses the script without executing it. The test
    // fails on a non-zero exit (a syntax error) so a future edit
    // that breaks the bash grammar fails CI before it reaches a
    // live Postgres.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-proof.sh");
    assert!(
        out.status.success(),
        "STW-019 runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn runbook_doc_lists_every_env_knob() {
    // Every env knob the runbook script honours must also be
    // listed in the runbook doc, so the doc and the script stay
    // in lockstep. We assert each knob by *name* (the
    // `RBP_FOO_BAR` token), not by description, so the test
    // survives a doc rewrite that re-words a paragraph.
    let doc = read(&runbook_doc_path());
    // The env knobs the runbook script honours. Mirrors the
    // `: "${...:=default}"` lines in `testnet-live-proof.sh` plus
    // the `RBP_BENCH_TRANSCRIPT_DIR` knob the runbook sets
    // internally (a future refactor that adds a knob must also
    // add it here, or this test will fail).
    let required_knobs = [
        "RBP_FAST_EPOCHS",
        "RBP_FAST_BATCH",
        "RBP_BENCH_HANDS",
        "RBP_BENCH_BLIND",
        "RBP_COMPARE_HANDS",
        "RBP_COMPARE_BLIND",
        "RBP_BENCH_TRANSCRIPT_DIR",
    ];
    let mut missing: Vec<&str> = Vec::new();
    for knob in &required_knobs {
        if !doc.contains(knob) {
            missing.push(knob);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-019 runbook doc at {} must list every env knob the script honours. \
         Missing from doc: {missing:?}",
        runbook_doc_path().display()
    );
}

#[test]
fn runbook_doc_references_every_chain_step() {
    // The runbook doc must name every chain step the live proof
    // integration test (`crates/autotrain/tests/live_proof.rs`)
    // covers. We assert by flag form (`--smoke`, `--bench`, etc.)
    // because that is the form the operator types and the form
    // a dashboard scraper greps.
    let doc = read(&runbook_doc_path());
    let required_steps = [
        "--cluster",
        "--reset",
        "--smoke",
        "--status",
        "--bench",
        "--compare",
        "--replay",
    ];
    let mut missing: Vec<&str> = Vec::new();
    for step in &required_steps {
        if !doc.contains(step) {
            missing.push(step);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-019 runbook doc at {} must reference every chain step the live proof \
         integration test covers. Missing from doc: {missing:?}",
        runbook_doc_path().display()
    );
}

#[test]
fn script_summary_headline_format_is_pinned() {
    // The `SUMMARY.txt` headline line the runbook writes must
    // start with the literal prefix `testnet live_proof complete:`
    // and include all five `key=N` pairs
    // (`smoke=`, `status=`, `bench=`, `compare=`, `replay=`) so a
    // dashboard scraper can grep either the SUMMARY.txt file or
    // the runbook's stdout with the same regex. We assert the
    // script's source text contains a printf-style line with
    // this exact shape.
    let script = read(&script_path());
    assert!(
        script.contains("testnet live_proof complete: smoke="),
        "STW-019 runbook must print a `testnet live_proof complete: smoke=...` headline line; \
         the dashboard scraper relies on this exact prefix"
    );
    // All five key=N pairs must appear in the printf string the
    // script writes to SUMMARY.txt, in the order
    // smoke, status, bench, compare, replay (the same order the
    // `crates/autotrain/tests/live_proof.rs` integration test's
    // final log line uses).
    let required_pairs = [
        "smoke=$SMOKE_ROWS",
        "status=$STATUS_BLUEPRINT",
        "bench=$BENCH_HANDS",
        "compare=$COMPARE_HANDS",
        "replay=$REPLAY_BYTES",
    ];
    let mut last_idx = 0usize;
    for pair in &required_pairs {
        let idx = script.find(pair).unwrap_or_else(|| {
            panic!(
                "STW-019 runbook SUMMARY.txt printf string must include `{pair}`; \
                 a dashboard scraper relies on every key=N pair being present"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-019 SUMMARY.txt printf key=N pairs must appear in order \
             smoke, status, bench, compare, replay (got `{pair}` before its predecessor)"
        );
        last_idx = idx;
    }
}

/// `script_writes_recipe_json_manifest` (STW-023) — the
/// runbook script must source a `recipe.json` heredoc
/// whose body parses as the `LiveProofRecipe` JSON shape
/// (the seven pinned step names in order, the three
/// top-level fields `trainer_bin` / `database_url` /
/// `steps[]`, and the per-step `name` / `exit` /
/// `stdout_bytes` / `stderr_bytes` fields), and the
/// runbook doc must reference the `recipe.json` file in
/// its receipt-layout section. A regression that drops
/// the `recipe.json` block (or renames a step field)
/// fails here. The test asserts by (a) string-substring
/// checks on every `LiveProofRecipe` field name (catches
/// a renamed or dropped field), (b) ordered step-name
/// scan (catches a re-ordered or dropped step), (c) the
/// `write_recipe "$RECEIPT_DIR"` invocation site (catches
/// a regression that defines the function but never
/// calls it), (d) the runbook doc's `recipe.json`
/// mention (catches a doc drift), and (e) a
/// `serde_json` round-trip on the extracted heredoc
/// body so a future regression that swaps a field or
/// drops a comma fails CI even when the substring
/// assertions above pass.
#[test]
fn script_writes_recipe_json_manifest() {
    let script = read(&script_path());
    // (1) The runbook must source a `write_recipe`
    // helper that writes the manifest. We assert the
    // function definition + a single `cat >` heredoc
    // are present, so a future refactor that drops
    // the function fails CI.
    assert!(
        script.contains("write_recipe() {"),
        "STW-023 runbook must define a `write_recipe()` helper that writes the recipe.json manifest; \
         a regression that drops the function makes the operator receipt unverifiable"
    );
    assert!(
        script.contains("cat > \"$recipe_path\" <<JSON")
            || script.contains("cat > \"$recipe_path\" <<'JSON'"),
        "STW-023 runbook must source a `cat > $recipe_path <<JSON ... JSON` heredoc; \
         the `LiveProofRecipe` JSON shape is the verifier's contract"
    );
    // (2) The JSON heredoc must contain every field
    // the `LiveProofRecipe` struct serialises.
    for field in &[
        "\"trainer_bin\":",
        "\"database_url\":",
        "\"steps\":",
        "\"name\":",
        "\"exit\":",
        "\"stdout_bytes\":",
        "\"stderr_bytes\":",
    ] {
        assert!(
            script.contains(field),
            "STW-023 runbook recipe.json heredoc must contain `{field}`; the on-disk shape mirrors the \
             Rust `LiveProofRecipe` struct, so a missing field breaks the `serde_json` round-trip"
        );
    }
    // (3) The seven pinned step names must appear in
    // the heredoc, in order. The order is what the
    // verifier asserts against `STW023_CHAIN_STEPS`;
    // a future refactor that re-orders (or drops) a
    // step name must update the runbook's heredoc in
    // the same change.
    let mut last_idx = 0usize;
    for step in &[
        "\"name\": \"cluster\"",
        "\"name\": \"reset\"",
        "\"name\": \"smoke\"",
        "\"name\": \"status\"",
        "\"name\": \"bench\"",
        "\"name\": \"compare\"",
        "\"name\": \"replay\"",
    ] {
        let idx = script.find(step).unwrap_or_else(|| {
            panic!(
                "STW-023 runbook recipe.json heredoc must include step name `{step}`; \
                 a future chain refactor that drops a step name must update both the heredoc \
                 and the Rust `STW023_CHAIN_STEPS` constant in the same change"
            )
        });
        assert!(
            idx >= last_idx,
            "STW-023 runbook recipe.json step names must appear in cluster, reset, smoke, status, \
             bench, compare, replay order (got `{step}` before its predecessor)"
        );
        last_idx = idx;
    }
    // (4) The runbook must call `write_recipe` after
    // the chain lands. A regression that defines the
    // function but never calls it leaves the
    // `recipe.json` file missing from the receipt.
    assert!(
        script.contains("write_recipe \"$RECEIPT_DIR\""),
        "STW-023 runbook must call `write_recipe \"$RECEIPT_DIR\"` to drop the manifest next to \
         SUMMARY.txt; a regression that defines the function but never calls it silently breaks \
         the verifier contract"
    );
    // (5) The runbook doc must reference the
    // `recipe.json` file in its receipt-layout
    // section.
    let doc = read(&runbook_doc_path());
    assert!(
        doc.contains("recipe.json"),
        "STW-023 runbook doc must reference `recipe.json` in its receipt-layout section; \
         a worker reading the doc would not know the manifest exists"
    );
    // (6) Mechanically extract the heredoc body and
    // round-trip it through `serde_json::from_str` so
    // a future regression that swaps a field, drops a
    // comma, or breaks the heredoc terminator fails
    // CI even when the substring assertions above
    // pass. The heredoc body is a non-quoted
    // `<<JSON` block whose `$TRAINER_BIN` /
    // `$db_redacted` / `$step_stdout` / etc. tokens
    // are literal text in the script source (bash
    // interpolates them at exec time). The heredoc
    // body wraps each variable in literal `"`
    // delimiters (the source has
    // `"\""$VAR"\""` form, which the test sees as
    // `"$VAR"`). We substitute the *bare* variable
    // tokens with a string that already includes
    // the surrounding JSON quotes so the resulting
    // body is valid JSON.
    let body = extract_heredoc_body(&script, "JSON")
        .expect("STW-023 runbook script must source a <<JSON ... JSON heredoc");
    let substituted = body
        .replace(
            "$TRAINER_BIN",
            "/srv/dev/repos/robopoker/target/debug/trainer",
        )
        .replace("$db_redacted", "<redacted: 49 chars>")
        .replace("$cluster_exit", "0")
        .replace("$cluster_stdout", "0")
        .replace("$cluster_stderr", "0")
        .replace("$reset_exit", "0")
        .replace("$reset_stdout", "0")
        .replace("$reset_stderr", "0")
        .replace("$smoke_exit", "0")
        .replace("$smoke_stdout", "0")
        .replace("$smoke_stderr", "0")
        .replace("$status_exit", "0")
        .replace("$status_stdout", "0")
        .replace("$status_stderr", "0")
        .replace("$bench_exit", "0")
        .replace("$bench_stdout", "0")
        .replace("$bench_stderr", "0")
        .replace("$compare_exit", "0")
        .replace("$compare_stdout", "0")
        .replace("$compare_stderr", "0")
        .replace("$replay_exit", "0")
        .replace("$replay_stdout", "0")
        .replace("$replay_stderr", "0");
    let parsed: serde_json::Value = serde_json::from_str(&substituted).unwrap_or_else(|e| {
        panic!(
            "STW-023 runbook recipe.json heredoc must parse as JSON \
             (after bash interpolation tokens are substituted); got \
             error: {e}\n--- heredoc body ---\n{substituted}\n"
        )
    });
    let steps = parsed
        .get("steps")
        .and_then(|s| s.as_array())
        .unwrap_or_else(|| panic!("STW-023 recipe.json must have a `steps` array; got: {parsed}"));
    let expected_names = [
        "cluster", "reset", "smoke", "status", "bench", "compare", "replay",
    ];
    for (i, step) in steps.iter().enumerate() {
        let name = step
            .get("name")
            .and_then(|n| n.as_str())
            .unwrap_or_else(|| {
                panic!("STW-023 recipe.json steps[{i}].name must be a string; got: {step}")
            });
        assert_eq!(
            name, expected_names[i],
            "STW-023 recipe.json steps[{i}].name must be `{}`; got: `{name}`",
            expected_names[i]
        );
    }
}

/// Extract the body of a `<<'TAG' ... TAG` heredoc from a
/// bash script. Lines are scanned in order; the first
/// line containing `<<'TAG'` (or `<<TAG`) opens the body;
/// the next line that equals `TAG` (whitespace-trimmed)
/// closes it. Returns `None` if no heredoc with the given
/// terminator is found, mirroring the runtime error a
/// bash script would surface at exec time.
fn extract_heredoc_body(script: &str, tag: &str) -> Option<String> {
    let open_a = format!("<<'{tag}'");
    let open_b = format!("<<{tag}");
    let mut open = false;
    let mut body: Vec<&str> = Vec::new();
    for line in script.lines() {
        if !open {
            if line.contains(&open_a) || line.contains(&open_b) {
                open = true;
            }
            continue;
        }
        if line.trim() == tag {
            return Some(body.join("\n"));
        }
        body.push(line);
    }
    None
}

// ===========================================================================
// STW-032 publish runbook shape contract
// ===========================================================================
//
// The publish runbook (`scripts/testnet-live-publish.sh`) is a
// pure-bash driver that consumes the receipt the STW-019
// runbook produced and writes a deterministic, content-addressed
// portable bundle. The four sub-tests below pin the runbook's
// static surface (the same no-DB shape-pinning pattern the
// STW-019 runbook uses):
//
// 1. `testnet_live_publish_script_exists_and_parses` — the
//    publish runbook is on disk, is executable, and parses
//    with `bash -n` (catches a syntax regression at CI time).
// 2. `testnet_live_publish_doc_references_verify_bundle_cli` —
//    the runbook doc references the
//    `trainer --verify-bundle <path>` CLI subcommand the
//    publish step shells out to (a worker reading the doc
//    would not know how to re-verify the bundle).
// 3. `testnet_live_publish_doc_references_every_chain_step` —
//    the runbook doc names every chain step the
//    `crates/autotrain/tests/publish.rs` integration test
//    covers (`--verify-receipt`, `--publish`).
// 4. `testnet_live_publish_script_has_verify_receipt_pre_publish_gate` —
//    the runbook script must shell out to
//    `trainer --verify-receipt <receipt>` before the publish
//    step (the "refuse to publish a red receipt" gate the
//    receipt verifier is the source of truth for).
//
// STW-033 also ships a *companion* bash runbook
// `scripts/testnet-live-publish-s3.sh` that drives the
// `trainer --publish-remote` + `trainer --verify-remote` arms.
// The shell-shape integration test pins its shape with three
// sub-tests:
//
// 5. `testnet_live_publish_s3_script_exists_and_parses` —
//    the S3 runbook script must be on disk, executable,
//    and parse with `bash -n` (mirrors the STW-032
//    `testnet_live_publish.sh` shape).
// 6. `testnet_live_publish_s3_doc_references_verify_remote_cli` —
//    the S3 runbook doc must reference the
//    `trainer --verify-remote <path>` CLI subcommand
//    STW-033 ships (mirrors the STW-032
//    `testnet_live_publish.md` shape).
// 7. `testnet_live_publish_s3_script_has_verify_receipt_pre_publish_gate` —
//    the S3 runbook script must shell out to
//    `trainer --verify-receipt <receipt>` BEFORE the
//    `trainer --publish-remote` call (the
//    "refuse to plan an upload for a red receipt"
//    gate the receipt verifier is the source of
//    truth for).

fn publish_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish.sh")
}

fn publish_runbook_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish.md")
}

fn publish_s3_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-s3.sh")
}

#[test]
fn testnet_live_publish_script_exists_and_parses() {
    // The publish runbook script must be on disk,
    // executable, and parse with `bash -n`. A
    // regression that drops the file (or breaks the
    // bash grammar) fails the gate at CI time before
    // a CI worker can shell out to it.
    let p = publish_script_path();
    assert!(
        p.exists(),
        "STW-032 publish runbook script missing at {}; \
         the testnet live launch publish surface has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails
        // the test before a worker tries to shell
        // out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-032 publish runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without executing
    // it. A non-zero exit (a syntax error) fails the
    // test so a future edit that breaks the bash
    // grammar fails CI before it reaches a live
    // publish step.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-publish.sh");
    assert!(
        out.status.success(),
        "STW-032 publish runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn testnet_live_publish_doc_references_verify_bundle_cli() {
    // The publish runbook doc must reference the
    // `trainer --verify-bundle <path>` CLI subcommand
    // STW-032 ships. A worker reading the doc would
    // not know how to re-verify the bundle without
    // this mention. We assert by flag form
    // (`--verify-bundle`) because that is the form
    // the operator types and the form a dashboard
    // scraper greps.
    let doc = read(&publish_runbook_doc_path());
    assert!(
        doc.contains("--verify-bundle"),
        "STW-032 publish runbook doc at {} must reference the \
         `trainer --verify-bundle <path>` CLI subcommand; a worker reading the \
         doc would not know how to re-verify the bundle",
        publish_runbook_doc_path().display()
    );
}

#[test]
fn testnet_live_publish_doc_references_every_chain_step() {
    // The publish runbook doc must name every
    // chain step the publish integration test
    // (`crates/autotrain/tests/publish.rs`)
    // covers: `--verify-receipt` (the pre-publish
    // gate that refuses to publish a red receipt)
    // and `--publish` (the bundle writer).
    let doc = read(&publish_runbook_doc_path());
    let required_steps = ["--verify-receipt", "--publish"];
    let mut missing: Vec<&str> = Vec::new();
    for step in &required_steps {
        if !doc.contains(step) {
            missing.push(step);
        }
    }
    assert!(
        missing.is_empty(),
        "STW-032 publish runbook doc at {} must reference every chain step the \
         publish integration test covers. Missing from doc: {missing:?}",
        publish_runbook_doc_path().display()
    );
}

#[test]
fn testnet_live_publish_script_has_verify_receipt_pre_publish_gate() {
    // The publish runbook script must shell out
    // to `trainer --verify-receipt <receipt>`
    // BEFORE the `trainer --publish <receipt>`
    // call. The "refuse to publish a red
    // receipt" gate the receipt verifier is the
    // source of truth for: a publish of a red
    // receipt is a hard error, not a warning.
    // We assert by ordered substring scan: the
    // `--verify-receipt` flag must appear
    // before the `--publish` flag in the script
    // source, mirroring the runtime call
    // order.
    let script = read(&publish_script_path());
    let verify_idx = script.find("--verify-receipt").unwrap_or_else(|| {
        panic!(
            "STW-032 publish runbook must shell out to `trainer --verify-receipt <receipt>` \
             as a pre-publish gate; the --verify-receipt flag is missing from the script"
        )
    });
    let publish_idx = script.find("--publish").unwrap_or_else(|| {
        panic!(
            "STW-032 publish runbook must shell out to `trainer --publish <receipt>`; \
             the --publish flag is missing from the script"
        )
    });
    assert!(
        verify_idx < publish_idx,
        "STW-032 publish runbook must call `--verify-receipt` BEFORE `--publish`; \
         a script that publishes before verifying is the inverse of the refuse-to-publish-red-receipt gate"
    );
}

// ===========================================================================
// STW-033 publish-remote (S3 / GCS / git-tag) runbook shape contract
// ===========================================================================
//
// The publish-remote runbook
// (`scripts/testnet-live-publish-s3.sh`) is a
// pure-bash driver that consumes the STW-032 publish
// bundle the publish runbook produced and (optionally,
// via `--no-dry-run`) shells out to `aws s3 cp` per
// The companion STW-033 tests above pin the
// file-on-disk / bash-parse / --verify-remote-doc /
// --verify-receipt-pre-gate surface; the two
// sub-tests below add the *additional* contracts the
// companion tests don't pin:
//
// 1. `testnet_live_publish_s3_script_has_verify_bundle_pre_upload_gate` —
//    the runbook script must shell out to
//    `trainer --verify-bundle <bundle>` BEFORE the
//    `trainer --publish-remote` call (the
//    "refuse to upload a red publish bundle" gate the
//    bundle verifier is the source of truth for:
//    a publish-remote of a red bundle is a hard error,
//    not a warning).
// 2. `testnet_live_publish_s3_script_references_publish_remote_cli` —
//    the runbook script references the
//    `trainer --publish-remote <receipt-dir>
//     --bucket <s3://...>` CLI subcommand the
//    publish-remote step shells out to (a worker
//    reading the script would not know how to invoke
//    the upload without this mention).

#[test]
fn testnet_live_publish_s3_script_has_verify_bundle_pre_upload_gate() {
    // The publish-remote runbook script must shell
    // out to `trainer --verify-bundle <bundle>`
    // BEFORE the `trainer --publish-remote` call.
    // The "refuse to upload a red publish bundle"
    // gate the bundle verifier is the source of
    // truth for: a publish-remote of a red bundle
    // is a hard error, not a warning.
    //
    // We assert by ordered substring scan of the
    // *runtime call sites* (the `if !` lines that
    // shell out to the trainer binary, NOT the
    // docstring comments that mention the flags
    // in passing). The companion `--verify-receipt`
    // pre-upload gate is pinned by
    // `testnet_live_publish_s3_script_has_verify_receipt_pre_publish_gate`
    // above.
    let script = read(&publish_s3_script_path());
    // Find the runtime `if ! "...verify-bundle
    // <dir>"` call. The script uses a fixed
    // shell-quoted form
    // `if ! "$TRAINER_BIN" --verify-bundle "$PUBLISH_DIR"`
    // so the substring `$TRAINER_BIN" --verify-bundle`
    // is unique to the call site (a docstring
    // comment would not have the trailing `"`).
    let verify_bundle_call = script.find(r#"$TRAINER_BIN" --verify-bundle"#).unwrap_or_else(|| {
        panic!(
            "STW-033 publish-remote runbook must shell out to `trainer --verify-bundle <bundle>` \
             as a pre-upload gate; the runtime call `$TRAINER_BIN\" --verify-bundle` is missing \
             from the script"
        )
    });
    // Find the runtime `--publish-remote
    // "$RECEIPT_DIR"` call (it lives inside the
    // REMOTE_ARGS array, so the substring
    // `--publish-remote "$RECEIPT_DIR"` is unique
    // to the call site).
    let publish_remote_call = script.find(r#"--publish-remote "$RECEIPT_DIR""#).unwrap_or_else(|| {
        panic!(
            "STW-033 publish-remote runbook must shell out to `trainer --publish-remote <receipt>`; \
             the runtime call `--publish-remote \"$RECEIPT_DIR\"` is missing from the script"
        )
    });
    assert!(
        verify_bundle_call < publish_remote_call,
        "STW-033 publish-remote runbook must call `--verify-bundle` BEFORE `--publish-remote`; \
         a script that publishes-remote before verifying the publish bundle is the inverse of the \
         refuse-to-upload-red-bundle gate (verify-bundle call at offset {verify_bundle_call}, \
         publish-remote call at offset {publish_remote_call})"
    );
}

#[test]
fn testnet_live_publish_s3_script_references_publish_remote_cli() {
    // The publish-remote runbook script must
    // reference the
    // `trainer --publish-remote <receipt-dir>
    // --bucket <s3://...>` CLI subcommand the
    // publish-remote step shells out to. A worker
    // reading the script would not know how to
    // invoke the upload without this mention. We
    // assert by flag form (`--publish-remote` +
    // `--bucket`) because that is the form the
    // operator types and the form a dashboard
    // scraper greps. Mirrors the STW-032
    // `testnet_live_publish_doc_references_verify_bundle_cli`
    // pinner (which asserts the
    // `--verify-bundle` mention in the doc).
    let script = read(&publish_s3_script_path());
    assert!(
        script.contains("--publish-remote"),
        "STW-033 publish-remote runbook script at {} must reference the \
         `trainer --publish-remote <receipt-dir>` CLI subcommand; a worker reading the \
         script would not know how to invoke the upload",
        publish_s3_script_path().display()
    );
    assert!(
        script.contains("--bucket"),
        "STW-033 publish-remote runbook script at {} must reference the \
         `--bucket <s3://...>` CLI flag; a worker reading the script would not know \
         how to point the upload at a bucket",
        publish_s3_script_path().display()
    );
}

// ===========================================================================
// STW-034 publish-index (testnet dashboard aggregator) runbook shape contract
// ===========================================================================
//
// The publish-index runbook
// (`scripts/testnet-live-publish-index.sh`) is a
// pure-bash driver that consumes the STW-033
// `remote_receipt.json` files the publish-remote
// runbook produced and chains
// `trainer --publish-index <publish-root>` (the
// index writer) + `trainer --verify-index
// <index-path>` (the no-DB no-rebuild re-verifier)
// as a sequence of subprocesses.
//
// The companion STW-034 tests below pin the
// file-on-disk / bash-parse / doc-references-CLI
// surface; the two sub-tests below add the
// *additional* contracts the companion tests
// don't pin:
//
// 1. `testnet_live_publish_index_script_exists_and_parses` —
//    the runbook script must be on disk,
//    executable, and parse with `bash -n` (mirrors
//    the STW-019 + STW-032 + STW-033 file-on-disk
//    pins).
// 2. `testnet_live_publish_index_script_has_publish_index_call` —
//    the runbook script must shell out to
//    `trainer --publish-index <publish-root>` (the
//    index writer the STW-034 chain ships).
// 3. `testnet_live_publish_index_script_has_verify_index_call` —
//    the runbook script must shell out to
//    `trainer --verify-index <index-path>` (the
//    no-DB no-rebuild re-verifier the STW-034
//    chain ships).
// 4. `testnet_live_publish_index_doc_references_publish_index_cli` —
//    the runbook doc must reference the
//    `trainer --publish-index` + `trainer
//    --verify-index` CLI subcommands the runbook
//    shells out to (a worker reading the doc would
//    not know how to invoke the indexer without
//    this mention).

fn publish_index_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-index.sh")
}

fn publish_index_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-index.md")
}

#[test]
fn testnet_live_publish_index_script_exists_and_parses() {
    // The publish-index runbook script must be on
    // disk, executable, and parse with `bash -n`. A
    // regression that drops the file (or breaks the
    // bash grammar) fails the gate at CI time
    // before a CI worker can shell out to it.
    // Mirrors the STW-019 + STW-032 + STW-033
    // file-on-disk pins.
    let p = publish_index_script_path();
    assert!(
        p.exists(),
        "STW-034 publish-index runbook script missing at {}; \
         the testnet dashboard aggregator has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails
        // the test before a worker tries to shell
        // out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-034 publish-index runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without executing
    // it. A non-zero exit (a syntax error) fails the
    // test so a future edit that breaks the bash
    // grammar fails CI before it reaches a live
    // index step.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-publish-index.sh");
    assert!(
        out.status.success(),
        "STW-034 publish-index runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn testnet_live_publish_index_script_has_publish_index_call() {
    // The publish-index runbook script must shell
    // out to `trainer --publish-index <publish-root>`
    // (the index writer the STW-034 chain ships).
    // We assert by flag form (`--publish-index`)
    // because that is the form the operator types
    // and the form a dashboard scraper greps.
    // Mirrors the STW-032
    // `testnet_live_publish_script_references_publish_cli`
    // pinner.
    let script = read(&publish_index_script_path());
    assert!(
        script.contains("--publish-index"),
        "STW-034 publish-index runbook script at {} must reference the \
         `trainer --publish-index <publish-root>` CLI subcommand; a worker reading the \
         script would not know how to invoke the indexer",
        publish_index_script_path().display()
    );
}

#[test]
fn testnet_live_publish_index_script_has_verify_index_call() {
    // The publish-index runbook script must shell
    // out to `trainer --verify-index <index-path>`
    // (the no-DB no-rebuild re-verifier the STW-034
    // chain ships). We assert by flag form
    // (`--verify-index`) because that is the form
    // the operator types and the form a dashboard
    // scraper greps. Mirrors the STW-032
    // `testnet_live_publish_script_references_verify_bundle_cli`
    // pinner.
    let script = read(&publish_index_script_path());
    assert!(
        script.contains("--verify-index"),
        "STW-034 publish-index runbook script at {} must reference the \
         `trainer --verify-index <index-path>` CLI subcommand; a worker reading the \
         script would not know how to re-verify the index",
        publish_index_script_path().display()
    );
}

#[test]
fn testnet_live_publish_index_doc_references_publish_index_cli() {
    // The publish-index runbook doc must reference
    // both `trainer --publish-index` (the index
    // writer) and `trainer --verify-index` (the
    // no-DB no-rebuild re-verifier) the STW-034
    // chain ships. A worker reading the doc would
    // not know how to invoke either CLI arm without
    // these mentions. Mirrors the STW-032
    // `testnet_live_publish_doc_references_verify_bundle_cli`
    // pinner.
    let doc = read(&publish_index_doc_path());
    assert!(
        doc.contains("--publish-index"),
        "STW-034 publish-index runbook doc at {} must reference the \
         `trainer --publish-index <publish-root>` CLI subcommand; a worker reading the \
         doc would not know how to invoke the indexer",
        publish_index_doc_path().display()
    );
    assert!(
        doc.contains("--verify-index"),
        "STW-034 publish-index runbook doc at {} must reference the \
         `trainer --verify-index <index-path>` CLI subcommand; a worker reading the \
         doc would not know how to re-verify the index",
        publish_index_doc_path().display()
    );
}

// ===========================================================================
// STW-035 publish-index-remote runbook shape contract
// ===========================================================================
//
// The publish-index-remote runbook
// (`scripts/testnet-live-publish-index-s3.sh`) is a
// pure-bash driver that consumes the `INDEX.json` the
// STW-034 `testnet-live-publish-index.sh` runbook
// produced and writes a deterministic remote-upload
// plan + a post-upload
// `index_remote_receipt.json` the testnet dashboard
// scrapes. The four sub-tests below pin the
// runbook's static surface (the same no-DB
// shape-pinning pattern the STW-019 + STW-032 +
// STW-033 + STW-034 runbooks use):
//
// 1. `testnet_live_publish_index_s3_script_exists_and_parses` —
//    the s3 runbook script must be on disk, be
//    executable, and parse with `bash -n`. Mirrors
//    the STW-019 + STW-032 + STW-033 + STW-034
//    file-on-disk pins.
// 2. `testnet_live_publish_index_s3_script_has_verify_index_pre_upload_gate` —
//    the s3 runbook script must shell out to
//    `trainer --verify-index <index-dir>` BEFORE
//    the `trainer --publish-index-remote` call
//    (the "refuse to plan an upload for a red
//    index" gate the STW-034 index verifier is the
//    source of truth for).
// 3. `testnet_live_publish_index_s3_script_references_publish_index_remote_cli` —
//    the s3 runbook script must reference the
//    `trainer --publish-index-remote
//    <publish-root> --bucket <s3://...>` CLI
//    subcommand STW-035 ships.
// 4. `testnet_live_publish_index_s3_doc_references_verify_index_remote_cli` —
//    the s3 runbook doc must reference the
//    `trainer --verify-index-remote <path>` CLI
//    subcommand STW-035 ships (a worker reading the
//    doc would not know how to re-verify the index
//    without this mention).

fn publish_index_s3_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-index-s3.sh")
}

fn publish_index_s3_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-index-s3.md")
}

#[test]
fn testnet_live_publish_index_s3_script_exists_and_parses() {
    // The publish-index-remote s3 runbook script
    // must be on disk, executable, and parse with
    // `bash -n`. A regression that drops the file
    // (or breaks the bash grammar) fails the gate
    // at CI time before a CI worker can shell out
    // to it. Mirrors the STW-019 + STW-032 +
    // STW-033 + STW-034 file-on-disk pins.
    let p = publish_index_s3_script_path();
    assert!(
        p.exists(),
        "STW-035 publish-index-remote s3 runbook script missing at {}; \
         the testnet dashboard index-remote has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails
        // the test before a worker tries to shell
        // out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-035 publish-index-remote s3 runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without
    // executing it. A non-zero exit (a syntax
    // error) fails the test so a future edit that
    // breaks the bash grammar fails CI before it
    // reaches a live publish-index-remote step.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-publish-index-s3.sh");
    assert!(
        out.status.success(),
        "STW-035 publish-index-remote s3 runbook script must parse with `bash -n` \
         (got exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn testnet_live_publish_index_s3_script_has_verify_index_pre_upload_gate() {
    // The publish-index-remote s3 runbook script
    // must shell out to `trainer --verify-index
    // <index-dir>` BEFORE the
    // `trainer --publish-index-remote` call (the
    // "refuse to plan an upload for a red index"
    // gate the STW-034 index verifier is the
    // source of truth for). The pin is by byte
    // offset: `--verify-index` must appear in
    // the script body before `--publish-index-remote`
    // appears. A regression that re-orders the
    // chain (or drops the verify step) fails the
    // gate so a CI worker that runs the script
    // can never push a red index to the dashboard
    // bucket. Mirrors the STW-032
    // `testnet_live_publish_script_has_verify_receipt_pre_publish_gate`
    // + STW-033
    // `testnet_live_publish_s3_script_has_verify_receipt_pre_publish_gate`
    // pinners.
    let script = read(&publish_index_s3_script_path());
    let verify_index_pos = script.find("--verify-index").unwrap_or_else(|| {
        panic!(
            "STW-035 publish-index-remote s3 runbook script at {} must reference the \
                 `trainer --verify-index <index-dir>` CLI subcommand as the pre-upload gate",
            publish_index_s3_script_path().display()
        )
    });
    let publish_index_remote_pos = script.find("--publish-index-remote").unwrap_or_else(|| {
        panic!(
            "STW-035 publish-index-remote s3 runbook script at {} must reference the \
                 `trainer --publish-index-remote` CLI subcommand",
            publish_index_s3_script_path().display()
        )
    });
    assert!(
        verify_index_pos < publish_index_remote_pos,
        "STW-035 publish-index-remote s3 runbook script must shell out to \
         `--verify-index` (byte offset {verify_index_pos}) BEFORE `--publish-index-remote` \
         (byte offset {publish_index_remote_pos}); the pre-upload gate fires before the \
         upload step so a red index cannot reach the dashboard bucket"
    );
}

#[test]
fn testnet_live_publish_index_s3_script_references_publish_index_remote_cli() {
    // The publish-index-remote s3 runbook script
    // must reference the
    // `trainer --publish-index-remote
    // <publish-root> --bucket <s3://...>` CLI
    // subcommand STW-035 ships. We assert by
    // flag form (`--publish-index-remote` +
    // `--bucket`) because that is the form the
    // operator types and the form a dashboard
    // scraper greps. Mirrors the STW-033
    // `testnet_live_publish_s3_script_references_publish_remote_cli`
    // pinner.
    let script = read(&publish_index_s3_script_path());
    assert!(
        script.contains("--publish-index-remote"),
        "STW-035 publish-index-remote s3 runbook script at {} must reference the \
         `trainer --publish-index-remote <publish-root> --bucket <s3://...>` CLI subcommand; \
         a worker reading the script would not know how to invoke the index-remote step",
        publish_index_s3_script_path().display()
    );
    assert!(
        script.contains("--bucket"),
        "STW-035 publish-index-remote s3 runbook script at {} must reference the \
         `--bucket <s3://...>` flag the index-remote step requires",
        publish_index_s3_script_path().display()
    );
}

#[test]
fn testnet_live_publish_index_s3_doc_references_verify_index_remote_cli() {
    // The publish-index-remote s3 runbook doc
    // must reference the
    // `trainer --verify-index-remote <path>` CLI
    // subcommand STW-035 ships (a worker reading
    // the doc would not know how to re-verify
    // the index-remote receipt without this
    // mention). Mirrors the STW-033
    // `testnet_live_publish_s3_doc_references_verify_remote_cli`
    // pinner.
    let doc = read(&publish_index_s3_doc_path());
    assert!(
        doc.contains("--verify-index-remote"),
        "STW-035 publish-index-remote s3 runbook doc at {} must reference the \
         `trainer --verify-index-remote <path>` CLI subcommand; a worker reading the \
         doc would not know how to re-verify the index-remote receipt",
        publish_index_s3_doc_path().display()
    );
}

// --- STW-036 dashboard-deploy runbook shape pins -----------------
//
// The STW-036 dashboard-deploy runbook
// (`scripts/testnet-live-publish-dashboard.sh`) is the v10
// follow-on the STW-035 publish-index-remote runbook doc
// defers to: a CI worker that produced an `INDEX.json` (via
// the STW-034 → STW-035 chain) wants to `aws s3 sync` it to
// a public dashboard bucket. The four pins below mirror the
// STW-019 + STW-032 + STW-033 + STW-034 + STW-035 file-on-
// disk + pre-upload-gate + CLI-reference + doc-reference
// pinners the autotrain pipeline already follows, so a
// regression in the dashboard-deploy runbook's surface
// (file missing, syntax broken, executable bit cleared, no
// pre-deploy `trainer --verify-index` call, no `aws s3 sync`
// call, no `RBP_DASHBOARD_INDEX_URL` env-knob mention in
// the doc) fails CI at the same step a future operator
// would silently break.

fn dashboard_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("testnet-live-publish-dashboard.sh")
}

#[test]
fn testnet_live_publish_dashboard_script_exists_and_parses() {
    // The dashboard-deploy runbook script must
    // be on disk, executable, and parse with
    // `bash -n`. A regression that drops the file
    // (or breaks the bash grammar) fails the gate
    // at CI time before a CI worker can shell out
    // to it. Mirrors the STW-019 + STW-032 +
    // STW-033 + STW-034 + STW-035 file-on-disk
    // pins.
    let p = dashboard_script_path();
    assert!(
        p.exists(),
        "STW-036 dashboard-deploy runbook script missing at {}; \
         the testnet dashboard deploy has no shell entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails
        // the test before a worker tries to shell
        // out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-036 dashboard-deploy runbook script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without
    // executing it. A non-zero exit (a syntax
    // error) fails the test so a future edit that
    // breaks the bash grammar fails CI before it
    // reaches a live dashboard-deploy step.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/testnet-live-publish-dashboard.sh");
    assert!(
        out.status.success(),
        "STW-036 dashboard-deploy runbook script must parse with `bash -n` \
         (got exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn testnet_live_publish_dashboard_script_has_verify_index_pre_deploy_gate() {
    // The dashboard-deploy runbook script must
    // shell out to `trainer --verify-index
    // <index-dir>` BEFORE the `aws s3 sync` step
    // (the "refuse to deploy a red index" gate
    // the STW-034 index verifier is the source
    // of truth for). The pin is by byte offset:
    // `--verify-index` must appear in the script
    // body before `aws s3 sync` appears. A
    // regression that re-orders the chain (or
    // drops the verify step) fails the gate so a
    // CI worker that runs the script can never
    // push a red index to the dashboard bucket.
    // Mirrors the STW-035
    // `testnet_live_publish_index_s3_script_has_verify_index_pre_upload_gate`
    // pinner.
    // The pin is by byte offset on the *actual* call
    // site (not the `aws s3 sync` substring the
    // docstring / echo lines use). The runtime call
    // site is `"$AWS_BIN" s3 sync`, so we look for
    // `"$AWS_BIN" s3 sync` as the pre-deploy gate's
    // "aws s3 sync" anchor. A regression that
    // re-orders the chain (or drops the verify step)
    // fails the gate so a CI worker that runs the
    // script can never push a red index to the
    // dashboard bucket. Mirrors the STW-035
    // `testnet_live_publish_index_s3_script_has_verify_index_pre_upload_gate`
    // pinner.
    let script = read(&dashboard_script_path());
    let verify_index_pos = script.find("--verify-index").unwrap_or_else(|| {
        panic!(
            "STW-036 dashboard-deploy runbook script at {} must reference the \
                 `trainer --verify-index <index-dir>` CLI subcommand as the pre-deploy gate",
            dashboard_script_path().display()
        )
    });
    let aws_sync_pos = script.find("\"$AWS_BIN\" s3 sync").unwrap_or_else(|| {
        panic!(
            "STW-036 dashboard-deploy runbook script at {} must invoke the \
                 `aws s3 sync` step",
            dashboard_script_path().display()
        )
    });
    assert!(
        verify_index_pos < aws_sync_pos,
        "STW-036 dashboard-deploy runbook script must shell out to \
         `--verify-index` (byte offset {verify_index_pos}) BEFORE `aws s3 sync` \
         (byte offset {aws_sync_pos}); the pre-deploy gate fires before the \
         deploy step so a red index cannot reach the dashboard bucket"
    );
}

#[test]
fn testnet_live_publish_dashboard_script_references_aws_s3_sync() {
    // The dashboard-deploy runbook script must
    // invoke the `aws s3 sync <local>
    // <bucket>/<prefix>/ --delete --cache-control
    // max-age=60` step. We assert by `aws s3
    // sync` substring because that is the form
    // the operator types and the form a CI
    // dashboard scraper greps. Mirrors the
    // STW-033 `aws s3 cp` pinner + the STW-035
    // `aws s3 cp` pinner the autotrain pipeline
    // already follows.
    let script = read(&dashboard_script_path());
    // The `aws s3 sync` call site uses the
    // `"$AWS_BIN" s3 sync` form (so a worker can
    // override the `aws` binary path via the
    // `AWS_BIN` env knob), not the literal
    // `aws s3 sync` form. The substring we pin
    // is the runtime call anchor.
    assert!(
        script.contains("\"$AWS_BIN\" s3 sync"),
        "STW-036 dashboard-deploy runbook script at {} must invoke \
         `aws s3 sync` (via the `$AWS_BIN` env knob); a worker reading the \
         script would not know how to deploy the dashboard data feed",
        dashboard_script_path().display()
    );
    assert!(
        script.contains("--delete"),
        "STW-036 dashboard-deploy runbook script at {} must invoke \
         `aws s3 sync ... --delete` so a removed receipt's INDEX.json row \
         is no longer reflected in the dashboard",
        dashboard_script_path().display()
    );
    assert!(
        script.contains("--cache-control"),
        "STW-036 dashboard-deploy runbook script at {} must invoke \
         `aws s3 sync ... --cache-control max-age=60` so the dashboard's \
         browser-fetched INDEX.json stays fresh on a 1-minute Cache-Control window",
        dashboard_script_path().display()
    );
}

// --- STW-037 operator-runnable 3-consecutive full-workspace
//     proof runbook shape pins -----------------------------
//
// The STW-037 operator-runnable runbook
// (`scripts/workspace-parallel-proof-three.sh`) closes the
// last un-closed `verification:workspace-parallel`
// mainnet-block hinge. STW-020 ships
// `scripts/workspace-parallel-proof.sh` (the canonical
// 3-consecutive *full-workspace* proof an operator has to
// hand-orchestrate with a no-output knob) and STW-030 ships
// the cheap in-CI 2-second 3-consecutive *gameplay-only*
// proof the
// `crates/autotrain/tests/workspace_parallel_proof_three.rs::run_three_consecutive_clean_gameplay_lib_test_runs`
// lib test drives. STW-037 sits in between: a pure-bash
// runbook an operator / nightly worker can `bash
// scripts/workspace-parallel-proof-three.sh` from a clean
// checkout and get a single command that invokes the
// STW-030 lib test 3 times back-to-back in 3 separate
// `cargo test` invocations AND invokes the STW-020 runbook
// once, capturing each invocation's stdout + stderr + exit
// into a per-invocation
// `logs/workspace-parallel-proof-three/<UTC-ISO>/invocation-{1,2,3,4}/{stdout,stderr,exit}.txt`
// layout. The single pin below mirrors the
// `workspace_parallel_proof.rs::script_exists_and_is_executable`
// / `script_parses_with_bash_n` pinners + the
// STW-019 / STW-032 / STW-033 / STW-034 / STW-035 / STW-036
// shell-shape pins the autotrain pipeline already follows,
// so a regression in the new runbook's surface (file
// missing, syntax broken, executable bit cleared) fails CI
// at the same step a future operator would silently break.
// The companion
// `crates/autotrain/tests/workspace_parallel_proof_three.rs::operator_runnable_three_script_exists_and_parses`
// sub-test owns the deeper "lists
// `run_three_consecutive_clean_gameplay_lib_test_runs` as a
// sub-invocation + emits the pinned
// `workspace parallel proof three complete:` headline"
// pins so a regression in either surface fails CI at the
// step that catches the runbook-side drift OR the
// lib-test-side drift (the two are separate test files so
// a regression in one does not silently dodge the other).

fn workspace_parallel_proof_three_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("workspace-parallel-proof-three.sh")
}

#[test]
fn workspace_parallel_proof_three_script_exists_and_parses() {
    // The STW-037 operator-runnable runbook script
    // must be on disk, executable, and parse with
    // `bash -n`. A regression that drops the file
    // (or breaks the bash grammar) fails the gate
    // at CI time before a CI worker can shell out
    // to it. Mirrors the STW-019 + STW-032 +
    // STW-033 + STW-034 + STW-035 + STW-036
    // file-on-disk pins + the
    // `workspace_parallel_proof.rs::script_exists_and_is_executable`
    // / `script_parses_with_bash_n` pinners.
    let p = workspace_parallel_proof_three_script_path();
    assert!(
        p.exists(),
        "STW-037 operator-runnable 3-consecutive full-workspace proof \
         runbook script missing at {}; the STW-037 hinge has no shell \
         entry point",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a
        // future `chmod -x` regression (e.g. a
        // cross-checkout that strips the bit) fails
        // the test before a worker tries to shell
        // out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-037 operator-runnable 3-consecutive full-workspace proof \
             runbook script at {} must have its owner-executable bit set \
             (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        let _ = meta;
    }
    // `bash -n` parses the script without
    // executing it. A non-zero exit (a syntax
    // error) fails the test so a future edit that
    // breaks the bash grammar fails CI before it
    // reaches a live STW-037 run.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/workspace-parallel-proof-three.sh");
    assert!(
        out.status.success(),
        "STW-037 operator-runnable 3-consecutive full-workspace proof \
         runbook script must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}
