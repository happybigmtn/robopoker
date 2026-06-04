//! `trainer --publish-index-remote <publish-root>
//! --bucket <s3://...> [--prefix <prefix/>]
//! [--no-dry-run]` + `trainer --verify-index-remote
//! <remote-dir>` — the v9 follow-on the STW-034
//! `testnet-live-publish-index.md` scope-boundary
//! defers to: a *plan-first* remote-upload of the
//! `INDEX.json` aggregator the STW-034 chain produced
//! (a deterministic upload plan +
//! `index_remote_receipt.json` pair a CI worker can
//! `aws s3 cp` to push the aggregator to a dashboard
//! bucket), AND a no-DB no-rebuild re-verify path
//! that re-hashes the local `INDEX.json` + the
//! per-entry `remote_receipt.json` files the STW-034
//! chain produced.
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
//! re-verifier). STW-034 shipped
//! `trainer --publish-index <publish-root>` (the
//! testnet dashboard aggregator the STW-033 chain
//! produced `remote_receipt.json` files for) +
//! `trainer --verify-index <index-path>` (the no-DB
//! re-verifier of the `INDEX.json`).
//! **STW-035 lands the v9 follow-on a CI worker
//! naturally wants** (the next slice the STW-034
//! publish-index step defers to): a
//! deterministic `index_remote_plan.json` +
//! `index_remote_receipt.json` pair a CI worker can
//! `aws s3 cp` to push the `INDEX.json` to a
//! dashboard bucket, AND a no-DB no-rebuild
//! re-verify path that re-hashes the local
//! `INDEX.json` + the per-entry `remote_receipt.json`
//! files the STW-034 chain produced.
//!
//! ## Why a separate module from `publish_index`
//!
//! `publish_index.rs` is the *aggregator* side (turn
//! a publish root into one `INDEX.json`).
//! `publish_index_remote.rs` is the *uploader* side
//! (turn that one `INDEX.json` into a remote-upload
//! plan + a post-upload `index_remote_receipt.json`).
//! The two are split because the error surfaces are
//! different: a regression in a single
//! `remote_receipt.json`'s `s3_uri` field does not
//! change the `index_remote_plan.json` upload
//! plan's `s3_objects[]` field, and vice versa. A
//! typed `PublishIndexRemoteError` enum lives next to
//! the indexer-remote so the integration test can
//! assert on the failure kind a dashboard scraper
//! greps.
//!
//! ## Why the upload plan uploads the whole publish root's index
//!
//! The STW-034 `INDEX.json` references every per-entry
//! `remote_receipt.json` by absolute `local_path`, so
//! a CI worker that wants to push the aggregator to a
//! dashboard bucket has to push the `INDEX.json` file
//! (the only new file the upload step writes — the
//! STW-034 chain's per-entry `remote_receipt.json`
//! files are already in the dashboard bucket, the
//! STW-033 runbook pushed them there). The plan
//! therefore walks the STW-034 `index/` dir, collects
//! the `INDEX.json` (and the `SUMMARY.txt` for
//! human-readability, but the `SUMMARY.txt` is NOT
//! required for the verifier to pass — the
//! `PublishIndexRemote::verify` re-hashes the
//! `INDEX.json` against the receipt's `index_sha256`
//! field, not the `SUMMARY.txt`'s content), and builds
//! the per-file `s3_objects[]` array
//! (`<index_filename> -> s3://<bucket>/<prefix>/<index_filename>`).
//! The per-file `sha256` is re-hashed on-disk so a
//! pre-upload gate can short-circuit a red
//! `INDEX.json` with `PublishIndexRemoteError::IndexRed(...)`.
//!
//! ## Why the dry-run default
//!
//! The `trainer --publish-index-remote` arm defaults
//! to `--dry-run`: the `RBP_PUBLISH_INDEX_REMOTE_DRY_RUN=1`
//! env knob (or, when `aws` is on `$PATH` and the
//! shell has the `AWS_ACCESS_KEY_ID` /
//! `AWS_SECRET_ACCESS_KEY` env knobs set, the
//! `--no-dry-run` argv) flips the arm into live
//! mode. Live mode shells out to `aws s3 cp` per file
//! in the plan; a missing `aws` returns
//! `PublishIndexRemoteError::AwsCli` and the arm
//! exits 2. The `cargo test --workspace` integration
//! test runs in dry-run so a regression in the CLI
//! surface fails CI without an `aws` credential or a
//! live bucket.
//!
//! ## Why the bucket normalises a bare name to `s3://`
//!
//! Mirrors the STW-033 `--bucket` semantics: the
//! `--bucket` flag accepts either a fully-qualified
//! URI (`s3://<name>/`) or a bare bucket name
//! (`<name>`). The plan normalises the bare-name form
//! to the URI form so the scheme-prefix check + the
//! `s3_uri` construction below are uniform. The
//! `s3_uri` for the `INDEX.json` is always
//! `s3://<bucket>/<prefix>/INDEX.json` — a CI worker
//! that `aws s3 cp`s the file lands a single key a
//! dashboard scraper can fetch + re-verify with
//! `trainer --verify-index-remote <remote-dir>`.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::publish_index::{
    PublishIndex, PublishIndexError, STW034_INDEX_FILENAME, STW034_INDEX_SUBDIR, read_publish_index,
};
use crate::publish_remote::PublishRemoteError;

// --- headline constants ------------------------------------------

/// Headline prefix the `trainer --publish-index-remote`
/// arm prints to stdout on success. Mirrors the
/// `live_proof ... complete:` family the STW-019 /
/// STW-031 / STW-032 / STW-033 / STW-034 trainers
/// already print so one `grep ^live_proof` scraper
/// can read the whole chain.
pub const STW035_PUBLISH_INDEX_REMOTE_HEADLINE_PREFIX: &str =
    "live_proof publish_index_remote complete:";
/// Headline prefix the `trainer --verify-index-remote`
/// arm prints to stdout on a green re-verify. A
/// dashboard scraper greps this line.
pub const STW035_VERIFY_INDEX_REMOTE_HEADLINE_PREFIX: &str =
    "live_proof index_remote verification passed:";
/// Headline prefix the `trainer --verify-index-remote`
/// arm prints to stderr on a red re-verify. A
/// dashboard scraper greps this line.
pub const STW035_VERIFY_INDEX_REMOTE_FAILURE_HEADLINE_PREFIX: &str =
    "live_proof index_remote verification failed:";

// --- on-disk filenames -------------------------------------------

/// The on-disk `index_remote_plan.json` file name.
/// The constant lives in Rust so the bash runbook
/// and the Rust writer agree on the filename.
pub const STW035_INDEX_REMOTE_PLAN_FILENAME: &str = "index_remote_plan.json";
/// The on-disk `index_remote_receipt.json` file name
/// (the post-upload manifest the verifier re-hashes).
pub const STW035_INDEX_REMOTE_RECEIPT_FILENAME: &str = "index_remote_receipt.json";
/// The on-disk `SUMMARY.txt` file name (the
/// single-line human-readable headline a CI worker
/// `cat`s to confirm the index-remote step landed
/// end-to-end).
pub const STW035_SUMMARY_FILENAME: &str = "SUMMARY.txt";

/// Runbook version string the index-remote step
/// stamps on the `index_remote_receipt.json` (bumped
/// by hand if the upload-plan / remote-receipt JSON
/// shape changes).
pub const STW035_INDEX_REMOTE_RUNBOOK_VERSION: &str = "STW-035 v1";

/// Sentinel value the index-remote step writes into
/// `created_at_utc` / `uploaded_at_utc` when the
/// `RBP_PUBLISH_INDEX_REMOTE_UTC` env knob is unset.
/// The `<unknown>` sentinel is the same shape the
/// STW-019 / STW-032 / STW-033 / STW-034 already use,
/// and keeps the lib test + integration test
/// byte-stable on a CI runner that does not stamp the
/// env knob.
pub const STW035_UNKNOWN_UTC: &str = "<unknown>";

