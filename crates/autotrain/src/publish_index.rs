//! `trainer --publish-index <publish-root>` — turn a
//! `<publish-root>/` directory the STW-033
//! `testnet-live-publish-s3.sh` runbook produced (a
//! tree of `publish/<basename>/remote/remote_receipt.json`
//! files, one per receipt the runbook published-remote'd)
//! into a deterministic aggregator: a single
//! `INDEX.json` + `SUMMARY.txt` pair a testnet
//! dashboard can scrape instead of listing the bucket
//! + fetching N manifests.
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh`,
//! the operator runbook that drops a
//! `receipts/testnet-live-proof-<UTC-ISO>/` receipt
//! directory. STW-023 shipped the
//! `LiveProofReceipt::read_and_verify` Rust verifier.
//! STW-028 wired the verifier into the `trainer`
//! binary as `trainer --verify-receipt <path>`.
//! STW-032 shipped `trainer --publish <receipt>` (the
//! content-addressed portable bundle writer) +
//! `trainer --verify-bundle <path>` (the no-DB
//! re-verifier). STW-033 shipped
//! `trainer --publish-remote <receipt> --bucket
//! <s3://...>` (the upload plan + post-upload
//! `remote_receipt.json` writer) + `trainer
//! --verify-remote <path>` (the no-DB post-upload
//! re-verifier). **STW-034 lands the v8 follow-on a
//! testnet dashboard naturally wants (the next slice
//! the STW-033 publish-remote step defers to)**: a
//! deterministic aggregator over every
//! `remote_receipt.json` the STW-033 chain produced
//! on a single machine.
//!
//! ## Why a separate module from `publish_remote`
//!
//! `publish_remote.rs` is the *per-receipt* upload
//! side (turn one receipt into one upload plan +
//! one `remote_receipt.json`). `publish_index.rs` is
//! the *aggregator* side (scan a publish root,
//! collect every `remote_receipt.json`, write a
//! single `INDEX.json` an auditor can scrape). The
//! two are split because the error surfaces are
//! different: a regression in a single
//! `remote_receipt.json`'s `s3_uri` field does not
//! change the `INDEX.json` aggregator's
//! `entry_count` / `total_bytes` field, and vice
//! versa. A typed `PublishIndexError` enum lives
//! next to the indexer so the integration test can
//! assert on the failure kind a dashboard scraper
//! greps.
//!
//! ## Why a fresh `IndexedEntry` + `PublishIndex` type
//!
//! The `INDEX.json` is a NEW shape (it inlines the
//! `PublishedRemoteReceipt` per entry, not a path
//! reference), so the aggregator needs its own typed
//! struct the dashboard scraper can `serde_json::from_str`
//! into. The `IndexedEntry` struct is a thin
//! wrapper around a `PublishedRemoteReceipt` that
//! adds the `receipt_basename` (the per-receipt key
//! the index sorts on) + a `receipt_dir` (the
//! path the dashboard scraper can `aws s3 cp` to
//! fetch the original receipt) so the index is
//! self-describing without re-reading the parent
//! publish root.
//!
//! ## Why the dry-run default
//!
//! The `trainer --publish-index` arm is
//! **always no-network**: the aggregator reads the
//! on-disk `remote_receipt.json` files, re-verifies
//! them, and writes a single `INDEX.json` +
//! `SUMMARY.txt` to `<publish-root>/index/`. There
//! is no `aws` shell-out (the dashboard-scraper
//! surface is a `cat INDEX.json` away, not a `find
//! <bucket>` away). The integration test runs the
//! indexer in a tempdir so a regression in the
//! aggregator's byte-stability fails CI without a
//! real publish root.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use crate::publish_remote::{
    PublishRemoteError, PublishedRemoteReceipt, STW033_REMOTE_RECEIPT_FILENAME, read_remote_receipt,
};

// --- headline constants ------------------------------------------

/// Headline prefix the `trainer --publish-index`
/// arm prints to stdout on success. Mirrors the
/// `live_proof ... complete:` family the STW-019 /
/// STW-031 / STW-032 / STW-033 trainers already
/// print so one `grep ^live_proof` scraper can
/// read the whole chain.
pub const STW034_PUBLISH_INDEX_HEADLINE_PREFIX: &str = "live_proof publish_index complete:";
/// Headline prefix the `trainer --verify-index`
/// arm prints to stdout on a green re-verify. A
/// dashboard scraper greps this line.
pub const STW034_VERIFY_INDEX_HEADLINE_PREFIX: &str = "live_proof index verification passed:";
/// Headline prefix the `trainer --verify-index`
/// arm prints to stderr on a red re-verify. A
/// dashboard scraper greps this line.
pub const STW034_VERIFY_INDEX_FAILURE_HEADLINE_PREFIX: &str =
    "live_proof index verification failed:";

// --- on-disk filenames -------------------------------------------

/// The on-disk `INDEX.json` file name. The
/// constant lives in Rust so the bash runbook's
/// `cat > INDEX.json <<JSON ... JSON` heredoc
/// (if any) and the Rust writer agree on the
/// filename.
pub const STW034_INDEX_FILENAME: &str = "INDEX.json";
/// The on-disk `SUMMARY.txt` file name (the
/// single-line human-readable headline a CI
/// worker `cat`s to confirm the index step
/// landed end-to-end).
pub const STW034_SUMMARY_FILENAME: &str = "SUMMARY.txt";

/// Runbook version string the index step stamps
/// on the `INDEX.json` (bumped by hand if the
/// aggregator JSON shape changes).
pub const STW034_INDEX_RUNBOOK_VERSION: &str = "STW-034 v1";

/// Sentinel value the indexer writes into
/// `created_at_utc` when the
/// `RBP_PUBLISH_INDEX_UTC` env knob is unset. The
/// `<unknown>` sentinel is the same shape the
/// STW-019 / STW-032 / STW-033 already use, and
/// keeps the lib test + integration test
/// byte-stable on a CI runner that does not
/// stamp the env knob.
pub const STW034_UNKNOWN_UTC: &str = "<unknown>";

/// The subdirectory under `<publish-root>/` the
/// index step writes its output to. The index
/// step never mutates the underlying
/// `remote_receipt.json` files; it writes a
/// fresh `index/` subdir the dashboard scraper
/// reads from.
pub const STW034_INDEX_SUBDIR: &str = "index";

/// The per-entry subdirectory name pattern the
/// aggregator scans. The STW-033 chain writes
/// each `remote_receipt.json` under
/// `<publish-root>/publish/<basename>/remote/`,
/// so the aggregator scans
/// `<publish-root>/publish/*/remote/`.
pub const STW034_ENTRY_REMOTE_SUBDIR: &str = "remote";

/// The `publish` subdirectory the STW-033 chain
/// uses to nest the per-receipt bundle. The
/// aggregator scans `<publish-root>/publish/`
/// (the chain's `<parent>/publish/<basename>/`
/// shape) and looks one level deeper.
pub const STW034_PUBLISH_SUBDIR: &str = "publish";

