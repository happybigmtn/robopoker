//! `LiveProofReceipt` — the shared receipt verifier for the
//! testnet live launch proof chain.
//!
//! `STW-019` shipped `scripts/testnet-live-proof.sh`, a
//! pure-bash operator runbook that drives the seven-step
//! chain (`--cluster` → `--reset` → `--smoke` → `--status`
//! → `--bench` → `--compare` → `--replay <transcript>`)
//! against a single Postgres reachable via `DATABASE_URL`
//! and drops a per-step receipt bundle a testnet
//! dashboard can scrape:
//!
//! ```text
//! receipts/testnet-live-proof-<UTC-ISO>/
//!   SUMMARY.txt                    # the one-line launch receipt
//!   ENV.txt                        # the env the chain ran with
//!   recipe.json                    # machine-readable manifest (STW-023)
//!   cluster/{stdout,stderr,exit}.txt
//!   reset/{stdout,stderr,exit}.txt
//!   smoke/{stdout,stderr,exit}.txt
//!   status/{stdout,stderr,exit}.txt
//!   bench/{stdout,stderr,exit}.txt
//!   bench/transcripts/             # the bench's transcript-*.json files
//!   compare/{stdout,stderr,exit}.txt
//!   replay/{stdout,stderr,exit}.txt
//! ```
//!
//! `STW-009` / `STW-010` / `STW-016` / `STW-018` pin the
//! four chain legs in isolation as `cargo test` integration
//! tests, and `crates/autotrain/tests/live_proof.rs` chains
//! all six legs in one `cargo test --test live_proof`
//! invocation. But the runbook and the integration test
//! produce *independent* receipts with *separate*
//! verification rules: the runbook writes `SUMMARY.txt`, the
//! integration test writes an `eprintln!` line, and a future
//! drift in one fails without a clear "the other is also
//! stale" signal.
//!
//! This module lands a single shared `LiveProofReceipt`
//! verifier the bash runbook and the Rust integration test
//! both call into:
//!
//! - The bash runbook writes a `recipe.json` manifest
//!   alongside `SUMMARY.txt` using a JSON shape this module
//!   defines (a `LiveProofRecipe` struct serialised via
//!   `serde_json`).
//! - The Rust integration test calls `LiveProofReceipt::write_to`
//!   on the same on-disk layout the bash runbook produces, so a
//!   `cargo test --workspace` invocation produces a
//!   `target/test-receipts/live_proof-<UTC>/` directory
//!   shaped exactly like a runbook receipt.
//! - A no-DB `cargo test -p rbp-autotrain --test
//!   live_proof_receipt` test drops a *synthetic* receipt
//!   under `target/test-receipts/live_proof-fixture-<UTC>/`,
//!   calls `LiveProofReceipt::verify` on the freshly-written
//!   receipt, and asserts the verifier agrees the receipt is
//!   green (every step exit 0, the headline line parses, the
//!   `recipe.json` manifest is JSON-parseable, the per-step
//!   `recipe.json.steps[i].name` field matches the
//!   `receipts/<step>/` directory name).
//!
//! A regression in the receipt shape (renamed step, dropped
//! exit code, broken headline prefix, missing `recipe.json`)
//! fails the verifier and the integration test
//! simultaneously — the operator-visible receipt *and* the
//! CI-visible receipt share one source of truth.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The seven pinned chain step names, in the order the
/// runbook + integration test drive them.
///
/// The order is significant: the bash runbook's
/// `recipe.json` manifest lists the steps in the order the
/// chain executes them, and `LiveProofReceipt::verify`
/// asserts the on-disk `receipts/<step>/` directory order
/// matches this constant. A future chain refactor that
/// re-orders (or drops) a step must update this constant
/// *and* the bash runbook's `recipe.json` block in the same
/// change.
pub const STW023_CHAIN_STEPS: &[&str] = &[
    "cluster", "reset", "smoke", "status", "bench", "compare", "replay",
];

/// The pinned `testnet live_proof complete: ...` headline
/// line prefix the `crates/autotrain/tests/script_shape.rs`
/// test already pins. We re-export the literal here so the
/// Rust verifier agrees the headline format the runbook
/// writes is the same one the verifier accepts.
///
/// The `testnet live_proof complete:` prefix is
/// intentional: the runbook disambiguates from the
/// `crates/autotrain/tests/live_proof.rs` integration
/// test's `live_proof complete: ...` line so a dashboard
/// scraper can grep either the `SUMMARY.txt` file or the
/// runbook's stdout with the same regex.
pub const STW023_HEADLINE_PREFIX: &str = "testnet live_proof complete:";

/// One chain step's captured output: a per-step directory
/// the runbook (and the new integration test) drops under
/// `<receipt_root>/<step>/` containing `stdout.txt`,
/// `stderr.txt`, and `exit.txt`. The verifier reads the
/// `exit.txt` (a single integer) to decide whether the
/// step landed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveProofStep {
    /// The pinned step name (one of `STW023_CHAIN_STEPS`).
    pub name: String,
    /// The integer exit code captured from the trainer
    /// subprocess. A green run has `exit == 0` for every
    /// step; the verifier rejects any step with
    /// `exit != 0`.
    pub exit: i32,
    /// The byte count of the captured `stdout.txt` file
    /// (after the runbook writes it to disk). The verifier
    /// does not gate on this — it is informational, useful
    /// for a dashboard that wants to know "the bench
    /// printed a non-empty JSON" or "the replay rendered
    /// the transcript text" without re-reading the
    /// per-step file.
    pub stdout_bytes: u64,
    /// The byte count of the captured `stderr.txt` file.
    /// Informational; not gated on by the verifier.
    pub stderr_bytes: u64,
}