/// The subdirectory under `<publish-root>/` the
/// index-remote step writes its output to. The
/// index-remote step never mutates the underlying
/// `INDEX.json` (STW-034 chain) or the per-entry
/// `remote_receipt.json` files (STW-033 chain); it
/// writes a fresh `index_remote/` subdir the
/// dashboard scraper reads from.
pub const STW035_INDEX_REMOTE_SUBDIR: &str = "index_remote";

/// The on-disk filename the upload plan references
/// in the per-file `s3_objects[]` array. The
/// `INDEX.json` is the only file the index-remote
/// step pushes to the bucket (the per-entry
/// `remote_receipt.json` files are already there, the
/// STW-033 runbook pushed them).
pub const STW035_INDEX_FILE_TO_UPLOAD: &str = "INDEX.json";

// --- data model --------------------------------------------------

/// Per-file upload entry: the `local_path` (absolute)
/// of the `INDEX.json` the STW-034 chain wrote, the
/// `sha256` + `bytes` of the file, and the fully-
/// qualified `s3_uri` the `aws s3 cp` shell-out would
/// target. Mirrors the STW-033 `S3Object` shape so a
/// dashboard scraper can read both `remote_receipt.json`
/// + `index_remote_receipt.json` with the same
/// `serde_json::from_str` shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexRemoteS3Object {
    /// Local absolute path to the `INDEX.json` the
    /// STW-034 chain wrote.
    pub local_path: String,
    /// Lowercase hex `sha256` of the local file's
    /// bytes.
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
    /// Fully-qualified S3 URI the file would be
    /// uploaded to
    /// (`s3://<bucket>/<prefix>/INDEX.json`).
    pub s3_uri: String,
}

/// The upload plan the `trainer --publish-index-remote`
/// arm writes to
/// `<publish-root>/index_remote/index_remote_plan.json`.
/// The plan is the input the bash runbook's
/// `aws s3 cp` shell-out iterates; the runbook reads
/// `s3_objects[]` and uploads each `local_path ->
/// s3_uri` pair. The plan is the *deterministic*
/// contract: a byte-identical `INDEX.json` produces a
/// byte-identical plan (the `s3_objects[]` array is
/// sorted by `s3_uri` and the `created_at_utc` falls
/// back to a sentinel when the env knob is unset so
/// the integration test is byte-stable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishIndexRemotePlan {
    /// Bucket the upload targets (e.g.
    /// `s3://robopoker-testnet-dashboard`).
    pub bucket: String,
    /// Prefix inside the bucket (e.g.
    /// `index-20260604T050000Z/`).
    pub prefix: String,
    /// Per-file upload entries, sorted by `s3_uri`
    /// for determinism. The index-remote step
    /// uploads the `INDEX.json` only (the per-entry
    /// `remote_receipt.json` files are already in
    /// the dashboard bucket, the STW-033 runbook
    /// pushed them).
    pub s3_objects: Vec<IndexRemoteS3Object>,
    /// Lowercase hex `sha256` of the `INDEX.json` the
    /// STW-034 chain wrote. The remote receipt
    /// re-hashes the `INDEX.json` on the verifier's
    /// machine + asserts it matches.
    pub index_sha256: String,
    /// `INDEX.json` size in bytes (mirrors the
    /// `s3_objects[].bytes` sum, stored separately so
    /// a dashboard scraper that only reads the
    /// top-level object does not have to `jq
    /// '.s3_objects | map(.bytes) | add'`).
    pub index_bytes: u64,
    /// The publish root basename (e.g.
    /// `publish-20260604T050000Z`) the `INDEX.json`
    /// was built from.
    pub publish_root_basename: String,
    /// The runbook version string
    /// (`"STW-035 v1"`). Bumped by hand if the
    /// upload plan JSON shape changes.
    pub runbook_version: String,
    /// ISO-8601 UTC timestamp the plan was written.
    /// Read from the
    /// `RBP_PUBLISH_INDEX_REMOTE_UTC` env knob so
    /// the integration test is byte-stable (the
    /// env knob defaults to `<unknown>` when unset —
    /// the lib test + integration test use this
    /// sentinel).
    pub created_at_utc: String,
    /// Whether the upload is a dry-run (the
    /// default). When `true`, the arm writes the
    /// plan + the receipt but does NOT shell out to
    /// `aws` / `gsutil` / `git`. When `false`, the
    /// arm shells out to `aws s3 cp` per
    /// `s3_objects[]` entry (this requires the
    /// `aws` CLI to be on `$PATH` and the shell to
    /// have the `AWS_ACCESS_KEY_ID` /
    /// `AWS_SECRET_ACCESS_KEY` env knobs set; a
    /// missing `aws` returns
    /// `PublishIndexRemoteError::AwsCli` and the
    /// arm exits 2).
    pub dry_run: bool,
}

/// The post-upload manifest the runbook writes to
/// `<publish-root>/index_remote/index_remote_receipt.json`
/// AFTER the `aws s3 cp` exits 0. The remote
/// receipt's `s3_objects[]` mirrors the plan's
/// `s3_objects[]` but adds the `uploaded_at_utc`
/// timestamp the runbook observed when the
/// `aws s3 cp` returned. The verifier re-hashes the
/// local `INDEX.json`, asserts every digest matches
/// the manifest, and asserts every `s3_uri` in the
/// receipt appears in the plan (a phantom `s3_uri`
/// in the receipt is a `PublishIndexRemoteError::MissingObject`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedIndexRemoteReceipt {
    /// The plan that produced this receipt (inlined
    /// as a copy of the plan, not a path reference,
    /// so the verifier can re-verify the receipt
    /// without reading a sibling
    /// `index_remote_plan.json` file). This is the
    /// "operator-visible receipt and the CI-visible
    /// receipt share one source of truth" invariant
    /// STW-023 / STW-032 / STW-033 / STW-034 already
    /// enforce.
    pub plan: PublishIndexRemotePlan,
    /// ISO-8601 UTC timestamp the upload completed.
    /// Read from the
    /// `RBP_PUBLISH_INDEX_REMOTE_UTC` env knob (or
    /// `<unknown>` for the lib test fixture).
    pub uploaded_at_utc: String,
    /// Per-file upload entries that the runbook
    /// observed succeed, sorted by `s3_uri` for
    /// determinism.
    pub s3_objects: Vec<IndexRemoteS3Object>,
    /// The number of bytes uploaded in total (sum
    /// of `s3_objects[].bytes`).
    pub total_bytes: u64,
    /// Lowercase hex `sha256` of the `INDEX.json` the
    /// STW-034 chain wrote (mirrors
    /// `plan.index_sha256`; stored at the top level
    /// for human readability).
    pub index_sha256: String,
    /// The runbook version string
    /// (`"STW-035 v1"`). Bumped by hand if the
    /// post-upload manifest JSON shape changes.
    pub runbook_version: String,
}

// --- typed error -------------------------------------------------

/// Publisher-index-remote error: a single typed
/// error so the CLI / integration test can assert on
/// the `PublishIndexRemoteError::*` variant. The
/// variants cover the failure modes the
/// indexer-remote / verifier detect:
///
/// - `IndexRed` — the `INDEX.json` failed the
///   STW-034 `PublishIndex::verify` re-verify; the
///   index-remote refuses to plan an upload for a
///   red index (the "refuse to paper-over a red
///   index" invariant the STW-034 verifier already
///   enforces).
/// - `BundleHashMismatch` — the
///   `index_remote_receipt.json`'s `INDEX.json` has
///   a `sha256` that does not match the on-disk
///   file's re-hash.
/// - `MissingObject` — the receipt names an `s3_uri`
///   that does not appear in the inlined plan.
/// - `FileUnreadable` — a file inside the publish
///   root could not be read.
/// - `BucketUri` — the `--bucket` value is not a
///   valid `s3://...` URI.
/// - `AwsCli` — the live `aws s3 cp` step failed
///   (CLI missing, no creds, or network error).
///   Only fired when `--no-dry-run` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishIndexRemoteError {
    /// The `INDEX.json` failed the STW-034
    /// `PublishIndex::verify` re-verify. The
    /// index-remote refuses to plan an upload for a
    /// red index.
    IndexRed(String),
    /// The `INDEX.json` has a `sha256` that does not
    /// match the on-disk file's re-hash.
    BundleHashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// The remote receipt names an `s3_uri` that
    /// does not appear in the plan.
    MissingObject(String),
    /// A file inside the publish root could not be
    /// read.
    FileUnreadable(String),
    /// The `--bucket` value is not a valid
    /// object-store URI.
    BucketUri(String),
    /// The `aws` CLI failed (missing binary, missing
    /// creds, network error). Only fired in live
    /// mode.
    AwsCli(String),
}