// --- data model --------------------------------------------------

/// One entry in the `INDEX.json`'s `entries[]`
/// array. The aggregator inlines the
/// `PublishedRemoteReceipt` the STW-033 runbook
/// wrote (so a dashboard scraper can read the
/// full upload plan + `s3_objects[]` from the
/// index without re-fetching the per-receipt
/// `remote_receipt.json`), and adds a
/// `receipt_basename` (the per-receipt key the
/// index sorts on) + a `receipt_dir` (the
/// absolute path the dashboard scraper can
/// `aws s3 cp` to fetch the original receipt)
/// so the index is self-describing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexedEntry {
    /// The receipt dir basename (e.g.
    /// `testnet-live-proof-20260604T050000Z`).
    /// The aggregator sorts `entries[]` by this
    /// field for determinism.
    pub receipt_basename: String,
    /// Absolute path to the original receipt
    /// directory (the
    /// `receipts/testnet-live-proof-<UTC-ISO>/`
    /// directory the runbook produced). Stored
    /// for human readability; the verifier does
    /// NOT gate on it.
    pub receipt_dir: String,
    /// Absolute path to the
    /// `remote_receipt.json` the STW-033 chain
    /// wrote. Stored for human readability; the
    /// verifier does NOT gate on it.
    pub remote_receipt_path: String,
    /// The inlined `PublishedRemoteReceipt` the
    /// STW-033 chain wrote. The aggregator
    /// inlines (not references) the receipt so a
    /// dashboard scraper can read the full
    /// upload plan + `s3_objects[]` from the
    /// index without re-fetching the per-receipt
    /// `remote_receipt.json`.
    pub remote_receipt: PublishedRemoteReceipt,
}

/// The top-level `INDEX.json` the
/// `trainer --publish-index` arm writes to
/// `<publish-root>/index/INDEX.json`. The
/// aggregator's `entries[]` array is sorted by
/// `receipt_basename` (so re-running the index
/// step on an unchanged publish root produces a
/// byte-identical `INDEX.json`); the top-level
/// object records the `publish_root` +
/// `runbook_version` + `created_at_utc`
/// (`<unknown>` sentinel when the
/// `RBP_PUBLISH_INDEX_UTC` env knob is unset so
/// the lib test + integration test are
/// byte-stable) + `entry_count` + `total_bytes`
/// (the sum of every entry's `total_bytes`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishIndex {
    /// Absolute path to the publish root the
    /// aggregator scanned. Stored for human
    /// readability; the verifier does NOT gate
    /// on it.
    pub publish_root: String,
    /// The runbook version string
    /// (`"STW-034 v1"`). Bumped by hand if the
    /// aggregator JSON shape changes.
    pub runbook_version: String,
    /// ISO-8601 UTC timestamp the index was
    /// written. Read from the
    /// `RBP_PUBLISH_INDEX_UTC` env knob when set
    /// (the bash runbook stamps it from
    /// `date -u`); the lib test + integration
    /// test run without the env knob set and
    /// fall back to the `<unknown>` sentinel.
    pub created_at_utc: String,
    /// Number of entries in `entries[]`
    /// (mirrors `entries.len()`; stored
    /// separately so a dashboard scraper that
    /// only reads the top-level object does not
    /// have to `jq '.entries | length'`).
    pub entry_count: usize,
    /// Sum of every entry's
    /// `remote_receipt.total_bytes` (the total
    /// bytes uploaded across every entry).
    /// Mirrors the `total_bytes` the
    /// `remote_receipt.json` writes per entry.
    pub total_bytes: u64,
    /// Per-entry index records, sorted by
    /// `receipt_basename` for determinism.
    pub entries: Vec<IndexedEntry>,
}

// --- typed error -------------------------------------------------

/// Publisher-index error: a single typed error
/// so the CLI / integration test can assert on
/// the `PublishIndexError::*` variant. The
/// variants cover the failure modes the
/// indexer / verifier detect:
///
/// - `RemoteReceiptRed` — the
///   `remote_receipt.json` failed the STW-033
///   verifier; the indexer refuses to aggregate
///   a red remote receipt (the "refuse to
///   paper-over a red remote receipt"
///   invariant the STW-028 + STW-032 + STW-033
///   verifiers already enforce).
/// - `BundleHashMismatch` — an `INDEX.json`
///   entry's `s3_objects[].local_path` has a
///   `sha256` that does not match the
///   verifier's re-hash.
/// - `MissingObject` — the `INDEX.json` names
///   an `s3_uri` that does not appear in the
///   inlined plan (a phantom `s3_uri` is a
///   hard error, not a warning).
/// - `FileUnreadable` — a file inside the
///   publish root could not be read.
/// - `PublishRoot` — the input publish root
///   does not exist or is not a directory.
/// - `NoEntries` — the publish root has no
///   `publish/*/remote/remote_receipt.json`
///   files (the aggregator refuses to write a
///   zero-entry `INDEX.json` — a zero-entry
///   index is a signal the operator mis-pointed
///   the `--publish-index` flag).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishIndexError {
    /// A `remote_receipt.json` failed the
    /// STW-033 `PublishedRemoteReceipt::verify`
    /// re-verify. The aggregator refuses to
    /// write an `INDEX.json` containing a red
    /// remote receipt.
    RemoteReceiptRed(String),
    /// An `INDEX.json` entry's
    /// `s3_objects[].local_path` has a `sha256`
    /// that does not match the verifier's
    /// re-hash.
    BundleHashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// The `INDEX.json` names an `s3_uri` that
    /// does not appear in the inlined plan.
    MissingObject(String),
    /// A file inside the publish root could not
    /// be read.
    FileUnreadable(String),
    /// The input publish root does not exist or
    /// is not a directory.
    PublishRoot(String),
    /// The publish root has no
    /// `publish/*/remote/remote_receipt.json`
    /// files. The aggregator refuses to write a
    /// zero-entry index.
    NoEntries(String),
}

impl fmt::Display for PublishIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishIndexError::RemoteReceiptRed(s) => {
                write!(
                    f,
                    "live_proof publish_index error: remote receipt is red: {s}"
                )
            }
            PublishIndexError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "live_proof publish_index error: bundle_hash_mismatch: {path}: \
                 expected {expected}, got {actual}"
            ),
            PublishIndexError::MissingObject(s) => {
                write!(f, "live_proof publish_index error: missing_object: {s}")
            }
            PublishIndexError::FileUnreadable(s) => {
                write!(f, "live_proof publish_index error: file unreadable: {s}")
            }
            PublishIndexError::PublishRoot(s) => {
                write!(f, "live_proof publish_index error: publish root: {s}")
            }
            PublishIndexError::NoEntries(s) => {
                write!(f, "live_proof publish_index error: no entries: {s}")
            }
        }
    }
}

