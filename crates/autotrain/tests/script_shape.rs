//! `scripts/testnet-live-proof.sh` shape contract (STW-019).
//!
//! This integration test pins the *shape* of the STW-019 runbook
//! without requiring a live Postgres. It runs in
//! `cargo test --workspace` (no `database` feature gate) so a
//! regression in the runbook's surface (file missing, syntax
//! broken, executable bit cleared, doc drift) fails CI before it
//! ever reaches a live DB.
//!
//! The six sub-tests assert the runbook's static contract:
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
//! 6. `testnet_live_proof_script_documents_fast_mode` (STW-069) —
//!    the runbook script contains the `RBP_TESTNET_FAST` string
//!    and the runbook doc documents the fast-mode env knob.
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
        "RBP_TESTNET_FAST",
        "RBP_FAST_EPOCHS",
        "RBP_FAST_BATCH",
        "RBP_BENCH_HANDS",
        "RBP_BENCH_BLIND",
        "RBP_COMPARE_HANDS",
        "RBP_COMPARE_BLIND",
        "RBP_FAST_KMEANS_SAMPLE",
        "RBP_FAST_KMEANS_ITERATIONS",
        "RBP_FAST_LOOKUP_SAMPLE",
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
// STW-069 fast-mode shape pin
// ===========================================================================
//
// The STW-069 fast-mode knob (`RBP_TESTNET_FAST=1`) lets an operator
// collapse the testnet-live-proof chain from hours to minutes by
// auto-selecting minimal epochs/hands/batch. The shape pin asserts
// the runbook script contains the knob string and the runbook doc
// documents it, so a future refactor that drops the fast-mode path
// fails CI before it reaches a live Postgres.