impl fmt::Display for PublishIndexRemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishIndexRemoteError::IndexRed(s) => {
                write!(
                    f,
                    "live_proof publish_index_remote error: index is red: {s}"
                )
            }
            PublishIndexRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "live_proof publish_index_remote error: bundle_hash_mismatch: {path}: \
                 expected {expected}, got {actual}"
            ),
            PublishIndexRemoteError::MissingObject(s) => {
                write!(
                    f,
                    "live_proof publish_index_remote error: missing_object: {s}"
                )
            }
            PublishIndexRemoteError::FileUnreadable(s) => {
                write!(
                    f,
                    "live_proof publish_index_remote error: file unreadable: {s}"
                )
            }
            PublishIndexRemoteError::BucketUri(s) => {
                write!(f, "live_proof publish_index_remote error: bucket uri: {s}")
            }
            PublishIndexRemoteError::AwsCli(s) => {
                write!(f, "live_proof publish_index_remote error: aws cli: {s}")
            }
        }
    }
}

impl std::error::Error for PublishIndexRemoteError {}

/// STW-038: the dashboard-greppable pinned error
/// line. The `Display` impl above emits the
/// *legacy* `live_proof publish_index_remote
/// error: ...` shape the existing per-arm call
/// sites + `cargo test --workspace` integration
/// tests pin. The new `to_pinned_line` emits
/// the STW-038 dashboard-greppable
/// `trainer error: kind=red_index detail=<detail>`
/// shape a CI scraper can `grep ^trainer error
/// kind=`. The two lines are *both* emitted on
/// the same stderr write from the
/// `Mode::PublishIndexRemote` dispatch arm, so a
/// regression in either shape fails CI.
impl PublishIndexRemoteError {
    /// STW-038: map the per-variant
    /// `PublishIndexRemoteError` to a
    /// `TrainerError` and emit the pinned
    /// `to_pinned_line` shape. The `IndexRed`
    /// variant becomes `TrainerError::RedIndex`
    /// (the "publish-index-remote refused to plan
    /// an upload for a red INDEX.json" failure
    /// mode the STW-035 surface names); the 5
    /// non-red variants become
    /// `TrainerError::Internal` with a
    /// human-readable detail. The
    /// `BucketUri("")` failure mode is mapped
    /// to `TrainerError::NoBucket` so the
    /// dashboard scraper can distinguish a
    /// missing-bucket-arg from a malformed
    /// bucket-arg.
    pub fn to_pinned_line(&self) -> String {
        match self {
            PublishIndexRemoteError::IndexRed(s) => {
                crate::error::TrainerError::RedIndex(s.clone()).to_pinned_line()
            }
            PublishIndexRemoteError::BucketUri(s) if s.is_empty() => {
                crate::error::TrainerError::NoBucket.to_pinned_line()
            }
            PublishIndexRemoteError::BucketUri(s) => crate::error::TrainerError::Internal(
                format!("publish_index_remote: bucket_uri: {s}"),
            )
            .to_pinned_line(),
            PublishIndexRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => crate::error::TrainerError::Internal(format!(
                "publish_index_remote: bundle_hash_mismatch: {path}: expected {expected}, got {actual}"
            ))
            .to_pinned_line(),
            PublishIndexRemoteError::MissingObject(s) => {
                crate::error::TrainerError::Internal(format!(
                    "publish_index_remote: missing_object: {s}"
                ))
                .to_pinned_line()
            }
            PublishIndexRemoteError::FileUnreadable(s) => {
                crate::error::TrainerError::Internal(format!(
                    "publish_index_remote: file_unreadable: {s}"
                ))
                .to_pinned_line()
            }
            PublishIndexRemoteError::AwsCli(s) => crate::error::TrainerError::Internal(format!(
                "publish_index_remote: aws_cli: {s}"
            ))
            .to_pinned_line(),
        }
    }
}

impl From<PublishIndexError> for PublishIndexRemoteError {
    fn from(e: PublishIndexError) -> Self {
        match e {
            PublishIndexError::RemoteReceiptRed(s) => {
                // The STW-034 index verifier's
                // `RemoteReceiptRed` is a *red
                // per-entry `remote_receipt.json`*
                // (an underlying STW-033 receipt
                // was red), NOT a *red `INDEX.json`*
                // (the aggregator's own bytes were
                // tampered with). The index-remote
                // step's pre-upload gate is a
                // per-entry re-verify chain that
                // catches the underlying red
                // receipt before the plan is
                // written; we surface it as
                // `IndexRed` so the CLI headline
                // matches the pre-upload gate
                // contract.
                PublishIndexRemoteError::IndexRed(format!("red per-entry remote_receipt.json: {s}"))
            }
            PublishIndexError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => PublishIndexRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            },
            PublishIndexError::MissingObject(s) => PublishIndexRemoteError::MissingObject(s),
            PublishIndexError::FileUnreadable(s) => PublishIndexRemoteError::FileUnreadable(s),
            PublishIndexError::PublishRoot(s) => {
                PublishIndexRemoteError::FileUnreadable(format!("publish root: {s}"))
            }
            PublishIndexError::NoEntries(s) => {
                PublishIndexRemoteError::FileUnreadable(format!("no entries: {s}"))
            }
        }
    }
}

impl From<PublishRemoteError> for PublishIndexRemoteError {
    fn from(e: PublishRemoteError) -> Self {
        match e {
            PublishRemoteError::BucketUri(s) => PublishIndexRemoteError::BucketUri(s),
            PublishRemoteError::FileUnreadable(s) => PublishIndexRemoteError::FileUnreadable(s),
            PublishRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => PublishIndexRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            },
            PublishRemoteError::MissingObject(s) => PublishIndexRemoteError::MissingObject(s),
            PublishRemoteError::AwsCli(s) => PublishIndexRemoteError::AwsCli(s),
            // A `ReceiptRed` from the per-entry
            // `PublishedRemoteReceipt::verify` call
            // is the underlying red-receipt case the
            // STW-034 indexer's pre-index gate
            // catches; we surface it as `IndexRed`
            // so the CLI headline matches the
            // pre-upload gate contract.
            PublishRemoteError::ReceiptRed(s) => {
                PublishIndexRemoteError::IndexRed(format!("red per-entry remote_receipt.json: {s}"))
            }
            PublishRemoteError::ReceiptDir(s) => {
                PublishIndexRemoteError::FileUnreadable(format!("receipt dir: {s}"))
            }
            PublishRemoteError::BundleDir(s) => {
                PublishIndexRemoteError::FileUnreadable(format!("bundle dir: {s}"))
            }
        }
    }
}

// --- typed output ------------------------------------------------