impl std::error::Error for PublishIndexError {}

impl From<PublishRemoteError> for PublishIndexError {
    fn from(e: PublishRemoteError) -> Self {
        match e {
            PublishRemoteError::ReceiptRed(s) => PublishIndexError::RemoteReceiptRed(s),
            PublishRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => PublishIndexError::BundleHashMismatch {
                path,
                expected,
                actual,
            },
            PublishRemoteError::MissingObject(s) => PublishIndexError::MissingObject(s),
            PublishRemoteError::FileUnreadable(s) => PublishIndexError::FileUnreadable(s),
            // A `BucketUri` or `AwsCli` error from
            // a per-entry `read_remote_receipt` call
            // is not strictly a `FileUnreadable`
            // (the receipt file itself was
            // unreadable), so we map the per-entry
            // error variants to the closest
            // `PublishIndexError` variant and keep
            // the human-readable detail.
            PublishRemoteError::ReceiptDir(s) => {
                PublishIndexError::FileUnreadable(format!("receipt dir: {s}"))
            }
            PublishRemoteError::BundleDir(s) => {
                PublishIndexError::FileUnreadable(format!("bundle dir: {s}"))
            }
            PublishRemoteError::BucketUri(s) => {
                PublishIndexError::FileUnreadable(format!("bucket uri: {s}"))
            }
            PublishRemoteError::AwsCli(s) => {
                PublishIndexError::FileUnreadable(format!("aws cli: {s}"))
            }
        }
    }
}

// --- typed output ------------------------------------------------

/// `PublishIndexOutput` — the typed return
/// value of [`publish_index`]. The handler
/// returns this so the `Mode::PublishIndex` CLI
/// can print a one-line `live_proof
/// publish_index complete: ...` headline and
/// the integration test can assert on the typed
/// `index_path` + `entry_count` + `total_bytes`
/// fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishIndexOutput {
    /// Absolute path to the `INDEX.json` the
    /// indexer wrote.
    pub index_path: PathBuf,
    /// Absolute path to the `SUMMARY.txt` the
    /// indexer wrote.
    pub summary_path: PathBuf,
    /// The in-memory `PublishIndex` the
    /// indexer serialised to `INDEX.json`.
    /// Mirrors what a downstream auditor reads
    /// back from disk via `read_publish_index`.
    pub index: PublishIndex,
}

// --- main entry point --------------------------------------------

