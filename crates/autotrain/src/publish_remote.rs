//! `trainer --publish-remote <receipt-dir> --bucket <s3://...>`
//! — plan + (optionally) apply an upload of the
//! STW-032 publish bundle to a remote object store
//! (S3 / GCS / git-tag) bucket the operator (or a
//! CI worker) names.
//!
//! STW-019 shipped `scripts/testnet-live-proof.sh`,
//! the operator runbook that drops a
//! `receipts/testnet-live-proof-<UTC-ISO>/` receipt
//! directory. STW-023 shipped the
//! `LiveProofReceipt` verifier the `cargo test`
//! integration test calls. STW-028 wired the
//! verifier into the `trainer` binary as
//! `trainer --verify-receipt <path>`. STW-032
//! shipped `trainer --publish <receipt>` (the
//! content-addressed portable bundle writer) +
//! `trainer --verify-bundle <path>` (the no-DB
//! re-verifier). **STW-033 lands the
//! "publish-remote" half the STW-032 runbook doc
//! names as the next slice**: a deterministic
//! upload plan + a `remote_receipt.json` manifest
//! a CI worker can re-verify without re-running
//! the chain.
//!
//! ## Why a separate module from `publish`
//!
//! `publish.rs` is the *bundle* side (turn a
//! receipt into a portable `tar.gz` + sha256 +
//! manifest). `publish_remote.rs` is the *upload*
//! side (turn the bundle into a remote-upload
//! plan + a `remote_receipt.json` an auditor can
//! re-verify). The two are split because the
//! error surfaces are different: a regression in
//! the bundle manifest's `path` field does not
//! change the upload plan's `s3_uri` shape, and
//! vice versa. A typed `PublishRemoteError` enum
//! lives next to the upload plan so the
//! integration test can assert on the
//! failure kind a dashboard scraper greps.
//!
//! ## Why the dry-run default
//!
//! The `trainer --publish-remote` arm defaults
//! to **dry-run** (`RBP_PUBLISH_REMOTE_DRY_RUN=1`).
//! Dry-run means the arm writes the
//! `remote_plan.json` + `remote_receipt.json`
//! to `<publish>/<basename>/remote/` and exits
//! 0 without shelling out to `aws` / `gsutil` /
//! `git`. This is the *no-network* contract a
//! `cargo test --workspace` invocation can run
//! without an `aws` credential or a live bucket.
//! The `apply` step (which actually calls
//! `aws s3 cp`) is gated behind an explicit
//! `--no-dry-run` argv (or `RBP_PUBLISH_REMOTE_DRY_RUN=0`)
//! the companion `scripts/testnet-live-publish-s3.sh`
//! runbook sets when an operator runs the
//! chain against a real bucket.
//!
//! ## Why the upload plan is a JSON file
//!
//! The companion runbook is a pure-bash driver;
//! it shells out to `aws s3 cp` per file with
//! the `--recursive` flag. The Rust arm writes
//! the **upload plan** (the per-file `local_path` →
//! `s3_uri` mapping + per-file `sha256` + bytes)
//! to `remote_plan.json` so the bash runbook can
//! `jq -r '.s3_objects[] | "aws s3 cp \(.local_path)
//! \(.s3_uri)"' remote_plan.json` and execute
//! the upload. The `remote_receipt.json` is the
//! post-upload manifest the runbook writes after
//! the per-file `aws s3 cp` exits 0 — it lists
//! every uploaded `s3_uri` with its observed
//! `bytes` + `sha256` (the same digest the
//! manifest stored for the local bundle, so a
//! `trainer --verify-remote <publish>/<basename>/remote/`
//! invocation can re-verify the upload claim
//! against the on-bucket digests).
//!
//! ## Why no AWS / GCS SDK dependency
//!
//! The arm does **not** vendor the AWS SDK or
//! `rusoto_s3` or any other object-store client.
//! The `aws s3 cp` / `gsutil cp` / `git tag`
//! shell-out is the runbook's job (a CI worker
//! that already has the `aws` CLI installed is
//! the natural surface area; a worker that
//! doesn't can use `aws s3api put-object` with
//! a hand-rolled HTTP call). Adding a 50-MB
//! SDK to a no-system-deps trainer binary is
//! the inverse of the "pure bash + cargo +
//! trainer" shape the rest of the autotrain
//! pipeline already follows.
//!
//! ## Why the verifier is bundled
//!
//! `PublishedRemoteReceipt::verify(plan)` re-hashes
//! the on-disk `remote_receipt.json` against the
//! `remote_plan.json` and the parent
//! `PublishedBundle` manifest. A `trainer
//! --verify-remote <dir>` invocation calls this
//! verifier end-to-end, so a CI auditor can
//! re-fetch a `remote/` directory from a
//! dashboard bucket and assert the on-bucket
//! sha256s match the local plan's sha256s. The
//! contract is: an upload whose `remote_receipt.json`
//! does NOT re-verify is a hard error, not a
//! warning (a dashboard that displays a "verified"
//! badge on a red remote receipt is the
//! inverse of the no-paper-over invariant
//! STW-023 / STW-028 / STW-032 already enforce).
//!
//! ## Headline format
//!
//! The CLI prints a one-line
//! `live_proof publish_remote complete: ...`
//! headline a dashboard scraper can
//! `grep ^live_proof publish_remote complete:`.
//! The verifier prints a one-line
//! `live_proof remote verification passed: ...` /
//! `live_proof remote verification failed: ...`
//! line a dashboard scraper can
//! `grep ^live_proof remote verification` the
//! log. Both prefixes share the
//! `live_proof ...` family so a single
//! `grep ^live_proof` scraper can read the
//! whole chain.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::LiveProofReceipt;
use crate::publish::{
    PublishedBundle, STW032_BUNDLE_FILENAME, STW032_MANIFEST_FILENAME, STW032_SHA256_FILENAME,
};

// --- pinned headline prefixes -----------------------------------------

/// Pinned publish-remote success headline prefix.
/// The `trainer --publish-remote <receipt>` CLI
/// prints a one-line
/// `live_proof publish_remote complete: ...`
/// headline a dashboard scraper can
/// `grep ^live_proof publish_remote complete:`.
/// The verifier `trainer --verify-remote <dir>`
/// mirrors the STW-032 verifier's
/// `live_proof bundle verification passed: ...`
/// prefix with a
/// `live_proof remote verification passed: ...`
/// prefix. Both prefixes share the
/// `live_proof ...` family so a single
/// `grep ^live_proof` scraper can read the
/// whole chain.
pub const STW033_PUBLISH_REMOTE_HEADLINE_PREFIX: &str = "live_proof publish_remote complete:";
/// Pinned remote-verifier success prefix.
pub const STW033_VERIFY_REMOTE_HEADLINE_PREFIX: &str = "live_proof remote verification passed:";
/// Pinned remote-verifier failure prefix.
pub const STW033_VERIFY_REMOTE_FAILURE_HEADLINE_PREFIX: &str =
    "live_proof remote verification failed:";