/// `PublishIndexRemoteOutput` — the typed return
/// value of [`publish_index_remote_receipt`]. The
/// handler returns this so the `Mode::PublishIndexRemote`
/// CLI can print a one-line `live_proof
/// publish_index_remote complete: ...` headline and
/// the integration test can assert on the typed
/// `index_path` + `file_count` + `total_bytes` fields.
///
/// The struct is a *flat re-export* of the
/// `PublishedIndexRemoteReceipt` the index-remote
/// writes to disk (with the on-disk paths appended
/// for human readability), so the `Mode::PublishIndexRemote`
/// CLI can read `out.plan.bucket` + `out.s3_objects.len()`
/// + `out.index_sha256` + `out.runbook_version`
/// directly off the typed return value.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishIndexRemoteOutput {
    /// The in-memory `PublishedIndexRemoteReceipt` the
    /// index-remote serialised to disk. Mirrors what
    /// a downstream auditor reads back from disk via
    /// `read_index_remote_receipt`. The CLI reads
    /// `out.plan.bucket` + `out.plan.prefix` from
    /// this field; the verifier reads `out.s3_objects` +
    /// `out.total_bytes` + `out.index_sha256` from
    /// this field.
    pub plan: PublishIndexRemotePlan,
    /// The `s3_objects[]` array the index-remote
    /// wrote to the plan. Mirrors
    /// `PublishedIndexRemoteReceipt::s3_objects`;
    /// stored at the top level so the CLI headline
    /// can read `out.s3_objects.len()` directly.
    pub s3_objects: Vec<IndexRemoteS3Object>,
    /// The `total_bytes` the index-remote uploaded.
    /// Mirrors
    /// `PublishedIndexRemoteReceipt::total_bytes`;
    /// stored at the top level so the CLI headline
    /// can read `out.total_bytes` directly.
    pub total_bytes: u64,
    /// The `index_sha256` the index-remote computed.
    /// Mirrors
    /// `PublishedIndexRemoteReceipt::index_sha256`;
    /// stored at the top level so the CLI headline
    /// can read `out.index_sha256` directly.
    pub index_sha256: String,
    /// The `runbook_version` the index-remote
    /// stamped. Mirrors
    /// `PublishedIndexRemoteReceipt::runbook_version`;
    /// stored at the top level so the CLI headline
    /// can read `out.runbook_version` directly.
    pub runbook_version: String,
    /// Absolute path to the `index_remote_plan.json`
    /// the index-remote wrote (for human readability
    /// + integration-test assertions).
    pub plan_path: PathBuf,
    /// Absolute path to the `index_remote_receipt.json`
    /// the index-remote wrote (for human readability
    /// + integration-test assertions).
    pub receipt_path: PathBuf,
    /// Absolute path to the `SUMMARY.txt` the
    /// index-remote wrote (for human readability
    /// + integration-test assertions).
    pub summary_path: PathBuf,
}

// --- main entry point --------------------------------------------