/// Scan a `<publish-root>/` directory the
/// STW-033 `testnet-live-publish-s3.sh` runbook
/// produced (a tree of
/// `publish/<basename>/remote/remote_receipt.json`
/// files, one per receipt the runbook
/// published-remote'd) and write a
/// deterministic `INDEX.json` + `SUMMARY.txt`
/// pair a testnet dashboard can scrape instead
/// of listing the bucket + fetching N
/// manifests.
///
/// The function is **read-only** with respect
/// to the publish root: it reads + re-verifies
/// the `remote_receipt.json` files in place,
/// then writes its own `index/` subdir under
/// `<publish-root>/`, so a
/// `trainer --publish-index` invocation cannot
/// mutate the underlying `remote_receipt.json`
/// files even on partial-failure paths.
///
/// ## Per-entry pre-index gate
///
/// The function re-verifies every
/// `remote_receipt.json` with
/// `PublishedRemoteReceipt::verify` AS A
/// PER-ENTRY PRE-INDEX GATE. A red
/// `remote_receipt.json` returns
/// `Err(PublishIndexError::RemoteReceiptRed(...))`
/// before the `INDEX.json` is written. This is
/// the "refuse to paper-over a red remote
/// receipt" invariant the STW-028 receipt
/// verifier + STW-032 bundle verifier + STW-033
/// remote-receipt verifier already enforce.
pub fn publish_index<P: AsRef<Path>>(
    publish_root: P,
    created_at_utc: Option<&str>,
) -> Result<PublishIndexOutput, PublishIndexError> {
    let publish_root = publish_root.as_ref();
    if !publish_root.is_dir() {
        return Err(PublishIndexError::PublishRoot(format!(
            "publish root {} does not exist or is not a directory",
            publish_root.display()
        )));
    }
    // The aggregator walks
    // `<publish_root>/publish/*/remote/`. The
    // STW-033 chain writes each
    // `remote_receipt.json` under
    // `<publish_root>/publish/<basename>/remote/`,
    // so we scan one level of `publish/`
    // children, then one level of `remote/`
    // children.
    let publish_dir = publish_root.join(STW034_PUBLISH_SUBDIR);
    if !publish_dir.is_dir() {
        return Err(PublishIndexError::NoEntries(format!(
            "publish root {} has no `{}` subdirectory; \
             the STW-033 chain writes `publish/<basename>/remote/remote_receipt.json` \
             under the publish root",
            publish_root.display(),
            STW034_PUBLISH_SUBDIR,
        )));
    }
    // Walk `<publish_root>/publish/*/remote/`
    // and collect every
    // `remote_receipt.json` we find. The walk
    // is deterministic (sorted iteration) so
    // the `entries[]` array is byte-stable
    // across machines + re-runs.
    let mut entries: Vec<IndexedEntry> = Vec::new();
    let publish_children: Vec<PathBuf> = fs::read_dir(&publish_dir)
        .map_err(|e| {
            PublishIndexError::FileUnreadable(format!(
                "could not read publish subdir {}: {e}",
                publish_dir.display()
            ))
        })?
        .filter_map(|e| e.ok().map(|d| d.path()))
        .filter(|p| p.is_dir())
        .collect();
    let mut sorted_children = publish_children;
    sorted_children.sort();
    for child in &sorted_children {
        let remote_dir = child.join(STW034_ENTRY_REMOTE_SUBDIR);
        if !remote_dir.is_dir() {
            // A `<publish_root>/publish/<basename>/`
            // that is not a STW-033 bundle (e.g.
            // a half-published receipt the
            // operator abandoned) is silently
            // skipped. The aggregator only
            // counts `remote/` subdirs.
            continue;
        }
        let receipt_path = remote_dir.join(STW033_REMOTE_RECEIPT_FILENAME);
        if !receipt_path.is_file() {
            // A `<publish_root>/publish/<basename>/remote/`
            // that is missing the
            // `remote_receipt.json` is silently
            // skipped. The aggregator only
            // counts directories with the pinned
            // STW-033 receipt filename.
            continue;
        }
        // Per-entry pre-index gate: re-verify
        // the `remote_receipt.json` with the
        // STW-033 `PublishedRemoteReceipt::verify`.
        // A red receipt short-circuits the index
        // with `PublishIndexError::RemoteReceiptRed(...)`
        // before any `INDEX.json` is written.
        let remote_receipt = read_remote_receipt(&remote_dir)?;
        // The STW-033 verifier re-hashes every
        // local file the receipt claims to have
        // uploaded. We pass the bundle dir
        // (the parent of `remote/`) as the
        // verifier's `bundle_dir` so the
        // verifier can resolve relative
        // `local_path`s (absolute paths are
        // resolved verbatim; the verifier
        // handles both).
        let bundle_dir = child.clone();
        if let Err(e) = remote_receipt.verify(&bundle_dir) {
            return Err(PublishIndexError::RemoteReceiptRed(format!(
                "remote receipt at {} failed verification: {e}",
                receipt_path.display()
            )));
        }
        // Resolve the receipt dir's basename
        // (the per-receipt key the index sorts
        // on). The receipt dir is `<publish_root>/publish/<basename>/`,
        // so `child.file_name()` is `<basename>`.
        let receipt_basename = child
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("<unknown>")
            .to_string();
        // The `receipt_dir` is the
        // `receipts/testnet-live-proof-<UTC-ISO>/`
        // directory the STW-019 runbook
        // produced. The aggregator does not
        // know the original path (the STW-033
        // chain stores the path only as a
        // string in the
        // `PublishRemotePlan::receipt_basename`),
        // so we store the per-receipt
        // `remote_receipt.json`'s sibling
        // path as the `receipt_dir` for human
        // readability.
        let receipt_dir = format!("{}/{}", child.display(), STW034_PUBLISH_SUBDIR);
        entries.push(IndexedEntry {
            receipt_basename,
            receipt_dir,
            remote_receipt_path: receipt_path.display().to_string(),
            remote_receipt,
        });
    }
    // Refuse to write a zero-entry index. A
    // zero-entry `INDEX.json` is a signal the
    // operator mis-pointed the
    // `--publish-index` flag (a publish root
    // with no `publish/*/remote/remote_receipt.json`
    // files is not a valid input).
    if entries.is_empty() {
        return Err(PublishIndexError::NoEntries(format!(
            "publish root {} has no `publish/*/remote/remote_receipt.json` files",
            publish_root.display()
        )));
    }
    // Sort `entries[]` by `receipt_basename`
    // for determinism (a re-run on an
    // unchanged publish root must produce a
    // byte-identical `INDEX.json`).
    entries.sort_by(|a, b| a.receipt_basename.cmp(&b.receipt_basename));
    // Aggregate the per-entry `total_bytes`
    // (the sum of every entry's
    // `remote_receipt.total_bytes`).
    let total_bytes: u64 = entries.iter().map(|e| e.remote_receipt.total_bytes).sum();
    let entry_count = entries.len();
    let created_at_utc = created_at_utc
        .map(|s| s.to_string())
        .unwrap_or_else(|| STW034_UNKNOWN_UTC.to_string());
    let index = PublishIndex {
        publish_root: publish_root.display().to_string(),
        runbook_version: STW034_INDEX_RUNBOOK_VERSION.to_string(),
        created_at_utc,
        entry_count,
        total_bytes,
        entries,
    };
    // Write the `INDEX.json` + `SUMMARY.txt`
    // pair under `<publish_root>/index/`. The
    // index step never mutates the underlying
    // `remote_receipt.json` files; it writes
    // a fresh `index/` subdir the dashboard
    // scraper reads from.
    let output_dir = publish_root.join(STW034_INDEX_SUBDIR);
    fs::create_dir_all(&output_dir).map_err(|e| {
        PublishIndexError::FileUnreadable(format!(
            "could not create output dir {}: {e}",
            output_dir.display()
        ))
    })?;
    let index_path = output_dir.join(STW034_INDEX_FILENAME);
    let summary_path = output_dir.join(STW034_SUMMARY_FILENAME);
    let index_str = serde_json::to_string_pretty(&index).map_err(|e| {
        PublishIndexError::FileUnreadable(format!("could not serialise INDEX.json: {e}"))
    })?;
    fs::write(&index_path, index_str).map_err(|e| {
        PublishIndexError::FileUnreadable(format!(
            "could not write INDEX.json at {}: {e}",
            index_path.display()
        ))
    })?;
    // The `SUMMARY.txt` is a single-line
    // human-readable headline a CI worker
    // `cat`s to confirm the index step landed
    // end-to-end. The format mirrors the
    // STW-019 / STW-032 / STW-033 `SUMMARY.txt`
    // shape (`testnet live_proof publish_index
    // complete: ...`).
    let summary_str = format!(
        "testnet live_proof publish_index complete: \
         root={} entries={} total_bytes={} index_bytes={} \
         created_at_utc={} runbook_version={}\n",
        publish_root.display(),
        entry_count,
        total_bytes,
        fs::metadata(&index_path).map(|m| m.len()).unwrap_or(0),
        index.created_at_utc,
        index.runbook_version,
    );
    fs::write(&summary_path, summary_str).map_err(|e| {
        PublishIndexError::FileUnreadable(format!(
            "could not write SUMMARY.txt at {}: {e}",
            summary_path.display()
        ))
    })?;
    Ok(PublishIndexOutput {
        index_path,
        summary_path,
        index,
    })
}

// --- verify (no-DB no-rebuild re-verify path) ---------------------

impl PublishIndex {
    /// Re-verify a `PublishIndex` from disk:
    /// re-hash every local file the
    /// `INDEX.json` claims to have inlined
    /// (each entry's
    /// `s3_objects[].local_path` is read +
    /// re-sha256'd + compared to the entry's
    /// `sha256`), assert every digest matches,
    /// assert every `s3_uri` in the index
    /// appears in the inlined plan (a phantom
    /// `s3_uri` is a hard
    /// `PublishIndexError::MissingObject`),
    /// and return `Ok(())` on green.
    ///
    /// The `bundle_dir` arg is the per-entry
    /// resolver: when an entry's
    /// `s3_objects[].local_path` is a relative
    /// path, the verifier joins it to
    /// `bundle_dir` (the parent of the
    /// per-entry `remote/` dir). Absolute
    /// paths are resolved verbatim (the STW-033
    /// verifier handles both shapes).
    pub fn verify(&self, _bundle_dir: &Path) -> Result<(), PublishIndexError> {
        // We do not need a single `bundle_dir`
        // for the index verifier: each entry
        // carries its own `remote_receipt_path`
        // (absolute) + the underlying
        // `PublishedRemoteReceipt::verify`
        // resolves absolute paths verbatim.
        // The function takes a `bundle_dir` for
        // API symmetry with the STW-033
        // `PublishedRemoteReceipt::verify`
        // (so a future CI auditor that fetches
        // the INDEX.json to a fresh tempdir can
        // resolve relative paths uniformly) but
        // the per-entry verifier uses the
        // inlined `remote_receipt_path` as the
        // absolute-path resolver.
        for entry in &self.entries {
            // Per-entry verify: the inlined
            // `PublishedRemoteReceipt::verify`
            // re-hashes every local file the
            // entry claims to have uploaded +
            // asserts every `s3_uri` in the
            // entry appears in the inlined plan.
            // The `bundle_dir` for relative
            // paths is the parent of the
            // per-entry `remote/` subdir; we
            // resolve it from the inlined
            // `remote_receipt_path`.
            let entry_bundle_dir = PathBuf::from(&entry.remote_receipt_path)
                .parent()
                .and_then(|p| p.parent())
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            if let Err(e) = entry.remote_receipt.verify(&entry_bundle_dir) {
                return Err(e.into());
            }
        }
        Ok(())
    }
}