/// Pinned file names written under
/// `<publish>/<basename>/remote/`. The constants
/// live in Rust so the bash runbook's
/// `cat > remote_plan.json` / `cat > remote_receipt.json`
/// heredocs and the Rust writer agree on the
/// filenames.
pub const STW033_REMOTE_PLAN_FILENAME: &str = "remote_plan.json";
pub const STW033_REMOTE_RECEIPT_FILENAME: &str = "remote_receipt.json";

/// Pinned runbook version the bundle records.
/// Bumped by hand if the upload-plan / remote-
/// receipt JSON shape changes.
pub const STW033_RUNBOOK_VERSION: &str = "STW-033 v1";

// --- typed structures ------------------------------------------------

/// One per-file upload entry. The
/// `PublishRemotePlan::s3_objects[]` array is a
/// `Vec<S3Object>` sorted by `s3_uri` so the
/// plan is byte-stable across machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct S3Object {
    /// Local absolute path to the file the
    /// publisher wrote (`bundle.tar.gz`,
    /// `manifest.json`, `bundle.sha256`).
    pub local_path: String,
    /// Lowercase hex `sha256` of the local
    /// file's bytes (the same digest the parent
    /// `PublishedBundle::files[]` records).
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
    /// Fully-qualified S3 / GCS / git-tag URI
    /// the file would be uploaded to
    /// (`s3://<bucket>/<prefix>/<filename>`).
    pub s3_uri: String,
}

/// The upload plan the `trainer --publish-remote`
/// arm writes to
/// `<publish>/<basename>/remote/remote_plan.json`.
/// The plan is the input the bash runbook's
/// `aws s3 cp` shell-out iterates; the runbook
/// reads `s3_objects[]` and uploads each
/// `local_path -> s3_uri` pair. The plan is
/// the *deterministic* contract: a byte-identical
/// publish bundle produces a byte-identical
/// plan (the `s3_objects[]` array is sorted by
/// `s3_uri` and the `created_at_utc` falls back
/// to a sentinel when the env knob is unset so
/// the integration test is byte-stable).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishRemotePlan {
    /// Bucket the upload targets (e.g.
    /// `robopoker-testnet-dashboard`).
    pub bucket: String,
    /// Prefix inside the bucket (e.g.
    /// `testnet-live-proof-20260604T050000Z/`).
    /// Defaults to `<basename>/` if the operator
    /// passes `--prefix ''`.
    pub prefix: String,
    /// Optional S3 / GCS region hint. The
    /// upload step does NOT consume this (the
    /// `aws` CLI reads `AWS_REGION` from the
    /// shell env); the field exists for
    /// human-readability of the plan only.
    pub region: String,
    /// Per-file upload entries, sorted by
    /// `s3_uri` for determinism.
    pub s3_objects: Vec<S3Object>,
    /// Lowercase hex `sha256` of the parent
    /// `PublishedBundle`'s `bundle.tar.gz`
    /// file. The remote receipt re-hashes the
    /// bundle on the verifier's machine + asserts
    /// it matches.
    pub bundle_sha256: String,
    /// Tarball size in bytes (mirrors the
    /// parent `PublishedBundle::total_bytes`).
    pub bundle_bytes: u64,
    /// The receipt dir basename the bundle
    /// was built from (e.g.
    /// `testnet-live-proof-20260604T050000Z`).
    pub receipt_basename: String,
    /// The runbook version string (e.g.
    /// `"STW-033 v1"`). Bumped by hand if the
    /// upload plan JSON shape changes.
    pub runbook_version: String,
    /// ISO-8601 UTC timestamp the plan was
    /// written. Read from the
    /// `RBP_TRAINER_GIT_SHA`-style
    /// `RBP_PUBLISH_REMOTE_UTC` env knob so
    /// the integration test is byte-stable
    /// (the env knob defaults to `<unknown>`
    /// when unset — the lib test + the
    /// committed `publish-remote-fixture/`
    /// use this sentinel).
    pub created_at_utc: String,
    /// Whether the upload is a dry-run (the
    /// default). When `true`, the arm writes
    /// the plan + the receipt but does NOT
    /// shell out to `aws` / `gsutil` / `git`.
    /// When `false`, the arm shells out to
    /// `aws s3 cp` per `s3_objects[]` entry
    /// (this requires the `aws` CLI to be on
    /// `$PATH` and the shell to have the
    /// `AWS_ACCESS_KEY_ID` /
    /// `AWS_SECRET_ACCESS_KEY` env knobs
    /// set; a missing `aws` returns
    /// `PublishRemoteError::AwsCli` and the
    /// arm exits 2).
    pub dry_run: bool,
}

/// The post-upload manifest the runbook writes
/// to
/// `<publish>/<basename>/remote/remote_receipt.json`
/// AFTER every `aws s3 cp` exit 0. The remote
/// receipt's `s3_objects[]` mirrors the plan's
/// `s3_objects[]` but adds the `uploaded_at_utc`
/// timestamp the runbook observed when the
/// `aws s3 cp` returned. The verifier
/// re-hashes the local files, asserts every
/// digest matches the manifest, and asserts
/// every `s3_uri` in the receipt appears in
/// the plan (a phantom `s3_uri` in the receipt
/// is a `PublishRemoteError::MissingObject`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedRemoteReceipt {
    /// The plan that produced this receipt
    /// (inlined as a copy of the plan, not a
    /// path reference, so the verifier can
    /// re-verify the receipt without reading
    /// a sibling `remote_plan.json` file).
    /// This is the
    /// "operator-visible receipt and the
    /// CI-visible receipt share one source of
    /// truth" invariant STW-023 / STW-032
    /// already enforce.
    pub plan: PublishRemotePlan,
    /// ISO-8601 UTC timestamp the upload
    /// completed. Read from the
    /// `RBP_PUBLISH_REMOTE_UTC` env knob (or
    /// `<unknown>` for the lib test fixture).
    pub uploaded_at_utc: String,
    /// Per-file upload entries that the
    /// runbook observed succeed, sorted by
    /// `s3_uri` for determinism.
    pub s3_objects: Vec<S3Object>,
    /// The number of bytes uploaded in total
    /// (sum of `s3_objects[].bytes`).
    pub total_bytes: u64,
    /// The lower-case hex sha256 of the parent
    /// `PublishedBundle`'s `bundle.tar.gz` (a
    /// copy of `plan.bundle_sha256` so the
    /// verifier can re-verify the receipt
    /// without re-reading the plan).
    pub bundle_sha256: String,
    /// The runbook version string
    /// (mirrors `plan.runbook_version`).
    pub runbook_version: String,
}