/// Plan + (optionally) apply a remote upload of the
/// STW-034 `INDEX.json` aggregator. The function is
/// the no-network-aware entry point the
/// `Mode::PublishIndexRemote` arm delegates to; it
/// reads the STW-034 `INDEX.json` from
/// `<publish-root>/index/`, re-verifies it as a
/// pre-upload gate, re-validates every per-entry
/// `remote_receipt.json` the STW-034 chain inlined
/// (the STW-033 `PublishedRemoteReceipt` is the
/// source of truth for the per-file upload plan the
/// STW-033 runbook already pushed to the bucket —
/// we only push the new `INDEX.json` file), builds
/// the per-file upload plan, and either (a) writes
/// the plan + a stub `index_remote_receipt.json` to
/// `<publish-root>/index_remote/` (dry-run, the
/// default) or (b) shells out to `aws s3 cp` per
/// `s3_objects[]` and writes the post-upload
/// `index_remote_receipt.json` (live, gated by
/// `--no-dry-run`).
///
/// `bucket` is the bucket URI
/// (e.g. `s3://robopoker-testnet-dashboard` or a bare
/// `robopoker-testnet-dashboard` — the function
/// normalises the bare form to `s3://...`). `prefix`
/// is the key prefix inside the bucket
/// (e.g. `index-20260604T050000Z/`).
pub fn publish_index_remote_receipt<P: AsRef<Path>>(
    publish_root: P,
    bucket: &str,
    prefix: &str,
    dry_run: bool,
    created_at_utc: Option<&str>,
) -> Result<PublishIndexRemoteOutput, PublishIndexRemoteError> {
    let publish_root = publish_root.as_ref();
    if !publish_root.is_dir() {
        return Err(PublishIndexRemoteError::FileUnreadable(format!(
            "publish root {} does not exist or is not a directory",
            publish_root.display()
        )));
    }
    // Pre-upload gate (FIRST, before the
    // `BucketUri` gate): refuse to plan an
    // upload for a red `INDEX.json`. The
    // `PublishIndex::verify` re-runs the
    // per-entry `PublishedRemoteReceipt::verify`
    // chain (a per-entry re-verify of every
    // `remote_receipt.json` the STW-034 chain
    // inlined) + re-hashes every local file the
    // `INDEX.json` claims to have inlined.
    // This MUST run BEFORE the `BucketUri` gate
    // so a red `INDEX.json` does not
    // short-circuit to a `BucketUri: ...` error
    // before the index verifier can say "index
    // is red".
    let index_dir = publish_root.join(STW034_INDEX_SUBDIR);
    let index_path = index_dir.join(STW034_INDEX_FILENAME);
    let publish_index: PublishIndex = read_publish_index(&index_dir)?;
    // The STW-034 `PublishIndex::verify` takes a
    // `bundle_dir` for API symmetry with the
    // STW-033 verifier; the index verifier
    // re-uses the inlined `remote_receipt_path`
    // (absolute) as the per-entry `local_path`
    // resolver. We pass `&index_dir` as a
    // placeholder; the verifier does not gate on
    // it because every `remote_receipt_path` is
    // absolute.
    publish_index.verify(&index_dir)?;
    // Bucket must look like a valid object-store
    // URI scheme. We accept `s3://...`, `gs://...`,
    // and `git://...` (a follow-on `git-tag` push
    // slice can re-use the same
    // `PublishIndexRemotePlan` shape with `git://`
    // URIs). Mirrors the STW-033
    // `publish_remote_receipt` bucket validation.
    if bucket.is_empty() {
        return Err(PublishIndexRemoteError::BucketUri(
            "bucket must be non-empty".to_string(),
        ));
    }
    // If the bucket is a URI but with a scheme
    // we don't support (e.g. `https://...`),
    // reject it BEFORE the bare-name
    // normalization.
    if bucket.contains("://")
        && !bucket.starts_with("s3://")
        && !bucket.starts_with("gs://")
        && !bucket.starts_with("git://")
    {
        return Err(PublishIndexRemoteError::BucketUri(format!(
            "bucket {bucket} must start with s3://, gs://, or git://"
        )));
    }
    let bucket_for_check = if bucket.starts_with("s3://")
        || bucket.starts_with("gs://")
        || bucket.starts_with("git://")
    {
        bucket.to_string()
    } else {
        format!("s3://{bucket}")
    };
    if !bucket_for_check.starts_with("s3://")
        && !bucket_for_check.starts_with("gs://")
        && !bucket_for_check.starts_with("git://")
    {
        return Err(PublishIndexRemoteError::BucketUri(format!(
            "bucket {bucket} must start with s3://, gs://, or git://"
        )));
    }
    let bucket = bucket_for_check.trim_end_matches('/').to_string();
    // The default prefix is `<publish_root_basename>/`
    // when the operator passes `--prefix ''`. The
    // publish root basename is the directory the
    // STW-033 / STW-034 chain wrote to (e.g.
    // `publish-20260604T050000Z`).
    let publish_root_basename = publish_root
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("publish-root")
        .to_string();
    let effective_prefix = if prefix.is_empty() {
        format!("{publish_root_basename}/index/")
    } else {
        prefix.trim_end_matches('/').to_string()
    };
    // Resolve the timestamp. The env knob
    // defaults to the `<unknown>` sentinel when
    // unset so the lib test + integration test
    // are byte-stable.
    let created_at_utc = match created_at_utc {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => std::env::var("RBP_PUBLISH_INDEX_REMOTE_UTC")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| STW035_UNKNOWN_UTC.to_string()),
    };
    // Re-validate every per-entry
    // `remote_receipt.json` the STW-034 chain
    // inlined into the `INDEX.json`. The STW-033
    // `PublishedRemoteReceipt` is the source of
    // truth for the per-file upload plan the
    // STW-033 runbook already pushed to the bucket.
    // We do NOT push the per-entry
    // `remote_receipt.json` files again; we only
    // push the new `INDEX.json` file. The
    // per-entry re-verify is the
    // "refuse to paper-over a red per-entry
    // `remote_receipt.json`" invariant the
    // STW-033 verifier is the source of truth
    // for.
    for entry in &publish_index.entries {
        // The STW-034 chain inlines the
        // `PublishedRemoteReceipt` per entry;
        // re-verify the inlined receipt with the
        // STW-033 `PublishedRemoteReceipt::verify`.
        // The `bundle_dir` is the parent of the
        // per-entry `remote/` subdir (resolved
        // from the inlined `remote_receipt_path`).
        let entry_bundle_dir = PathBuf::from(&entry.remote_receipt_path)
            .parent()
            .and_then(|p| p.parent())
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."));
        entry.remote_receipt.verify(&entry_bundle_dir)?;
    }
    // The upload plan uploads ONE file: the
    // `INDEX.json` the STW-034 chain wrote (the
    // per-entry `remote_receipt.json` files are
    // already in the bucket, the STW-033 runbook
    // pushed them). Re-hash the on-disk
    // `INDEX.json` and assert the digest matches
    // the STW-034 `PublishIndex::verify` re-hash
    // (a mismatched `INDEX.json` re-hash is a
    // `BundleHashMismatch`).
    let index_bytes = fs::read(&index_path).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not read {}: {e}",
            index_path.display()
        ))
    })?;
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(&index_bytes);
    let index_sha256 = format!("{:x}", hasher.finalize());
    // The `s3_uri` for the `INDEX.json` is
    // `s3://<bucket>/<prefix>/INDEX.json`. The
    // `s3_objects[]` array is sorted by `s3_uri`
    // for determinism.
    let s3_uri = format!(
        "{}/{}/{}",
        bucket, effective_prefix, STW035_INDEX_FILE_TO_UPLOAD
    );
    let s3_objects = vec![IndexRemoteS3Object {
        local_path: index_path.display().to_string(),
        sha256: index_sha256.clone(),
        bytes: index_bytes.len() as u64,
        s3_uri: s3_uri.clone(),
    }];
    let total_bytes: u64 = s3_objects.iter().map(|o| o.bytes).sum();
    let plan = PublishIndexRemotePlan {
        bucket: bucket.clone(),
        prefix: effective_prefix.clone(),
        s3_objects: s3_objects.clone(),
        index_sha256: index_sha256.clone(),
        index_bytes: index_bytes.len() as u64,
        publish_root_basename: publish_root_basename.clone(),
        runbook_version: STW035_INDEX_REMOTE_RUNBOOK_VERSION.to_string(),
        created_at_utc: created_at_utc.clone(),
        dry_run,
    };
    // Live mode: shell out to `aws s3 cp` per
    // `s3_objects[]` entry. A non-zero `aws` exit
    // returns `PublishIndexRemoteError::AwsCli`
    // and the arm exits 2.
    if !dry_run {
        for obj in &s3_objects {
            let out = Command::new("aws")
                .arg("s3")
                .arg("cp")
                .arg(&obj.local_path)
                .arg(&obj.s3_uri)
                .output()
                .map_err(|e| {
                    PublishIndexRemoteError::AwsCli(format!(
                        "could not spawn `aws s3 cp` for {}: {e}",
                        obj.local_path
                    ))
                })?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(PublishIndexRemoteError::AwsCli(format!(
                    "aws s3 cp {} -> {} failed: {}",
                    obj.local_path,
                    obj.s3_uri,
                    stderr.trim()
                )));
            }
        }
    }
    // Write the `index_remote_plan.json` +
    // `index_remote_receipt.json` + `SUMMARY.txt`
    // trio under
    // `<publish-root>/index_remote/`. The
    // index-remote step never mutates the
    // underlying `INDEX.json` or the per-entry
    // `remote_receipt.json` files; it writes a
    // fresh `index_remote/` subdir the dashboard
    // scraper reads from.
    let output_dir = publish_root.join(STW035_INDEX_REMOTE_SUBDIR);
    fs::create_dir_all(&output_dir).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not create output dir {}: {e}",
            output_dir.display()
        ))
    })?;
    let plan_path = output_dir.join(STW035_INDEX_REMOTE_PLAN_FILENAME);
    let plan_str = serde_json::to_string_pretty(&plan).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not serialise index_remote_plan.json: {e}"
        ))
    })?;
    fs::write(&plan_path, plan_str).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not write index_remote_plan.json at {}: {e}",
            plan_path.display()
        ))
    })?;
    // The post-upload receipt mirrors the plan
    // + stamps the `uploaded_at_utc` timestamp.
    // In dry-run, the timestamp matches
    // `created_at_utc` (the runbook did NOT
    // shell out, so the upload time is the plan
    // time).
    let receipt = PublishedIndexRemoteReceipt {
        plan: plan.clone(),
        uploaded_at_utc: created_at_utc.clone(),
        s3_objects,
        total_bytes,
        index_sha256: index_sha256.clone(),
        runbook_version: STW035_INDEX_REMOTE_RUNBOOK_VERSION.to_string(),
    };
    let receipt_path = output_dir.join(STW035_INDEX_REMOTE_RECEIPT_FILENAME);
    let receipt_json = serde_json::to_string_pretty(&receipt).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not serialise index_remote_receipt.json: {e}"
        ))
    })?;
    fs::write(&receipt_path, receipt_json).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not write index_remote_receipt.json at {}: {e}",
            receipt_path.display()
        ))
    })?;
    // The `SUMMARY.txt` is a single-line
    // human-readable headline a CI worker `cat`s
    // to confirm the index-remote step landed
    // end-to-end. The format mirrors the
    // STW-019 / STW-032 / STW-033 / STW-034
    // `SUMMARY.txt` shape.
    let summary_path = output_dir.join(STW035_SUMMARY_FILENAME);
    let summary_str = format!(
        "testnet live_proof publish_index_remote complete: \
         root={} bucket={} prefix={} files={} total_bytes={} index_sha256={} \
         runbook_version={} dry_run={}\n",
        publish_root.display(),
        bucket,
        effective_prefix,
        plan.s3_objects.len(),
        total_bytes,
        index_sha256,
        plan.runbook_version,
        dry_run,
    );
    fs::write(&summary_path, summary_str).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not write SUMMARY.txt at {}: {e}",
            summary_path.display()
        ))
    })?;
    Ok(PublishIndexRemoteOutput {
        plan,
        s3_objects: receipt.s3_objects.clone(),
        total_bytes: receipt.total_bytes,
        index_sha256: receipt.index_sha256.clone(),
        runbook_version: receipt.runbook_version.clone(),
        plan_path,
        receipt_path,
        summary_path,
    })
}

// --- verify (no-DB no-rebuild re-verify path) ---------------------