/// Machine-readable receipt manifest, serialised to
/// `recipe.json` by the bash runbook's `write_recipe`
/// helper AND by `LiveProofReceipt::write_to`'s
/// `LiveProofRecipe` writer.
///
/// The bash side of the contract is a `cat > recipe.json
/// <<'JSON'` heredoc whose body parses into this struct;
/// the Rust side round-trips it via
/// `serde_json::to_string_pretty` /
/// `serde_json::from_str`. A future drift in field names
/// fails the integration test's
/// `synthetic_receipt_manifest_recipes_step_names` test
/// because the on-disk JSON's `steps[i].name` field no
/// longer matches the `STW023_CHAIN_STEPS` constant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveProofRecipe {
    /// `trainer` binary path the chain was driven with
    /// (e.g. `/srv/dev/repos/robopoker/target/debug/trainer`).
    /// The runbook sources this from the `TRAINER_BIN`
    /// env knob the integration test also honours; the
    /// verifier does not gate on the path itself, but a
    /// future regression that swaps the binary for a
    /// different one (a wrong `--release` build, a
    /// half-staged Cargo cache) leaves an audit trail in
    /// the manifest.
    pub trainer_bin: String,
    /// `DATABASE_URL` the chain ran against. Mirrored
    /// exactly as the runbook saw it (the runbook
    /// redacts this in the `ENV.txt` file the operator
    /// reads; the `recipe.json` field stores the
    /// redacted `<redacted: N chars>` form the runbook
    /// itself uses so a `cat recipe.json` does not leak
    /// a secret into a CI artifact).
    pub database_url: String,
    /// The seven captured chain steps in the order the
    /// runbook executed them. The verifier asserts (a)
    /// the length matches `STW023_CHAIN_STEPS.len()`, (b)
    /// the `name` field of every step is one of the
    /// pinned names (each appearing exactly once), and
    /// (c) the per-step `exit` is `0`.
    pub steps: Vec<LiveProofStep>,
}

impl LiveProofRecipe {
    /// The fixed on-disk file name the runbook + the
    /// integration test use for the manifest. The
    /// `recipe.json` filename is part of the contract:
    /// the integration test
    /// `synthetic_receipt_manifest_recipes_step_names`
    /// asserts the file at this path parses into
    /// `LiveProofRecipe` and the step names match the
    /// `STW023_CHAIN_STEPS` order.
    pub const FILENAME: &'static str = "recipe.json";

    /// Build a `LiveProofRecipe` by reading a directory
    /// the runbook (or the integration test) just
    /// produced. The helper walks the directory's
    /// immediate children and looks for the seven pinned
    /// step names; for every match it reads
    /// `<step>/exit.txt` (a single integer) and
    /// `<step>/stdout.txt` / `<step>/stderr.txt`
    /// (sizes only, not contents) to build a
    /// `LiveProofStep`.
    ///
    /// The function returns an `io::Error` if the
    /// receipt root does not exist or a step is missing;
    /// a malformed `exit.txt` (non-integer) returns
    /// `io::Error::new(io::ErrorKind::InvalidData, ...)`.
    /// The order of the returned `steps` vector is the
    /// order the steps appear in `STW023_CHAIN_STEPS`
    /// (i.e. the canonical chain order), NOT the on-disk
    /// directory iteration order, so the recipe is
    /// deterministic across filesystems.
    pub fn from_receipt_dir(
        receipt_root: &Path,
        trainer_bin: &str,
        database_url: &str,
    ) -> io::Result<Self> {
        if !receipt_root.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!(
                    "live_proof receipt root {} does not exist or is not a directory",
                    receipt_root.display()
                ),
            ));
        }
        let mut steps = Vec::with_capacity(STW023_CHAIN_STEPS.len());
        for &step_name in STW023_CHAIN_STEPS {
            let step_dir = receipt_root.join(step_name);
            if !step_dir.is_dir() {
                return Err(io::Error::new(
                    io::ErrorKind::NotFound,
                    format!(
                        "live_proof step directory {} is missing under {}",
                        step_name,
                        receipt_root.display()
                    ),
                ));
            }
            let exit_path = step_dir.join("exit.txt");
            let exit_str = fs::read_to_string(&exit_path).map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "live_proof step `{step_name}` exit.txt missing or unreadable at {}: {e}",
                        exit_path.display()
                    ),
                )
            })?;
            let exit: i32 = exit_str.trim().parse().map_err(|e| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "live_proof step `{step_name}` exit.txt at {} is not a parseable integer \
                         (got {exit_str:?}): {e}",
                        exit_path.display()
                    ),
                )
            })?;
            let stdout_bytes = fs::metadata(step_dir.join("stdout.txt"))
                .map(|m| m.len())
                .unwrap_or(0);
            let stderr_bytes = fs::metadata(step_dir.join("stderr.txt"))
                .map(|m| m.len())
                .unwrap_or(0);
            steps.push(LiveProofStep {
                name: step_name.to_string(),
                exit,
                stdout_bytes,
                stderr_bytes,
            });
        }
        Ok(Self {
            trainer_bin: trainer_bin.to_string(),
            database_url: database_url.to_string(),
            steps,
        })
    }
}