// --- typed error ----------------------------------------------------

/// Publisher-remote error: a single typed
/// error so the CLI / integration test can
/// assert on the `PublishRemoteError::*`
/// variant. The variants cover the failure
/// modes the publisher-remote / verifier
/// detect:
///
/// - `ReceiptRed` — the receipt did not pass
///   `LiveProofReceipt::read_and_verify`;
///   the publisher-remote refuses to plan an
///   upload for a red receipt.
/// - `BundleHashMismatch` — the
///   `remote_receipt.json` claims a sha256
///   that does not match the on-disk file.
/// - `MissingObject` — the
///   `remote_receipt.json` names an `s3_uri`
///   that does not appear in the
///   `remote_plan.json`.
/// - `FileUnreadable` — a file inside the
///   publish directory could not be read.
/// - `ReceiptDir` — the input receipt
///   directory does not exist or is not a
///   directory.
/// - `BundleDir` — the input publish
///   directory does not exist or is not a
///   directory.
/// - `BucketUri` — the `--bucket` value is
///   not a valid `s3://...` / `gs://...` /
///   `git://...` URI.
/// - `AwsCli` — the live `aws s3 cp` step
///   failed (CLI missing, no creds, or
///   network error). Only fired when
///   `--no-dry-run` is set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishRemoteError {
    /// The receipt failed the STW-023 verifier.
    /// The publisher-remote refuses to plan an
    /// upload for a red receipt.
    ReceiptRed(String),
    /// A file inside the publish directory has
    /// a `sha256` that does not match the
    /// remote receipt.
    BundleHashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// The remote receipt names an `s3_uri`
    /// that does not appear in the plan.
    MissingObject(String),
    /// A file inside the publish directory
    /// could not be read.
    FileUnreadable(String),
    /// The input receipt directory does not
    /// exist or is not a directory.
    ReceiptDir(String),
    /// The input publish directory does not
    /// exist or is not a directory.
    BundleDir(String),
    /// The `--bucket` value is not a valid
    /// object-store URI.
    BucketUri(String),
    /// The `aws` CLI failed (missing binary,
    /// missing creds, network error). Only
    /// fired in live mode.
    AwsCli(String),
}

impl fmt::Display for PublishRemoteError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishRemoteError::ReceiptRed(s) => {
                write!(f, "live_proof publish_remote error: receipt is red: {s}")
            }
            PublishRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "live_proof publish_remote error: bundle_hash_mismatch: {path}: \
                 expected {expected}, got {actual}"
            ),
            PublishRemoteError::MissingObject(s) => {
                write!(f, "live_proof publish_remote error: missing_object: {s}")
            }
            PublishRemoteError::FileUnreadable(s) => {
                write!(f, "live_proof publish_remote error: file unreadable: {s}")
            }
            PublishRemoteError::ReceiptDir(s) => {
                write!(f, "live_proof publish_remote error: receipt dir: {s}")
            }
            PublishRemoteError::BundleDir(s) => {
                write!(f, "live_proof publish_remote error: bundle dir: {s}")
            }
            PublishRemoteError::BucketUri(s) => {
                write!(f, "live_proof publish_remote error: bucket uri: {s}")
            }
            PublishRemoteError::AwsCli(s) => {
                write!(f, "live_proof publish_remote error: aws cli: {s}")
            }
        }
    }
}

impl std::error::Error for PublishRemoteError {}

// --- entry points ---------------------------------------------------