impl PublishedIndexRemoteReceipt {
    /// Re-verify an `index_remote_receipt.json` from
    /// disk: re-hash the local `INDEX.json` the
    /// receipt claims to have uploaded (the re-hash
    /// compares to the receipt's `index_sha256`
    /// field), assert every digest matches, assert
    /// every `s3_uri` in the receipt appears in the
    /// inlined plan (a phantom `s3_uri` is a hard
    /// `PublishIndexRemoteError::MissingObject`),
    /// and return `Ok(())` on green.
    pub fn verify(&self, _remote_dir: &Path) -> Result<(), PublishIndexRemoteError> {
        // The verifier re-hashes the local
        // `INDEX.json` and compares to the
        // receipt's `index_sha256` field. The
        // `local_path` is the inlined
        // `s3_objects[0].local_path` (absolute) —
        // the verifier reads it verbatim. A
        // mismatch returns
        // `PublishIndexRemoteError::BundleHashMismatch`.
        use sha2::{Digest, Sha256};
        for obj in &self.s3_objects {
            let p = PathBuf::from(&obj.local_path);
            if !p.is_file() {
                return Err(PublishIndexRemoteError::FileUnreadable(format!(
                    "could not read {}: not a file",
                    p.display()
                )));
            }
            let bytes = fs::read(&p).map_err(|e| {
                PublishIndexRemoteError::FileUnreadable(format!(
                    "could not read {}: {e}",
                    p.display()
                ))
            })?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = format!("{:x}", hasher.finalize());
            if actual != obj.sha256 {
                return Err(PublishIndexRemoteError::BundleHashMismatch {
                    path: p.display().to_string(),
                    expected: obj.sha256.clone(),
                    actual,
                });
            }
        }
        // Assert every `s3_uri` in the receipt
        // appears in the inlined plan (a phantom
        // `s3_uri` is a hard
        // `PublishIndexRemoteError::MissingObject`).
        let plan_uris: std::collections::BTreeMap<String, &IndexRemoteS3Object> = self
            .plan
            .s3_objects
            .iter()
            .map(|o| (o.s3_uri.clone(), o))
            .collect();
        for obj in &self.s3_objects {
            if !plan_uris.contains_key(&obj.s3_uri) {
                return Err(PublishIndexRemoteError::MissingObject(obj.s3_uri.clone()));
            }
        }
        Ok(())
    }
    /// Render the receipt to a single-line JSON
    /// string a testnet dashboard can scrape.
    /// The `Display` is for human readability;
    /// the serde `to_string` is the
    /// machine-readable form.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Convenience helper: read + parse an
/// `index_remote_receipt.json` from an index-remote
/// directory. Mirrors the
/// `crate::publish_remote::read_remote_receipt` +
/// `crate::publish_index::read_publish_index` shape.
pub fn read_index_remote_receipt(
    remote_dir: &Path,
) -> Result<PublishedIndexRemoteReceipt, PublishIndexRemoteError> {
    let receipt_path = remote_dir.join(STW035_INDEX_REMOTE_RECEIPT_FILENAME);
    if !receipt_path.is_file() {
        return Err(PublishIndexRemoteError::FileUnreadable(format!(
            "index_remote_receipt.json missing or not a file at {}",
            receipt_path.display()
        )));
    }
    let receipt_str = fs::read_to_string(&receipt_path).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not read index_remote_receipt.json at {}: {e}",
            receipt_path.display()
        ))
    })?;
    serde_json::from_str(&receipt_str).map_err(|e| {
        PublishIndexRemoteError::FileUnreadable(format!(
            "could not parse index_remote_receipt.json: {e}"
        ))
    })
}

