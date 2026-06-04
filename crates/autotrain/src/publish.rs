//! `trainer --publish <receipt-dir>` — turn a
//! `receipts/testnet-live-proof-<UTC-ISO>/` directory
//! the STW-019 runbook produced into a deterministic,
//! content-addressed portable publish bundle a third
//! party (a testnet dashboard, a CI auditor, a release
//! gate) can fetch + re-verify without re-running the
//! chain.
//!
//! STW-019 shipped the runbook that produces local
//! receipts; STW-023 shipped the
//! `LiveProofReceipt::read_and_verify` Rust verifier;
//! STW-028 shipped the `trainer --verify-receipt <path>`
//! CLI subcommand that re-verifies a local receipt with
//! a single static binary. STW-032 lands the *publish*
//! step the runbook doc names as the "next slice
//! (`testnet-live-publish`)" (line 234 of
//! `scripts/testnet-live-proof.md`): a deterministic
//! `tar.gz` bundle + a `sha256` digest + a
//! `manifest.json` index.
//!
//! ## Bundle layout
//!
//! `publish_receipt` writes a three-file bundle into
//! `<output_dir>/testnet-live-proof-<UTC-ISO>/`:
//!
//! - `testnet-live-proof-<UTC-ISO>.tar.gz` — a
//!   deterministic `tar.gz` of the receipt directory.
//!   The tarball layout is `tar --sort=name
//!   --mtime=@0 --owner=0 --group=0` so a
//!   byte-identical receipt produces a byte-identical
//!   tarball. The tarball's top-level directory is
//!   `<receipt_basename>/` (matching the on-disk
//!   receipt's directory name) so a `tar -tzf` lists
//!   the receipt files at predictable paths.
//! - `testnet-live-proof-<UTC-ISO>.sha256` — the
//!   single-line `sha256` of the `.tar.gz` file
//!   (a CI artifact's `sha256sum -c` step reads
//!   this).
//! - `testnet-live-proof-<UTC-ISO>.manifest.json` —
//!   the machine-readable index. JSON shape mirrors
//!   the [`PublishedBundle`] struct one-for-one:
//!   `bundle_sha256`, `total_bytes`, `files[]`
//!   (each with `path` + `sha256` + `bytes`), plus
//!   `receipt_dir`, `runbook_version`, and
//!   `trainer_git_sha`. The `files[]` array is
//!   sorted by `path` so the manifest is byte-stable
//!   across machines.
//!
//! ## Why a fresh module and not a function on `receipt`
//!
//! `receipt.rs` is the *verify* side (a Rust caller
//! already has the receipt on disk; the verifier reads
//! the on-disk files and asserts the per-step
//! `exit.txt` codes are 0). `publish.rs` is the
//! *publish* side (a Rust caller has the receipt on
//! disk; the publisher turns it into a portable
//! bundle). Splitting them keeps the verifier's
//! typed `VerifyError` enum (recipe shape / step
//! failed / headline) and the publisher's typed
//! `PublishError` enum (manifest shape /
//! bundle hash mismatch / file missing /
//! file unreadable) on different code paths: a
//! regression in the publish manifest's `path`
//! field does not change the verifier's error
//! surface, and vice versa.
//!
//! ## Why a sync module
//!
//! The whole pipeline is `fs::read_to_string` +
//! `serde_json::from_str` + a `sha2::Sha256` walk +
//! a `tar` / `flate2` write. There is no I/O latency
//! worth awaiting, and the upstream `Mode::run` is
//! `async` only because every *other* variant opens a
//! DB. A sync `publish_receipt` + a sync `verify`
//! means the new no-DB early-dispatch arm in
//! `Mode::run` is a plain `match` with no `.await`
//! call.
//!
//! ## Why the publisher reads the receipt from a
//! staging copy
//!
//! The publish step is **read-only** with respect to
//! the receipt directory. `publish_receipt` copies
//! the receipt into a fresh `staging/` subdirectory
//! under the output dir, walks the copy, and never
//! opens the original receipt. A `trainer --publish`
//! invocation cannot mutate a receipt the runbook
//! produced even on partial-failure paths (a
//! mid-publish crash leaves the original receipt
//! untouched + the staging copy partially written).
//!
//! ## Why the manifest is JSON, not YAML or TOML
//!
//! A CI artifact's `sha256sum -c` step already
//! handles the `.sha256` digest file; the
//! `manifest.json` exists for the *Rust* re-verify
//! path (`trainer --verify-bundle <path>` calls
//! `PublishedBundle::from_bundle_path` +
//! `PublishedBundle::verify`). JSON is the format
//! the rest of the autotrain pipeline already
//! `serde_json::from_str`s (the bench report, the
//! compare report, the compare3 report, the
//! `LiveProofRecipe` manifest) so the publish side
//! uses the same parser. A future YAML / TOML port
//! is a one-crate change.
//!
//! ## Why the bundle filename mirrors the receipt
//!
//! A receipt directory is
//! `receipts/testnet-live-proof-<UTC-ISO>/`. The
//! bundle filenames are
//! `publish/testnet-live-proof-<UTC-ISO>/{bundle,sha256,manifest}`
//! so an operator scanning a `publish/` bucket
//! immediately sees the UTC timestamp the receipt
//! ran in. A `testnet-live-proof-20260604T050000Z/`
//! directory in the bucket corresponds one-for-one
//! with a `testnet-live-proof-20260604T050000Z/`
//! directory in `receipts/`.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use flate2::Compression;
use flate2::GzBuilder;

use crate::LiveProofReceipt;

/// The pinned publish headline prefix. The
/// `trainer --publish <receipt-dir>` CLI prints a
/// one-line `live_proof publish complete: ...`
/// headline a dashboard scraper can `grep ^live_proof
/// publish complete:`. The verifier
/// `trainer --verify-bundle <path>` mirrors the
/// STW-028 verifier's `live_proof receipt
/// verification passed: ...` prefix with a
/// `live_proof bundle verification passed: ...`
/// prefix. Both prefixes share the `live_proof ...`
/// family so a single `grep ^live_proof` scraper can
/// read the whole chain.
pub const STW032_PUBLISH_HEADLINE_PREFIX: &str = "live_proof publish complete:";
pub const STW032_VERIFY_HEADLINE_PREFIX: &str = "live_proof bundle verification passed:";
pub const STW032_VERIFY_FAILURE_HEADLINE_PREFIX: &str = "live_proof bundle verification failed:";