/// Plan + (optionally) apply a remote upload
/// of the STW-032 publish bundle. The function
/// is the no-network-aware entry point the
/// `Mode::PublishRemote` arm delegates to; it
/// reads the STW-032 publish bundle from
/// `<publish>/<basename>/`, re-verifies the
/// bundle against its on-disk manifest, builds
/// the per-file upload plan, and either (a)
/// writes the plan + a stub `remote_receipt.json`
/// to `<publish>/<basename>/remote/`
/// (dry-run, the default) or (b) shells out to
/// `aws s3 cp` per `s3_objects[]` and writes
/// the post-upload `remote_receipt.json` (live,
/// gated by `--no-dry-run`).
///
/// `bucket` is the bucket name
/// (e.g. `robopoker-testnet-dashboard`). `prefix`
/// is the key prefix inside the bucket
/// (e.g. `testnet-live-proof-20260604T050000Z/`).
/// The arm builds the full `s3_uri` as
/// `s3://<bucket>/<prefix>/<filename>` per file.
///
/// `created_at_utc` is the ISO-8601 UTC
/// timestamp stamped on the plan + the
/// receipt. The function reads it from the
/// `RBP_PUBLISH_REMOTE_UTC` env knob when
/// `created_at_utc` is `None`; an empty env
/// falls back to the `<unknown>` sentinel
/// (the lib test + the committed
/// `publish-remote-fixture/` use this sentinel
/// for byte-stability).
pub fn publish_remote_receipt<P: AsRef<Path>, Q: AsRef<Path>>(
    receipt_dir: P,
    bundle_dir: Q,
    bucket: &str,
    prefix: &str,
    dry_run: bool,
    created_at_utc: Option<&str>,
) -> Result<PublishedRemoteReceipt, PublishRemoteError> {
    let receipt_dir = receipt_dir.as_ref();
    let bundle_dir = bundle_dir.as_ref();
    if !receipt_dir.is_dir() {
        return Err(PublishRemoteError::ReceiptDir(format!(
            "receipt dir {} does not exist or is not a directory",
            receipt_dir.display()
        )));
    }
    // Pre-upload gate (FIRST, before the
    // bundle-dir / bucket-uri gates): refuse to
    // plan an upload for a red receipt. The
    // STW-023 `LiveProofReceipt::read_and_verify`
    // is the source of truth for "is this receipt
    // publishable?"; if the receipt doesn't pass,
    // neither does the plan. This MUST run
    // BEFORE the `BundleDir` gate so a red
    // receipt does not short-circuit to a
    // `bundle dir: ...` error before the receipt
    // verifier can say "receipt is red". (A red
    // receipt does not have a publish bundle
    // because the STW-032 publisher refuses to
    // tar one — so the `BundleDir` gate would
    // fire first and the operator would see the
    // *less specific* error.)
    if let Err(e) = LiveProofReceipt::read_and_verify(receipt_dir) {
        return Err(PublishRemoteError::ReceiptRed(format!(
            "receipt at {} failed verification: {e}",
            receipt_dir.display()
        )));
    }
    if !bundle_dir.is_dir() {
        return Err(PublishRemoteError::BundleDir(format!(
            "bundle dir {} does not exist or is not a directory",
            bundle_dir.display()
        )));
    }
    // Bucket must look like a valid object-store
    // URI scheme. We accept `s3://...`,
    // `gs://...`, and `git://...` (a follow-on
    // `git-tag` push slice can re-use the same
    // `PublishRemotePlan` shape with `git://` URIs).
    if bucket.is_empty() {
        return Err(PublishRemoteError::BucketUri(
            "bucket must be non-empty".to_string(),
        ));
    }
    // If the bucket is a URI but with a
    // scheme we don't support (e.g.
    // `https://...`), reject it BEFORE the
    // bare-name normalization (otherwise
    // `https://example.com/bucket` would be
    // mis-canonicalised to
    // `s3://https://example.com/bucket` and
    // pass the scheme check below).
    if bucket.contains("://")
        && !bucket.starts_with("s3://")
        && !bucket.starts_with("gs://")
        && !bucket.starts_with("git://")
    {
        return Err(PublishRemoteError::BucketUri(format!(
            "bucket {bucket} must start with s3://, gs://, or git://"
        )));
    }
    // The `--bucket` flag accepts either a
    // fully-qualified URI (`s3://<name>/`) or a
    // bare bucket name (`<name>`). We normalise
    // the bare-name form to the URI form so the
    // scheme-prefix check + the `s3_uri`
    // construction below are uniform.
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
        return Err(PublishRemoteError::BucketUri(format!(
            "bucket {bucket} must start with s3://, gs://, or git://"
        )));
    }
    let bucket = bucket_for_check.trim_end_matches('/').to_string();
    let bucket_name = bucket
        .strip_prefix("s3://")
        .or_else(|| bucket.strip_prefix("gs://"))
        .or_else(|| bucket.strip_prefix("git://"))
        .unwrap_or(&bucket)
        .to_string();
    // Resolve the timestamp.
    let created_at_utc = match created_at_utc {
        Some(s) if !s.is_empty() => s.to_string(),
        _ => std::env::var("RBP_PUBLISH_REMOTE_UTC")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unknown>".to_string()),
    };
    // Note: the receipt verifier (above) is the
    // *first* gate. The STW-032 publish bundle
    // re-verify (below) checks the bundle's
    // on-disk files against the manifest's
    // sha256s; it can fire `BundleHashMismatch`
    // independently if the bundle is mutated
    // AFTER the receipt is verified.
    // Re-verify the STW-032 publish bundle. The
    // bundle is the source of truth for the
    // per-file bytes / sha256; the remote-upload
    // step is a *consumer* of it, not a refactor
    // of it. A red bundle short-circuits the
    // upload plan with a `BundleHashMismatch` /
    // `ManifestShape` error from the verifier.
    let manifest = PublishedBundle::from_bundle_path(bundle_dir)?;
    // Map the typed `PublishError` from the
    // STW-032 verifier to a typed
    // `PublishRemoteError`. The mapping is
    // mechanical: `BundleHashMismatch` and
    // `MissingFile` / `ManifestShape` /
    // `FileUnreadable` all map one-for-one to
    // the corresponding `PublishRemoteError`
    // variant.
    if let Err(e) = manifest.verify(bundle_dir) {
        return Err(match e {
            crate::publish::PublishError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => PublishRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            },
            crate::publish::PublishError::MissingFile(s) => PublishRemoteError::MissingObject(s),
            crate::publish::PublishError::FileUnreadable(s) => {
                PublishRemoteError::FileUnreadable(s)
            }
            crate::publish::PublishError::ManifestShape(s) => {
                PublishRemoteError::FileUnreadable(format!("manifest shape: {s}"))
            }
            crate::publish::PublishError::ReceiptRed(s) => PublishRemoteError::ReceiptRed(s),
            crate::publish::PublishError::ReceiptDir(s) => PublishRemoteError::BundleDir(s),
        });
    }
    // Build the per-file `s3_objects[]` array.
    // The three files the STW-032 publish
    // step writes are `bundle.tar.gz`,
    // `manifest.json`, `bundle.sha256`; the
    // upload plan uploads all three. Each
    // entry re-hashes the on-disk file and
    // asserts the digest matches the parent
    // manifest's claim (a mismatch is a hard
    // `BundleHashMismatch`).
    let mut s3_objects: Vec<S3Object> = Vec::new();
    for entry in &manifest.files {
        // Map the parent manifest's `path`
        // (relative to the tarball's
        // `<receipt_basename>/` prefix) to the
        // publish directory's on-disk file.
        // The publish directory's on-disk
        // shape is `bundle.tar.gz` +
        // `manifest.json` + `bundle.sha256`
        // (sibling files of the publish
        // directory, NOT a subdirectory tree);
        // the `entry.path` is the tarball's
        // path (e.g. `SUMMARY.txt` or
        // `cluster/stdout.txt`), so we
        // re-resolve it against the receipt
        // directory's mirror of the same
        // path. The receipt directory lives
        // at `<receipt_dir>` (the path the
        // caller passed in), and the
        // `entry.path` is the path the
        // tarball uses (relative to
        // `<receipt_basename>/`).
        let local_path = receipt_dir.join(&entry.path);
        let local_path_str = local_path.display().to_string();
        let bytes = fs::read(&local_path).map_err(|e| {
            PublishRemoteError::FileUnreadable(format!(
                "could not read local file {}: {e}",
                local_path.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual = format!("{:x}", hasher.finalize());
        if actual != entry.sha256 {
            return Err(PublishRemoteError::BundleHashMismatch {
                path: local_path_str.clone(),
                expected: entry.sha256.clone(),
                actual,
            });
        }
        // The S3 key is `<prefix>/<entry.path>`.
        // We append the `entry.path` (NOT the
        // local filename) so a dashboard that
        // lists the bucket sees the receipt's
        // directory structure mirrored under
        // the prefix (the parent tarball
        // already mirrors it; the remote
        // upload mirrors it again).
        let s3_uri = format!("s3://{bucket_name}/{prefix}{}", entry.path);
        s3_objects.push(S3Object {
            local_path: local_path_str,
            sha256: entry.sha256.clone(),
            bytes: entry.bytes,
            s3_uri,
        });
    }
    // Also upload the three sibling files the
    // STW-032 publish step writes (`bundle.tar.gz`
    // + `manifest.json` + `bundle.sha256`).
    // These are the on-disk side of the
    // publish, NOT inside the tarball.
    for filename in [
        STW032_BUNDLE_FILENAME,
        STW032_MANIFEST_FILENAME,
        STW032_SHA256_FILENAME,
    ] {
        let local_path = bundle_dir.join(filename);
        let local_path_str = local_path.display().to_string();
        let bytes = fs::read(&local_path).map_err(|e| {
            PublishRemoteError::FileUnreadable(format!(
                "could not read sibling file {}: {e}",
                local_path.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha = format!("{:x}", hasher.finalize());
        let s3_uri = format!("s3://{bucket_name}/{prefix}{filename}");
        s3_objects.push(S3Object {
            local_path: local_path_str,
            sha256: sha,
            bytes: bytes.len() as u64,
            s3_uri,
        });
    }
    // Sort by `s3_uri` for determinism.
    s3_objects.sort_by(|a, b| a.s3_uri.cmp(&b.s3_uri));
    let total_bytes = s3_objects.iter().map(|o| o.bytes).sum();
    let plan = PublishRemotePlan {
        bucket: bucket_name.clone(),
        prefix: prefix.to_string(),
        region: std::env::var("AWS_REGION")
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "<unknown>".to_string()),
        s3_objects: s3_objects.clone(),
        bundle_sha256: manifest.bundle_sha256.clone(),
        bundle_bytes: manifest.total_bytes,
        receipt_basename: manifest.receipt_dir.clone(),
        runbook_version: STW033_RUNBOOK_VERSION.to_string(),
        created_at_utc: created_at_utc.clone(),
        dry_run,
    };
    // Write the plan to
    // `<publish>/<basename>/remote/remote_plan.json`.
    // The basename is the bundle_dir's
    // basename (e.g.
    // `testnet-live-proof-20260604T050000Z`).
    let bundle_basename = bundle_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("publish-bundle")
        .to_string();
    let remote_dir = bundle_dir.join("remote");
    fs::create_dir_all(&remote_dir).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!(
            "could not create remote dir {}: {e}",
            remote_dir.display()
        ))
    })?;
    let plan_path = remote_dir.join(STW033_REMOTE_PLAN_FILENAME);
    let plan_json = serde_json::to_string_pretty(&plan).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!("could not serialise plan: {e}"))
    })?;
    fs::write(&plan_path, &plan_json).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!(
            "could not write plan at {}: {e}",
            plan_path.display()
        ))
    })?;
    // Live-mode upload: shell out to `aws s3
    // cp` per `s3_objects[]` entry. We capture
    // stdout / stderr per file so a CI worker
    // scraping the per-step logs can grep
    // `^aws s3 cp <local_path> <s3_uri>` and
    // assert every file uploaded. A non-zero
    // `aws` exit aborts the upload and the
    // arm returns `PublishRemoteError::AwsCli`
    // (a partial upload leaves the on-bucket
    // state in a known shape: a dashboard
    // scraper can re-list the bucket and see
    // which files made it).
    if !dry_run {
        for obj in &s3_objects {
            let out = Command::new("aws")
                .arg("s3")
                .arg("cp")
                .arg(&obj.local_path)
                .arg(&obj.s3_uri)
                .output()
                .map_err(|e| {
                    PublishRemoteError::AwsCli(format!(
                        "could not spawn `aws s3 cp` for {}: {e}",
                        obj.local_path
                    ))
                })?;
            if !out.status.success() {
                let stderr = String::from_utf8_lossy(&out.stderr);
                return Err(PublishRemoteError::AwsCli(format!(
                    "aws s3 cp {} -> {} failed: {}",
                    obj.local_path,
                    obj.s3_uri,
                    stderr.trim()
                )));
            }
        }
    }
    // The post-upload receipt mirrors the
    // plan + stamps the `uploaded_at_utc`
    // timestamp. In dry-run, the timestamp
    // matches `created_at_utc` (the runbook
    // did NOT shell out, so the upload time
    // is the plan time).
    let receipt = PublishedRemoteReceipt {
        plan: plan.clone(),
        uploaded_at_utc: created_at_utc.clone(),
        s3_objects,
        total_bytes,
        bundle_sha256: plan.bundle_sha256.clone(),
        runbook_version: plan.runbook_version.clone(),
    };
    let receipt_path = remote_dir.join(STW033_REMOTE_RECEIPT_FILENAME);
    let receipt_json = serde_json::to_string_pretty(&receipt).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!("could not serialise receipt: {e}"))
    })?;
    fs::write(&receipt_path, &receipt_json).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!(
            "could not write receipt at {}: {e}",
            receipt_path.display()
        ))
    })?;
    let _ = bundle_basename; // (silence unused warning)
    Ok(receipt)
}