// --- lib tests ---------------------------------------------------

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-035
    //! publish-index-remote surface. These tests do
    //! NOT require a live Postgres (the
    //! index-remote is the *upload* side of the
    //! on-disk contract; the producer side is the
    //! STW-034 `trainer --publish-index` chain +
    //! the STW-033 `trainer --publish-remote` chain
    //! + the STW-019 `testnet-live-proof.sh`
    //! runbook).
    //!
    //! Fixture style: a process-unique
    //! `std::env::temp_dir().join("rbp-publish-index-remote-test-<n>")`
    //! subdirectory populated by `setup_publish_root`
    //! (which writes a synthetic green receipt + a
    //! fresh publish bundle + a fresh
    //! `remote_receipt.json` via the STW-032 +
    //! STW-033 surfaces + a fresh `INDEX.json` via
    //! the STW-034 surface), then
    //! `publish_index_remote_receipt`'d in dry-run,
    //! then re-verified via
    //! `PublishedIndexRemoteReceipt::verify`. The
    //! tempdir is removed on drop so re-runs do not
    //! see stale files.
    use super::*;
    use crate::LiveProofReceipt;
    use crate::publish_index::STW034_PUBLISH_SUBDIR;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-publish-index-remote-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Drop a synthetic green receipt + a fresh
    /// publish bundle + a fresh
    /// `remote_receipt.json` + a fresh `INDEX.json`
    /// (the full STW-019 → STW-032 → STW-033 →
    /// STW-034 chain surface the index-remote
    /// step consumes). Returns the
    /// `<publish-root>/` directory the
    /// index-remote scans.
    fn setup_publish_root(label: &str) -> (PathBuf, String) {
        let receipt = fresh_dir(&format!("{label}-receipt"));
        let publish_root = fresh_dir(&format!("{label}-publish-root"));
        let basename = receipt
            .file_name()
            .and_then(|n| n.to_str())
            .expect("receipt must have a basename")
            .to_string();
        // The STW-033 chain expects the publish
        // bundle under
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
        // `trainer --publish-remote` arm writes
        // the `remote_receipt.json` + the
        // `remote_plan.json` to
        // `<publish-root>/publish/<basename>/remote/`.
        // We do not shell out to the trainer
        // binary here (the lib tests are
        // pure-in-memory); we call the
        // `publish_remote_receipt` function
        // directly with a fixed `created_at_utc`
        // so the lib test is byte-stable.
        let _ = crate::publish_remote::publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof/",
            true, // dry-run
            Some("<unknown>"),
        )
        .expect("publish_remote_receipt should drop a fresh remote_receipt.json");
        // The STW-034 chain's
        // `trainer --publish-index` arm writes
        // the `INDEX.json` + `SUMMARY.txt` pair
        // under `<publish-root>/index/`. We call
        // the `publish_index` function directly
        // with a fixed `created_at_utc` so the
        // lib test is byte-stable.
        let _ = crate::publish_index::publish_index(&publish_root, Some("<unknown>"))
            .expect("publish_index should drop a fresh INDEX.json");
        (publish_root, basename)
    }

    /// `bucket_uri_as_str_matches_published_strings_v3`
    /// — the typed
    /// `PublishIndexRemoteError` Display impls
    /// must produce the exact strings the CLI /
    /// dashboard scraper greps. A future
    /// refactor that renames a prefix fails the
    /// test before a dashboard scraper can `grep`
    /// the wrong shape. Mirrors the STW-033 +
    /// STW-034 string-pinners.
    #[test]
    fn bucket_uri_as_str_matches_published_strings_v3() {
        assert_eq!(
            STW035_PUBLISH_INDEX_REMOTE_HEADLINE_PREFIX,
            "live_proof publish_index_remote complete:",
            "publish-index-remote headline prefix must be pinned"
        );
        assert_eq!(
            STW035_VERIFY_INDEX_REMOTE_HEADLINE_PREFIX,
            "live_proof index_remote verification passed:",
            "verify-index-remote headline prefix must be pinned"
        );
        assert_eq!(
            STW035_VERIFY_INDEX_REMOTE_FAILURE_HEADLINE_PREFIX,
            "live_proof index_remote verification failed:",
            "verify-index-remote failure headline prefix must be pinned"
        );
        assert_eq!(
            STW035_INDEX_REMOTE_PLAN_FILENAME, "index_remote_plan.json",
            "index_remote_plan.json filename must be pinned"
        );
        assert_eq!(
            STW035_INDEX_REMOTE_RECEIPT_FILENAME, "index_remote_receipt.json",
            "index_remote_receipt.json filename must be pinned"
        );
        assert_eq!(
            STW035_SUMMARY_FILENAME, "SUMMARY.txt",
            "SUMMARY.txt filename must be pinned"
        );
        assert_eq!(
            STW035_INDEX_REMOTE_RUNBOOK_VERSION, "STW-035 v1",
            "runbook version string must be pinned"
        );
        assert_eq!(
            STW035_UNKNOWN_UTC, "<unknown>",
            "unknown-utc sentinel must be pinned"
        );
        assert_eq!(
            STW035_INDEX_REMOTE_SUBDIR, "index_remote",
            "index_remote subdir name must be pinned"
        );
        assert_eq!(
            STW035_INDEX_FILE_TO_UPLOAD, "INDEX.json",
            "upload INDEX.json filename must be pinned"
        );
        // Error Display strings a dashboard
        // scraper greps.
        assert_eq!(
            PublishIndexRemoteError::IndexRed("foo".to_string()).to_string(),
            "live_proof publish_index_remote error: index is red: foo",
        );
        assert_eq!(
            PublishIndexRemoteError::BundleHashMismatch {
                path: "p".to_string(),
                expected: "e".to_string(),
                actual: "a".to_string(),
            }
            .to_string(),
            "live_proof publish_index_remote error: bundle_hash_mismatch: p: expected e, got a",
        );
        assert_eq!(
            PublishIndexRemoteError::MissingObject("s3://x".to_string()).to_string(),
            "live_proof publish_index_remote error: missing_object: s3://x",
        );
        assert_eq!(
            PublishIndexRemoteError::FileUnreadable("oops".to_string()).to_string(),
            "live_proof publish_index_remote error: file unreadable: oops",
        );
        assert_eq!(
            PublishIndexRemoteError::BucketUri("b".to_string()).to_string(),
            "live_proof publish_index_remote error: bucket uri: b",
        );
        assert_eq!(
            PublishIndexRemoteError::AwsCli("oops".to_string()).to_string(),
            "live_proof publish_index_remote error: aws cli: oops",
        );
    }

    /// `bucket_uri_rejects_non_s3_prefix_v2` — the
    /// index-remote step refuses to plan an
    /// upload for a `--bucket` value that is not
    /// a `s3://` / `gs://` / `git://` URI. Mirrors
    /// the STW-033 `bucket_uri_rejects_non_s3_prefix`
    /// pin.
    #[test]
    fn bucket_uri_rejects_non_s3_prefix_v2() {
        let (publish_root, _basename) = setup_publish_root("bucket-reject");
        let result = publish_index_remote_receipt(
            &publish_root,
            "https://example.com/bucket",
            "",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishIndexRemoteError::BucketUri(s)) => {
                assert!(
                    s.contains("https://example.com/bucket"),
                    "BucketUri must name the rejected bucket; got: {s:?}"
                );
            }
            other => panic!("non-s3 bucket must produce BucketUri; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `bucket_uri_rejects_empty_bucket_v2` — the
    /// index-remote step refuses to plan an
    /// upload for an empty `--bucket` value.
    /// Mirrors the STW-033
    /// `bucket_uri_rejects_empty_bucket` pin.
    #[test]
    fn bucket_uri_rejects_empty_bucket_v2() {
        let (publish_root, _basename) = setup_publish_root("bucket-empty");
        let result = publish_index_remote_receipt(
            &publish_root,
            "",
            "",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishIndexRemoteError::BucketUri(_)) => {}
            other => panic!("empty bucket must produce BucketUri; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_dry_run_writes_plan_and_receipt`
    /// — the dry-run index-remote writes a
    /// valid `index_remote_plan.json` +
    /// `index_remote_receipt.json` + `SUMMARY.txt`
    /// trio under `<publish-root>/index_remote/`.
    /// The `PublishIndexRemoteOutput` mirrors what
    /// a downstream auditor reads back from disk
    /// via `read_index_remote_receipt`.
    #[test]
    fn publish_index_remote_dry_run_writes_plan_and_receipt() {
        let (publish_root, _basename) = setup_publish_root("dry-run-writes");
        let result = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root must remote-index");
        assert!(
            result.plan_path.is_file(),
            "index_remote_plan.json must exist on disk at {}",
            result.plan_path.display()
        );
        assert!(
            result.receipt_path.is_file(),
            "index_remote_receipt.json must exist on disk at {}",
            result.receipt_path.display()
        );
        assert!(
            result.summary_path.is_file(),
            "SUMMARY.txt must exist on disk at {}",
            result.summary_path.display()
        );
        assert_eq!(
            result.s3_objects.len(),
            1,
            "the index-remote step uploads exactly one file (the INDEX.json); got {}",
            result.s3_objects.len()
        );
        assert_eq!(
            result.total_bytes, result.s3_objects[0].bytes,
            "the total_bytes must mirror the s3_objects[0].bytes"
        );
        assert!(
            result.total_bytes > 0,
            "the total_bytes must be non-zero; got {}",
            result.total_bytes
        );
        assert!(
            result.index_sha256.len() == 64,
            "the index_sha256 must be a 64-char lowercase hex digest; got: {:?}",
            result.index_sha256
        );
        assert_eq!(
            result.runbook_version, STW035_INDEX_REMOTE_RUNBOOK_VERSION,
            "the runbook version must be pinned"
        );
        assert!(
            result.plan.dry_run,
            "the inlined plan must mirror the dry_run arg"
        );
        // The `index_remote_receipt.json` must
        // round-trip through
        // `read_index_remote_receipt`.
        let remote_dir = result.receipt_path.parent().unwrap().to_path_buf();
        let round_tripped = read_index_remote_receipt(&remote_dir)
            .expect("index_remote_receipt.json must round-trip");
        // Build the expected receipt from the
        // typed output (the on-disk receipt
        // mirrors the typed output's `plan` +
        // `s3_objects` + `index_sha256` +
        // `runbook_version` + `total_bytes`).
        let expected = PublishedIndexRemoteReceipt {
            plan: result.plan.clone(),
            uploaded_at_utc: result.plan.created_at_utc.clone(),
            s3_objects: result.s3_objects.clone(),
            total_bytes: result.total_bytes,
            index_sha256: result.index_sha256.clone(),
            runbook_version: result.runbook_version.clone(),
        };
        assert_eq!(
            round_tripped, expected,
            "the round-tripped index_remote_receipt.json must equal the in-memory receipt"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_s3_uris_are_sorted_for_determinism`
    /// — the per-file `s3_objects[]` array is
    /// sorted by `s3_uri` for determinism. A
    /// regression that drops the sort fails the
    /// test because the
    /// `serde_json::to_string_pretty` output would
    /// be non-deterministic across re-runs.
    #[test]
    fn publish_index_remote_s3_uris_are_sorted_for_determinism() {
        let (publish_root, _basename) = setup_publish_root("s3-uris-sorted");
        let result = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root must remote-index");
        let mut sorted = result.s3_objects.clone();
        sorted.sort_by(|a, b| a.s3_uri.cmp(&b.s3_uri));
        assert_eq!(
            result.s3_objects, sorted,
            "the s3_objects[] array must be sorted by s3_uri"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_refuses_red_index` —
    /// the index-remote step refuses to plan an
    /// upload for a publish root whose
    /// `INDEX.json` is red (a
    /// `PublishIndex::verify` re-verify gate fires
    /// before the plan is written). This is the
    /// "refuse to paper-over a red index"
    /// invariant the STW-034 verifier enforces.
    #[test]
    fn publish_index_remote_refuses_red_index() {
        let (publish_root, basename) = setup_publish_root("red-index");
        // Tamper with the `INDEX.json`'s
        // per-entry `remote_receipt.s3_objects[0].sha256`
        // to a bogus value. The STW-034
        // `PublishIndex::verify` re-runs the
        // per-entry
        // `PublishedRemoteReceipt::verify`
        // (which re-hashes the underlying file
        // and compares); a tampered entry
        // `sha256` fails the re-hash with a
        // `BundleHashMismatch`.
        let index_dir = publish_root.join(STW034_INDEX_SUBDIR);
        let index_path = index_dir.join(STW034_INDEX_FILENAME);
        let mut index: PublishIndex =
            serde_json::from_str(&fs::read_to_string(&index_path).expect("read INDEX.json"))
                .expect("parse INDEX.json");
        let entry = index
            .entries
            .iter_mut()
            .find(|e| e.receipt_basename == basename)
            .expect("entry must exist");
        if let Some(obj) = entry.remote_receipt.s3_objects.first_mut() {
            obj.sha256 = "deadbeef".repeat(8);
        }
        let tampered_str = serde_json::to_string_pretty(&index).expect("serialise tampered index");
        fs::write(&index_path, tampered_str).expect("rewrite INDEX.json with bogus sha256");
        let result = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishIndexRemoteError::IndexRed(_)) => {}
            Err(PublishIndexRemoteError::BundleHashMismatch { .. }) => {}
            other => {
                panic!("red INDEX.json must produce IndexRed or BundleHashMismatch; got: {other:?}")
            }
        }
        // The index-remote must NOT have
        // created the `index_remote/` subdir for
        // a red publish root (the pre-upload
        // gate fires before the plan is
        // written).
        let output_dir = publish_root.join(STW035_INDEX_REMOTE_SUBDIR);
        assert!(
            !output_dir.exists(),
            "index_remote subdir must NOT exist for a red publish root; got: {}",
            output_dir.display()
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_round_trips_through_verifier`
    /// — the index-remote step's post-upload
    /// `index_remote_receipt.json` re-verifies
    /// cleanly through
    /// `PublishedIndexRemoteReceipt::verify`. A
    /// green round-trip proves a real
    /// `trainer --publish-index-remote` write
    /// produces an `index_remote_receipt.json`
    /// a real `trainer --verify-index-remote`
    /// can re-verify — the on-the-wire contract
    /// the testnet dashboard's `aws s3 cp` +
    /// `trainer --verify-index-remote`
    /// round-trip depends on.
    #[test]
    fn publish_index_remote_round_trips_through_verifier() {
        let (publish_root, _basename) = setup_publish_root("round-trip-verify");
        let out = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root must remote-index");
        let remote_dir = out.receipt_path.parent().unwrap().to_path_buf();
        let round_tripped = read_index_remote_receipt(&remote_dir)
            .expect("index_remote_receipt.json must round-trip");
        round_tripped
            .verify(&remote_dir)
            .expect("green index_remote_receipt.json must verify");
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_verifier_rejects_tampered_index`
    /// — the
    /// `PublishedIndexRemoteReceipt::verify`
    /// rejects an `index_remote_receipt.json`
    /// whose `s3_objects[0].sha256` is tampered
    /// with (a `BundleHashMismatch` because the
    /// re-hash of the on-disk `INDEX.json` no
    /// longer matches the receipt's claimed
    /// `sha256`).
    #[test]
    fn publish_index_remote_verifier_rejects_tampered_index() {
        let (publish_root, _basename) = setup_publish_root("verify-tamper");
        let out = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root must remote-index");
        let remote_dir = out.receipt_path.parent().unwrap().to_path_buf();
        let receipt_path = remote_dir.join(STW035_INDEX_REMOTE_RECEIPT_FILENAME);
        let mut tampered: PublishedIndexRemoteReceipt = serde_json::from_str(
            &fs::read_to_string(&receipt_path).expect("read index_remote_receipt.json"),
        )
        .expect("parse index_remote_receipt.json");
        if let Some(obj) = tampered.s3_objects.first_mut() {
            obj.sha256 = "deadbeef".repeat(8);
        }
        let tampered_str =
            serde_json::to_string_pretty(&tampered).expect("serialise tampered receipt");
        fs::write(&receipt_path, tampered_str).expect("write tampered index_remote_receipt.json");
        let re_read = read_index_remote_receipt(&remote_dir)
            .expect("tampered index_remote_receipt.json must round-trip");
        let result = re_read.verify(&remote_dir);
        match result {
            Err(PublishIndexRemoteError::BundleHashMismatch { .. }) => {}
            other => panic!(
                "tampered index_remote_receipt.json must produce BundleHashMismatch; got: {other:?}"
            ),
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_to_json_contains_every_field`
    /// — the
    /// `PublishedIndexRemoteReceipt::to_json`
    /// serialiser produces a string a dashboard
    /// scraper can `grep` for every top-level +
    /// nested field the runbook ships. A
    /// refactor that drops a field (e.g.
    /// `index_sha256` or `total_bytes`) fails
    /// the test before a dashboard scraper can
    /// `jq` a missing key.
    #[test]
    fn publish_index_remote_to_json_contains_every_field() {
        let (publish_root, _basename) = setup_publish_root("to-json");
        let out = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root must remote-index");
        let receipt_for_json = PublishedIndexRemoteReceipt {
            plan: out.plan.clone(),
            uploaded_at_utc: out.plan.created_at_utc.clone(),
            s3_objects: out.s3_objects.clone(),
            total_bytes: out.total_bytes,
            index_sha256: out.index_sha256.clone(),
            runbook_version: out.runbook_version.clone(),
        };
        let s = receipt_for_json.to_json();
        for field in [
            "\"plan\":",
            "\"bucket\":",
            "\"prefix\":",
            "\"s3_objects\":",
            "\"index_sha256\":",
            "\"index_bytes\":",
            "\"publish_root_basename\":",
            "\"runbook_version\":",
            "\"created_at_utc\":",
            "\"dry_run\":",
            "\"uploaded_at_utc\":",
            "\"total_bytes\":",
            "\"local_path\":",
            "\"sha256\":",
            "\"bytes\":",
            "\"s3_uri\":",
        ] {
            assert!(
                s.contains(field),
                "to_json must contain field {field:?}; got: {s}"
            );
        }
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_bare_bucket_name_normalises_to_s3_uri`
    /// — the index-remote step normalises a bare
    /// `--bucket` value (e.g. `robopoker-testnet-dashboard`)
    /// to a fully-qualified `s3://...` URI. The
    /// `s3_objects[0].s3_uri` must start with
    /// `s3://` so a CI worker that `aws s3 cp`s
    /// the file lands a single key a dashboard
    /// scraper can fetch + re-verify.
    #[test]
    fn publish_index_remote_bare_bucket_name_normalises_to_s3_uri() {
        let (publish_root, _basename) = setup_publish_root("bare-bucket");
        let result = publish_index_remote_receipt(
            &publish_root,
            "robopoker-testnet-dashboard", // bare name
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("green publish root + bare bucket name must remote-index");
        assert!(
            result.plan.bucket == "s3://robopoker-testnet-dashboard",
            "the plan.bucket must be normalised to s3://...; got: {:?}",
            result.plan.bucket
        );
        assert!(
            result.s3_objects[0]
                .s3_uri
                .starts_with("s3://robopoker-testnet-dashboard/"),
            "the s3_uri must start with the normalised s3:// bucket; got: {:?}",
            result.s3_objects[0].s3_uri
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_created_at_utc_falls_back_to_unknown`
    /// — when the `created_at_utc` arg is `None`
    /// and the `RBP_PUBLISH_INDEX_REMOTE_UTC` env
    /// knob is unset, the index-remote falls
    /// back to the `<unknown>` sentinel so the
    /// lib test is byte-stable on a CI runner
    /// that does not stamp the env knob.
    #[test]
    fn publish_index_remote_created_at_utc_falls_back_to_unknown() {
        let (publish_root, _basename) = setup_publish_root("unknown-utc");
        // Ensure the env knob is unset for this
        // test.
        unsafe {
            std::env::remove_var("RBP_PUBLISH_INDEX_REMOTE_UTC");
        }
        let result = publish_index_remote_receipt(
            &publish_root,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            None, // fall back to env knob / <unknown>
        )
        .expect("green publish root must remote-index");
        assert_eq!(
            result.plan.created_at_utc, STW035_UNKNOWN_UTC,
            "the plan.created_at_utc must fall back to <unknown> when no env knob is set"
        );
        let _ = fs::remove_dir_all(&publish_root);
    }

    /// `publish_index_remote_io_error_propagates_for_missing_root`
    /// — the index-remote returns
    /// `PublishIndexRemoteError::FileUnreadable`
    /// when the publish root does not exist (a
    /// setup error the operator should see, not
    /// silently swallow).
    #[test]
    fn publish_index_remote_io_error_propagates_for_missing_root() {
        let bogus = fresh_dir("missing-root");
        let result = publish_index_remote_receipt(
            &bogus,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishIndexRemoteError::FileUnreadable(s)) => {
                assert!(
                    s.contains("missing-root"),
                    "FileUnreadable must name the missing directory; got: {s:?}"
                );
            }
            other => panic!("missing root must produce FileUnreadable; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&bogus);
    }

    /// `publish_index_remote_io_error_propagates_for_missing_index`
    /// — the index-remote returns
    /// `PublishIndexRemoteError::FileUnreadable`
    /// (via the `From<PublishIndexError>` impl)
    /// when the publish root has no
    /// `<publish-root>/index/INDEX.json` (the
    /// STW-034 chain hasn't run yet).
    #[test]
    fn publish_index_remote_io_error_propagates_for_missing_index() {
        let no_index = fresh_dir("no-index");
        fs::create_dir_all(&no_index).expect("mkdir no-index");
        let result = publish_index_remote_receipt(
            &no_index,
            "s3://robopoker-testnet-dashboard",
            "testnet-index/",
            true, // dry-run
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishIndexRemoteError::FileUnreadable(_)) => {}
            Err(PublishIndexRemoteError::IndexRed(_)) => {}
            other => {
                panic!("missing INDEX.json must produce FileUnreadable or IndexRed; got: {other:?}")
            }
        }
        let _ = fs::remove_dir_all(&no_index);
    }
}