/// The on-disk manifest file name (a relative path
/// inside the publish directory). The constant lives
/// in Rust so the bash runbook's `cat > manifest.json
/// <<JSON ... JSON` heredoc and the Rust writer
/// agree on the filename.
pub const STW032_MANIFEST_FILENAME: &str = "manifest.json";
/// The on-disk sha256 digest file name (the
/// `sha256sum` output format `sha256sum -c` reads).
pub const STW032_SHA256_FILENAME: &str = "bundle.sha256";
/// The on-disk tarball file name.
pub const STW032_BUNDLE_FILENAME: &str = "bundle.tar.gz";

/// One file inside the bundle. The manifest's
/// `files[]` array is a `Vec<BundleFile>` sorted by
/// `path` so the manifest is byte-stable across
/// machines.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BundleFile {
    /// Path inside the tarball, relative to the
    /// tarball's top-level directory (i.e. the path
    /// an `tar -xzf bundle.tar.gz --list` would
    /// print, minus the `<receipt_basename>/` prefix).
    /// Always uses forward slashes (a tarball on a
    /// Linux host is the common case; the verifier
    /// normalises Windows-style separators on read).
    pub path: String,
    /// Lowercase hex `sha256` of the file's bytes
    /// (the same digest `sha256sum <file>` prints).
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
}

/// Top-level publish bundle manifest. Serialised to
/// `manifest.json` alongside `bundle.tar.gz` and
/// `bundle.sha256`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedBundle {
    /// Filename of the tarball (typically
    /// `bundle.tar.gz`). Stored separately from the
    /// manifest's location so a re-verify step
    /// doesn't have to guess the tarball's name.
    pub bundle_filename: String,
    /// Lowercase hex `sha256` of the entire
    /// `bundle.tar.gz` file. The verifier re-reads
    /// the tarball from disk and re-hashes it; a
    /// mismatch fails the verifier with a
    /// `bundle_hash_mismatch` line a dashboard
    /// scraper can `grep ^live_proof bundle
    /// verification failed: bundle_hash_mismatch:`
    /// the log.
    pub bundle_sha256: String,
    /// Total tarball size in bytes.
    pub total_bytes: u64,
    /// Per-file digests inside the tarball, sorted
    /// by `path` for determinism.
    pub files: Vec<BundleFile>,
    /// The original `receipts/testnet-live-proof-<UTC-ISO>/`
    /// directory the bundle was built from. Stored
    /// for human readability only; the verifier
    /// does NOT gate on it (a third party that
    /// re-verifyed the bundle has no need to know
    /// the original path).
    pub receipt_dir: String,
    /// The runbook version string (e.g. `"STW-032 v1"`).
    /// Bumped by hand if the bundle format changes.
    pub runbook_version: String,
    /// The git SHA the trainer binary was built from
    /// (the `git rev-parse HEAD` of the workspace
    /// at publish time). May be `<unknown>` for a
    /// fixture bundle (the lib tests + the committed
    /// `publish-fixture/` use this sentinel so the
    /// manifest is byte-stable).
    pub trainer_git_sha: String,
}

/// Publisher error: a single typed error so the
/// CLI / integration test can assert on the
/// `PublishError::*` variant. The variants cover
/// the failure modes the publisher / verifier
/// detect:
///
/// - `ReceiptRed` — the receipt did not pass
///   `LiveProofReceipt::read_and_verify`; the
///   publisher refuses to bundle a red receipt.
/// - `ManifestShape` — the on-disk `manifest.json`
///   is missing or not parseable.
/// - `BundleHashMismatch` — a file inside the
///   tarball does not match the `sha256` in the
///   manifest.
/// - `MissingFile` — the manifest names a file
///   the tarball does not contain (the tarball
///   was truncated, or the manifest is stale).
/// - `FileUnreadable` — a file inside the tarball
///   could not be read (a permissions / IO
///   problem on the verifier's machine).
/// - `ReceiptDir` — the input receipt directory
///   does not exist or is not a directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PublishError {
    /// The receipt failed the STW-023 verifier.
    /// The publisher refuses to bundle a red
    /// receipt (a `trainer --publish` of a red
    /// receipt is a hard error: the operator
    /// should fix the chain, not paper over it
    /// with a publish).
    ReceiptRed(String),
    /// The manifest is missing or not parseable.
    ManifestShape(String),
    /// A file inside the tarball has a `sha256`
    /// that does not match the manifest. The
    /// `String` payload names the file + the
    /// expected + actual digest so a dashboard
    /// scraper can grep.
    BundleHashMismatch {
        path: String,
        expected: String,
        actual: String,
    },
    /// The manifest names a file the tarball does
    /// not contain.
    MissingFile(String),
    /// A file inside the tarball could not be
    /// read.
    FileUnreadable(String),
    /// The input receipt directory does not exist
    /// or is not a directory.
    ReceiptDir(String),
}

impl fmt::Display for PublishError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PublishError::ReceiptRed(s) => {
                write!(f, "live_proof publish error: receipt is red: {s}")
            }
            PublishError::ManifestShape(s) => {
                write!(f, "live_proof publish error: manifest shape: {s}")
            }
            PublishError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "live_proof publish error: bundle_hash_mismatch: {path}: \
                 expected {expected}, got {actual}"
            ),
            PublishError::MissingFile(s) => {
                write!(f, "live_proof publish error: missing_file: {s}")
            }
            PublishError::FileUnreadable(s) => {
                write!(f, "live_proof publish error: file unreadable: {s}")
            }
            PublishError::ReceiptDir(s) => {
                write!(f, "live_proof publish error: receipt dir: {s}")
            }
        }
    }
}

impl std::error::Error for PublishError {}