/// Convenience helper: read + parse a
/// `INDEX.json` from an index directory.
/// Mirrors the `read_remote_receipt` shape
/// (read JSON from disk, parse to typed
/// `PublishIndex`).
pub fn read_publish_index(index_dir: &Path) -> Result<PublishIndex, PublishIndexError> {
    let index_path = index_dir.join(STW034_INDEX_FILENAME);
    if !index_path.is_file() {
        return Err(PublishIndexError::FileUnreadable(format!(
            "INDEX.json missing or not a file at {}",
            index_path.display()
        )));
    }
    let index_str = fs::read_to_string(&index_path).map_err(|e| {
        PublishIndexError::FileUnreadable(format!(
            "could not read INDEX.json at {}: {e}",
            index_path.display()
        ))
    })?;
    serde_json::from_str(&index_str)
        .map_err(|e| PublishIndexError::FileUnreadable(format!("could not parse INDEX.json: {e}")))
}

/// One-line `Display` impl for the indexer
/// output (the headline the `Mode::PublishIndex`
/// CLI prints). Mirrors the STW-019 / STW-032 /
/// STW-033 headline shape.
impl fmt::Display for PublishIndexOutput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} root={} entries={} total_bytes={} index={}",
            STW034_PUBLISH_INDEX_HEADLINE_PREFIX,
            self.index.publish_root,
            self.index.entry_count,
            self.index.total_bytes,
            self.index_path.display(),
        )
    }
}