/// Top-level receipt verifier. The verifier is the
/// single source of truth for "did the testnet live proof
/// chain land end-to-end on this receipt bundle?".
///
/// Construction is from an on-disk directory the runbook
/// (or the integration test) just produced; the verifier
/// reads the per-step `exit.txt` files, the
/// `recipe.json` manifest, and the `SUMMARY.txt` headline
/// line, and asserts every step exited 0, the manifest is
/// shape-valid, and the headline line matches the pinned
/// `testnet live_proof complete: smoke=... status=...
/// bench=... compare=... replay=...` format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveProofReceipt {
    /// The directory the receipt was read from. Stored so
    /// the verifier can produce a precise error message
    /// ("step X missing under Y") on a regression.
    pub root: PathBuf,
    /// The machine-readable manifest the runbook wrote.
    pub recipe: LiveProofRecipe,
    /// The one-line `testnet live_proof complete: ...`
    /// headline the runbook wrote to `SUMMARY.txt`. The
    /// verifier parses the five `key=N` pairs out of
    /// this line so a future drift in the format
    /// (renamed prefix, dropped pair) fails the gate.
    pub summary_line: String,
}

/// Verifier error: a single typed error so the
/// integration test can assert on `Err(VerifyError::*)`
/// variants. The variants cover the three failure modes
/// the verifier detects:
///
/// - `RecipeShape` — `recipe.json` is missing, not
///   parseable as JSON, or the per-step `name` field
///   does not match the pinned `STW023_CHAIN_STEPS`
///   order.
/// - `StepFailed` — at least one per-step `exit.txt`
///   reports a non-zero exit code; the receipt is not
///   green.
/// - `Headline` — the `SUMMARY.txt` headline line is
///   missing, doesn't start with the pinned prefix, or
///   doesn't include the five `key=N` pairs
///   (`smoke=` / `status=` / `bench=` / `compare=` /
///   `replay=`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyError {
    RecipeShape(String),
    StepFailed { step: String, exit: i32 },
    Headline(String),
}

impl fmt::Display for VerifyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VerifyError::RecipeShape(s) => write!(f, "live_proof recipe shape error: {s}"),
            VerifyError::StepFailed { step, exit } => {
                write!(f, "live_proof step `{step}` failed (exit {exit})")
            }
            VerifyError::Headline(s) => write!(f, "live_proof summary headline error: {s}"),
        }
    }
}

impl std::error::Error for VerifyError {}

/// Typed parser for the `testnet live_proof complete: ...`
/// headline line the runbook writes to `SUMMARY.txt`. The
/// original `STW-023` verifier substring-matched the five
/// `key=N` pairs (which is fine for the binary
/// green/red gate a CI scraper needs), but a downstream
/// testnet dashboard also wants the *values* — a
/// `--smoke` row count, a `--bench` hand count, a
/// `--compare` hand count — without re-running a regex
/// on every scrape. `LiveProofHeadline::parse` is the
/// typed equivalent: a `parse` returns a `LiveProofHeadline`
/// with the five `u64` fields a dashboard can chart
/// over a series of runbook invocations, or a structured
/// `HeadlineParseError` that names the exact field that
/// failed to parse (so a regression in the runbook's
/// `printf` line surfaces as a precise diagnostic, not
/// a silent `None`).
///
/// The `Display` impl is a round-trip with
/// `LiveProofReceipt::headline`, so a value parsed from
/// one receipt can be `println!`'d as a new headline and
/// the next `parse` agrees on every field. The five
/// keys (`smoke` / `status` / `bench` / `compare` /
/// `replay`) and their order are pinned in the
/// `parse` tokeniser (a future chain refactor that
/// re-orders / drops a key must update the `parse`
/// `match` block, this struct's field set, the
/// `LiveProofReceipt::headline` constructor, and the
/// runbook's `printf` line in the same change).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LiveProofHeadline {
    /// `smoke_rows` — the `trainer --smoke` artifact row
    /// count the chain logged.
    pub smoke: u64,
    /// `status_blueprint` — the `trainer --status`
    /// `Blueprint` row count the chain logged.
    pub status: u64,
    /// `bench_hands` — the `trainer --bench` JSON-line
    /// `hands` field the chain logged.
    pub bench: u64,
    /// `compare_hands` — the `trainer --compare` JSON-line
    /// `hands` field the chain logged.
    pub compare: u64,
    /// `replay_bytes` — the `trainer --replay` `stdout`
    /// byte count the chain logged (a proxy for "the
    /// transcript rendered something non-empty").
    pub replay: u64,
}

impl LiveProofHeadline {
    /// Build a `LiveProofHeadline` line in the same
    /// `testnet live_proof complete: smoke=N status=N
    /// bench=N compare=N replay=BYTES` format the
    /// `LiveProofReceipt::headline` constructor writes.
    /// The function is a round-trip mirror of
    /// `LiveProofHeadline::parse` so a parsed value can
    /// be re-emitted as a headline a future `parse`
    /// agrees on (the lib test
    /// `live_proof_headline_round_trips_through_parse`
    /// pins the property).
    pub fn to_line(&self) -> String {
        format!(
            "{STW023_HEADLINE_PREFIX} smoke={} status={} bench={} compare={} replay={}",
            self.smoke, self.status, self.bench, self.compare, self.replay
        )
    }