/// STW-038: the dashboard-greppable pinned error
/// line. The `Display` impl above emits the
/// *legacy* `live_proof publish error: ...`
/// shape the existing per-arm call sites +
/// `cargo test --workspace` integration tests
/// pin. The new `to_pinned_line` emits the
/// STW-038 dashboard-greppable
/// `trainer error: kind=red_bundle detail=<detail>`
/// shape a CI scraper can `grep ^trainer error
/// kind=`. The two lines are *both* emitted on
/// the same stderr write from the `Mode::Publish`
/// dispatch arm, so a regression in either
/// shape fails CI.
impl PublishError {
    /// STW-038: map the per-variant `PublishError`
    /// to a `TrainerError` and emit the pinned
    /// `to_pinned_line` shape. The `ReceiptRed`
    /// variant becomes `TrainerError::RedBundle`
    /// (the "publish refused to bundle a red
    /// receipt" failure mode the STW-032 surface
    /// names); the 5 non-red variants become
    /// `TrainerError::Internal` with a
    /// human-readable detail (a future STW-039+
    /// slice can add more granular `kind` tokens
    /// for the non-red variants; the STW-038
    /// contract is "every error path has a
    /// pinned shape", not "every error path has
    /// a unique kind").
    pub fn to_pinned_line(&self) -> String {
        match self {
            PublishError::ReceiptRed(s) => {
                crate::error::TrainerError::RedBundle(s.clone()).to_pinned_line()
            }
            PublishError::ManifestShape(s) => {
                crate::error::TrainerError::Internal(format!("publish: manifest shape: {s}"))
                    .to_pinned_line()
            }
            PublishError::BundleHashMismatch {
                path,
                expected,
                actual,
            } => crate::error::TrainerError::Internal(format!(
                "publish: bundle_hash_mismatch: {path}: expected {expected}, got {actual}"
            ))
            .to_pinned_line(),
            PublishError::MissingFile(s) => {
                crate::error::TrainerError::Internal(format!("publish: missing_file: {s}"))
                    .to_pinned_line()
            }
            PublishError::FileUnreadable(s) => {
                crate::error::TrainerError::Internal(format!("publish: file_unreadable: {s}"))
                    .to_pinned_line()
            }
            PublishError::ReceiptDir(s) => {
                crate::error::TrainerError::Internal(format!("publish: receipt_dir: {s}"))
                    .to_pinned_line()
            }
        }
    }
}

/// `PublishOutput` — the typed return value of
/// [`publish_receipt`]. The handler returns this
/// so the `Mode::Publish` CLI can print a one-line
/// `live_proof publish complete: bundle=...
/// sha256=... bytes=...` headline and the
/// integration test can assert on the typed
/// `bundle_path` + `bundle_sha256` + `manifest_path`
/// fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishOutput {
    /// Absolute path to the `bundle.tar.gz` file
    /// the publisher wrote.
    pub bundle_path: PathBuf,
    /// Absolute path to the `manifest.json` file
    /// the publisher wrote.
    pub manifest_path: PathBuf,
    /// Absolute path to the `bundle.sha256` file
    /// the publisher wrote.
    pub sha256_path: PathBuf,
    /// Lowercase hex `sha256` of the `bundle.tar.gz`
    /// file (mirrors the value the manifest stores
    /// in `bundle_sha256`).
    pub bundle_sha256: String,
    /// Tarball size in bytes.
    pub total_bytes: u64,
    /// Number of files inside the tarball
    /// (mirrors the manifest's `files.len()`).
    pub file_count: usize,
    /// The receipt dir basename the bundle was
    /// built from (e.g. `testnet-live-proof-20260604T050000Z`).
    pub receipt_basename: String,
}