#[test]
fn testnet_live_proof_script_documents_fast_mode() {
    let script = read(&script_path());
    assert!(
        script.contains("RBP_TESTNET_FAST"),
        "STW-069 runbook script must contain the `RBP_TESTNET_FAST` string; \
         the fast-mode contract must be present in the source"
    );
    // The script must conditionally set the minimal values when
    // RBP_TESTNET_FAST=1 is set. We assert the conditional block
    // shape (an `if` test on the env var) so a future refactor
    // that drops the conditional fails here.
    assert!(
        script.contains("RBP_TESTNET_FAST")
            && (script.contains("[[ \"${RBP_TESTNET_FAST:-}\" == \"1\" ]]")
                || script.contains("[[ \"${RBP_TESTNET_FAST:-}\" == \"1\" ]]")),
        "STW-069 runbook script must conditionally honour RBP_TESTNET_FAST=1; \
         a future refactor that drops the conditional breaks the fast-mode contract"
    );
    // The runbook doc must mention the knob so an operator reading
    // the doc knows how to invoke fast mode.
    let doc = read(&runbook_doc_path());
    assert!(
        doc.contains("RBP_TESTNET_FAST"),
        "STW-069 runbook doc at {} must document the RBP_TESTNET_FAST knob; \
         an operator would not know how to invoke fast mode",
        runbook_doc_path().display()
    );
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
    let verify_bundle_call = script
        .find(r#"$TRAINER_BIN" --verify-bundle"#)
        .unwrap_or_else(|| {
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

// --- STW-054 cloudflare-pages-deploy runbook shape pins ------
//
// The STW-054 Cloudflare Pages dashboard-deploy runbook
// (`scripts/deploy-dashboard-cloudflare.sh`) is the
// *deploy* leg of the public-surface north star the prior
// CEO lens named: a CI worker that produced an
// `INDEX.json` (via the STW-034 → STW-035 chain) wants
// to `wrangler pages deploy` it to a Cloudflare Pages
// project the dashboard's `RBP_DASHBOARD_INDEX_URL` env
// knob points at. The STW-036 S3/CloudFront runbook
// already ships (a parallel deploy surface the operator
// picks between via the script they invoke), and STW-054
// adds the Cloudflare Pages path alongside it. The four
// pins below mirror the STW-019 + STW-032 + STW-033 +
// STW-034 + STW-035 + STW-036 file-on-disk +
// pre-deploy-gate + CLI-reference + no-secrets-in-config
// pinners the autotrain pipeline already follows, so a
// regression in the Cloudflare Pages runbook's surface
// (file missing, syntax broken, executable bit cleared,
// no pre-deploy `trainer --verify-index` call, no
// `wrangler pages deploy` call, no
// `RBP_DASHBOARD_CF_API_TOKEN` env-knob reference, or
// — most important — a `[vars]` block with a real
// Cloudflare API token committed in `wrangler.toml`)
// fails CI at the same step a future operator would
// silently break or — worse — silently leak a secret.

fn deploy_dashboard_cloudflare_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("deploy-dashboard-cloudflare.sh")
}

fn wrangler_toml_path() -> PathBuf {
    workspace_root().join("wrangler.toml")
}

#[test]
fn deploy_dashboard_cloudflare_script_exists_and_parses() {
    // The STW-054 Cloudflare Pages dashboard-deploy
    // runbook script must be on disk, executable,
    // and parse with `bash -n`. A regression that
    // drops the file (or breaks the bash grammar)
    // fails the gate at CI time before a CI worker
    // can shell out to it. Mirrors the STW-019 +
    // STW-032 + STW-033 + STW-034 + STW-035 +
    // STW-036 file-on-disk pins.
    let p = deploy_dashboard_cloudflare_script_path();
    assert!(
        p.exists(),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script missing at {}; \
         the Cloudflare Pages dashboard deploy has no shell entry point",
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
            "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must have \
             its owner-executable bit set (got mode {mode:o})",
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
    // Cloudflare Pages dashboard-deploy step.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/deploy-dashboard-cloudflare.sh");
    assert!(
        out.status.success(),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script must parse with \
         `bash -n` (got exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn deploy_dashboard_cloudflare_script_references_wrangler_pages_deploy() {
    // The STW-054 runbook must invoke
    // `wrangler pages deploy <index-dir>` — the
    // Cloudflare Pages push the dashboard's
    // `RBP_DASHBOARD_INDEX_URL` env knob points
    // at. We assert by `wrangler pages deploy`
    // substring because that is the form the
    // operator types and the form a CI dashboard
    // scraper greps. The runtime call site uses
    // the `"$WRANGLER_BIN" pages deploy` form
    // (so a worker can override the `wrangler`
    // binary path via the `WRANGLER_BIN` env
    // knob), not the literal `wrangler pages
    // deploy` form. The substring we pin is the
    // literal `wrangler pages deploy` so the
    // pinner fails if a future edit drops the
    // call site (or renames the Cloudflare CLI
    // command). Mirrors the STW-036
    // `testnet_live_publish_dashboard_script_references_aws_s3_sync`
    // pinner.
    let script = read(&deploy_dashboard_cloudflare_script_path());
    assert!(
        script.contains("wrangler pages deploy"),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must invoke \
         `wrangler pages deploy` (via the `$WRANGLER_BIN` env knob); a worker reading \
         the script would not know how to deploy the dashboard data feed to Cloudflare \
         Pages",
        deploy_dashboard_cloudflare_script_path().display()
    );
    assert!(
        script.contains("robopoker-testnet-dashboard"),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must pin the \
         Pages project name `robopoker-testnet-dashboard` (the project the README's \
         `<https://robopoker-testnet-dashboard.pages.dev/>` placeholder becomes real \
         after the first deploy); a worker reading the script would not know which \
         Pages project the deploy targets",
        deploy_dashboard_cloudflare_script_path().display()
    );
    assert!(
        script.contains("--commit-dirty=true"),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must invoke \
         `wrangler pages deploy ... --commit-dirty=true` so the deploy proceeds even \
         when the Pages project's git working tree is dirty (the `wrangler.toml` is \
         the only committed file the Pages path cares about; the deploy payload is \
         the local `index/` dir, not a git commit)",
        deploy_dashboard_cloudflare_script_path().display()
    );
}

#[test]
fn deploy_dashboard_cloudflare_script_references_rbp_dashboard_cf_api_token() {
    // The STW-054 runbook must reference the
    // `RBP_DASHBOARD_CF_API_TOKEN` env knob (the
    // Cloudflare API token the operator sets at
    // deploy time; the token is NEVER echoed and
    // NEVER written to disk). A regression that
    // renames the env knob (or drops the
    // `exit 3` fail-fast gate the runbook
    // implements) fails this pin so a CI worker
    // reading the script can wire the same env
    // knob the operator is expected to set.
    // Mirrors the STW-035
    // `testnet_live_publish_index_s3_script_references_*`
    // env-knob pinners.
    let script = read(&deploy_dashboard_cloudflare_script_path());
    assert!(
        script.contains("RBP_DASHBOARD_CF_API_TOKEN"),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must \
         reference the `RBP_DASHBOARD_CF_API_TOKEN` env knob (the Cloudflare API \
         token the operator sets at deploy time; the runbook routes the token to \
         the `wrangler` CLI as a `CLOUDFLARE_API_TOKEN` env override at the runtime \
         call site only); a worker reading the script would not know which env \
         knob the deploy authenticates against",
        deploy_dashboard_cloudflare_script_path().display()
    );
    assert!(
        script.contains("CLOUDFLARE_API_TOKEN"),
        "STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must route \
         the `RBP_DASHBOARD_CF_API_TOKEN` env knob to the `wrangler` CLI via a \
         `CLOUDFLARE_API_TOKEN` env override (the `wrangler` CLI's documented auth \
         env-var name); a hand-rolled `--api-token $RBP_DASHBOARD_CF_API_TOKEN` \
         form would echo the token in `ps -ef` and the CI worker's process list",
        deploy_dashboard_cloudflare_script_path().display()
    );
}

#[test]
fn deploy_dashboard_cloudflare_wrangler_toml_has_no_secrets() {
    // The committed `wrangler.toml` at the repo
    // root pins the Pages project name +
    // `pages_build_output_dir` +
    // `compatibility_date` the `wrangler pages
    // deploy` call consumes. The config MUST NOT
    // contain a `[vars]` block (or any other key
    // that maps to a Cloudflare secret — e.g.
    // `api_token = "..."` under a project
    // header) because the runbook reads the
    // Cloudflare API token from the
    // `RBP_DASHBOARD_CF_API_TOKEN` env knob at
    // deploy time. A regression that adds a real
    // secret to `wrangler.toml` leaks the secret
    // to every `git clone` (and the public
    // GitHub mirror) and is irrecoverable without
    // rotating the secret. This pin is the
    // cheapest possible early-warning system: a
    // `[vars]` block with a `=` line, or a
    // `api_token` key anywhere in the file, fails
    // the test at CI time before the secret
    // reaches a Cloudflare Pages deploy.
    let p = wrangler_toml_path();
    assert!(
        p.exists(),
        "STW-054 Cloudflare Pages dashboard-deploy runbook expects a `wrangler.toml` \
         at the repo root pinning the Pages project config; file missing at {}",
        p.display()
    );
    let config = read(&p);
    // (a) The file must pin the Pages project
    // name. Per the sixth-pass STW-054 spec, the
    // `wrangler pages deploy <dir>` shell-out the
    // runbook uses is the explicit directory path
    // shape, so the `pages_build_output_dir` /
    // `compatibility_date` keys are unnecessary
    // and are intentionally NOT pinned in the
    // committed `wrangler.toml` (adding them would
    // create a config/env drift surface the runbook
    // does not defend against — the runbook is
    // env-knob-driven, not config-file-driven).
    // The `name = "robopoker-testnet-dashboard"`
    // project name is the only config key the
    // `wrangler pages deploy` call needs.
    assert!(
        config.contains("name = \"robopoker-testnet-dashboard\""),
        "STW-054 `wrangler.toml` at {} must pin the Pages project name \
         `robopoker-testnet-dashboard`",
        p.display()
    );
    // (b) The file must NOT contain a
    // `pages_build_output_dir` key (the sixth-pass
    // STW-054 spec explicitly excludes the key —
    // `wrangler pages deploy <dir>` is the
    // explicit directory path shape the runbook
    // uses, so a `pages_build_output_dir` line
    // would silently shadow the on-the-wire
    // directory the runbook passes on the command
    // line). A regression that adds a
    // `pages_build_output_dir = "..."` line fails
    // this pin.
    assert!(
        !config.contains("pages_build_output_dir"),
        "STW-054 `wrangler.toml` at {} must NOT contain a `pages_build_output_dir` \
         key; the `wrangler pages deploy <dir>` shell-out the runbook uses is the \
         explicit directory path shape, so a `pages_build_output_dir` line would \
         silently shadow the on-the-wire directory the runbook passes on the \
         command line. The runbook is env-knob-driven, not config-file-driven.",
        p.display()
    );
    // (c) The file must NOT contain a
    // `compatibility_date` key (the sixth-pass
    // STW-054 spec explicitly excludes the key —
    // the runbook's `wrangler pages deploy` call
    // does not pass a `--compatibility-date` flag
    // and the config-file form would be a
    // config/env drift surface a future refactor
    // would have to defend against). A regression
    // that adds a `compatibility_date = "..."`
    // line fails this pin.
    assert!(
        !config.contains("compatibility_date"),
        "STW-054 `wrangler.toml` at {} must NOT contain a `compatibility_date` key; \
         the runbook's `wrangler pages deploy` call does not pass a \
         `--compatibility-date` flag and the config-file form is intentionally \
         absent (the runbook is env-knob-driven, not config-file-driven).",
        p.display()
    );
    // (d) The file must NOT contain a TOML
    // section header that maps to a Cloudflare
    // secret — `[vars]`, `[env]`, or a
    // `[env.production]` / `[env.preview]`
    // block (the canonical env-injection
    // surfaces Cloudflare Pages exposes, and
    // the most likely place a future
    // contributor will paste a Cloudflare API
    // token by mistake). A regression that adds
    // any of these as a *TOML section header*
    // (i.e. a `[xxx]` line at the start of a
    // non-comment line) fails this pin. The pin
    // is line-anchored so a *comment* that
    // references `[vars]` (e.g. the comment in
    // this very pin's docstring) does NOT fail
    // the pin.
    let has_secret_section = config.lines().map(|l| l.trim_start()).any(|l| {
        l.starts_with("[vars]")
            || l.starts_with("[env]")
            || l.starts_with("[env.production]")
            || l.starts_with("[env.preview]")
    });
    assert!(
        !has_secret_section,
        "STW-054 `wrangler.toml` at {} must NOT contain a TOML section header for \
         a Cloudflare secret (one of `[vars]`, `[env]`, `[env.production]`, \
         `[env.preview]`); the Cloudflare API token is read from the \
         `RBP_DASHBOARD_CF_API_TOKEN` env knob at deploy time (no secrets are \
         committed)",
        p.display()
    );
    // auth" by mistake). The pin is line-
    // anchored (`key = "value"` form, not a
    // substring match) so a *comment* that
    // mentions the word `api_token` does NOT
    // fail the pin. A regression that adds an
    // `api_token = "..."` line (a real TOML
    // key=value with a non-empty string
    // payload) fails this pin.
    let has_api_token_line = config.lines().any(|l| {
        let trimmed = l.trim_start();
        trimmed.starts_with("api_token = \"")
            || trimmed.starts_with("api_token='")
            || trimmed.starts_with("auth_token = \"")
            || trimmed.starts_with("auth_token='")
    });
    assert!(
        !has_api_token_line,
        "STW-054 `wrangler.toml` at {} must NOT contain a real secret key line \
         (one of `api_token = \"...\"`, `api_token='...'`, `auth_token = \"...\"`, \
         `auth_token='...'`); the Cloudflare API token is read from the \
         `RBP_DASHBOARD_CF_API_TOKEN` env knob at deploy time (no secrets are \
         committed)",
        p.display()
    );
}

#[test]
fn deploy_dashboard_cloudflare_script_exports_rbp_dashboard_deployed_url() {
    // The STW-054 runbook must `export
    // RBP_DASHBOARD_DEPLOYED_URL=<pages_url>` AFTER
    // the `wrangler pages deploy` call succeeds AND
    // the runbook reads the URL `wrangler` printed
    // to stdout. The export stamps the resolved
    // Pages URL back into the env knob the STW-058
    // `serve_static_index` handler reads, so a
    // *subsequent* `wrangler pages deploy`
    // invocation (or a follow-on `cargo run -p
    // rbp-dashboard` smoke) is sourced from the
    // same env knob the dashboard reads. The
    // `replace_in_readme` sed step + the dashboard's
    // meta line + the `deploy.json` `pages_url`
    // field are all driven from the same
    // `pages_url` variable; the export closes the
    // loop. We assert by *string presence* of the
    // literal `export RBP_DASHBOARD_DEPLOYED_URL=`
    // substring so a future regression that drops
    // the export (or renames the env knob the
    // runbook stamps) fails the pin at CI time
    // before a Cloudflare Pages deploy. Mirrors the
    // STW-058
    // `dashboard_router_reads_rbp_dashboard_deployed_url`
    // env-knob-read pin (this is the *write* side;
    // STW-058 is the *read* side).
    let script = read(&deploy_dashboard_cloudflare_script_path());
    assert!(
        script.contains("export RBP_DASHBOARD_DEPLOYED_URL="),
        "STW-059 STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must \
         `export RBP_DASHBOARD_DEPLOYED_URL=<pages_url>` AFTER the `wrangler pages \
         deploy` call succeeds AND the runbook reads the URL `wrangler` printed to \
         stdout; a runbook that reads the URL but does NOT stamp it back into the env \
         knob forces a downstream `cargo run -p rbp-dashboard` smoke to read a \
         stale `RBP_DASHBOARD_DEPLOYED_URL=<project>.pages.dev/` default, breaking the \
         single-source-of-truth pattern the STW-058 `serve_static_index` handler \
         reads",
        deploy_dashboard_cloudflare_script_path().display()
    );
    // (b) The export must be ordered AFTER the
    // `wrangler pages deploy` call site (the URL
    // the export stamps is the URL wrangler printed
    // — exporting before the deploy would stamp an
    // empty / placeholder URL). A future regression
    // that re-orders the export above the
    // `wrangler pages deploy` call site (e.g. a
    // "declare-then-deploy" refactor) fails this
    // pin. We assert by *string index* ordering
    // (the deploy call site must appear in the
    // script source BEFORE the export line).
    let deploy_idx = script
        .find("wrangler pages deploy")
        .expect("STW-054 deploy call site must be present in the script");
    let export_idx = script
        .find("export RBP_DASHBOARD_DEPLOYED_URL=")
        .expect("STW-059 export must be present in the script");
    assert!(
        deploy_idx < export_idx,
        "STW-059 `export RBP_DASHBOARD_DEPLOYED_URL=` line must appear AFTER the \
         `wrangler pages deploy` call site in the runbook source; exporting before \
         the deploy would stamp an empty / pre-deploy URL into the env knob (got \
         export at offset {export_idx} before deploy at offset {deploy_idx})"
    );
    // (c) The runbook must also print the export
    // line to stdout so a CI worker scraping the
    // runbook's stdout can confirm the stamp
    // landed (the `export` builtin writes to the
    // calling process env but does NOT print, so
    // the explicit `echo` is the observation
    // surface the STW-059 hand-test contract
    // depends on). A regression that drops the
    // `echo` line (or renames the prefix) silently
    // breaks the `prints export
    // RBP_DASHBOARD_DEPLOYED_URL=<url> to stdout`
    // hand-test contract.
    assert!(
        script.contains("echo \"export RBP_DASHBOARD_DEPLOYED_URL="),
        "STW-059 STW-054 runbook must `echo \"export RBP_DASHBOARD_DEPLOYED_URL=...\"` \
        to stdout so a CI worker scraping the runbook's stdout can confirm the \
        stamp landed (the `export` builtin does not print); a runbook that exports \
        the knob but does NOT echo it leaves no stdout-side observation surface for \
        a CI dashboard to scrape at {}",
        deploy_dashboard_cloudflare_script_path().display()
    );
}

#[test]
fn deploy_dashboard_cloudflare_script_emits_live_proof_headline() {
    // The STW-054 deploy runbook must append a
    // `live_proof dashboard deploy complete: ...`
    // headline line to its `SUMMARY.txt` after the
    // `wrangler pages deploy` call succeeds. The
    // headline is the same `grep ^live_proof`
    // scrape contract the prior STW-019 + STW-032 +
    // STW-033 + STW-034 + STW-035 + STW-036 runbooks
    // pin (`live_proof publish ...` /
    // `live_proof receipt verification ...` /
    // `live_proof bundle verification ...` /
    // `live_proof index verification ...` /
    // `live_proof remote verification ...` /
    // `live_proof index_remote verification ...`).
    // A future regression that drops the
    // `printf 'live_proof dashboard deploy complete:
    // pages_url=%s files=%d bytes=%d\n' ... >> "$SUMMARY"`
    // line fails this pin at the static shell-shape
    // layer, before any Cloudflare Pages deploy
    // is attempted.
    let script = read(&deploy_dashboard_cloudflare_script_path());
    assert!(
        script.contains("live_proof dashboard deploy complete: pages_url="),
        "STW-057 STW-054 Cloudflare Pages dashboard-deploy runbook script at {} must \
         append a `live_proof dashboard deploy complete: pages_url=%s files=%d bytes=%d` \
         headline to its `SUMMARY.txt` after the `wrangler pages deploy` call succeeds; \
         a CI dashboard scraping the runbook via `grep ^live_proof` would not see the \
         deploy step's headline, breaking the `live_proof ...` scrape contract the \
         STW-019 + STW-032 + STW-033 + STW-034 + STW-035 + STW-036 runbooks already pin",
        deploy_dashboard_cloudflare_script_path().display()
    );
    // (b) The `printf` must be ordered AFTER the
    // `wrangler pages deploy` call site (the
    // `PAGES_URL` / `FILES` / `BYTES` variables the
    // printf references are computed from the deploy
    // output). A regression that re-orders the printf
    // above the `wrangler pages deploy` call site
    // (e.g. a "headline-then-deploy" refactor) fails
    // this pin. We assert by *string index* ordering
    // (the deploy call site must appear in the
    // script source BEFORE the printf line).
    let deploy_idx = script
        .find("wrangler pages deploy")
        .expect("STW-054 deploy call site must be present in the script");
    let printf_idx = script
        .find("live_proof dashboard deploy complete: pages_url=")
        .expect("STW-057 printf must be present in the script");
    assert!(
        deploy_idx < printf_idx,
        "STW-057 `printf 'live_proof dashboard deploy complete: ...'` line must \
         appear AFTER the `wrangler pages deploy` call site in the runbook source; \
         a printf before the deploy would stamp a pre-deploy / placeholder URL into \
         the headline (got printf at offset {printf_idx} before deploy at offset \
         {deploy_idx})"
    );
    // (c) The printf must `>> "$SUMMARY"` (append,
    // not truncate) so the runbook preserves the
    // pre-deploy `SUMMARY.txt` content the STW-034
    // publish-index chain wrote. A regression that
    // rewrites the `>>` to `>` (truncate) would
    // destroy the upstream `live_proof publish ...
    // / live_proof index verification ...` lines
    // a CI dashboard scrapes in the same file.
    let printf_segment = &script[printf_idx..];
    assert!(
        printf_segment.contains(">> \"$SUMMARY\"") || printf_segment.contains(">>$SUMMARY"),
        "STW-057 `printf 'live_proof dashboard deploy complete: ...'` line must \
         `>> \"$SUMMARY\"` (append, not truncate) so the runbook preserves the \
         pre-deploy `SUMMARY.txt` content the STW-034 publish-index chain wrote; \
         a `>` (truncate) regression would destroy the upstream `live_proof ...` \
         lines a CI dashboard scrapes in the same file"
    );
}

#[test]
fn deploy_dashboard_cloudflare_script_parses_with_bash_n() {
    // STW-061 static `bash -n` parse pin for the STW-054
    // Cloudflare Pages dashboard-deploy runbook. A future
    // edit that introduces a bash syntax error fails this
    // sub-test at the same CI step the sibling
    // `testnet_live_publish_*_script_exists_and_parses` /
    // `workspace_parallel_proof_three_script_exists_and_parses`
    // pinners follow, before any operator tries to invoke the
    // runbook in production.
    let p = deploy_dashboard_cloudflare_script_path();
    assert!(
        p.exists(),
        "STW-061 Cloudflare Pages dashboard-deploy runbook script missing at {}; \
         cannot run `bash -n` on a missing file",
        p.display()
    );
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/deploy-dashboard-cloudflare.sh");
    assert!(
        out.status.success(),
        "STW-061 Cloudflare Pages dashboard-deploy runbook script must parse with \
         `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
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

// STW-043 shell-shape pinner: the
// `scripts/commit-bench-fixture.sh` bench-result fixture
// shim is on disk + executable + parses with `bash -n`,
// AND its `strip_run_id` awk one-liner is present in the
// script source so a future refactor that drops the
// strip pass fails CI before a downstream auditor can
// `cat` a non-byte-stable committed fixture. Mirrors the
// STW-019 / STW-032 / STW-033 / STW-034 / STW-035 / STW-036
// / STW-037 file-on-disk + bash-n pins; the second pin
// is the STW-043-specific shape contract the fixture's
// byte-stability depends on (the `BenchReport::to_json`
// format string STW-010 / STW-017 / STW-031 emits carries
// `run_id` + `started_at_utc`, so the shim's strip pass
// is the *only* thing that turns the per-run output
// into a byte-stable committed fixture a stranger can
// grep).

fn commit_bench_fixture_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("commit-bench-fixture.sh")
}

#[test]
fn commit_bench_fixture_script_exists_and_parses() {
    // The STW-043 bench-fixture shim must be on disk,
    // executable, and parse with `bash -n`. A regression
    // that drops the file (or breaks the bash grammar)
    // fails the gate at CI time before a CI worker can
    // shell out to it. Mirrors the STW-019 / STW-032 /
    // STW-033 / STW-034 / STW-035 / STW-036 / STW-037
    // file-on-disk + bash-n pinners.
    let p = commit_bench_fixture_script_path();
    assert!(
        p.exists(),
        "STW-043 bench-fixture shim missing at {}; \
         the committed bench result has no shell entry point",
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
        // out to the shim.
        assert!(
            mode & 0o100 != 0,
            "STW-043 bench-fixture shim at {} must have its owner-executable bit set \
             (got mode {mode:o})",
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
    // bench-fixture re-run.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/commit-bench-fixture.sh");
    assert!(
        out.status.success(),
        "STW-043 bench-fixture shim must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn commit_bench_fixture_script_strips_run_id_fields() {
    // The shim's `strip_run_id` helper must be present
    // in the script source AND must reference the two
    // per-run fields (`run_id` + `started_at_utc`) the
    // STW-043 byte-stability contract is pinned on. A
    // regression that drops the strip pass (or renames
    // the helper) lets `run_id` / `started_at_utc` leak
    // into the committed fixture, breaking the
    // byte-stability promise the `bench_report_fixture.
    // json.sha256` sidecar pins. The fixture's
    // SHA256 + the script's source are the two
    // auditable surfaces; a future regression in
    // either fails CI.
    let script = read(&commit_bench_fixture_script_path());
    assert!(
        script.contains("strip_run_id"),
        "STW-043 bench-fixture shim must define a `strip_run_id` helper; \
         the byte-stability contract depends on the strip pass being \
         present in the script source"
    );
    assert!(
        script.contains("\"run_id\""),
        "STW-043 bench-fixture shim's strip pass must reference the `run_id` \
         field name (`\"run_id\"`); a future `BenchReport::to_json` revision \
         that drops `run_id` should also drop this pin"
    );
    assert!(
        script.contains("\"started_at_utc\""),
        "STW-043 bench-fixture shim's strip pass must reference the \
         `started_at_utc` field name (`\"started_at_utc\"`); a future \
         `BenchReport::to_json` revision that drops `started_at_utc` \
         should also drop this pin"
    );
}

// ===========================================================================
// STW-045 shell-shape pinner: the
// `scripts/trainer-observe.sh` observability wrapper is on disk +
// executable + parses with `bash -n`, AND the script source
// contains the `emit_step` helper + the `jq` one-liner + the
// `trainer observe complete: exit=` summary trailer. Mirrors the
// STW-019 / STW-032 / STW-033 / STW-034 / STW-035 / STW-036
// / STW-037 / STW-043 file-on-disk + bash-n pinners; the
// `emit_step` / `jq` / summary-trailer pinners are the
// STW-045-specific shape contract a downstream CI dashboard
// depends on (a `jq -c . <output-jsonl>` round-trip + a
// `jq -c 'select(.stream == "summary")' <output-jsonl>`
// grep on the trailing line are the two scraper paths the
// wrapper's README contract publishes; a regression that
// drops `emit_step` (the per-line JSONL append) or renames
// the `summary` stream (the trailer-tag the CI dashboard
// `select`s on) silently breaks both scraper paths).
//
// The shell-shape integration test deliberately does not
// shell out to the wrapper itself: that would require
// either a fake trainer binary (a unit test) or a real
// `DATABASE_URL` (the `trainer_observe.rs` integration
// test); the shape pin is the *no-DB gate* that lets
// `cargo test --workspace` stay green even on machines
// that have no Postgres and no `trainer` binary on PATH.

fn trainer_observe_script_path() -> PathBuf {
    workspace_root().join("scripts").join("trainer-observe.sh")
}

#[test]
fn trainer_observe_script_exists_and_parses() {
    // The STW-045 observability wrapper script must be on
    // disk, executable, and parse with `bash -n`. A
    // regression that drops the file (or breaks the bash
    // grammar) fails the gate at CI time before a CI
    // worker can shell out to it. Mirrors the STW-019 /
    // STW-032 / STW-033 / STW-034 / STW-035 / STW-036 /
    // STW-037 / STW-043 file-on-disk + bash-n pinners.
    let p = trainer_observe_script_path();
    assert!(
        p.exists(),
        "STW-045 observability wrapper missing at {}; \
         the trainer wrapper has no shell entry point",
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
        // out to the wrapper.
        assert!(
            mode & 0o100 != 0,
            "STW-045 observability wrapper at {} must have its \
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
    // wrapper invocation.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/trainer-observe.sh");
    assert!(
        out.status.success(),
        "STW-045 observability wrapper must parse with `bash -n` (got exit {:?})\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn trainer_observe_script_emits_three_field_jsonl() {
    // The wrapper's per-line JSONL encoder is the contract
    // a CI dashboard scrapes. The encoder must (a) define
    // a `emit_step <stream> <line>` helper (the per-line
    // append the drainers call from the stderr / stdout
    // background subshells), (b) invoke `jq` to
    // JSON-escape the line content (the `line` field may
    // contain `"` / `\` / control chars that a hand-rolled
    // encoder would mishandle), and (c) produce a
    // three-field object `ts` / `stream` / `line` with the
    // right `ts` type. A regression that drops the helper,
    // substitutes a hand-rolled `sed` encoder, or flattens
    // the `ts` to a string breaks the CI dashboard's
    // `jq -c .` round-trip silently.
    let script = read(&trainer_observe_script_path());
    // (a) The `emit_step` helper must be defined.
    assert!(
        script.contains("emit_step() {"),
        "STW-045 observability wrapper must define an `emit_step()` helper that \
         writes one JSONL line; the per-line append is the contract a CI dashboard \
         scrapes"
    );
    // (b) The encoder must use `jq` (the JSON-escape path
    // is the only way a `line` field with embedded
    // double-quotes / backslashes / control chars survives
    // a `jq -c .` round-trip byte-stable).
    assert!(
        script.contains("jq -cn --arg ts"),
        "STW-045 observability wrapper must use `jq -cn --arg ts ...` to build the \
         JSONL line; a hand-rolled `sed` / `awk` encoder would mishandle embedded \
         `\"` / `\\` / control chars the trainer's log stream may carry"
    );
    assert!(
        script.contains("--arg line"),
        "STW-045 observability wrapper's `jq` encoder must pass the line content \
         via `--arg line` (the `--arg` form handles JSON-string escaping for us); \
         a `--argjson` / `--raw-input` substitution would change the `line` field \
         shape and break the CI dashboard's `jq` round-trip"
    );
    // (c) The three-field object shape must be present
    // in the script source. We assert by *string
    // presence* of the field names (a regression that
    // renames a field name must update this pin in the
    // same change).
    for field in &["\"ts\":", "\"stream\":", "\"line\":"] {
        assert!(
            script.contains(field),
            "STW-045 observability wrapper's `jq` encoder must produce a `{field}` \
             field; the CI dashboard's `jq -c .` round-trip and `select(.stream == \
             \"summary\")` grep both depend on these field names being byte-stable"
        );
    }
    // (c.ii) The `ts` field must be cast to a number
    // (`($ts|tonumber)`) so the CI dashboard can do
    // arithmetic on it (per-step duration = current_ts -
    // prior_ts) without an extra `tonumber` round-trip.
    assert!(
        script.contains("($ts|tonumber)"),
        "STW-045 observability wrapper's `jq` encoder must cast the `ts` field to a \
         number via `($ts|tonumber)`; a `ts` field that survives as a string forces \
         every CI dashboard consumer to do an extra `tonumber` round-trip"
    );
    // (c.iii) The three stream values the wrapper
    // publishes must appear in the source. A regression
    // that drops `stderr` / `stdout` / `summary` (or
    // renames them to `err` / `out` / `trailer`) silently
    // breaks the `select(.stream == "summary")` CI
    // dashboard path.
    for stream in &["\"stderr\"", "\"stdout\"", "\"summary\""] {
        assert!(
            script.contains(stream),
            "STW-045 observability wrapper's JSONL stream tag must include `{stream}`; \
             a CI dashboard `select(.stream == \"summary\")` grep depends on every \
             stream tag being present and byte-stable"
        );
    }
}

#[test]
fn trainer_observe_script_summary_trailer_format_is_pinned() {
    // The wrapper's per-run `summary` trailer line is the
    // one-line-per-run summary a CI dashboard
    // `jq -c 'select(.stream == "summary")' <output-jsonl>`
    // consumes. The trailer's `line` field must be
    // a fixed-shape
    // `trainer observe complete: exit=<rc> cmd=<argv...>`
    // string. A regression that renames the prefix
    // (`trainer observe complete:` → `trainer_observe done:`)
    // or drops the `exit=` / `cmd=` key=value pairs
    // silently breaks the dashboard's grep / parse.
    let script = read(&trainer_observe_script_path());
    // (1) The prefix must be present.
    assert!(
        script.contains("trainer observe complete: exit="),
        "STW-045 observability wrapper must emit a `trainer observe complete: exit=...` \
         summary trailer line; the CI dashboard's `select(.stream == \"summary\")` grep \
         depends on the `trainer observe complete:` prefix being byte-stable"
    );
    // (2) The two key=value pairs (`exit=$TRAINER_RC` +
    // `cmd=${TRAINER_ARGV[*]}`) must appear in the
    // summary-line template, in that order, so a dashboard
    // scraper can `grep -oE 'exit=[0-9]+' <output-jsonl>`
    // to extract the per-run exit code without parsing
    // JSON.
    let exit_idx = script
        .find("exit=$TRAINER_RC")
        .expect("STW-045 summary trailer must include `exit=$TRAINER_RC`");
    let cmd_idx = script
        .find("cmd=${TRAINER_ARGV[*]}")
        .expect("STW-045 summary trailer must include `cmd=${TRAINER_ARGV[*]}`");
    assert!(
        exit_idx < cmd_idx,
        "STW-045 summary trailer key=value pairs must appear in order exit, cmd (got \
         `cmd=` before `exit=`, or `cmd=` not present after `exit=`) so a CI dashboard \
         scraper can `grep -oE 'exit=[0-9]+ cmd=.*'` and receive a stable per-run \
         summary"
    );
}

// --- STW-060 dashboard-fixtures INDEX.json tracking pin --------------
//
// The STW-036 dashboard crate's
// `crates/dashboard/tests/fixtures/INDEX.json` demo
// fixture must be tracked by `git ls-files` AND
// non-empty AND parse as JSON, so a CI worker
// running `cargo test -p rbp-dashboard --test smoke`
// from a fresh `git clone` (or `git clean -fdx`
// against a tracked-only checkout) finds the
// fixture the smoke test's `IndexClient::from_path`
// read expects. The pin runs at the *static*
// `script_shape.rs` layer (not the `smoke.rs`
// runtime layer) so a `git clean` regression
// fails at the cheapest possible CI step —
// the same single-source-of-truth pattern the
// sibling
// `testnet_live_publish_*_script_exists_and_parses`
// pinners follow. Mirrors the STW-019 +
// STW-032 + STW-033 + STW-034 + STW-035 +
// STW-036 + STW-037 + STW-043 + STW-045 + STW-054
// static-shape contract.

/// Path to the STW-060 `INDEX.json` demo fixture the
/// dashboard's `tests/fixtures/` folder ships. The
/// path is resolved relative to the workspace root
/// (the same convention `deploy_dashboard_cloudflare_script_path`
/// follows) so a CI worker running
/// `git ls-files` from any subdir lands on the
/// right file.
fn dashboard_fixtures_index_json_path() -> PathBuf {
    workspace_root()
        .join("crates")
        .join("dashboard")
        .join("tests")
        .join("fixtures")
        .join("INDEX.json")
}

#[test]
fn dashboard_fixtures_index_json_is_tracked_and_nonempty() {
    // (a) `git ls-files <path>` must exit 0 and print
    // the path on stdout. A non-zero exit means the
    // fixture is untracked / ignored / missing from
    // the index — a future `git clean -fdx` would
    // delete the file and the dashboard's smoke
    // test would 500 on `IndexClient::from_path`'s
    // `Io` error. We assert by exit code (not by
    // stdout substring) so the test fails on a
    // missing file regardless of the workspace's
    // exact path layout.
    let p = dashboard_fixtures_index_json_path();
    let rel = p
        .strip_prefix(&workspace_root())
        .unwrap_or(&p)
        .display()
        .to_string();
    let out = std::process::Command::new("git")
        .arg("ls-files")
        .arg("--error-unmatch")
        .arg("--")
        .arg(&rel)
        .current_dir(&workspace_root())
        .output()
        .expect("spawn git ls-files crates/dashboard/tests/fixtures/INDEX.json");
    assert!(
        out.status.success(),
        "STW-060 dashboard-fixtures `INDEX.json` must be tracked by git at {rel} \
         (got exit {:?}); a `git clean -fdx` would delete the file and the \
         `rbp-dashboard` smoke test would 500 on the next `IndexClient::from_path` \
         read. Run `git add crates/dashboard/tests/fixtures/INDEX.json` to track it.\n\
         --- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    // (b) The file must be non-empty. `git ls-files`
    // exits 0 on a tracked empty file, so a
    // hand-authoring regression that `git add`s an
    // empty placeholder still fails the smoke test
    // (an empty body fails `serde_json::from_str`
    // with a "EOF while parsing" error). The
    // non-empty check is the cheapest possible
    // guard at the static layer.
    let body = std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    assert!(
        !body.trim().is_empty(),
        "STW-060 dashboard-fixtures `INDEX.json` at {} must be non-empty (got {} bytes); \
         an empty fixture fails the dashboard smoke test's `IndexClient::from_path` \
         parse with a `serde_json` EOF error",
        p.display(),
        body.len()
    );
    // (c) The body must parse as JSON. A regression
    // that hand-edits the fixture and breaks the
    // JSON syntax (e.g. drops a trailing comma)
    // fails the dashboard smoke test's typed
    // `PublishIndex` read; the static pin catches
    // the same regression at the cheapest possible
    // CI step. We parse via `serde_json::Value`
    // (a shape-blind parse) so the pin is robust
    // to future `PublishIndex` shape changes —
    // the shape contract is the `smoke.rs`
    // integration test's job, not this static
    // pin's.
    let _v: serde_json::Value = serde_json::from_str(&body).unwrap_or_else(|e| {
        panic!(
            "STW-060 INDEX.json at {} must parse as JSON: {e}",
            p.display()
        )
    });
}

// ===========================================================================
// STW-078 testnet-postgres-env-provisioning runbook shape contract
// ===========================================================================
//
// `scripts/setup-testnet-postgres.sh` is the pure-bash, idempotent,
// no-`docker` script the `scripts/testnet-live-proof.sh` runbook
// assumes when it reads `DATABASE_URL` / `DB_URL` from the env. The
// receipt `receipts/testnet-live-proof-20260609T060233Z/` (the most
// recent runbook invocation as of 2026-06-09) shows the `doctor`
// step failed with `"db_reachable":false,"detail":"SELECT 1 failed:
// psql: error: connection to server at \"127.0.0.1\", port 5433
// failed: FATAL:  password authentication failed for user
// \"rbp_live\""` — the `rbp_live` user's password is not
// reproducible across reboots, and the *operator-runnable
// provisioning script* the receipt chain assumes is missing. STW-078
// ships that script; the two sub-tests below pin its shell-shape
// contract (no DB required) so a regression in the script's surface
// (file missing, syntax broken, executable bit cleared, env-file
// shape drift) fails CI before it ever reaches a live Postgres.
//
// The two sub-tests:
//
// 1. `setup_testnet_postgres_script_exists_and_parses` (STW-078) —
//    the script is on disk, has its owner-executable bit set, and
//    parses with `bash -n`. A regression that drops the file (or
//    breaks the bash grammar) fails the gate at CI time before a
//    CI worker can shell out to it.
// 2. `setup_testnet_postgres_script_writes_env_file` (STW-078) —
//    the script sources a `cat > "$PG_ENV_FILE" <<ENV ... ENV`
//    heredoc whose body, after bash interpolation tokens are
//    substituted, parses as the expected
//    `DATABASE_URL=postgres://user:***@host:port/dbname` +
//    `DB_URL=...` + `RBP_TESTNET_PG_*` env-file shape. The
//    companion runbook doc
//    `scripts/setup-testnet-postgres.md` must also reference the
//    script (so a worker reading the doc can find the script by
//    name). The end-to-end integration test
//    `crates/autotrain/tests/setup_testnet_postgres.rs` additionally
//    drives the script against fake Postgres binaries in a clean
//    tmpdir; that integration test is the behavioural pinner, this
//    pair is the *no-DB* shape pinner.

fn setup_testnet_postgres_script_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("setup-testnet-postgres.sh")
}

fn setup_testnet_postgres_doc_path() -> PathBuf {
    workspace_root()
        .join("scripts")
        .join("setup-testnet-postgres.md")
}

#[test]
fn setup_testnet_postgres_script_exists_and_parses() {
    // The provisioning script must be on disk, executable, and
    // parse with `bash -n`. A regression that drops the file
    // (or breaks the bash grammar) fails the gate at CI time
    // before a CI worker can shell out to it.
    let p = setup_testnet_postgres_script_path();
    assert!(
        p.exists(),
        "STW-078 testnet-postgres provisioning script missing at {}; \
         the testnet live proof runbook has no env producer",
        p.display()
    );
    let meta = std::fs::metadata(&p).unwrap_or_else(|e| panic!("stat {}: {e}", p.display()));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = meta.permissions().mode();
        // The owner-executable bit must be set; a future
        // `chmod -x` regression (e.g. a cross-checkout that
        // strips the bit) fails the test before a worker
        // tries to shell out to the script.
        assert!(
            mode & 0o100 != 0,
            "STW-078 provisioning script at {} must have its \
             owner-executable bit set (got mode {mode:o})",
            p.display()
        );
    }
    #[cfg(not(unix))]
    {
        // On non-Unix hosts we can only assert the file is
        // present; the bash -n check below covers the "is
        // the file actually a bash script" question.
        let _ = meta;
    }
    // `bash -n` parses the script without executing it. A
    // non-zero exit (a syntax error) fails the test so a
    // future edit that breaks the bash grammar fails CI
    // before it reaches a live Postgres.
    let out = std::process::Command::new("bash")
        .arg("-n")
        .arg(&p)
        .output()
        .expect("spawn bash -n scripts/setup-testnet-postgres.sh");
    assert!(
        out.status.success(),
        "STW-078 provisioning script must parse with `bash -n` \
         (got exit {:?})\n--- stdout ---\n{}\n--- stderr ---\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn setup_testnet_postgres_script_writes_env_file() {
    // The script must source a `cat > "$PG_ENV_FILE" <<ENV ... ENV`
    // heredoc whose body, after bash interpolation tokens are
    // substituted, parses as the expected
    // `DATABASE_URL=postgres://user:***@host:port/dbname` +
    // `DB_URL=...` + `RBP_TESTNET_PG_*` env-file shape. The
    // end-to-end `setup_testnet_postgres.rs` integration test
    // is the *behavioural* pinner (it actually runs the
    // script with fake `initdb` / `pg_ctl` / `postgres` /
    // `psql` / `createuser` / `createdb` shims); this test
    // is the no-DB *shape* pinner that catches a regression
    // in the heredoc body (e.g. a missing `DB_URL=` line,
    // or a typo in the `postgres://` URL shape) before
    // CI ever spins up the fakes.
    let script = read(&setup_testnet_postgres_script_path());
    // The script must source a `cat > "$PG_ENV_FILE" <<ENV` heredoc
    // (unquoted, so bash interpolates `${PG_USER}` /
    // `${PG_PASSWORD}` / `${PG_PORT}` / `${PG_DATABASE}` at
    // write time — the operator's `source` of the resulting
    // file must see literal `host:port:user` values, not bash
    // variables that resolve to nothing in the operator's shell).
    assert!(
        script.contains("cat > \"$PG_ENV_FILE\" <<ENV"),
        "STW-078 provisioning script must source a `cat > \"$PG_ENV_FILE\" <<ENV ... ENV` \
         heredoc that writes the env file the runbook reads; a regression that drops the \
         heredoc makes the `DATABASE_URL` the runbook reads unreproducible"
    );
    // The env-file body must carry the four `RBP_TESTNET_PG_*`
    // operator-knob keys the runbook honours, so a worker
    // who `source`s the env file gets a known contract.
    for key in &[
        "DATABASE_URL=postgres://",
        "DB_URL=postgres://",
        "RBP_TESTNET_PG_PORT=",
        "RBP_TESTNET_PG_USER=",
        "RBP_TESTNET_PG_PASSWORD=",
        "RBP_TESTNET_PG_DATABASE=",
    ] {
        assert!(
            script.contains(key),
            "STW-078 provisioning script env-file heredoc must include `{key}`; \
             the env file the runbook sources is missing a required contract field"
        );
    }
    // Mechanically extract the heredoc body and substitute the
    // bash interpolation tokens so the result is a parseable
    // `KEY=VALUE` env-file (one assignment per line). The
    // heredoc terminator is unquoted (`<<ENV`) so the
    // `${PG_USER}` / `${PG_PASSWORD}` / `${PG_PORT}` /
    // `${PG_DATABASE}` / `${PG_DATA_DIR}` / `${PG_LOG_DIR}`
    // tokens are *literal text* in the script source. We
    // find the *last* `<<ENV` (the actual env-file emitter;
    // the comment block above the emitter also mentions
    // `<<ENV` in prose, and the generic
    // `extract_heredoc_body` helper returns the first hit),
    // substitute each bash token with a known-value
    // string, and assert the result looks like a real env
    // file (every non-comment, non-empty line matches
    // `^KEY=VALUE$`).
    //
    // We use a custom extractor here (rather than the
    // shared `extract_heredoc_body` helper above) because
    // the script has a *comment* that mentions `<<ENV`
    // in prose — the shared helper returns the comment's
    // `<<ENV` token and we want the emitter's `<<ENV` token.
    fn extract_last_heredoc_body(script: &str, tag: &str) -> Option<String> {
        let needle = format!("<<{tag}");
        // Find every line that contains `<<TAG`; pick the
        // last one (the actual emitter, not the prose
        // comment above it).
        let mut last_open_idx: Option<usize> = None;
        for (i, line) in script.lines().enumerate() {
            if line.contains(&needle) {
                last_open_idx = Some(i);
            }
        }
        let open_idx = last_open_idx?;
        let mut body: Vec<&str> = Vec::new();
        for line in script.lines().skip(open_idx + 1) {
            if line.trim() == tag {
                return Some(body.join("\n"));
            }
            body.push(line);
        }
        None
    }
    let body = extract_last_heredoc_body(&script, "ENV")
        .expect("STW-078 provisioning script must source a <<ENV ... ENV heredoc");
    let substituted = body
        .replace("${PG_USER}", "rbp_live")
        .replace("${PG_PASSWORD}", "rbp_live")
        .replace("${PG_PORT}", "5433")
        .replace("${PG_DATABASE}", "rbp_live")
        .replace("${PG_DATA_DIR}", "/tmp/rbp-testnet-postgres/data")
        .replace("${PG_LOG_DIR}", "/tmp/rbp-testnet-postgres/log");
    let mut saw_database_url = false;
    let mut saw_db_url = false;
    let mut saw_port = false;
    for line in substituted.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        // Every non-comment, non-empty line must be a
        // `KEY=VALUE` assignment (no shell metacharacters,
        // no backticks, no `$(...)` command substitution —
        // the operator's `source` of the file must be
        // safe-by-construction).
        assert!(
            !trimmed.contains('`') && !trimmed.contains("$("),
            "STW-078 provisioning script env-file heredoc must not contain shell \
             metacharacters (` or $(...)) — a regression that re-introduces \
             command substitution makes `source`ing the env file unsafe. \
             Offending line: `{trimmed}`"
        );
        let eq_idx = trimmed.find('=').unwrap_or_else(|| {
            panic!(
                "STW-078 provisioning script env-file heredoc line is not a \
                 `KEY=VALUE` assignment: `{trimmed}`"
            )
        });
        let key = &trimmed[..eq_idx];
        let value = &trimmed[eq_idx + 1..];
        assert!(
            !key.is_empty()
                && !value.is_empty()
                && key
                    .chars()
                    .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_'),
            "STW-078 provisioning script env-file heredoc line has a malformed \
             `KEY=VALUE` shape: `{trimmed}` (key must be `[A-Z0-9_]+`, value \
             must be non-empty)"
        );
        if key == "DATABASE_URL" {
            saw_database_url = true;
            assert!(
                value.starts_with("postgres://")
                    && value.contains("rbp_live:rbp_live@127.0.0.1:5433/rbp_live"),
                "STW-078 env-file `DATABASE_URL` must match the runbook's expected \
                 `postgres://user:pass@host:port/dbname` shape; got `{value}`"
            );
        }
        if key == "DB_URL" {
            saw_db_url = true;
            assert!(
                value.starts_with("postgres://")
                    && value.contains("rbp_live:rbp_live@127.0.0.1:5433/rbp_live"),
                "STW-078 env-file `DB_URL` must match the runbook's expected \
                 `postgres://user:pass@host:port/dbname` shape; got `{value}`"
            );
        }
        if key == "RBP_TESTNET_PG_PORT" {
            saw_port = true;
            assert_eq!(
                value, "5433",
                "STW-078 env-file `RBP_TESTNET_PG_PORT` must round-trip to the \
                 same value the heredoc substituted; got `{value}`"
            );
        }
    }
    assert!(
        saw_database_url && saw_db_url && saw_port,
        "STW-078 provisioning script env-file heredoc must carry `DATABASE_URL` + \
         `DB_URL` + `RBP_TESTNET_PG_PORT` assignments (saw database_url={saw_database_url} \
         db_url={saw_db_url} port={saw_port})"
    );
    // The companion runbook doc must reference the script
    // by file name so a worker reading the doc can find it.
    let doc = read(&setup_testnet_postgres_doc_path());
    assert!(
        doc.contains("scripts/setup-testnet-postgres.sh"),
        "STW-078 provisioning doc at {} must reference the script by file name; \
         a worker reading the doc would not know how to invoke the provisioner",
        setup_testnet_postgres_doc_path().display()
    );
    // The doc must mention the headline / env-file handoff so
    // a worker knows the `source .auto/testnet-postgres.env`
    // step is the contract.
    assert!(
        doc.contains(".auto/testnet-postgres.env") || doc.contains("RBP_TESTNET_PG_ENV_FILE"),
        "STW-078 provisioning doc must reference the env-file handoff the script writes; \
         a worker reading the doc would not know to `source` the env file"
    );
    // The script must print the pinned `testnet-postgres: complete: ...`
    // headline so a CI dashboard scraper has a stable regex to grep.
    assert!(
        script.contains("testnet-postgres: complete: port=")
            && script.contains("testnet-postgres: already provisioned"),
        "STW-078 provisioning script must print both the `complete:` and the \
         `already provisioned` headline lines; a CI dashboard scraper greps the \
         `complete:` line for the receipt"
    );
}

fn readme_path() -> PathBuf {
    workspace_root().join("README.md")
}

fn replay_locally_script_path() -> PathBuf {
    workspace_root().join("scripts").join("replay-locally.sh")
}

#[test]
fn stw_046_replay_locally_script_remains_dropped() {
    let p = replay_locally_script_path();
    assert!(
        !p.exists(),
        "STW-046 dropped the `scripts/replay-locally.sh` shim as busywork; \
         a regression that re-adds it fails this static pin. \
         Remove the file or update this test if the drop decision is reversed."
    );
}

#[test]
fn stw_046_readme_try_it_now_section_remains_dropped() {
    let readme = read(&readme_path());
    assert!(
        !readme.contains("## Try it now"),
        "STW-046 dropped the README `## Try it now` section as busywork; \
         the existing `## Quick Start` + `## TUI Preview` + `## Testnet launch proof` \
         + `## Testnet publish bundle` + `## Public dashboard` sections already cover \
         the first-time-visitor path. A regression that re-adds `## Try it now` \
         fails this static pin."
    );
}