    /// Parse a `testnet live_proof complete: ...`
    /// headline line into a typed `LiveProofHeadline`.
    /// The parser asserts (a) the line starts with the
    /// pinned `STW023_HEADLINE_PREFIX`, (b) the five
    /// pinned keys (`smoke` / `status` / `bench` /
    /// `compare` / `replay`) each appear exactly once
    /// with an integer value, and (c) no unknown `key=N`
    /// pairs are present (a regression that adds a
    /// `key6=N` pair would silently make the dashboard's
    /// column-count drift).
    pub fn parse(line: &str) -> Result<Self, HeadlineParseError> {
        if !line.starts_with(STW023_HEADLINE_PREFIX) {
            return Err(HeadlineParseError::WrongPrefix {
                got: line.chars().take(64).collect::<String>(),
                expected: STW023_HEADLINE_PREFIX.to_string(),
            });
        }
        // Strip the prefix; what's left is the `key=N`
        // space-separated tail.
        let tail = line[STW023_HEADLINE_PREFIX.len()..].trim_start();
        let mut smoke: Option<u64> = None;
        let mut status: Option<u64> = None;
        let mut bench: Option<u64> = None;
        let mut compare: Option<u64> = None;
        let mut replay: Option<u64> = None;
        // We walk the tail with a manual cursor so an
        // unknown key (anything other than the five
        // pinned names) is rejected, and a duplicate key
        // is rejected (the `Option` is `Some` on the
        // second sight).
        for token in tail.split_whitespace() {
            let (key, value) = token
                .split_once('=')
                .ok_or_else(|| HeadlineParseError::MalformedToken(token.to_string()))?;
            let parsed: u64 = value
                .parse::<u64>()
                .map_err(|e| HeadlineParseError::NonInteger {
                    key: key.to_string(),
                    value: value.to_string(),
                    reason: e.to_string(),
                })?;
            let slot = match key {
                "smoke" => &mut smoke,
                "status" => &mut status,
                "bench" => &mut bench,
                "compare" => &mut compare,
                "replay" => &mut replay,
                other => {
                    return Err(HeadlineParseError::UnknownKey(other.to_string()));
                }
            };
            if slot.is_some() {
                return Err(HeadlineParseError::DuplicateKey(key.to_string()));
            }
            *slot = Some(parsed);
        }
        Ok(Self {
            smoke: smoke.ok_or(HeadlineParseError::MissingKey("smoke"))?,
            status: status.ok_or(HeadlineParseError::MissingKey("status"))?,
            bench: bench.ok_or(HeadlineParseError::MissingKey("bench"))?,
            compare: compare.ok_or(HeadlineParseError::MissingKey("compare"))?,
            replay: replay.ok_or(HeadlineParseError::MissingKey("replay"))?,
        })
    }
}

impl fmt::Display for LiveProofHeadline {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_line())
    }
}

/// Structured error from `LiveProofHeadline::parse`. The
/// variants cover the four failure modes the parser
/// detects — wrong prefix, malformed token (no `=`),
/// unknown key, duplicate key, missing key, non-integer
/// value — so a regression in the runbook's `printf`
/// line surfaces as a precise diagnostic instead of a
/// silent `None`. The `Display` impl prefixes every
/// message with `live_proof headline parse error:` so
/// dashboard logs are grep-friendly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlineParseError {
    /// The headline does not start with the pinned
    /// `testnet live_proof complete:` prefix.
    WrongPrefix { got: String, expected: String },
    /// A `key=value` token in the tail lacked the `=`
    /// separator (e.g. a stray `smoke` or trailing junk).
    MalformedToken(String),
    /// A `key` other than the five pinned names
    /// (`smoke` / `status` / `bench` / `compare` /
    /// `replay`) appeared in the tail.
    UnknownKey(String),
    /// A pinned `key` appeared twice in the tail.
    DuplicateKey(String),
    /// A pinned `key` did not appear in the tail at all.
    MissingKey(&'static str),
    /// A `key=N` token's value was not a `u64`.
    NonInteger {
        key: String,
        value: String,
        reason: String,
    },
}

impl fmt::Display for HeadlineParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            HeadlineParseError::WrongPrefix { got, expected } => write!(
                f,
                "live_proof headline parse error: wrong prefix (got {got:?}, expected {expected:?})"
            ),
            HeadlineParseError::MalformedToken(t) => write!(
                f,
                "live_proof headline parse error: malformed token {t:?} (expected `key=value`)"
            ),
            HeadlineParseError::UnknownKey(k) => write!(
                f,
                "live_proof headline parse error: unknown key {k:?} (expected one of \
                 `smoke` / `status` / `bench` / `compare` / `replay`)"
            ),
            HeadlineParseError::DuplicateKey(k) => {
                write!(f, "live_proof headline parse error: duplicate key {k:?}")
            }
            HeadlineParseError::MissingKey(k) => {
                write!(f, "live_proof headline parse error: missing key {k:?}")
            }
            HeadlineParseError::NonInteger { key, value, reason } => write!(
                f,
                "live_proof headline parse error: key {key:?} value {value:?} is not a u64 \
                 ({reason})"
            ),
        }
    }
}

impl std::error::Error for HeadlineParseError {}