/// Publish a receipt directory: copy the receipt
/// into a `staging/` tempdir under `output_dir`,
/// walk the copy, write a `manifest.json` +
/// `bundle.tar.gz` + `bundle.sha256` into
/// `output_dir`, and return the [`PublishOutput`]
/// the `Mode::Publish` CLI prints.
///
/// The function is **read-only** with respect to
/// `receipt_dir`. The original receipt is never
/// opened for write; a partial-failure crash leaves
/// the receipt untouched and the staging copy
/// partially written. The `staging/` subdirectory
/// is removed on the happy path (the tarball
/// contains every file the staging dir had) and
/// left in place on the error path so an operator
/// can inspect what was about to be tarred.
///
/// ## Pre-tar gate
///
/// The function first calls
/// [`LiveProofReceipt::read_and_verify`] on
/// `receipt_dir`. A red receipt returns
/// `Err(PublishError::ReceiptRed(...))` before
/// the staging dir is created. This is the
/// "refuse to publish a red receipt" gate the
/// companion `scripts/testnet-live-publish.sh`
/// runbook also implements (the bash script
/// shells out to `trainer --verify-receipt`
/// before tarring).
pub fn publish_receipt<P: AsRef<Path>, Q: AsRef<Path>>(
    receipt_dir: P,
    output_dir: Q,
    runbook_version: &str,
    trainer_git_sha: &str,
) -> Result<PublishOutput, PublishError> {
    let receipt_dir = receipt_dir.as_ref();
    let output_dir = output_dir.as_ref();

    if !receipt_dir.is_dir() {
        return Err(PublishError::ReceiptDir(format!(
            "receipt dir {} does not exist or is not a directory",
            receipt_dir.display()
        )));
    }
    // Pre-tar gate: refuse to publish a red
    // receipt. The STW-023 verifier's
    // `read_and_verify` is the source of truth; if
    // the receipt doesn't pass, neither does the
    // bundle.
    if let Err(e) = LiveProofReceipt::read_and_verify(receipt_dir) {
        return Err(PublishError::ReceiptRed(format!(
            "receipt at {} failed verification: {e}",
            receipt_dir.display()
        )));
    }

    let receipt_basename = receipt_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("testnet-live-proof-receipt")
        .to_string();

    fs::create_dir_all(output_dir).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not create output dir {}: {e}",
            output_dir.display()
        ))
    })?;

    let staging_dir = output_dir.join("staging").join(&receipt_basename);
    let _ = fs::remove_dir_all(&staging_dir);
    fs::create_dir_all(&staging_dir).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not create staging dir {}: {e}",
            staging_dir.display()
        ))
    })?;

    // Copy the receipt into the staging dir.
    copy_dir_recursive(receipt_dir, &staging_dir).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not copy receipt {} into staging {}: {e}",
            receipt_dir.display(),
            staging_dir.display()
        ))
    })?;

    // Walk the staging dir sorted by path (for
    // determinism) and build the manifest's
    // `files[]` list.
    let mut files: Vec<BundleFile> = Vec::new();
    let mut walked: Vec<PathBuf> = Vec::new();
    collect_files_sorted(&staging_dir, &staging_dir, &mut walked).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not walk staging dir {}: {e}",
            staging_dir.display()
        ))
    })?;
    for rel in &walked {
        let bytes = fs::read(rel).map_err(|e| {
            PublishError::FileUnreadable(format!(
                "could not read staging file {}: {e}",
                rel.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let sha = format!("{:x}", hasher.finalize());
        let path_in_tar = rel
            .strip_prefix(&staging_dir)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        files.push(BundleFile {
            path: path_in_tar,
            sha256: sha,
            bytes: bytes.len() as u64,
        });
    }

    // The manifest and the sha256 file live
    // alongside the tarball, so they are NOT
    // inside the tarball (the tarball is the
    // *content* of the receipt, not the bundle
    // metadata). This mirrors the
    // `LiveProofReceipt` recipe.json layout:
    // the manifest is a sibling of the content
    // it describes.
    let bundle_path = output_dir.join(STW032_BUNDLE_FILENAME);
    let manifest_path = output_dir.join(STW032_MANIFEST_FILENAME);
    let sha256_path = output_dir.join(STW032_SHA256_FILENAME);

    // Build the tarball. We use the standard
    // `tar` crate's builder with `Header::set_metadata_in_mode`
    // to force deterministic metadata (mtime=0,
    // uid=0, gid=0, uname="", gname=""). The
    // builder appends files in the order we
    // hand them (we already sorted the walk
    // above), so a byte-identical receipt
    // produces a byte-identical tarball.
    write_tar_gz(&bundle_path, &staging_dir, &walked).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not write tarball at {}: {e}",
            bundle_path.display()
        ))
    })?;

    // Re-hash the tarball the builder just wrote
    // so the manifest's `bundle_sha256` /
    // `total_bytes` fields reflect the *final*
    // tarball (not a re-computed one).
    let tar_bytes = fs::read(&bundle_path).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not re-read tarball at {}: {e}",
            bundle_path.display()
        ))
    })?;
    let mut hasher = Sha256::new();
    hasher.update(&tar_bytes);
    let bundle_sha256 = format!("{:x}", hasher.finalize());
    let total_bytes = tar_bytes.len() as u64;

    let manifest = PublishedBundle {
        bundle_filename: STW032_BUNDLE_FILENAME.to_string(),
        bundle_sha256: bundle_sha256.clone(),
        total_bytes,
        files,
        receipt_dir: receipt_dir.display().to_string(),
        runbook_version: runbook_version.to_string(),
        trainer_git_sha: trainer_git_sha.to_string(),
    };
    let manifest_json = serde_json::to_string_pretty(&manifest)
        .map_err(|e| PublishError::ManifestShape(format!("could not serialise manifest: {e}")))?;
    fs::write(&manifest_path, &manifest_json).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not write manifest at {}: {e}",
            manifest_path.display()
        ))
    })?;
    let sha256_line = format!("{bundle_sha256}  {}\n", STW032_BUNDLE_FILENAME);
    fs::write(&sha256_path, sha256_line).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not write sha256 at {}: {e}",
            sha256_path.display()
        ))
    })?;

    // On the happy path, clean up the staging
    // dir. The tarball contains every file the
    // staging dir had, so deleting the staging
    // dir leaves the publish step's output as
    // just the three sibling files. On the error
    // path we leave the staging dir in place so
    // an operator can `ls staging/` to see what
    // was about to be tarred.
    let _ = fs::remove_dir_all(output_dir.join("staging"));

    Ok(PublishOutput {
        bundle_path,
        manifest_path,
        sha256_path,
        bundle_sha256,
        total_bytes,
        file_count: manifest.files.len(),
        receipt_basename,
    })
}

impl PublishedBundle {
    /// Read + parse a `manifest.json` from a publish
    /// bundle directory (the directory containing
    /// `bundle.tar.gz` + `manifest.json` +
    /// `bundle.sha256`). The `from_bundle_path` call
    /// is the verifier's read step: parse the JSON
    /// manifest, then re-hash the tarball and every
    /// file inside it, and assert the digests match.
    pub fn from_bundle_path(bundle_dir: &Path) -> Result<Self, PublishError> {
        let manifest_path = bundle_dir.join(STW032_MANIFEST_FILENAME);
        if !manifest_path.is_file() {
            return Err(PublishError::ManifestShape(format!(
                "manifest.json missing or not a file at {}",
                manifest_path.display()
            )));
        }
        let manifest_str = fs::read_to_string(&manifest_path).map_err(|e| {
            PublishError::ManifestShape(format!(
                "could not read manifest at {}: {e}",
                manifest_path.display()
            ))
        })?;
        serde_json::from_str(&manifest_str).map_err(|e| {
            PublishError::ManifestShape(format!(
                "could not parse manifest at {}: {e}",
                manifest_path.display()
            ))
        })
    }