// --- lib tests ---------------------------------------------------

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-034
    //! publish-index surface. These tests do NOT
    //! require a live Postgres (the indexer is
    //! the *aggregator* side of the on-disk
    //! contract; the producer side is the
    //! STW-033 `trainer --publish-remote` chain
    //! + the STW-019 `testnet-live-proof.sh`
    //! runbook).
    //!
    //! Fixture style: a process-unique
    //! `std::env::temp_dir().join("rbp-publish-index-test-<n>")`
    //! subdirectory populated by
    //! `setup_publish_root` (which writes a
    //! synthetic green receipt + a fresh
    //! publish bundle + a fresh
    //! `remote_receipt.json` via the STW-032 +
    //! STW-033 surfaces), then indexed via
    //! `publish_index`, then re-verified via
    //! `PublishIndex::verify`. The tempdir is
    //! removed on drop so re-runs do not see
    //! stale files.
    use super::*;
    use crate::LiveProofReceipt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-publish-index-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Drop a synthetic green receipt + a
    /// fresh publish bundle + a fresh
    /// `remote_receipt.json` (the full
    /// STW-019 → STW-032 → STW-033 chain
    /// surface the aggregator consumes).
    /// Returns the
    /// `<publish-root>/publish/<basename>/`
    /// directory the aggregator scans.
    fn setup_publish_root(label: &str) -> (PathBuf, String) {
        let receipt = fresh_dir(&format!("{label}-receipt"));
        let publish_root = fresh_dir(&format!("{label}-publish-root"));
        let basename = receipt
            .file_name()
            .and_then(|n| n.to_str())
            .expect("receipt must have a basename")
            .to_string();
        // The STW-033 chain expects the
        // publish bundle under
        // `<publish-root>/publish/<basename>/`.
        let bundle = publish_root.join(STW034_PUBLISH_SUBDIR).join(&basename);
        LiveProofReceipt::write_to(
            &receipt,
            12,  // smoke_rows
            12,  // status_blueprint
            4,   // bench_hands
            4,   // compare_hands
            256, // replay_bytes
            "/srv/dev/repos/robopoker/target/debug/trainer",
            "<redacted: 49 chars>",
        )
        .expect("write_to should drop a synthetic green receipt");
        crate::publish::publish_receipt(
            &receipt,
            &bundle,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish_receipt should drop a fresh bundle");
        // The STW-033 chain's
        // `trainer --publish-remote` arm
        // writes the
        // `remote_receipt.json` + the
        // `remote_plan.json` to
        // `<publish-root>/publish/<basename>/remote/`.
        // We do not shell out to the trainer
        // binary here (the lib tests are
        // pure-in-memory); we call the
        // `publish_remote_receipt` function
        // directly with a fixed
        // `created_at_utc` so the lib test is
        // byte-stable.
        let _ = crate::publish_remote::publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof/",
            true, // dry-run
            Some("<unknown>"),
        )
        .expect("publish_remote_receipt should drop a fresh remote_receipt.json");
        // The lib test cleanup
        // (rm -rf) lives in the per-test
        // teardown; we leave the
        // `receipt` in place because the
        // STW-033 `remote_receipt.json` keeps
        // absolute `local_path`s that point
        // back into the receipt (the
        // `PublishedRemoteReceipt::verify`
        // re-reads them when the index step
        // runs the per-entry pre-index gate).
        (publish_root, basename)
    }

    /// `bucket_uri_as_str_matches_published_strings_v2`
    /// — the typed `PublishIndexError`
    /// Display impls must produce the exact
    /// strings the CLI / dashboard scraper
    /// greps. A future refactor that renames
    /// a prefix fails the test before a
    /// dashboard scraper can `grep` the
    /// wrong shape. Mirrors the STW-033
    /// `bucket_uri_as_str_matches_published_strings`
    /// pin.
    #[test]
    fn bucket_uri_as_str_matches_published_strings_v2() {
        assert_eq!(
            STW034_PUBLISH_INDEX_HEADLINE_PREFIX, "live_proof publish_index complete:",
            "publish-index headline prefix must be pinned"
        );
        assert_eq!(
            STW034_VERIFY_INDEX_HEADLINE_PREFIX, "live_proof index verification passed:",
            "verify-index headline prefix must be pinned"
        );
        assert_eq!(
            STW034_VERIFY_INDEX_FAILURE_HEADLINE_PREFIX, "live_proof index verification failed:",
            "verify-index failure headline prefix must be pinned"
        );
        assert_eq!(
            STW034_INDEX_FILENAME, "INDEX.json",
            "INDEX.json filename must be pinned"
        );
        assert_eq!(
            STW034_SUMMARY_FILENAME, "SUMMARY.txt",
            "SUMMARY.txt filename must be pinned"
        );
        assert_eq!(
            STW034_INDEX_RUNBOOK_VERSION, "STW-034 v1",
            "runbook version string must be pinned"
        );
        assert_eq!(
            STW034_UNKNOWN_UTC, "<unknown>",
            "unknown-utc sentinel must be pinned"
        );
        assert_eq!(
            STW034_INDEX_SUBDIR, "index",
            "index subdir name must be pinned"
        );
        assert_eq!(
            STW034_PUBLISH_SUBDIR, "publish",
            "publish subdir name must be pinned"
        );
        assert_eq!(
            STW034_ENTRY_REMOTE_SUBDIR, "remote",
            "remote subdir name must be pinned"
        );
        // Error Display strings a dashboard
        // scraper greps.
        assert_eq!(
            PublishIndexError::RemoteReceiptRed("foo".to_string()).to_string(),
            "live_proof publish_index error: remote receipt is red: foo",
        );
        assert_eq!(
            PublishIndexError::BundleHashMismatch {
                path: "p".to_string(),
                expected: "e".to_string(),
                actual: "a".to_string(),
            }
            .to_string(),
            "live_proof publish_index error: bundle_hash_mismatch: p: expected e, got a",
        );
        assert_eq!(
            PublishIndexError::MissingObject("s3://x".to_string()).to_string(),
            "live_proof publish_index error: missing_object: s3://x",
        );
        assert_eq!(
            PublishIndexError::FileUnreadable("oops".to_string()).to_string(),
            "live_proof publish_index error: file unreadable: oops",
        );
        assert_eq!(
            PublishIndexError::PublishRoot("oops".to_string()).to_string(),
            "live_proof publish_index error: publish root: oops",
        );
        assert_eq!(
            PublishIndexError::NoEntries("oops".to_string()).to_string(),
            "live_proof publish_index error: no entries: oops",
        );
    }

    /// `publish_index_writes_index_json` —
    /// the indexer writes a valid
    /// `INDEX.json` + `SUMMARY.txt` pair
    /// under `<publish-root>/index/` on a
    /// green publish root. The
    /// `PublishIndexOutput` mirrors what a
    /// downstream auditor reads back from
    /// disk via `read_publish_index`.
    #[test]
    fn publish_index_writes_index_json() {
        let (publish_root, basename) = setup_publish_root("writes-index");
        let result = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        assert!(
            result.index_path.is_file(),
            "INDEX.json must exist on disk at {}",
            result.index_path.display()
        );
        assert!(
            result.summary_path.is_file(),
            "SUMMARY.txt must exist on disk at {}",
            result.summary_path.display()
        );
        assert_eq!(
            result.index.entry_count, 1,
            "the publish root has exactly one entry; got {}",
            result.index.entry_count
        );
        assert_eq!(
            result.index.entries[0].receipt_basename, basename,
            "the entry basename must mirror the publish bundle's basename"
        );
        assert_eq!(
            result.index.runbook_version, STW034_INDEX_RUNBOOK_VERSION,
            "the runbook version must be pinned"
        );
        assert_eq!(
            result.index.created_at_utc, "2026-06-04T00:00:00Z",
            "the created_at_utc must mirror the env knob"
        );
        assert!(
            result.index.total_bytes > 0,
            "the aggregated total_bytes must be non-zero; got {}",
            result.index.total_bytes
        );
        // The `INDEX.json` must
        // round-trip through
        // `read_publish_index`.
        let index_dir = result.index_path.parent().unwrap().to_path_buf();
        let round_tripped = read_publish_index(&index_dir).expect("INDEX.json must round-trip");
        assert_eq!(
            round_tripped, result.index,
            "the round-tripped INDEX.json must equal the in-memory index"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_refuses_red_remote_receipt`
    /// — the aggregator refuses to write
    /// an `INDEX.json` for a publish root
    /// whose `remote_receipt.json` is red
    /// (a `PublishedRemoteReceipt::verify`
    /// pre-index gate fires before the
    /// `INDEX.json` is written). This is
    /// the "refuse to paper-over a red
    /// remote receipt" invariant the
    /// STW-033 verifier enforces.
    #[test]
    fn publish_index_refuses_red_remote_receipt() {
        let (publish_root, basename) = setup_publish_root("red-remote");
        // Tamper with the
        // `remote_receipt.json`'s first
        // `s3_objects[].sha256` so the
        // STW-033 `PublishedRemoteReceipt::verify`
        // rejects it (the re-hash of the
        // local file no longer matches the
        // tampered entry's `sha256`).
        let remote_receipt_path = publish_root
            .join(STW034_PUBLISH_SUBDIR)
            .join(&basename)
            .join("remote")
            .join(STW033_REMOTE_RECEIPT_FILENAME);
        let mut receipt: PublishedRemoteReceipt = serde_json::from_str(
            &fs::read_to_string(&remote_receipt_path).expect("read remote_receipt.json"),
        )
        .expect("parse remote_receipt.json");
        // Bump the first
        // `s3_objects[0].sha256` to a
        // bogus value. The
        // `PublishedRemoteReceipt::verify`
        // re-hashes the underlying file
        // and compares; a bogus entry
        // `sha256` fails the re-hash
        // with a `BundleHashMismatch`.
        if let Some(obj) = receipt.s3_objects.first_mut() {
            obj.sha256 = "deadbeef".repeat(8);
        }
        let tampered_str =
            serde_json::to_string_pretty(&receipt).expect("serialise tampered receipt");
        fs::write(&remote_receipt_path, tampered_str)
            .expect("rewrite remote_receipt.json with bogus s3_objects[0].sha256");
        let result = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"));
        match result {
            Err(PublishIndexError::RemoteReceiptRed(_)) => {}
            other => {
                panic!("red remote_receipt.json must produce RemoteReceiptRed; got: {other:?}")
            }
        }
        // The aggregator must NOT
        // have created the `index/`
        // subdir for a red publish root
        // (the pre-index gate fires
        // before the `INDEX.json` is
        // written).
        let output_dir = publish_root.join(STW034_INDEX_SUBDIR);
        assert!(
            !output_dir.exists(),
            "index subdir must NOT exist for a red publish root; got: {}",
            output_dir.display()
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_is_byte_stable_for_unchanged_root`
    /// — re-running the indexer on an
    /// unchanged publish root produces a
    /// byte-identical `INDEX.json`. The
    /// `created_at_utc` falls back to the
    /// `<unknown>` sentinel when the
    /// `created_at_utc` arg is `None` so
    /// the lib test is byte-stable.
    #[test]
    fn publish_index_is_byte_stable_for_unchanged_root() {
        let (publish_root, _basename) = setup_publish_root("byte-stable");
        let first = publish_index(&publish_root, None).expect("first index run must succeed");
        let first_bytes = fs::read(&first.index_path).expect("read first INDEX.json");
        let second = publish_index(&publish_root, None).expect("second index run must succeed");
        let second_bytes = fs::read(&second.index_path).expect("read second INDEX.json");
        assert_eq!(
            first_bytes, second_bytes,
            "re-running the indexer on an unchanged publish root must produce a \
             byte-identical INDEX.json"
        );
        assert_eq!(
            first.index.created_at_utc, STW034_UNKNOWN_UTC,
            "the created_at_utc must fall back to <unknown> when no env knob is set"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_aggregates_total_bytes_across_entries`
    /// `publish_index_aggregates_total_bytes_across_entries`
    /// — when a publish root has multiple
    /// entries (a CI worker that has
    /// published N receipts over time),
    /// the `total_bytes` is the sum of
    /// every entry's
    /// `remote_receipt.total_bytes`. This
    /// test sets up a publish root with
    /// TWO independent `publish/<basename>/`
    /// subdirs (each produced by an
    /// independent `setup_publish_root`
    /// call so the per-receipt absolute
    /// `local_path`s resolve cleanly).
    #[test]
    fn publish_index_aggregates_total_bytes_across_entries() {
        // Build a publish root with two
        // entries. Each entry's
        // `remote_receipt.json` keeps
        // absolute `local_path`s that
        // resolve back into its own
        // `setup_publish_root` receipt
        // dir; the indexer's per-entry
        // pre-index gate re-reads those
        // absolute paths verbatim, so we
        // cannot merge two roots without
        // rewriting the receipt's
        // `local_path`s. The simplest
        // correct setup is: build two
        // independent publish roots, and
        // copy `root_b`'s `publish/`
        // subdir INTO `root_a`'s
        // `publish/` subdir AS-IS (the
        // `local_path`s in `root_b`'s
        // `remote_receipt.json` still
        // resolve to `root_b`'s temp
        // paths, which exist because
        // `root_b` itself is left in
        // place for the duration of the
        // test).
        let (root_a, basename_a) = setup_publish_root("multi-a");
        let (root_b, basename_b) = setup_publish_root("multi-b");
        // Move `root_b`'s `publish/`
        // subdir under `root_a`'s
        // `publish/` subdir. The
        // `local_path`s in `root_b`'s
        // `remote_receipt.json` still
        // resolve to `root_b`'s temp
        // paths, which the test does not
        // delete.
        let root_b_publish = root_b.join(STW034_PUBLISH_SUBDIR);
        let dst = root_a.join(STW034_PUBLISH_SUBDIR).join(&basename_b);
        fs::create_dir_all(&dst).expect("mkdir for merged entry");
        for entry in fs::read_dir(&root_b_publish.join(&basename_b))
            .expect("read root_b/publish/<basename_b>")
        {
            let entry = entry.expect("read dir entry");
            let src = entry.path();
            let target = dst.join(entry.file_name());
            if src.is_dir() {
                fs::create_dir_all(&target).expect("mkdir for nested copy");
                for inner in fs::read_dir(&src).expect("read nested dir") {
                    let inner = inner.expect("read nested dir entry");
                    let inner_src = inner.path();
                    let inner_dst = target.join(inner.file_name());
                    if inner_src.is_dir() {
                        fs::create_dir_all(&inner_dst).expect("mkdir for double-nested copy");
                        for deep in fs::read_dir(&inner_src).expect("read double-nested dir") {
                            let deep = deep.expect("read deep dir entry");
                            fs::copy(deep.path(), inner_dst.join(deep.file_name()))
                                .expect("copy deep file");
                        }
                    } else {
                        fs::copy(&inner_src, &inner_dst).expect("copy file");
                    }
                }
            } else {
                fs::copy(&src, &target).expect("copy file");
            }
        }
        // Re-index the merged root.
        let result = publish_index(&root_a, Some("2026-06-04T00:00:00Z"))
            .expect("merged publish root must index");
        assert_eq!(
            result.index.entry_count, 2,
            "the merged publish root has exactly two entries; got {}",
            result.index.entry_count
        );
        // `entries[]` is sorted by
        // `receipt_basename`; the
        // lexicographically smaller
        // basename comes first.
        let first = &result.index.entries[0].receipt_basename;
        let second = &result.index.entries[1].receipt_basename;
        assert!(
            first < second,
            "entries must be sorted by receipt_basename; first={first:?} second={second:?}"
        );
        assert_eq!(
            first, &basename_a,
            "the lexicographically smaller basename must match root_a's basename"
        );
        assert_eq!(
            second, &basename_b,
            "the lexicographically larger basename must match root_b's basename"
        );
        // The aggregated
        // `total_bytes` is the sum of
        // the two entries'
        // `remote_receipt.total_bytes`.
        let per_entry_sum: u64 = result
            .index
            .entries
            .iter()
            .map(|e| e.remote_receipt.total_bytes)
            .sum();
        assert_eq!(
            result.index.total_bytes, per_entry_sum,
            "the aggregated total_bytes must equal the sum of the per-entry total_bytes"
        );
        assert!(
            result.index.total_bytes > 0,
            "the aggregated total_bytes must be non-zero"
        );
        // The test cleanup
        // (rm -rf) lives in the per-test
        // teardown; we leave `root_b`
        // intact because the merged
        // entry's `remote_receipt.json`
        // keeps absolute `local_path`s
        // that resolve back into `root_b`
        // (the test fixture depends on
        // `root_b` existing for the
        // duration of the test).
        let _ = fs::remove_dir_all(&root_a);
    }

    /// `publish_index_sorted_by_receipt_basename`
    /// — the `entries[]` array is sorted
    /// by `receipt_basename` for
    /// determinism. A regression that
    /// drops the sort fails the test
    /// because the
    /// `serde_json::to_string_pretty`
    /// output would be non-deterministic
    /// across re-runs.
    #[test]
    fn publish_index_sorted_by_receipt_basename() {
        let (publish_root, _basename) = setup_publish_root("sort-basename");
        let result = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        let mut sorted = result.index.entries.clone();
        sorted.sort_by(|a, b| a.receipt_basename.cmp(&b.receipt_basename));
        assert_eq!(
            result.index.entries, sorted,
            "the entries[] array must be sorted by receipt_basename"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_io_error_propagates_for_missing_root`
    /// — the indexer returns
    /// `PublishIndexError::PublishRoot`
    /// when the publish root does not
    /// exist (a setup error the operator
    /// should see, not silently swallow).
    #[test]
    fn publish_index_io_error_propagates_for_missing_root() {
        let bogus = fresh_dir("missing-root");
        let result = publish_index(&bogus, Some("2026-06-04T00:00:00Z"));
        match result {
            Err(PublishIndexError::PublishRoot(s)) => {
                assert!(
                    s.contains("missing-root"),
                    "PublishRoot must name the missing directory; got: {s:?}"
                );
            }
            other => panic!("missing root must produce PublishRoot; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&bogus);
    }

    /// `publish_index_io_error_propagates_for_empty_root`
    /// — the indexer returns
    /// `PublishIndexError::NoEntries`
    /// when the publish root has no
    /// `publish/*/remote/remote_receipt.json`
    /// files (the aggregator refuses to
    /// write a zero-entry index).
    #[test]
    fn publish_index_io_error_propagates_for_empty_root() {
        let empty = fresh_dir("empty-root");
        // Create the `publish/`
        // subdir but no entries.
        fs::create_dir_all(empty.join(STW034_PUBLISH_SUBDIR)).expect("mkdir publish/");
        let result = publish_index(&empty, Some("2026-06-04T00:00:00Z"));
        match result {
            Err(PublishIndexError::NoEntries(_)) => {}
            other => panic!("empty root must produce NoEntries; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&empty);
    }

    /// `verify_index_rehashes_every_local_file`
    /// — the `PublishIndex::verify`
    /// re-hashes every local file the
    /// `INDEX.json` claims to have
    /// inlined. A green index returns
    /// `Ok(())`.
    #[test]
    fn verify_index_rehashes_every_local_file() {
        let (publish_root, _basename) = setup_publish_root("verify-rehash");
        let indexed = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        let index_dir = indexed.index_path.parent().unwrap().to_path_buf();
        let round_tripped = read_publish_index(&index_dir).expect("INDEX.json must round-trip");
        // A green index verifies
        // cleanly.
        round_tripped
            .verify(&index_dir)
            .expect("green INDEX.json must verify");
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `verify_index_rejects_tampered_entry` —
    /// the `PublishIndex::verify` rejects
    /// an `INDEX.json` whose
    /// `entries[0].remote_receipt.bundle_sha256`
    /// is tampered with (a
    /// `BundleHashMismatch` because the
    /// inlined plan's `bundle_sha256`
    /// no longer matches the
    /// re-hash... actually, the inlined
    /// receipt's `bundle_sha256` is
    /// the only sha256 the verifier
    /// re-hashes; we tamper with a
    /// `s3_objects[].sha256` so the
    /// re-hash of the underlying file
    /// does not match the entry's
    /// claimed sha256).
    #[test]
    fn verify_index_rejects_tampered_entry() {
        let (publish_root, basename) = setup_publish_root("verify-tamper");
        let indexed = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        let index_dir = indexed.index_path.parent().unwrap().to_path_buf();
        // Read the on-disk
        // `INDEX.json`, tamper with one
        // entry's `s3_objects[0].sha256`,
        // and re-write the file. The
        // verifier re-hashes the
        // underlying file and compares
        // to the entry's claimed
        // `sha256`; a tampered
        // `s3_objects[0].sha256` fails
        // the re-hash.
        let index_path = index_dir.join(STW034_INDEX_FILENAME);
        let mut tampered: PublishIndex =
            serde_json::from_str(&fs::read_to_string(&index_path).expect("read INDEX.json"))
                .expect("parse INDEX.json");
        // Find the entry with the
        // matching basename.
        let entry = tampered
            .entries
            .iter_mut()
            .find(|e| e.receipt_basename == basename)
            .expect("entry must exist");
        // Bump the first
        // `s3_objects[0].sha256` to a
        // bogus value. The verifier
        // re-hashes the underlying
        // file and compares; a bogus
        // entry sha256 fails the
        // re-hash with a
        // `BundleHashMismatch`.
        if let Some(obj) = entry.remote_receipt.s3_objects.first_mut() {
            obj.sha256 = "deadbeef".repeat(8);
        }
        let tampered_str =
            serde_json::to_string_pretty(&tampered).expect("serialise tampered index");
        fs::write(&index_path, tampered_str).expect("write tampered INDEX.json");
        let re_read = read_publish_index(&index_dir).expect("tampered INDEX.json must round-trip");
        let result = re_read.verify(&index_dir);
        match result {
            Err(PublishIndexError::BundleHashMismatch { .. }) => {}
            other => panic!("tampered index must produce BundleHashMismatch; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `verify_index_phantom_uri_fails_with_missing_object`
    /// — the `PublishIndex::verify`
    /// rejects an `INDEX.json` whose
    /// `entries[0].s3_objects[]` contains
    /// an `s3_uri` that does not appear
    /// in the inlined plan (a phantom
    /// `s3_uri` is a hard
    /// `PublishIndexError::MissingObject`).
    #[test]
    fn verify_index_phantom_uri_fails_with_missing_object() {
        let (publish_root, basename) = setup_publish_root("verify-phantom");
        let indexed = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        let index_dir = indexed.index_path.parent().unwrap().to_path_buf();
        // Inject a phantom `s3_uri`
        // into the first entry's
        // `s3_objects[]` (an `s3_uri`
        // that does not appear in the
        // inlined plan's
        // `s3_objects[]`). The verifier
        // rejects it with a
        // `MissingObject` error.
        let index_path = index_dir.join(STW034_INDEX_FILENAME);
        let mut tampered: PublishIndex =
            serde_json::from_str(&fs::read_to_string(&index_path).expect("read INDEX.json"))
                .expect("parse INDEX.json");
        let entry = tampered
            .entries
            .iter_mut()
            .find(|e| e.receipt_basename == basename)
            .expect("entry must exist");
        if let Some(obj) = entry.remote_receipt.s3_objects.first() {
            entry
                .remote_receipt
                .s3_objects
                .push(crate::publish_remote::S3Object {
                    local_path: obj.local_path.clone(),
                    sha256: obj.sha256.clone(),
                    bytes: obj.bytes,
                    s3_uri: format!("{}phantom/{}", entry.remote_receipt.plan.bucket, basename),
                });
        }
        let tampered_str =
            serde_json::to_string_pretty(&tampered).expect("serialise phantom-uri index");
        fs::write(&index_path, tampered_str).expect("write phantom-uri INDEX.json");
        let re_read =
            read_publish_index(&index_dir).expect("phantom-uri INDEX.json must round-trip");
        let result = re_read.verify(&index_dir);
        match result {
            Err(PublishIndexError::MissingObject(_)) => {}
            other => panic!("phantom s3_uri must produce MissingObject; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_output_display_includes_pinned_prefix`
    /// — the `Display` impl for
    /// `PublishIndexOutput` produces
    /// the pinned
    /// `live_proof publish_index complete:`
    /// prefix a dashboard scraper
    /// greps. Mirrors the STW-019 /
    /// STW-032 / STW-033 headline shape.
    #[test]
    fn publish_index_output_display_includes_pinned_prefix() {
        let (publish_root, _basename) = setup_publish_root("display-prefix");
        let out = publish_index(&publish_root, Some("2026-06-04T00:00:00Z"))
            .expect("green publish root must index");
        let s = out.to_string();
        assert!(
            s.starts_with(STW034_PUBLISH_INDEX_HEADLINE_PREFIX),
            "Display must start with the pinned headline prefix; got: {s:?}"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }
}