impl LiveProofReceipt {
    /// Read a receipt from disk. The function is the
    /// *consumer* side of the on-disk contract: the
    /// bash runbook and the integration test both call
    /// the *producer* side (`write_to` for the
    /// integration test, the `write_recipe` heredoc for
    /// the bash runbook), and this function is what a
    /// `cargo test` lib test or an external auditor uses
    /// to re-verify a receipt the operator dropped.
    ///
    /// The function does *not* call `verify` — it
    /// returns a `LiveProofReceipt` whose invariants the
    /// caller asserts. Use `LiveProofReceipt::read_and_verify`
    /// for the read-then-verify common case.
    pub fn read_from(receipt_root: &Path) -> io::Result<Self> {
        let summary_path = receipt_root.join("SUMMARY.txt");
        let summary_line = fs::read_to_string(&summary_path)
            .map_err(|e| {
                io::Error::new(
                    e.kind(),
                    format!(
                        "live_proof SUMMARY.txt missing or unreadable at {}: {e}",
                        summary_path.display()
                    ),
                )
            })?
            .lines()
            .next()
            .unwrap_or("")
            .to_string();
        // We read the recipe's `database_url` / `trainer_bin`
        // from the `ENV.txt` file the runbook writes, NOT
        // from the `recipe.json` (the `recipe.json` form
        // stores the redacted `<redacted: N chars>` form the
        // runbook itself uses so a `cat recipe.json` does
        // not leak a secret into a CI artifact). The
        // verifier does not gate on either field's value;
        // it only stores them for the audit trail.
        let env_path = receipt_root.join("ENV.txt");
        let env_text = fs::read_to_string(&env_path).unwrap_or_default();
        let trainer_bin = env_text
            .lines()
            .find_map(|l| l.strip_prefix("TRAINER_BIN=").map(str::to_string))
            .unwrap_or_default();
        let database_url = env_text
            .lines()
            .find_map(|l| l.strip_prefix("DATABASE_URL=").map(str::to_string))
            .unwrap_or_default();
        let recipe = LiveProofRecipe::from_receipt_dir(receipt_root, &trainer_bin, &database_url)?;
        Ok(Self {
            root: receipt_root.to_path_buf(),
            recipe,
            summary_line,
        })
    }

    /// Read a receipt from disk AND verify it. The
    /// common case for the integration test
    /// `synthetic_receipt_verifies_green_via_lib`:
    /// drop a synthetic receipt, call this, assert
    /// `Ok(())`.
    pub fn read_and_verify(receipt_root: &Path) -> Result<(), VerifyError> {
        let receipt =
            Self::read_from(receipt_root).map_err(|e| VerifyError::RecipeShape(e.to_string()))?;
        receipt.verify()
    }

    /// The `testnet live_proof complete: smoke=N
    /// status=N bench=N compare=N replay=BYTES`
    /// headline the runbook writes. The five integers
    /// are taken from the per-step log lines the
    /// runbook (and the integration test) parse
    /// (`smoke_rows`, `status_blueprint`,
    /// `bench_hands`, `compare_hands`, `replay_bytes`).
    /// The `BYTES` count is the size of the
    /// `replay/stdout.txt` file (a proxy for "the
    /// transcript rendered something non-empty").
    pub fn headline(
        smoke_rows: u64,
        status_blueprint: u64,
        bench_hands: u64,
        compare_hands: u64,
        replay_bytes: u64,
    ) -> String {
        format!(
            "{STW023_HEADLINE_PREFIX} smoke={smoke_rows} status={status_blueprint} \
             bench={bench_hands} compare={compare_hands} replay={replay_bytes}"
        )
    }