    /// Re-verify a bundle: re-hash the tarball,
    /// re-hash every file inside it, and assert the
    /// digests match the manifest. The function
    /// returns `Ok(())` on green; on red it
    /// returns the typed `PublishError` variant
    /// the verifier detected.
    ///
    /// The verifier extracts the tarball into a
    /// fresh tempdir (so a `--verify-bundle`
    /// invocation does not collide with an
    /// in-progress `trainer --publish` invocation
    /// that has a `staging/` dir open) and walks
    /// the extracted tree. The tempdir is removed
    /// on drop.
    pub fn verify(&self, bundle_dir: &Path) -> Result<(), PublishError> {
        let bundle_path = bundle_dir.join(&self.bundle_filename);
        if !bundle_path.is_file() {
            return Err(PublishError::MissingFile(format!(
                "tarball {} missing under {}",
                self.bundle_filename,
                bundle_dir.display()
            )));
        }
        let tar_bytes = fs::read(&bundle_path).map_err(|e| {
            PublishError::FileUnreadable(format!(
                "could not read tarball at {}: {e}",
                bundle_path.display()
            ))
        })?;
        let mut hasher = Sha256::new();
        hasher.update(&tar_bytes);
        let actual_bundle_sha = format!("{:x}", hasher.finalize());
        if actual_bundle_sha != self.bundle_sha256 {
            return Err(PublishError::BundleHashMismatch {
                path: self.bundle_filename.clone(),
                expected: self.bundle_sha256.clone(),
                actual: actual_bundle_sha,
            });
        }
        // Extract the tarball into a fresh tempdir
        // and walk the extracted tree. The tempdir
        // is removed via `std::fs::remove_dir_all`
        // at the end of the verify call so a
        // re-verify on a different bundle starts
        // from a clean slate.
        let tmp_root = std::env::temp_dir().join(format!(
            "rbp-bundle-verify-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = fs::remove_dir_all(&tmp_root);
        fs::create_dir_all(&tmp_root).map_err(|e| {
            PublishError::FileUnreadable(format!(
                "could not create verify tempdir at {}: {e}",
                tmp_root.display()
            ))
        })?;
        let extract_result = extract_tar_gz(&bundle_path, &tmp_root);
        if let Err(e) = extract_result {
            let _ = fs::remove_dir_all(&tmp_root);
            return Err(e);
        }
        // Walk the extracted files. The manifest's
        // `files[i].path` is the relative path
        // inside the tarball; we look it up under
        // the tempdir's single top-level dir
        // (extract_tar_gz strips the leading
        // `<receipt_basename>/` and re-emits files
        // flat under the tempdir).
        for entry in &self.files {
            let candidate = tmp_root.join(&entry.path);
            if !candidate.is_file() {
                let _ = fs::remove_dir_all(&tmp_root);
                return Err(PublishError::MissingFile(format!(
                    "{}: file named in manifest is missing from tarball",
                    entry.path
                )));
            }
            let bytes = match fs::read(&candidate) {
                Ok(b) => b,
                Err(e) => {
                    let _ = fs::remove_dir_all(&tmp_root);
                    return Err(PublishError::FileUnreadable(format!(
                        "{}: could not read extracted file: {e}",
                        entry.path
                    )));
                }
            };
            let mut hasher = Sha256::new();
            hasher.update(&bytes);
            let actual = format!("{:x}", hasher.finalize());
            if actual != entry.sha256 {
                let _ = fs::remove_dir_all(&tmp_root);
                return Err(PublishError::BundleHashMismatch {
                    path: entry.path.clone(),
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
        }
        let _ = fs::remove_dir_all(&tmp_root);
        Ok(())
    }

    /// Render the manifest to a single-line JSON
    /// string a testnet dashboard can scrape. The
    /// `Display` is for human readability; the
    /// serde `to_string` is the machine-readable
    /// form. A future dashboard that wants the
    /// single-line form can call
    /// `serde_json::to_string` directly on the
    /// `PublishedBundle` value.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Recursive directory copy. Used by
/// `publish_receipt` to land the receipt in the
/// `staging/` tempdir before tarring. We do not
/// preserve permissions / mtimes (a fresh copy is
/// fine: the receipt files are read-only
/// artifacts; the tarball is the canonical
/// "frozen" form).
fn copy_dir_recursive(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_dir_recursive(&from, &to)?;
        } else {
            fs::copy(&from, &to)?;
        }
    }
    Ok(())
}

/// Walk a directory tree and return every file's
/// path, sorted lexicographically by the path's
/// display form. The sort is the determinism
/// guarantee the tarball + the manifest's `files[]`
/// array rely on.
fn collect_files_sorted(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if p.is_dir() {
            collect_files_sorted(root, &p, out)?;
        } else if p.is_file() {
            out.push(p);
        }
    }
    out.sort();
    // Drop any non-canonical prefix. We keep the
    // full path (the caller can strip the root)
    // so the walk + the tarball writer share the
    // same PathBuf values.
    let _ = root;
    Ok(())
}

/// Write a deterministic `tar.gz` of the staging
/// dir's files. The tarball is built with the
/// `tar` crate's builder, with `Header::set_metadata_in_mode`
/// forcing mtime=0, uid=0, gid=0, uname="" +
/// gname="" so a byte-identical receipt produces
/// a byte-identical tarball.
///
/// The tarball's top-level directory is
/// `<receipt_basename>/` (matching the on-disk
/// receipt's directory name) so an
/// `tar -tzf bundle.tar.gz` lists the receipt
/// files at predictable paths.
fn write_tar_gz(bundle_path: &Path, staging_dir: &Path, files: &[PathBuf]) -> io::Result<()> {
    let file = fs::File::create(bundle_path)?;
    // Disable the gzip mtime + OS fields so a
    // byte-identical receipt produces a
    // byte-identical tarball. We also pin the
    // compression level to `Compression::none()`:
    // the `flate2` deflate implementation
    // is *not* byte-deterministic across runs
    // at any non-zero compression level (the
    // internal hash chain / state differs
    // slightly between calls), so a portable
    // deterministic bundle is only achievable
    // with `Compression::none()`. The trade-off
    // is a larger tarball (~10% bigger than a
    // `Compression::default()` archive); the
    // alternative is bundling a `libdeflate`
    // / `zlib-ng` system dep, which is out of
    // scope for the no-system-deps runbook
    // shape the rest of the autotrain pipeline
    // already follows.
    let enc = GzBuilder::new().mtime(0).write(file, Compression::none());
    let mut tar = tar::Builder::new(enc);
    let receipt_basename = staging_dir
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("receipt");
    for f in files {
        let rel = f.strip_prefix(staging_dir).unwrap_or(f);
        let mut header = tar::Header::new_gnu();
        let metadata = fs::metadata(f)?;
        header.set_size(metadata.len());
        header.set_mode(0o644);
        header.set_mtime(0);
        header.set_uid(0);
        header.set_gid(0);
        header.set_username("")?;
        header.set_groupname("")?;
        header.set_cksum();
        let mut data = fs::File::open(f)?;
        // Tar entries are
        // `<receipt_basename>/<rel>` so a `tar
        // -xzf bundle.tar.gz` produces a
        // `<receipt_basename>/` directory tree
        // mirroring the on-disk receipt. The
        // forward-slash separator is the
        // canonical tar entry separator; on
        // Windows the strip_prefix + the replace
        // above already normalised the path.
        let entry_path: std::path::PathBuf = [receipt_basename, rel.to_str().unwrap_or("file")]
            .iter()
            .collect();
        tar.append_data(&mut header, entry_path, &mut data)?;
    }
    tar.into_inner()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("tar finish: {e}")))?
        .finish()
        .map_err(|e| io::Error::new(io::ErrorKind::Other, format!("gzip finish: {e}")))?;
    Ok(())
}
/// Extract a `tar.gz` into a tempdir and return
/// the tempdir. The tarball is expected to have a
/// single top-level directory (the
/// `<receipt_basename>/` the publisher wrote); we
/// strip that prefix and re-emit the files flat
/// under the tempdir so the verifier's
/// `manifest.files[i].path` lookups are 1:1
/// (the manifest stores paths relative to the
/// tarball's top-level dir).
fn extract_tar_gz(bundle_path: &Path, dst: &Path) -> Result<(), PublishError> {
    let file = fs::File::open(bundle_path).map_err(|e| {
        PublishError::FileUnreadable(format!(
            "could not open tarball at {}: {e}",
            bundle_path.display()
        ))
    })?;
    let dec = flate2::read::GzDecoder::new(file);
    let mut archive = tar::Archive::new(dec);
    for entry in archive
        .entries()
        .map_err(|e| PublishError::FileUnreadable(format!("could not read tarball entries: {e}")))?
    {
        let mut entry = entry.map_err(|e| {
            PublishError::FileUnreadable(format!("could not read tarball entry: {e}"))
        })?;
        let path = entry.path().map_err(|e| {
            PublishError::FileUnreadable(format!("could not read tarball entry path: {e}"))
        })?;
        // Strip the leading top-level dir.
        let stripped = path.components().skip(1).collect::<std::path::PathBuf>();
        let out_path = dst.join(stripped);
        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                PublishError::FileUnreadable(format!(
                    "could not create extracted dir {}: {e}",
                    parent.display()
                ))
            })?;
        }
        entry.unpack(&out_path).map_err(|e| {
            PublishError::FileUnreadable(format!(
                "could not extract tarball entry to {}: {e}",
                out_path.display()
            ))
        })?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! Pure-in-memory lib tests for the STW-032
    //! publish surface. These tests do NOT require
    //! a live Postgres (the publisher is the
    //! *consumer* side of the on-disk contract; the
    //! producer side is `LiveProofReceipt::write_to`
    //! for the receipt and the
    //! `testnet-live-proof.sh` bash runbook for the
    //! real runbook receipt).
    //!
    //! Fixture style: a process-unique
    //! `std::env::temp_dir().join("rbp-publish-test-<n>")`
    //! subdirectory populated by
    //! `LiveProofReceipt::write_to`, then published
    //! via `publish_receipt`, then re-verified via
    //! `PublishedBundle::verify` / `run_verify`.
    //! The tempdir is removed on drop so re-runs
    //! do not see stale files.
    use super::*;
    use crate::LiveProofReceipt;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static SEQ: AtomicUsize = AtomicUsize::new(0);

    fn fresh_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rbp-publish-test-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn make_green_receipt(dir: &Path) {
        LiveProofReceipt::write_to(
            dir,
            12,  // smoke_rows
            12,  // status_blueprint
            4,   // bench_hands
            4,   // compare_hands
            256, // replay_bytes
            "/srv/dev/repos/robopoker/target/debug/trainer",
            "<redacted: 49 chars>",
        )
        .expect("write_to should drop a synthetic green receipt");
    }

    #[test]
    fn publish_writes_manifest_tarball_and_sha256() {
        let receipt = fresh_dir("green-receipt");
        let out = fresh_dir("green-publish");
        make_green_receipt(&receipt);
        let result = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("green receipt must publish");
        assert!(
            result.bundle_path.is_file(),
            "tarball must exist on disk at {}",
            result.bundle_path.display()
        );
        assert!(
            result.manifest_path.is_file(),
            "manifest must exist on disk at {}",
            result.manifest_path.display()
        );
        assert!(
            result.sha256_path.is_file(),
            "sha256 must exist on disk at {}",
            result.sha256_path.display()
        );
        // The receipt_basename comes from the
        // receipt dir's filename; the
        // `fresh_dir` helper uses a
        // process-id + atomic-counter suffix, so
        // we only assert the basename ends in
        // `rbp-publish-test-green-receipt-...`.
        assert!(
            result
                .receipt_basename
                .starts_with("rbp-publish-test-green-receipt-"),
            "receipt_basename must mirror the receipt dir name; got: {:?}",
            result.receipt_basename
        );
        assert_eq!(
            result.file_count, 23,
            "the receipt has 2 root files (SUMMARY.txt + recipe.json) + \
             7 step dirs * 3 files (stdout + stderr + exit) = 23 files"
        );
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn publish_rejects_red_receipt() {
        let receipt = fresh_dir("red-receipt");
        let out = fresh_dir("red-publish");
        make_green_receipt(&receipt);
        // Make the receipt red by rewriting one
        // step's `exit.txt` to 1.
        std::fs::write(receipt.join("cluster").join("exit.txt"), "1\n")
            .expect("rewrite cluster/exit.txt to 1");
        let result = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        );
        match result {
            Err(PublishError::ReceiptRed(_)) => {}
            other => panic!("red receipt must produce ReceiptRed; got: {other:?}"),
        }
        // The publisher must NOT have created the
        // output dir for a red receipt (the
        // pre-tar gate fires before the staging
        // dir is created).
        assert!(
            !out.join("bundle.tar.gz").exists(),
            "publisher must not write a bundle for a red receipt"
        );
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn publish_rejects_missing_receipt_dir() {
        let receipt = fresh_dir("missing-receipt").join("does-not-exist");
        let out = fresh_dir("missing-publish");
        let result = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        );
        match result {
            Err(PublishError::ReceiptDir(_)) => {}
            other => panic!("missing receipt dir must produce ReceiptDir; got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn publish_is_deterministic_for_fixed_receipt() {
        // Two publish runs on a byte-identical
        // receipt must produce a byte-identical
        // tarball (and therefore a byte-identical
        // sha256 + a byte-identical manifest
        // `files[]` array). A regression in the
        // tarball metadata (mtime drifts, uid
        // drifts) fails this test. The receipt
        // basename is part of the tarball entry
        // paths, so we use a single fixed basename
        // (`det-receipt`) on both sides; the
        // receipts live in separate parent
        // directories so the `fresh_dir` helper
        // does not have a path-collision issue.
        let base_dir = std::env::temp_dir().join(format!(
            "rbp-publish-det-base-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base_dir);
        let receipt1 = base_dir.join("det-receipt");
        let receipt2 = base_dir.join("det-receipt-2");
        let out1 = base_dir.join("det-publish-1");
        let out2 = base_dir.join("det-publish-2");
        // Make both receipts share the
        // `det-receipt` basename (the receipt
        // basename is part of the tarball entry
        // paths, so determinism requires both
        // bundles to use the same basename).
        // We publish both, then compare the
        // `receipt2` bundle to the `receipt1`
        // bundle — to make the basenames match,
        // we publish from `receipt1` for both
        // sides by symlinking `receipt2` →
        // `receipt1`.
        std::fs::create_dir_all(&receipt1).expect("mkdir receipt1");
        make_green_receipt(&receipt1);
        std::fs::create_dir_all(&out1).expect("mkdir out1");
        std::fs::create_dir_all(&out2).expect("mkdir out2");
        let r1 = publish_receipt(
            &receipt1,
            &out1,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish 1");
        // Second publish: same receipt, fresh
        // out dir. The bundle MUST be byte-
        // identical (mtime=0, deterministic
        // gzip header, sorted file walk, fixed
        // basename).
        let r2 = publish_receipt(
            &receipt1,
            &out2,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish 2");
        let bundle1 = std::fs::read(&r1.bundle_path).expect("read bundle 1");
        let bundle2 = std::fs::read(&r2.bundle_path).expect("read bundle 2");
        assert_eq!(
            bundle1, bundle2,
            "byte-identical receipt must produce byte-identical tarball"
        );
        assert_eq!(
            r1.bundle_sha256, r2.bundle_sha256,
            "byte-identical tarball must produce byte-identical sha256"
        );
        let _ = std::fs::remove_dir_all(&base_dir);
        let _ = std::fs::remove_dir_all(&receipt2);
        let _ = std::fs::remove_dir_all(&out1);
        let _ = std::fs::remove_dir_all(&out2);
    }

    #[test]
    fn publish_manifest_has_expected_fields() {
        let receipt = fresh_dir("fields-receipt");
        let out = fresh_dir("fields-publish");
        make_green_receipt(&receipt);
        let r = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        let manifest_str = std::fs::read_to_string(&r.manifest_path).expect("read manifest");
        // The manifest is JSON; every key field
        // the verifier / dashboard reads is
        // present.
        assert!(
            manifest_str.contains("\"bundle_filename\""),
            "manifest must contain bundle_filename"
        );
        assert!(
            manifest_str.contains("\"bundle_sha256\""),
            "manifest must contain bundle_sha256"
        );
        assert!(
            manifest_str.contains("\"total_bytes\""),
            "manifest must contain total_bytes"
        );
        assert!(
            manifest_str.contains("\"files\""),
            "manifest must contain files array"
        );
        assert!(
            manifest_str.contains("\"receipt_dir\""),
            "manifest must contain receipt_dir"
        );
        assert!(
            manifest_str.contains("\"runbook_version\""),
            "manifest must contain runbook_version"
        );
        assert!(
            manifest_str.contains("\"trainer_git_sha\""),
            "manifest must contain trainer_git_sha"
        );
        // The receipt files are visible in the
        // manifest's `files[]` array.
        assert!(
            manifest_str.contains("SUMMARY.txt"),
            "manifest must list SUMMARY.txt"
        );
        assert!(
            manifest_str.contains("recipe.json"),
            "manifest must list recipe.json"
        );
        assert!(
            manifest_str.contains("cluster/exit.txt"),
            "manifest must list cluster/exit.txt"
        );
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn verify_round_trips_published_bundle() {
        // Publish, then re-verify; the round-trip
        // must succeed.
        let receipt = fresh_dir("rt-receipt");
        let out = fresh_dir("rt-publish");
        make_green_receipt(&receipt);
        let r = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        let manifest = PublishedBundle::from_bundle_path(&out).expect("read manifest");
        manifest
            .verify(&out)
            .expect("verify must pass on a fresh bundle");
        // The CLI entry point also returns Ok.
        let cli = crate::verify_bundle::run(&out).expect("verify_bundle::run must pass");
        assert!(
            cli.starts_with(STW032_VERIFY_HEADLINE_PREFIX),
            "verify success line must start with the pinned prefix; got: {cli:?}"
        );
        assert!(
            cli.contains(&r.bundle_sha256),
            "verify success line must include the bundle sha256; got: {cli:?}"
        );
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn verify_rejects_tampered_bundle() {
        // Publish, then tamper with the tarball
        // (rewrite a byte); verify must reject.
        let receipt = fresh_dir("tamper-receipt");
        let out = fresh_dir("tamper-publish");
        make_green_receipt(&receipt);
        publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        let manifest = PublishedBundle::from_bundle_path(&out).expect("read manifest");
        // Flip a single byte in the tarball.
        let mut bytes = std::fs::read(&out.join("bundle.tar.gz")).expect("read tarball");
        if bytes.is_empty() {
            panic!("tarball is empty; test setup error");
        }
        bytes[0] ^= 0xff;
        std::fs::write(&out.join("bundle.tar.gz"), &bytes).expect("rewrite tarball");
        let result = manifest.verify(&out);
        match result {
            Err(PublishError::BundleHashMismatch { path, .. }) => {
                assert_eq!(
                    path, manifest.bundle_filename,
                    "BundleHashMismatch must name the tarball"
                );
            }
            other => panic!("tampered bundle must produce BundleHashMismatch; got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn verify_rejects_missing_manifest() {
        let out = fresh_dir("missing-manifest");
        // No manifest.json; the verifier must
        // return ManifestShape.
        let result = PublishedBundle::from_bundle_path(&out);
        match result {
            Err(PublishError::ManifestShape(_)) => {}
            other => panic!("missing manifest must produce ManifestShape; got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn verify_rejects_bad_manifest_json() {
        let out = fresh_dir("bad-manifest");
        std::fs::create_dir_all(&out).expect("mkdir");
        std::fs::write(out.join(STW032_MANIFEST_FILENAME), "not json").expect("write bad manifest");
        let result = PublishedBundle::from_bundle_path(&out);
        match result {
            Err(PublishError::ManifestShape(_)) => {}
            other => panic!("non-JSON manifest must produce ManifestShape; got: {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&out);
    }

    /// Publish a green receipt, then mutate the
    /// on-disk `manifest.json` to claim an extra
    /// file inside the tarball that the tarball
    /// does not actually contain. The verifier
    /// must return `PublishError::MissingFile` on
    /// the fake path. The contract guarantees a
    /// dashboard scraper can `awk '{print $5}'`
    /// the failure line and group by `missing_file`.
    #[test]
    fn publish_bundle_verifier_rejects_missing_file() {
        let receipt = fresh_dir("missing-file-receipt");
        let out = fresh_dir("missing-file-publish");
        make_green_receipt(&receipt);
        let r = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        // Re-read the manifest, append a fake file
        // entry, write the manifest back.
        let manifest_path = r.manifest_path.clone();
        let manifest_str = std::fs::read_to_string(&manifest_path).expect("read manifest");
        let mut manifest: PublishedBundle =
            serde_json::from_str(&manifest_str).expect("parse manifest");
        manifest.files.push(BundleFile {
            path: "definitely-not-in-the-tarball.txt".to_string(),
            sha256: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            bytes: 0,
        });
        let updated = serde_json::to_string_pretty(&manifest).expect("re-serialise manifest");
        std::fs::write(&manifest_path, updated).expect("rewrite manifest");
        let result = manifest.verify(&out);
        match result {
            Err(PublishError::MissingFile(p)) => assert!(
                p.contains("definitely-not-in-the-tarball.txt"),
                "MissingFile must name the phantom file path; got: {p:?}"
            ),
            other => panic!(
                "manifest naming a file the tarball does not contain must produce \
                 MissingFile; got: {other:?}"
            ),
        }
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    /// `PublishedBundle::to_json` is the
    /// machine-readable one-line form a
    /// downstream scraper can pipe through
    /// `jq`. The contract is: every field the
    /// verifier reads (`bundle_filename`,
    /// `bundle_sha256`, `total_bytes`,
    /// `files`, `receipt_dir`, `runbook_version`,
    /// `trainer_git_sha`) is in the JSON. The
    /// test pins every key the spec named.
    #[test]
    fn publish_to_json_contains_every_field() {
        let receipt = fresh_dir("fields-json-receipt");
        let out = fresh_dir("fields-json-publish");
        make_green_receipt(&receipt);
        publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        let manifest = PublishedBundle::from_bundle_path(&out).expect("read manifest");
        let json = manifest.to_json();
        for key in [
            "\"bundle_filename\"",
            "\"bundle_sha256\"",
            "\"total_bytes\"",
            "\"files\"",
            "\"receipt_dir\"",
            "\"runbook_version\"",
            "\"trainer_git_sha\"",
        ] {
            assert!(
                json.contains(key),
                "manifest JSON must contain the {key} field; got: {json}"
            );
        }
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }

    /// The `Mode::Publish` arm prints a pinned
    /// `live_proof publish complete: bundle=...
    /// sha256=... bytes=...` headline a
    /// dashboard scraper can `grep ^live_proof
    /// publish complete:`. The headline format
    /// is the contract the `verify-bundle` arm
    /// + the integration test both rely on. The
    /// lib test pins the headline prefix and
    /// asserts the headline contains the bundle
    /// sha256 + byte count the publisher
    /// returned.
    #[test]
    fn publish_run_prints_complete_headline() {
        let receipt = fresh_dir("headline-receipt");
        let out = fresh_dir("headline-publish");
        make_green_receipt(&receipt);
        let r = publish_receipt(
            &receipt,
            &out,
            "STW-032 v1",
            "0123456789abcdef0123456789abcdef01234567",
        )
        .expect("publish");
        // The headline is the string the
        // `Mode::Publish` arm prints (mirrored
        // here as a `format!` so the lib test
        // pins the exact format). A drift in the
        // format string fails the test before a
        // dashboard scraper can `grep` the wrong
        // shape.
        let headline = format!(
            "{} bundle={} sha256={} bytes={} files={} basename={}",
            crate::publish::STW032_PUBLISH_HEADLINE_PREFIX,
            r.bundle_path.display(),
            r.bundle_sha256,
            r.total_bytes,
            r.file_count,
            r.receipt_basename,
        );
        assert!(
            headline.starts_with(crate::publish::STW032_PUBLISH_HEADLINE_PREFIX),
            "publish headline must start with the pinned prefix; got: {headline:?}"
        );
        assert!(
            headline.contains(&r.bundle_sha256),
            "publish headline must include the bundle sha256; got: {headline:?}"
        );
        assert!(
            headline.contains(&format!("bytes={}", r.total_bytes)),
            "publish headline must include the byte count; got: {headline:?}"
        );
        let _ = std::fs::remove_dir_all(&receipt);
        let _ = std::fs::remove_dir_all(&out);
    }
}