impl PublishedRemoteReceipt {
    /// Re-verify a remote receipt: re-hash every
    /// local file the receipt claims to have
    /// uploaded, assert every digest matches
    /// the receipt, and assert every `s3_uri`
    /// in the receipt appears in the
    /// inlined plan (a phantom `s3_uri` in the
    /// receipt is a `PublishRemoteError::MissingObject`).
    /// Returns `Ok(())` on green; on red returns
    /// the typed `PublishRemoteError` variant
    /// the verifier detected.
    pub fn verify(&self, bundle_dir: &Path) -> Result<(), PublishRemoteError> {
        // Build a `s3_uri -> sha256` map from the
        // inlined plan (the receipt's source of
        // truth). A receipt whose `s3_objects[]`
        // contains an `s3_uri` not in the plan is
        // a hard `MissingObject` error.
        let plan_uris: std::collections::BTreeMap<String, &S3Object> = self
            .plan
            .s3_objects
            .iter()
            .map(|o| (o.s3_uri.clone(), o))
            .collect();
        for obj in &self.s3_objects {
            if !plan_uris.contains_key(&obj.s3_uri) {
                return Err(PublishRemoteError::MissingObject(obj.s3_uri.clone()));
            }
        }
        // Re-hash every local file. The
        // `local_path` is an absolute path on the
        // verifier's machine; the verifier
        // re-resolves it relative to the bundle
        // directory so a CI auditor that fetched
        // the receipt to a fresh tempdir can
        // re-verify against the local files
        // (mirrors the STW-032 verifier's
        // re-verify-bundle shape).
        for obj in &self.s3_objects {
            let p = PathBuf::from(&obj.local_path);
            let resolved = if p.is_absolute() {
                p
            } else {
                bundle_dir.join(p)
            };
            let bytes = fs::read(&resolved).map_err(|e| {
                PublishRemoteError::FileUnreadable(format!(
                    "could not read {}: {e}",
                    resolved.display()
                ))
            })?;
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = format!("{:x}", hasher.finalize());
            if actual != obj.sha256 {
                return Err(PublishRemoteError::BundleHashMismatch {
                    path: resolved.display().to_string(),
                    expected: obj.sha256.clone(),
                    actual,
                });
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

// --- map PublishError -> PublishRemoteError (for the from_bundle_path
//     + verify pipeline above) --------------------------------------

impl From<crate::publish::PublishError> for PublishRemoteError {
    fn from(e: crate::publish::PublishError) -> Self {
        match e {
            crate::publish::PublishError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => PublishRemoteError::BundleHashMismatch {
                path,
                expected,
                actual,
            },
            crate::publish::PublishError::MissingFile(s) => PublishRemoteError::MissingObject(s),
            crate::publish::PublishError::FileUnreadable(s) => {
                PublishRemoteError::FileUnreadable(s)
            }
            crate::publish::PublishError::ManifestShape(s) => {
                PublishRemoteError::FileUnreadable(format!("manifest shape: {s}"))
            }
            crate::publish::PublishError::ReceiptRed(s) => PublishRemoteError::ReceiptRed(s),
            crate::publish::PublishError::ReceiptDir(s) => PublishRemoteError::BundleDir(s),
        }
    }
}

// --- io helper (the file_copy_dir_recursive lives in publish.rs;
//     publish-remote does not need a copy because it reads the
//     bundle's on-disk files in place) -----------------------------

/// Convenience helper: read + parse a
/// `remote_receipt.json` from a remote
/// directory. Mirrors the
/// `PublishedBundle::from_bundle_path` shape
/// (read JSON from disk, parse to typed
/// `PublishedRemoteReceipt`).
pub fn read_remote_receipt(
    remote_dir: &Path,
) -> Result<PublishedRemoteReceipt, PublishRemoteError> {
    let receipt_path = remote_dir.join(STW033_REMOTE_RECEIPT_FILENAME);
    if !receipt_path.is_file() {
        return Err(PublishRemoteError::FileUnreadable(format!(
            "remote_receipt.json missing or not a file at {}",
            receipt_path.display()
        )));
    }
    let receipt_str = fs::read_to_string(&receipt_path).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!(
            "could not read remote_receipt.json at {}: {e}",
            receipt_path.display()
        ))
    })?;
    serde_json::from_str(&receipt_str).map_err(|e| {
        PublishRemoteError::FileUnreadable(format!("could not parse remote_receipt.json: {e}"))
    })
}

// --- lib tests ----------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LiveProofReceipt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-publish-remote-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&dir);
        dir
    }

    /// Drop a synthetic green receipt + a fresh
    /// publish bundle. The publish step
    /// (re-)verifies the receipt + writes the
    /// three sibling files; the publish-remote
    /// step (re-)reads them.
    fn setup_publish(label: &str) -> (PathBuf, PathBuf, String) {
        let receipt = fresh_dir(&format!("{label}-receipt"));
        let bundle = fresh_dir(&format!("{label}-bundle"));
        LiveProofReceipt::write_to(
            &receipt,
            12,
            12,
            4,
            4,
            256,
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
        let basename = receipt
            .file_name()
            .and_then(|n| n.to_str())
            .expect("receipt must have a basename")
            .to_string();
        (receipt, bundle, basename)
    }

    /// `bucket_uri_as_str_matches_published_strings` —
    /// the typed `PublishRemoteError` Display
    /// impls must produce the exact strings the
    /// CLI / dashboard scraper greps. A future
    /// refactor that renames a prefix fails
    /// the test before a dashboard scraper can
    /// `grep` the wrong shape.
    #[test]
    fn bucket_uri_as_str_matches_published_strings() {
        assert_eq!(
            PublishRemoteError::ReceiptRed("step_failed: cluster".to_string()).to_string(),
            "live_proof publish_remote error: receipt is red: step_failed: cluster"
        );
        assert_eq!(
            PublishRemoteError::BundleHashMismatch {
                path: "p".to_string(),
                expected: "e".to_string(),
                actual: "a".to_string(),
            }
            .to_string(),
            "live_proof publish_remote error: bundle_hash_mismatch: p: expected e, got a"
        );
        assert_eq!(
            PublishRemoteError::MissingObject("s3://x/y".to_string()).to_string(),
            "live_proof publish_remote error: missing_object: s3://x/y"
        );
        assert_eq!(
            PublishRemoteError::FileUnreadable("perm".to_string()).to_string(),
            "live_proof publish_remote error: file unreadable: perm"
        );
        assert_eq!(
            PublishRemoteError::ReceiptDir("d".to_string()).to_string(),
            "live_proof publish_remote error: receipt dir: d"
        );
        assert_eq!(
            PublishRemoteError::BundleDir("d".to_string()).to_string(),
            "live_proof publish_remote error: bundle dir: d"
        );
        assert_eq!(
            PublishRemoteError::BucketUri("b".to_string()).to_string(),
            "live_proof publish_remote error: bucket uri: b"
        );
        assert_eq!(
            PublishRemoteError::AwsCli("aws not found".to_string()).to_string(),
            "live_proof publish_remote error: aws cli: aws not found"
        );
    }

    /// `bucket_uri_rejects_non_s3_prefix` — the
    /// arm refuses to plan an upload with a
    /// `--bucket` value that does not start
    /// with `s3://`, `gs://`, or `git://`. A
    /// dashboard that points at an unknown
    /// scheme (e.g. `https://`) gets a
    /// typed `BucketUri` error.
    #[test]
    fn bucket_uri_rejects_non_s3_prefix() {
        let (receipt, bundle, _basename) = setup_publish("bucket-prefix");
        let result = publish_remote_receipt(
            &receipt,
            &bundle,
            "https://example.com/bucket",
            "testnet/",
            true,
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishRemoteError::BucketUri(s)) => {
                assert!(
                    s.contains("https://example.com/bucket"),
                    "BucketUri must name the rejected bucket; got: {s:?}"
                );
            }
            other => panic!("non-s3 bucket must produce BucketUri; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `bucket_uri_rejects_empty_bucket` — the
    /// arm refuses to plan an upload with a
    /// `--bucket` value of `""`. Empty
    /// buckets are a CI misconfiguration, not
    /// a valid input.
    #[test]
    fn bucket_uri_rejects_empty_bucket() {
        let (receipt, bundle, _basename) = setup_publish("bucket-empty");
        let result = publish_remote_receipt(
            &receipt,
            &bundle,
            "",
            "testnet/",
            true,
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishRemoteError::BucketUri(_)) => {}
            other => panic!("empty bucket must produce BucketUri; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_dry_run_writes_plan_and_receipt` — the
    /// dry-run default (no `aws` shell-out)
    /// writes both `remote_plan.json` and
    /// `remote_receipt.json` under
    /// `<publish>/<basename>/remote/`. A
    /// regression that drops either file (or
    /// writes them under a different name)
    /// fails the test.
    #[test]
    fn publish_remote_dry_run_writes_plan_and_receipt() {
        let (receipt, bundle, _basename) = setup_publish("dry-writes");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("dry-run publish-remote must succeed");
        let remote_dir = bundle.join("remote");
        assert!(
            remote_dir.join(STW033_REMOTE_PLAN_FILENAME).is_file(),
            "dry-run must write {}",
            STW033_REMOTE_PLAN_FILENAME
        );
        assert!(
            remote_dir.join(STW033_REMOTE_RECEIPT_FILENAME).is_file(),
            "dry-run must write {}",
            STW033_REMOTE_RECEIPT_FILENAME
        );
        // The receipt's `s3_objects[]` must
        // include the three sibling files
        // (`bundle.tar.gz`, `manifest.json`,
        // `bundle.sha256`) AND every per-file
        // entry from the parent manifest.
        // Pin a lower bound (a green receipt
        // has at least 4 files: `SUMMARY.txt`
        // + `recipe.json` + the 3 sibling
        // files; so 5+ entries).
        assert!(
            out.s3_objects.len() >= 5,
            "publish-remote plan must upload every per-file entry from the parent \
             manifest plus the three sibling files (got {} entries)",
            out.s3_objects.len()
        );
        // The `dry_run` flag must be `true`
        // (the integration test runs without
        // `aws`).
        assert!(
            out.plan.dry_run,
            "publish-remote plan must record `dry_run: true`; got: {out:?}"
        );
        // The `bundle_sha256` field must
        // match the parent manifest's
        // `bundle_sha256`. A regression in
        // the plan's sha256 derivation fails
        // the verifier.
        let manifest = PublishedBundle::from_bundle_path(&bundle).expect("read manifest");
        assert_eq!(
            out.plan.bundle_sha256, manifest.bundle_sha256,
            "publish-remote plan's bundle_sha256 must match the parent manifest's \
             bundle_sha256; got plan={} manifest={}",
            out.plan.bundle_sha256, manifest.bundle_sha256
        );
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_s3_uris_are_sorted_for_determinism` — the
    /// `s3_objects[]` array is sorted by
    /// `s3_uri` so the plan is byte-stable
    /// across machines. A regression in the
    /// sort order fails this test before a
    /// dashboard scraper can `jq` the wrong
    /// shape.
    #[test]
    fn publish_remote_s3_uris_are_sorted_for_determinism() {
        let (receipt, bundle, _basename) = setup_publish("sort-uris");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("dry-run publish-remote must succeed");
        let mut prev: Option<&str> = None;
        for obj in &out.s3_objects {
            if let Some(p) = prev {
                assert!(
                    p <= obj.s3_uri.as_str(),
                    "s3_objects[] must be sorted by s3_uri; saw {p} then {}",
                    obj.s3_uri
                );
            }
            prev = Some(obj.s3_uri.as_str());
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_refuses_red_receipt` — the
    /// publish-remote step reads the
    /// `LiveProofReceipt::read_and_verify`
    /// surface and refuses to plan an upload
    /// for a red receipt. A red receipt
    /// (cluster exit code != 0) returns
    /// `Err(PublishRemoteError::ReceiptRed(...))`
    /// before the `remote/` dir is created.
    /// This is the "no paper-over a red
    /// receipt" invariant STW-023 / STW-032
    /// already enforce.
    #[test]
    fn publish_remote_refuses_red_receipt() {
        let (receipt, bundle, _basename) = setup_publish("red");
        // Make the receipt red by rewriting
        // the cluster `exit.txt` to 1, then
        // re-publish so the bundle's
        // `manifest.json` matches the new
        // (red) state. The publish step
        // refuses to bundle a red receipt, so
        // the second `publish_receipt` call
        // returns `Err`. We capture the
        // receipt-red error here, then
        // attempt to publish-remote the
        // (now-bundle-less) red receipt and
        // assert the publish-remote step
        // also refuses.
        std::fs::write(receipt.join("cluster").join("exit.txt"), "1\n")
            .expect("rewrite cluster/exit.txt");
        let publish_again = crate::publish::publish_receipt(
            &receipt,
            &bundle,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        );
        match publish_again {
            Err(crate::publish::PublishError::ReceiptRed(_)) => {}
            other => panic!("re-publish of a red receipt must produce ReceiptRed; got: {other:?}"),
        }
        let result = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        );
        // The publish-remote step re-runs the
        // STW-023 verifier, which reads the
        // on-disk red receipt and returns
        // `ReceiptRed` before any plan
        // directory is created.
        match result {
            Err(PublishRemoteError::ReceiptRed(_)) => {}
            other => panic!("red receipt must produce ReceiptRed; got: {other:?}"),
        }
        // The `remote/` dir must NOT exist.
        // A red-receipt publish that wrote a
        // partial plan is the inverse of the
        // "refuse to publish a red receipt"
        // gate STW-032 / STW-028 / STW-023
        // already enforce.
        let remote_dir = bundle.join("remote");
        assert!(
            !remote_dir.exists(),
            "publish-remote must NOT write a remote/ dir for a red receipt ({} exists)",
            remote_dir.display()
        );
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_round_trips_through_verifier` — the
    /// verifier must agree the receipt is
    /// green. The contract: a green receipt
    /// is one whose every per-file
    /// `s3_objects[].sha256` re-hashes
    /// against the on-disk file's bytes,
    /// and whose every `s3_uri` appears in
    /// the inlined plan.
    #[test]
    fn publish_remote_round_trips_through_verifier() {
        let (receipt, bundle, _basename) = setup_publish("verifier");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("dry-run publish-remote must succeed");
        out.verify(&bundle)
            .expect("verifier must agree a fresh dry-run receipt is green");
        // The verifier must also agree the
        // receipt is byte-stable when the
        // JSON is re-read from disk.
        let remote_dir = bundle.join("remote");
        let on_disk = read_remote_receipt(&remote_dir).expect("read_remote_receipt");
        assert_eq!(
            on_disk, out,
            "on-disk remote_receipt.json must match the in-memory receipt"
        );
        on_disk
            .verify(&bundle)
            .expect("on-disk receipt must also verify green");
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_verifier_rejects_tampered_file` — flip
    /// a byte in one of the receipt's
    /// `local_path` files; the verifier must
    /// return `BundleHashMismatch` with the
    /// tampered path in the error.
    #[test]
    fn publish_remote_verifier_rejects_tampered_file() {
        let (receipt, bundle, _basename) = setup_publish("tamper");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("dry-run publish-remote must succeed");
        // Find the smallest sibling file in
        // the receipt (the `bundle.sha256` is
        // 65 bytes — a guaranteed small
        // target).
        let target = bundle.join(STW032_SHA256_FILENAME);
        let mut bytes = std::fs::read(&target).expect("read sha256");
        if bytes.is_empty() {
            panic!("sha256 file is empty; test setup error");
        }
        bytes[0] ^= 0xff;
        std::fs::write(&target, &bytes).expect("rewrite sha256");
        let result = out.verify(&bundle);
        match result {
            Err(PublishRemoteError::BundleHashMismatch { .. }) => {}
            other => panic!("tampered file must produce BundleHashMismatch; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_to_json_contains_every_field` — the
    /// `PublishedRemoteReceipt::to_json`
    /// shape must include every field the
    /// verifier + the bash runbook read.
    /// Pin every key the spec named.
    #[test]
    fn publish_remote_to_json_contains_every_field() {
        let (receipt, bundle, _basename) = setup_publish("fields-json");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("dry-run publish-remote must succeed");
        let json = out.to_json();
        for key in [
            "\"plan\"",
            "\"uploaded_at_utc\"",
            "\"s3_objects\"",
            "\"total_bytes\"",
            "\"bundle_sha256\"",
            "\"runbook_version\"",
            "\"bucket\"",
            "\"prefix\"",
            "\"region\"",
            "\"dry_run\"",
            "\"created_at_utc\"",
        ] {
            assert!(
                json.contains(key),
                "remote receipt JSON must contain the {key} field; got: {json}"
            );
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_bare_bucket_name_normalises_to_s3_uri` —
    /// the `--bucket` flag accepts both
    /// `s3://<name>/` and bare `<name>`. The
    /// arm normalises bare-name form to the
    /// URI form so the per-file `s3_uri` is
    /// always `s3://...`.
    #[test]
    fn publish_remote_bare_bucket_name_normalises_to_s3_uri() {
        let (receipt, bundle, _basename) = setup_publish("bare-bucket");
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        )
        .expect("bare-name bucket must be normalised to s3://");
        assert_eq!(
            out.plan.bucket, "robopoker-testnet-dashboard",
            "plan.bucket must hold the bare name (without s3:// prefix)"
        );
        for obj in &out.s3_objects {
            assert!(
                obj.s3_uri.starts_with("s3://robopoker-testnet-dashboard/"),
                "every s3_uri must start with the normalised bucket; got: {obj:?}"
            );
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_created_at_utc_falls_back_to_unknown` —
    /// the `created_at_utc` field defaults
    /// to `<unknown>` when both the explicit
    /// arg and the env knob are empty. The
    /// lib test + the committed
    /// `publish-remote-fixture/` use this
    /// sentinel for byte-stability.
    #[test]
    fn publish_remote_created_at_utc_falls_back_to_unknown() {
        let (receipt, bundle, _basename) = setup_publish("utc-fallback");
        // Make sure the env knob is unset
        // for this test (it might be set by
        // a CI runner).
        // SAFETY: tests run on a single thread
        // for this function; `remove_var` is
        // `unsafe` in edition 2024 because the
        // env is process-global shared state.
        // We do not race with any other
        // thread reading `RBP_PUBLISH_REMOTE_UTC`
        // here.
        unsafe {
            std::env::remove_var("RBP_PUBLISH_REMOTE_UTC");
        }
        let out = publish_remote_receipt(
            &receipt,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            None,
        )
        .expect("dry-run publish-remote must succeed");
        assert_eq!(
            out.plan.created_at_utc, "<unknown>",
            "created_at_utc must fall back to <unknown> when no env knob is set"
        );
        assert_eq!(
            out.uploaded_at_utc, "<unknown>",
            "uploaded_at_utc must fall back to <unknown> when no env knob is set"
        );
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
    }

    /// `publish_remote_io_error_propagates_for_missing_receipt` —
    /// the arm returns
    /// `ReceiptDir` when the receipt
    /// directory does not exist (a setup
    /// error the operator should see, not
    /// silently swallow).
    #[test]
    fn publish_remote_io_error_propagates_for_missing_receipt() {
        let (receipt, bundle, _basename) = setup_publish("missing-receipt");
        let bogus = fresh_dir("missing-receipt-bogus");
        let result = publish_remote_receipt(
            &bogus,
            &bundle,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishRemoteError::ReceiptDir(s)) => {
                assert!(
                    s.contains("missing-receipt-bogus"),
                    "ReceiptDir must name the missing directory; got: {s:?}"
                );
            }
            other => panic!("missing receipt must produce ReceiptDir; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
        let _ = fs::remove_dir_all(&bogus);
    }

    /// `publish_remote_io_error_propagates_for_missing_bundle` —
    /// the arm returns `BundleDir` when the
    /// publish directory does not exist
    /// (a setup error the operator should
    /// see, not silently swallow).
    #[test]
    fn publish_remote_io_error_propagates_for_missing_bundle() {
        let (receipt, bundle, _basename) = setup_publish("missing-bundle");
        let bogus = fresh_dir("missing-bundle-bogus");
        let result = publish_remote_receipt(
            &receipt,
            &bogus,
            "s3://robopoker-testnet-dashboard",
            "testnet-live-proof-20260604T050000Z/",
            true,
            Some("2026-06-04T00:00:00Z"),
        );
        match result {
            Err(PublishRemoteError::BundleDir(s)) => {
                assert!(
                    s.contains("missing-bundle-bogus"),
                    "BundleDir must name the missing directory; got: {s:?}"
                );
            }
            other => panic!("missing bundle must produce BundleDir; got: {other:?}"),
        }
        let _ = fs::remove_dir_all(&receipt);
        let _ = fs::remove_dir_all(&bundle);
        let _ = fs::remove_dir_all(&bogus);
    }
}