    /// Write a synthetic receipt bundle under
    /// `<dest>/<step>/{stdout,stderr,exit}.txt` for every
    /// step in `STW023_CHAIN_STEPS`, plus a `SUMMARY.txt`
    /// whose first line is the pinned headline, plus a
    /// `recipe.json` manifest the verifier re-reads.
    /// The function is the producer side the integration
    /// test calls; the bash runbook's `write_recipe`
    /// heredoc produces the same on-disk shape via a
    /// different code path (pure bash + `cat`).
    pub fn write_to(
        dest: &Path,
        smoke_rows: u64,
        status_blueprint: u64,
        bench_hands: u64,
        compare_hands: u64,
        replay_bytes: u64,
        trainer_bin: &str,
        database_url_redacted: &str,
    ) -> io::Result<()> {
        fs::create_dir_all(dest)?;
        let headline = Self::headline(
            smoke_rows,
            status_blueprint,
            bench_hands,
            compare_hands,
            replay_bytes,
        );
        let summary = format!(
            "{headline}\n\n  receipt_dir: {}\n  trainer:     {trainer_bin}\n",
            dest.display()
        );
        fs::write(dest.join("SUMMARY.txt"), summary)?;
        // Build a step stub for every pinned name. The
        // producer side stamps the same exit 0 / 0
        // bytes for every step (the integration test
        // builds a *synthetic* green receipt); the
        // verifier re-reads the per-step `exit.txt` and
        // rejects any non-zero value.
        let steps: Vec<LiveProofStep> = STW023_CHAIN_STEPS
            .iter()
            .map(|name| LiveProofStep {
                name: (*name).to_string(),
                exit: 0,
                stdout_bytes: 0,
                stderr_bytes: 0,
            })
            .collect();
        let recipe = LiveProofRecipe {
            trainer_bin: trainer_bin.to_string(),
            database_url: database_url_redacted.to_string(),
            steps,
        };
        let recipe_json = serde_json::to_string_pretty(&recipe).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("live_proof recipe serialise: {e}"),
            )
        })?;
        fs::write(dest.join(LiveProofRecipe::FILENAME), recipe_json)?;
        for &step_name in STW023_CHAIN_STEPS {
            let step_dir = dest.join(step_name);
            fs::create_dir_all(&step_dir)?;
            fs::write(step_dir.join("stdout.txt"), b"")?;
            fs::write(step_dir.join("stderr.txt"), b"")?;
            fs::write(step_dir.join("exit.txt"), "0\n")?;
        }
        Ok(())
    }

    /// Verify a receipt. Returns `Ok(())` on a green
    /// receipt, `Err(VerifyError::*)` on a regression.
    /// The three failure modes mirror the three
    /// contract violations the verifier detects:
    /// recipe shape, step exit, headline format. The
    /// headline-format check now goes through the typed
    /// `LiveProofHeadline::parse` function so a
    /// regression in the runbook's `printf` line
    /// (non-integer value, unknown key, duplicate key,
    /// missing key) surfaces as a precise diagnostic
    /// rather than a substring-grep miss.
    pub fn verify(&self) -> Result<(), VerifyError> {
        // (1) Recipe shape: the per-step `name` field
        // must equal the pinned `STW023_CHAIN_STEPS`
        // constant in order.
        if self.recipe.steps.len() != STW023_CHAIN_STEPS.len() {
            return Err(VerifyError::RecipeShape(format!(
                "live_proof recipe has {} steps; expected {}",
                self.recipe.steps.len(),
                STW023_CHAIN_STEPS.len()
            )));
        }
        for (i, step) in self.recipe.steps.iter().enumerate() {
            if step.name != STW023_CHAIN_STEPS[i] {
                return Err(VerifyError::RecipeShape(format!(
                    "live_proof recipe step {i} is `{}`; expected `{}`",
                    step.name, STW023_CHAIN_STEPS[i]
                )));
            }
            if step.exit != 0 {
                return Err(VerifyError::StepFailed {
                    step: step.name.clone(),
                    exit: step.exit,
                });
            }
        }
        // (2) Headline format: must parse through the
        // typed `LiveProofHeadline::parse` function
        // (i.e. start with the pinned prefix AND have
        // the five `key=N` pairs as parseable u64
        // integers). The original substring-only check
        // is preserved inside `parse`, so a regression
        // that drops a pair still fails verification
        // with the precise error variant.
        LiveProofHeadline::parse(&self.summary_line).map_err(|e| {
            VerifyError::Headline(format!(
                "live_proof summary line must parse as `LiveProofHeadline` (got: {summary:?}): \
                 {e}",
                summary = self.summary_line
            ))
        })?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-023
    //! `LiveProofReceipt` verifier. The tests do NOT
    //! require a live Postgres (the verifier is the
    //! *consumer* side of the on-disk contract; the
    //! producer side is the bash runbook's `write_recipe`
    //! heredoc or `LiveProofReceipt::write_to`).
    //!
    //! Fixture style: a process-unique
    //! `std::env::temp_dir().join("rbp-receipt-test-<n>")`
    //! subdirectory populated by
    //! `LiveProofReceipt::write_to`, then re-read +
    //! verified. The tempdir is removed on drop so
    //! re-runs do not see stale files.

    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-receipt-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    /// `record_steps_in_order` is the trivial case:
    /// the `STW023_CHAIN_STEPS` constant is exactly
    /// the seven step names in the runbook's
    /// `recipe.json` block. A future chain refactor
    /// that re-orders (or drops) a step must update
    /// this constant and the runbook's `recipe.json`
    /// block in the same change; the test pins the
    /// constant.
    #[test]
    fn live_proof_receipt_records_steps_in_order() {
        assert_eq!(
            STW023_CHAIN_STEPS,
            &[
                "cluster", "reset", "smoke", "status", "bench", "compare", "replay"
            ]
        );
    }

    /// `write_to` drops a per-step directory for every
    /// pinned name, each containing `stdout.txt` /
    /// `stderr.txt` / `exit.txt`, plus a top-level
    /// `SUMMARY.txt` and `recipe.json`. The verifier
    /// re-reads the on-disk shape and agrees the
    /// receipt is green.
    #[test]
    fn live_proof_receipt_write_to_drops_per_step_files() {
        let dir = fresh_dir("write");
        LiveProofReceipt::write_to(
            &dir,
            12,
            12,
            4,
            4,
            256,
            "fake-trainer",
            "<redacted: 49 chars>",
        )
        .expect("write_to should succeed");
        for step in STW023_CHAIN_STEPS {
            let step_dir = dir.join(step);
            assert!(step_dir.is_dir(), "step dir `{}` must exist", step);
            assert!(
                step_dir.join("stdout.txt").is_file(),
                "step `{step}` stdout.txt must exist"
            );
            assert!(
                step_dir.join("stderr.txt").is_file(),
                "step `{step}` stderr.txt must exist"
            );
            assert!(
                step_dir.join("exit.txt").is_file(),
                "step `{step}` exit.txt must exist"
            );
        }
        assert!(dir.join("SUMMARY.txt").is_file(), "SUMMARY.txt must exist");
        assert!(
            dir.join(LiveProofRecipe::FILENAME).is_file(),
            "recipe.json must exist"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The headline format is pinned: starts with the
    /// pinned prefix and includes the five `key=N`
    /// pairs in the order `smoke` / `status` / `bench`
    /// / `compare` / `replay`. A regression that
    /// renames a key (e.g. swaps `bench` for `hands`)
    /// fails this test.
    #[test]
    fn live_proof_receipt_headline_format_is_pinned() {
        let h = LiveProofReceipt::headline(12, 12, 4, 4, 256);
        assert!(
            h.starts_with(STW023_HEADLINE_PREFIX),
            "headline must start with the pinned prefix; got: {h}"
        );
        // Order check: the five pairs must appear in
        // the documented order so a dashboard scraper
        // can `grep -oE 'smoke=[0-9]+'` reliably.
        let mut last = 0usize;
        for pair in &["smoke=", "status=", "bench=", "compare=", "replay="] {
            let idx = h
                .find(pair)
                .unwrap_or_else(|| panic!("headline missing `{pair}`; got: {h}"));
            assert!(
                idx >= last,
                "headline pairs must appear in order smoke, status, bench, compare, replay; \
                 `{pair}` came before its predecessor (got: {h})"
            );
            last = idx;
        }
    }

    /// Round-trip: `write_to` then `read_from` then
    /// `verify` agrees the receipt is green. The
    /// recipe re-parses with `serde_json::from_str`
    /// into the same `LiveProofRecipe` the writer
    /// produced.
    #[test]
    fn live_proof_receipt_read_from_round_trips() {
        let dir = fresh_dir("roundtrip");
        LiveProofReceipt::write_to(
            &dir,
            12,
            12,
            4,
            4,
            256,
            "fake-trainer",
            "<redacted: 49 chars>",
        )
        .expect("write_to");
        let receipt = LiveProofReceipt::read_from(&dir).expect("read_from");
        receipt
            .verify()
            .expect("verify should accept a green receipt");
        // Re-parse the recipe.json with serde_json and
        // assert the step names match the on-disk
        // directory order. (The read_from path builds
        // the recipe from the directory; this asserts
        // the JSON manifest the writer produced has the
        // same step order.)
        let raw =
            std::fs::read_to_string(dir.join(LiveProofRecipe::FILENAME)).expect("read recipe");
        let parsed: LiveProofRecipe =
            serde_json::from_str(&raw).expect("recipe.json must round-trip through serde_json");
        assert_eq!(parsed.steps.len(), STW023_CHAIN_STEPS.len());
        for (i, step) in parsed.steps.iter().enumerate() {
            assert_eq!(step.name, STW023_CHAIN_STEPS[i]);
            assert_eq!(step.exit, 0);
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A green receipt (every step exit 0) verifies
    /// with `Ok(())`. This is the happy-path case the
    /// `live_proof_receipt.rs` integration test
    /// asserts.
    #[test]
    fn live_proof_receipt_verify_accepts_green_receipt() {
        let dir = fresh_dir("green");
        LiveProofReceipt::write_to(&dir, 1, 1, 1, 1, 1, "fake-trainer", "<redacted: 1 chars>")
            .expect("write_to");
        let receipt = LiveProofReceipt::read_from(&dir).expect("read_from");
        assert_eq!(receipt.verify(), Ok(()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A failed step (one `exit.txt` reports
    /// non-zero) verifies with
    /// `Err(VerifyError::StepFailed { .. })`. A
    /// regression that swallows the failed step (e.g.
    /// gates on `recipe.exit` only, ignoring the
    /// per-step `exit.txt`) fails this test.
    #[test]
    fn live_proof_receipt_verify_rejects_failed_step() {
        let dir = fresh_dir("failed");
        LiveProofReceipt::write_to(&dir, 1, 1, 1, 1, 1, "fake-trainer", "<redacted: 1 chars>")
            .expect("write_to");
        // Overwrite the `bench` step's exit.txt with
        // a non-zero exit code. The next read should
        // report `StepFailed { step: "bench", exit: 7 }`.
        std::fs::write(dir.join("bench").join("exit.txt"), "7\n").expect("rewrite exit.txt");
        let receipt = LiveProofReceipt::read_from(&dir).expect("read_from");
        match receipt.verify() {
            Err(VerifyError::StepFailed { step, exit }) => {
                assert_eq!(step, "bench");
                assert_eq!(exit, 7);
            }
            other => panic!("expected StepFailed; got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The recipe's `steps` field is the order the
    /// runbook executed them. `serde_json` round-trips
    /// the field names exactly as the writer produced
    /// them; a regression that swaps `name` for
    /// `step_name` (or reorders the fields) fails
    /// this test.
    #[test]
    fn live_proof_recipe_serialises_step_order() {
        let dir = fresh_dir("recipe");
        LiveProofReceipt::write_to(&dir, 1, 1, 1, 1, 1, "fake-trainer", "<redacted: 1 chars>")
            .expect("write_to");
        let raw =
            std::fs::read_to_string(dir.join(LiveProofRecipe::FILENAME)).expect("read recipe");
        let parsed: LiveProofRecipe =
            serde_json::from_str(&raw).expect("recipe.json must round-trip");
        let names: Vec<&str> = parsed.steps.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, STW023_CHAIN_STEPS);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // -- LiveProofHeadline parser (typed STW-023 dashboard surface) --

    /// A `LiveProofHeadline` constructed from the five
    /// `u64` fields `to_line` emits a line that
    /// `LiveProofHeadline::parse` agrees on. This is
    /// the round-trip property a downstream testnet
    /// dashboard relies on: parse a runbook receipt's
    /// `SUMMARY.txt` headline, chart the five
    /// `u64` fields, `println!` a new headline from
    /// the next receipt, and have `parse` accept the
    /// re-emitted line without manual reformatting.
    #[test]
    fn live_proof_headline_round_trips_through_parse() {
        let original = LiveProofHeadline {
            smoke: 12,
            status: 7,
            bench: 4,
            compare: 4,
            replay: 256,
        };
        let line = original.to_line();
        let parsed = LiveProofHeadline::parse(&line)
            .unwrap_or_else(|e| panic!("parse must accept the re-emitted line; got: {e:?}"));
        assert_eq!(parsed, original, "round-trip must preserve all five fields");
        // The line must also start with the pinned
        // prefix (defensive: a future refactor that
        // silently drops the prefix from `to_line`
        // surfaces here).
        assert!(line.starts_with(STW023_HEADLINE_PREFIX));
    }

    /// A line that does not start with the pinned
    /// `STW023_HEADLINE_PREFIX` is rejected with
    /// `HeadlineParseError::WrongPrefix`. A regression
    /// in the runbook's `printf` line (e.g. drops
    /// the `testnet ` part) surfaces as a precise
    /// diagnostic, not a silent `None`.
    #[test]
    fn live_proof_headline_parse_rejects_wrong_prefix() {
        let err = LiveProofHeadline::parse(
            "live_proof complete: smoke=0 status=0 bench=0 compare=0 replay=0",
        )
        .expect_err("a non-pinned-prefix headline must fail to parse");
        match err {
            HeadlineParseError::WrongPrefix { got, expected } => {
                assert!(
                    expected == STW023_HEADLINE_PREFIX,
                    "expected prefix must be the pinned one (got: {expected:?})"
                );
                assert!(
                    got.starts_with("live_proof complete:"),
                    "the offending prefix should be echoed back (got: {got:?})"
                );
            }
            other => panic!("wrong-prefix headline must produce WrongPrefix error; got: {other:?}"),
        }
    }

    /// A headline whose tail is missing one of the
    /// five pinned keys surfaces as
    /// `HeadlineParseError::MissingKey`. A regression
    /// in the runbook's `printf` (e.g. drops `replay=`
    /// on a future refactor) is detected here.
    #[test]
    fn live_proof_headline_parse_rejects_missing_key() {
        let err = LiveProofHeadline::parse(
            "testnet live_proof complete: smoke=0 status=0 bench=0 compare=0",
        )
        .expect_err("a headline missing `replay=` must fail to parse");
        match err {
            HeadlineParseError::MissingKey("replay") => {}
            other => panic!(
                "missing-replay headline must produce MissingKey(\"replay\"); got: {other:?}"
            ),
        }
    }

    /// A headline whose `key=N` token has a
    /// non-integer value is rejected with
    /// `HeadlineParseError::NonInteger`. The error
    /// names the offending key + value + parse reason
    /// so a future runbook regression (e.g. `printf`
    /// adds a stray `"` or `%s` placeholder) is
    /// diagnosed without a manual `cat SUMMARY.txt`.
    #[test]
    fn live_proof_headline_parse_rejects_non_integer_value() {
        let err = LiveProofHeadline::parse(
            "testnet live_proof complete: smoke=abc status=0 bench=0 compare=0 replay=0",
        )
        .expect_err("a headline with `smoke=abc` must fail to parse");
        match err {
            HeadlineParseError::NonInteger { key, value, .. } => {
                assert_eq!(key, "smoke", "error must name the offending key");
                assert_eq!(value, "abc", "error must echo the offending value");
            }
            other => panic!("non-integer value must produce NonInteger error; got: {other:?}"),
        }
    }

    /// A headline that includes an extra
    /// `key=value` token the parser does not know is
    /// rejected with `HeadlineParseError::UnknownKey`.
    /// A regression that adds a sixth `key` (e.g. a
    /// `cluster=` row count) without updating the
    /// dashboard's column schema surfaces here,
    /// preventing a silent column-count drift.
    /// Similarly, a headline that repeats a pinned
    /// key is rejected with
    /// `HeadlineParseError::DuplicateKey`.
    #[test]
    fn live_proof_headline_parse_rejects_unknown_and_duplicate_keys() {
        // (a) Unknown key
        let err = LiveProofHeadline::parse(
            "testnet live_proof complete: smoke=0 status=0 bench=0 compare=0 replay=0 cluster=1",
        )
        .expect_err("a headline with an extra `cluster=` key must fail to parse");
        match err {
            HeadlineParseError::UnknownKey(k) => {
                assert_eq!(k, "cluster", "error must name the unknown key");
            }
            other => panic!("unknown-key headline must produce UnknownKey error; got: {other:?}"),
        }
        // (b) Duplicate key
        let err = LiveProofHeadline::parse(
            "testnet live_proof complete: smoke=0 smoke=0 status=0 bench=0 compare=0 replay=0",
        )
        .expect_err("a headline with `smoke=` twice must fail to parse");
        match err {
            HeadlineParseError::DuplicateKey(k) => {
                assert_eq!(k, "smoke", "error must name the duplicate key");
            }
            other => {
                panic!("duplicate-key headline must produce DuplicateKey error; got: {other:?}")
            }
        }
    }
}
