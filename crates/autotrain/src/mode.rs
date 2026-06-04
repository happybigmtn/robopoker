//! Training mode selection from command line arguments.
use crate::*;
use rbp_database::Check;
use rbp_database::Check2;
use rbp_database::Check3;
use rbp_database::Schema;
use rbp_nlhe::EpochMetaV2;
use rbp_nlhe::EpochMetaV3;
use rbp_nlhe::NlheProfile;
use rbp_nlhe::NlheProfileV2;
use rbp_nlhe::NlheProfileV3;
use std::path::PathBuf;

/// Training mode parsed from command line arguments
pub enum Mode {
    Status,
    Cluster,
    Fast,
    Slow,
    Reset,
    Smoke,
    Bench,
    /// STW-017: train the v2 trained config
    /// ([`crate::Fast2Session`]) against the v2 tables
    /// ([`rbp_database::BLUEPRINT2`] /
    /// [`rbp_database::EPOCH2`]). Parallels `Self::Fast`
    /// for the v1 trained config; the two can be run
    /// sequentially (or interleaved across the same
    /// database) without colliding because the v1 +
    /// v2 staging tables, blueprint tables, and epoch
    /// rows are all separate physical objects. Honors
    /// the same `RBP_FAST_EPOCHS` / `RBP_FAST_BATCH`
    /// env knobs as `Self::Fast` (a `--fast2` worker
    /// uses the same env-gated budget so a small
    /// smoke run completes in seconds).
    Fast2,
    /// STW-029: train the v3 trained config
    /// ([`crate::Fast3Session`]) against the v3 tables
    /// ([`rbp_database::BLUEPRINT3`] /
    /// [`rbp_database::EPOCH3`]). Parallels `Self::Fast`
    /// and `Self::Fast2` for the v1 / v2 trained configs;
    /// the three can be run sequentially (or interleaved
    /// across the same database) without colliding
    /// because the v1 / v2 / v3 staging tables, blueprint
    /// tables, and epoch rows are all separate physical
    /// objects. Honors the same `RBP_FAST_EPOCHS` /
    /// `RBP_FAST_BATCH` env knobs as `Self::Fast` (a
    /// `--fast3` worker uses the same env-gated budget so
    /// a small smoke run completes in seconds). Lands the
    /// "third DCFR-with-LinearWeight variant" the CEO
    /// testnet roadmap names as the v6 next slice after
    /// STW-017's `Flagship2` trained config.
    Fast3,
    /// STW-016: read a `transcript-<hand_id>.json` file the
    /// bench harness wrote and re-derive the `(Position,
    /// Action)` sequence + a renderable text summary,
    /// without a database connection. The `path` is
    /// either the value after `--replay` (the documented
    /// form) or the lone positional argument (the
    /// README quickstart shortcut). An empty `path`
    /// is the "missing path arg" error the handler
    /// converts into a one-line usage + exit 2.
    Replay {
        /// Absolute or CWD-relative path to a
        /// `transcript-*.json` file.
        path: PathBuf,
    },
    /// STW-018: head-to-head v1-vs-v2 trained-config
    /// bench. Seats the v1 `DatabasePlayer` (seat 0) and
    /// the v2 `DatabasePlayer2` (seat 1) against each
    /// other in the same `Room`, runs K heads-up hands,
    /// and prints a single-line JSON `CompareReport`
    /// declaring the winner (`"v1"`, `"v2"`, or `"tie"`)
    /// with the per-side mbb/100 / CI / win-rate numbers
    /// and the v1-vs-v2 `delta_mbb_per_100`. The CEO
    /// testnet roadmap explicitly names "a third
    /// DCFR-with-LinearWeight variant, or a 'named bot
    /// vs second trained config' comparison" as the v6
    /// next slice after STW-017's `Flagship2` trained
    /// config; STW-018 lands the comparison half. Sized
    /// by `RBP_COMPARE_HANDS` (default 200) and
    /// `RBP_COMPARE_BLIND` (default `B_BLIND`).
    Compare,
    /// STW-031: head-to-head v1-vs-v2-vs-v3
    /// three-way trained-config compare.
    /// Seats the v1 / v2 / v3 `DatabasePlayer` /
    /// `DatabasePlayer2` / `DatabasePlayer3` in three
    /// pairwise heads-up rotations (v1 vs v2, v2 vs v3,
    /// v3 vs v1) for K hands each — each config plays
    /// both seat 0 and seat 1 across the three
    /// rotations, so the per-config `mbb_per_100`
    /// ranking is unbiased by seat position. Prints a
    /// single-line JSON `Compare3Report` declaring the
    /// ranked winner (`"v1"`, `"v2"`, `"v3"`, or
    /// `"tie"`) with the per-config mbb/100 / CI /
    /// win-rate numbers and the three pairwise
    /// `delta_mbb_per_100` values (v1-vs-v2, v2-vs-v3,
    /// v3-vs-v1). Sized by `RBP_COMPARE3_HANDS` (default
    /// 100) and `RBP_COMPARE3_BLIND` (default
    /// `B_BLIND`). Lands the v1-vs-v2-vs-v3 follow-on
    /// the STW-029 v3 trained-config row names as the
    /// "next-next slice if a v3 trained config proves
    /// meaningfully different from the v1 / v2 pair".
    Compare3,
    /// STW-028: re-verify a testnet live launch proof
    /// receipt bundle on disk (the directory the
    /// `scripts/testnet-live-proof.sh` runbook writes
    /// or `LiveProofReceipt::write_to` synthesises).
    /// The mode is read-only and bypasses the DB open
    /// (mirrors `Self::Replay`); the handler delegates
    /// to `crate::verify_receipt::run` which calls
    /// `LiveProofReceipt::read_and_verify` and prints a
    /// one-line verdict a testnet dashboard can grep.
    /// The mode closes the `testnet-live-proof`
    /// mainnet-block hinge: a future dashboard or
    /// auditor can verify a receipt an operator dropped
    /// without re-running `cargo test`. The `path` is
    /// the directory containing `SUMMARY.txt`,
    /// `recipe.json`, and the seven per-step
    /// `exit.txt` files; a bare `--verify-receipt`
    /// with no path is the "missing path arg" error
    /// the handler converts into a one-line usage +
    /// exit 2 (the smoke + replay "data-quality
    /// problem is a non-zero exit" convention).
    VerifyReceipt {
        /// Absolute or CWD-relative path to a receipt
        /// bundle directory the runbook (or the
        /// committed `crates/autotrain/tests/fixtures/
        /// testnet-live-proof-fixture/`) produced.
        path: PathBuf,
    },
    /// STW-032: turn a `receipts/testnet-live-proof-<UTC-ISO>/`
    /// directory the STW-019 runbook produced into a
    /// deterministic, content-addressed portable
    /// publish bundle. Mirrors `Self::VerifyReceipt`:
    /// read-only + bypasses the DB open, calls the
    /// `crate::publish::publish_receipt` handler, and
    /// prints a one-line
    /// `live_proof publish complete: bundle=...
    /// sha256=... bytes=...` headline a dashboard
    /// scraper can `grep ^live_proof publish
    /// complete:` the log. The handler refuses to
    /// publish a red receipt (calls
    /// `LiveProofReceipt::read_and_verify` as a
    /// pre-tar gate). The `path` is the
    /// `receipts/testnet-live-proof-<UTC-ISO>/`
    /// directory the runbook produced; a bare
    /// `--publish` with no path is the "missing
    /// path arg" error the handler converts into a
    /// one-line usage + exit 2.
    Publish {
        /// Absolute or CWD-relative path to a
        /// `receipts/testnet-live-proof-<UTC-ISO>/`
        /// directory the runbook produced.
        path: PathBuf,
    },
    /// STW-032: re-verify a published bundle (the
    /// tarball + manifest + sha256 the
    /// `trainer --publish` arm wrote). Mirrors
    /// `Self::VerifyReceipt`: read-only + bypasses
    /// the DB open, calls the
    /// `crate::verify_bundle::run` handler, and
    /// prints a one-line
    /// `live_proof bundle verification passed: ...`
    /// / `live_proof bundle verification failed: ...`
    /// line a dashboard scraper can
    /// `grep ^live_proof bundle verification` the
    /// log. The handler re-hashes the tarball + every
    /// file inside it, asserts every digest matches
    /// the manifest, and rejects the bundle with a
    /// typed `PublishError` on a mismatch. The `path`
    /// is the publish directory containing
    /// `bundle.tar.gz` + `manifest.json` +
    /// `bundle.sha256`.
    VerifyBundle {
        /// Absolute or CWD-relative path to a
        /// publish directory the
        /// `trainer --publish` arm wrote.
        path: PathBuf,
    },
    /// STW-033: plan + (optionally) apply a
    /// remote upload of the STW-032 publish
    /// bundle to a remote object store (S3 /
    /// GCS / git-tag) bucket the operator (or
    /// a CI worker) names. Mirrors
    /// `Self::Publish` + `Self::VerifyBundle`:
    /// read-only + bypasses the DB open, calls
    /// the
    /// `crate::publish_remote::publish_remote_receipt`
    /// handler, and prints a one-line
    /// `live_proof publish_remote complete: ...`
    /// headline a dashboard scraper can
    /// `grep ^live_proof publish_remote complete:`
    /// the log. The handler refuses to plan
    /// an upload for a red receipt (re-runs
    /// the STW-023 verifier + the STW-032
    /// `trainer --verify-bundle` check as
    /// pre-upload gates). The `path` is the
    /// `receipts/testnet-live-proof-<UTC-ISO>/`
    /// directory the runbook produced; the
    /// `bucket` is the bucket URI
    /// (`s3://<name>/` or bare `<name>`); the
    /// `prefix` is the key prefix inside the
    /// bucket (defaults to `<basename>/`); the
    /// `dry_run` flag (default `true`) skips
    /// the actual `aws s3 cp` shell-out and
    /// only writes the upload plan + a stub
    /// `remote_receipt.json` (the
    /// `cargo test --workspace` integration
    /// test runs in dry-run so a regression
    /// in the CLI surface fails CI without an
    /// `aws` credential or a live bucket).
    PublishRemote {
        /// Absolute or CWD-relative path to a
        /// `receipts/testnet-live-proof-<UTC-ISO>/`
        /// directory the runbook produced.
        path: PathBuf,
        /// Bucket URI (`s3://<name>/` or bare
        /// `<name>`). The arm normalises the
        /// bare-name form to `s3://<name>/` so
        /// the per-file `s3_uri` is always
        /// `s3://...`.
        bucket: String,
        /// Key prefix inside the bucket.
        /// Defaults to `<basename>/` if the
        /// operator passes `--prefix ''`.
        prefix: String,
        /// When `true` (the default), the arm
        /// writes the upload plan + a stub
        /// `remote_receipt.json` and exits 0
        /// without shelling out to `aws` /
        /// `gsutil` / `git`. When `false`, the
        /// arm shells out to `aws s3 cp` per
        /// `s3_objects[]` entry and writes the
        /// post-upload `remote_receipt.json`.
        dry_run: bool,
    },
    /// STW-033: re-verify a published remote
    /// receipt (the `remote_receipt.json` the
    /// `trainer --publish-remote` arm wrote
    /// under `<publish>/<basename>/remote/`).
    /// Mirrors `Self::VerifyBundle`: read-only
    /// + bypasses the DB open, calls the
    /// `crate::publish_remote::read_remote_receipt`
    /// + `PublishedRemoteReceipt::verify`
    /// verifier pair, and prints a one-line
    /// `live_proof remote verification passed: ...`
    /// / `live_proof remote verification failed: ...`
    /// line a dashboard scraper can
    /// `grep ^live_proof remote verification`
    /// the log. The verifier re-hashes every
    /// local file the receipt claims to have
    /// uploaded, asserts every digest matches
    /// the receipt, and rejects the receipt
    /// with a typed `PublishRemoteError` on a
    /// mismatch.
    VerifyRemote {
        /// Absolute or CWD-relative path to a
        /// `<publish>/<basename>/remote/`
        /// directory the
        /// `trainer --publish-remote` arm wrote.
        path: PathBuf,
    },
}

impl Mode {
    pub fn from_args() -> Self {
        let mut positional: Option<String> = None;
        let mut iter = std::env::args().skip(1).peekable();
        while let Some(a) = iter.next() {
            match a.as_str() {
                "--cluster" => return Self::Cluster,
                "--status" => return Self::Status,
                "--fast" => return Self::Fast,
                "--fast2" => return Self::Fast2,
                "--fast3" => return Self::Fast3,
                "--slow" => return Self::Slow,
                "--reset" => return Self::Reset,
                "--smoke" => return Self::Smoke,
                "--bench" => return Self::Bench,
                "--compare" => return Self::Compare,
                "--compare3" => return Self::Compare3,
                "--verify-receipt" => {
                    // The value is the next argv (matches
                    // the `trainer --replay` style of not
                    // using `=`). A bare `--verify-receipt`
                    // with no value returns
                    // `Self::VerifyReceipt` with an empty
                    // `PathBuf`; the dispatch arm in `run()`
                    // prints a one-line usage and exits 2
                    // (the "data-quality problem is a
                    // non-zero exit" convention shared with
                    // the replay + smoke modes).
                    return match iter.next() {
                        Some(p) => Self::VerifyReceipt {
                            path: PathBuf::from(p),
                        },
                        None => Self::VerifyReceipt {
                            path: PathBuf::new(),
                        },
                    };
                }
                "--publish" => {
                    // STW-032: the publish arm's value
                    // is the next argv. A bare
                    // `--publish` with no value returns
                    // `Self::Publish` with an empty
                    // `PathBuf`; the dispatch arm in
                    // `run()` prints a one-line usage
                    // and exits 2.
                    return match iter.next() {
                        Some(p) => Self::Publish {
                            path: PathBuf::from(p),
                        },
                        None => Self::Publish {
                            path: PathBuf::new(),
                        },
                    };
                }
                "--verify-bundle" => {
                    // STW-032: the verify-bundle
                    // arm's value is the next argv.
                    // A bare `--verify-bundle` with
                    // no value returns
                    // `Self::VerifyBundle` with an
                    // empty `PathBuf`; the dispatch
                    // arm in `run()` prints a
                    // one-line usage and exits 2.
                    return match iter.next() {
                        Some(p) => Self::VerifyBundle {
                            path: PathBuf::from(p),
                        },
                        None => Self::VerifyBundle {
                            path: PathBuf::new(),
                        },
                    };
                }
                "--publish-remote" => {
                    // STW-033: the publish-remote
                    // arm takes a positional
                    // `<receipt-dir>` followed by
                    // `--bucket <s3://...>` (or bare
                    // bucket name) + optional
                    // `--prefix <prefix/>` +
                    // optional `--no-dry-run` (or
                    // `--dry-run` to make the
                    // default explicit). The argv
                    // shape mirrors the runbook's
                    // `trainer --publish-remote
                    // <receipt-dir> --bucket
                    // <bucket> --prefix <prefix>
                    // [--no-dry-run]` invocation.
                    // We scan ahead collecting the
                    // optional flags; the bare
                    // `--publish-remote` with no
                    // value returns
                    // `Self::PublishRemote` with an
                    // empty `PathBuf`; the dispatch
                    // arm in `run()` prints a
                    // one-line usage and exits 2.
                    let receipt = match iter.next() {
                        Some(p) => PathBuf::from(p),
                        None => PathBuf::new(),
                    };
                    let mut bucket: String = String::new();
                    let mut prefix: String = String::new();
                    let mut dry_run: bool = true;
                    // The bucket / prefix / dry-run
                    // flags can appear in any order
                    // AFTER the receipt positional;
                    // a second positional (a future
                    // multi-positional extension) is
                    // ignored.
                    while let Some(flag) = iter.peek() {
                        match flag.as_str() {
                            "--bucket" => {
                                iter.next();
                                bucket = iter.next().unwrap_or_default();
                            }
                            "--prefix" => {
                                iter.next();
                                prefix = iter.next().unwrap_or_default();
                            }
                            "--no-dry-run" => {
                                iter.next();
                                dry_run = false;
                            }
                            "--dry-run" => {
                                iter.next();
                                dry_run = true;
                            }
                            // A non-flag token or a
                            // different flag ends the
                            // publish-remote argv
                            // scope; the next iteration
                            // of the outer loop will
                            // dispatch it.
                            _ => break,
                        }
                    }
                    return Self::PublishRemote {
                        path: receipt,
                        bucket,
                        prefix,
                        dry_run,
                    };
                }
                "--verify-remote" => {
                    // STW-033: the verify-remote
                    // arm's value is the next argv.
                    // A bare `--verify-remote` with
                    // no value returns
                    // `Self::VerifyRemote` with an
                    // empty `PathBuf`; the dispatch
                    // arm in `run()` prints a
                    // one-line usage and exits 2.
                    return match iter.next() {
                        Some(p) => Self::VerifyRemote {
                            path: PathBuf::from(p),
                        },
                        None => Self::VerifyRemote {
                            path: PathBuf::new(),
                        },
                    };
                }
                "--replay" => {
                    // The value is the next argv (matches
                    // the `trainer --smoke` style of not
                    // using `=`). A bare `--replay` with
                    // no value returns `Self::Replay` with
                    // an empty `PathBuf`; the dispatch
                    // arm in `run()` prints a one-line
                    // usage and exits 2 (the
                    // "data-quality problem is a non-zero
                    // exit" convention shared with the
                    // smoke mode).
                    return match iter.next() {
                        Some(p) => Self::Replay {
                            path: PathBuf::from(p),
                        },
                        None => Self::Replay {
                            path: PathBuf::new(),
                        },
                    };
                }
                // Anything that does not start with `--`
                // is a positional arg. The first one is
                // the replay path (the README quickstart
                // shortcut). A second positional is
                // ignored (the trainer binary is
                // single-mode; clap-style strict
                // rejection is the next slice if a
                // multi-mode composition is needed).
                s if !s.starts_with("--") => {
                    if positional.is_none() {
                        positional = Some(s.to_string());
                    }
                }
                _ => {}
            }
        }
        // `--replay <path>` wins over the positional
        // shortcut if both are present (the explicit
        // form is the more specific user intent). Note
        // that a bare `--replay` is handled inside the
        // argv loop (it returns a `Replay` with an
        // empty path); this branch only fires when
        // `--replay` is absent and at least one
        // positional was supplied.
        if let Some(p) = positional {
            return Self::Replay {
                path: PathBuf::from(p),
            };
        }
        eprintln!(
            "Usage: trainer --status | --cluster | --fast | --fast2 | --fast3 | --slow | --reset | --smoke | --bench | --compare | --compare3 | --replay <path> | --verify-receipt <path> | --publish <receipt-dir> | --verify-bundle <path> | --publish-remote <receipt-dir> --bucket <s3://...> [--prefix <prefix/>] [--no-dry-run] | --verify-remote <path>"
        );
        std::process::exit(1);
    }

    pub async fn run() {
        // The dispatch opens a `tokio_postgres::Client`
        // *before* the match because every variant
        // other than `Replay` and `VerifyReceipt` is a
        // database-backed training pipeline. STW-016
        // deliberately bypasses the DB open for
        // `Replay`: the whole point of the slice is "no
        // DB needed" (a downstream tool runs this
        // without `DATABASE_URL` set). STW-028 follows
        // the same shape for `VerifyReceipt`: the
        // verifier reads the receipt from disk via
        // `LiveProofReceipt::read_and_verify`, no DB
        // needed. The early `match` arm keeps the cost
        // out of the hot path.
        if let Self::Replay { path } = Self::from_args() {
            if path.as_os_str().is_empty() {
                eprintln!("Usage: trainer --replay <path>");
                std::process::exit(2);
            }
            match crate::replay::run(&path) {
                Ok(s) => {
                    print!("{s}");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        // STW-028: also bypass the DB open for
        // `VerifyReceipt`. The verifier is read-only
        // (no `DB_URL` / `tokio_postgres::Client` use);
        // an empty `PathBuf` is the missing-path-arg
        // error the mode is contractually required to
        // convert into exit 2.
        if let Self::VerifyReceipt { path } = Self::from_args() {
            if path.as_os_str().is_empty() {
                eprintln!("Usage: trainer --verify-receipt <path>");
                std::process::exit(2);
            }
            match crate::verify_receipt::run(&path) {
                Ok(s) => {
                    print!("{s}");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        // STW-032: also bypass the DB open for
        // `Publish`. The publisher is read-only
        // with respect to the receipt (it copies
        // the receipt into a staging tempdir, then
        // tars + sha256s the copy). An empty
        // `PathBuf` is the missing-path-arg error
        // the mode is contractually required to
        // convert into exit 2.
        //
        // The publish step writes the bundle to a
        // sibling `publish/<basename>/` directory
        // next to the receipt, not to the receipt
        // itself (a publish step that mutates the
        // receipt would corrupt the runbook's
        // output on partial-failure paths). The
        // `trainer_git_sha` is read from the
        // `RBP_TRAINER_GIT_SHA` env var the
        // companion `scripts/testnet-live-publish.sh`
        // runbook sets from `git rev-parse HEAD` —
        // the trainer is a single static binary
        // and has no good way to read its own
        // build-time git SHA without an extra
        // build script. The fallback `<unknown>`
        // sentinel keeps the manifest byte-stable
        // for the integration test that runs the
        // trainer without the env knob set.
        if let Self::Publish { path } = Self::from_args() {
            if path.as_os_str().is_empty() {
                eprintln!("Usage: trainer --publish <receipt-dir>");
                std::process::exit(2);
            }
            // Compute the output dir as
            // `<parent>/publish/<basename>/` so the
            // publish step never writes inside the
            // receipt (the receipt is the
            // runbook's read-only artifact; the
            // publish is a follow-on consumer of
            // it, not a refactor of it).
            let basename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("testnet-live-proof-receipt")
                .to_string();
            let parent = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let output_dir = parent.join("publish").join(&basename);
            let trainer_git_sha =
                std::env::var("RBP_TRAINER_GIT_SHA").unwrap_or_else(|_| "<unknown>".to_string());
            match crate::publish::publish_receipt(
                &path,
                &output_dir,
                "STW-032 v1",
                &trainer_git_sha,
            ) {
                Ok(out) => {
                    println!(
                        "{} bundle={} sha256={} bytes={} files={} basename={}",
                        crate::publish::STW032_PUBLISH_HEADLINE_PREFIX,
                        out.bundle_path.display(),
                        out.bundle_sha256,
                        out.total_bytes,
                        out.file_count,
                        out.receipt_basename,
                    );
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        // STW-032: also bypass the DB open for
        // `VerifyBundle`. The verifier is read-only
        // (no `DB_URL` / `tokio_postgres::Client`
        // use); an empty `PathBuf` is the
        // missing-path-arg error the mode is
        // contractually required to convert into
        // exit 2.
        if let Self::VerifyBundle { path } = Self::from_args() {
            if path.as_os_str().is_empty() {
                eprintln!("Usage: trainer --verify-bundle <path>");
                std::process::exit(2);
            }
            match crate::verify_bundle::run(&path) {
                Ok(s) => {
                    print!("{s}");
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        // STW-033: also bypass the DB open for
        // `PublishRemote`. The publisher is
        // read-only with respect to the
        // receipt (it re-verifies the STW-032
        // publish bundle as a pre-upload gate,
        // builds an upload plan, and either
        // writes the plan + a stub
        // `remote_receipt.json` in dry-run or
        // shells out to `aws s3 cp` in live
        // mode). An empty `PathBuf` is the
        // missing-path-arg error the mode is
        // contractually required to convert
        // into exit 2. An empty `bucket` is the
        // missing-bucket-arg error the mode is
        // contractually required to convert
        // into exit 2.
        if let Self::PublishRemote {
            path,
            bucket,
            prefix,
            dry_run,
        } = Self::from_args()
        {
            if path.as_os_str().is_empty() {
                eprintln!(
                    "Usage: trainer --publish-remote <receipt-dir> --bucket <s3://...> \
                     [--prefix <prefix/>] [--no-dry-run]"
                );
                std::process::exit(2);
            }
            if bucket.is_empty() {
                eprintln!(
                    "Usage: trainer --publish-remote <receipt-dir> --bucket <s3://...> \
                     [--prefix <prefix/>] [--no-dry-run]"
                );
                std::process::exit(2);
            }
            // Compute the publish directory
            // (where the STW-032 bundle lives)
            // as
            // `<parent>/publish/<basename>/` so
            // the publish-remote step never
            // touches the receipt directly (it
            // reads the bundle's three sibling
            // files instead).
            let basename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("testnet-live-proof-receipt")
                .to_string();
            let parent = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            let bundle_dir = parent.join("publish").join(&basename);
            // Default the prefix to
            // `<basename>/` when the operator
            // passes `--prefix ''` (the same
            // `PUBLISH_DIR` choice the STW-032
            // `trainer --publish` arm makes).
            let effective_prefix = if prefix.is_empty() {
                format!("{basename}/")
            } else {
                prefix
            };
            let created_at_utc =
                std::env::var("RBP_PUBLISH_REMOTE_UTC").unwrap_or_else(|_| "<unknown>".to_string());
            match crate::publish_remote::publish_remote_receipt(
                &path,
                &bundle_dir,
                &bucket,
                &effective_prefix,
                dry_run,
                Some(&created_at_utc),
            ) {
                Ok(out) => {
                    println!(
                        "{} bucket={} prefix={} files={} bytes={} bundle_sha256={} basename={} dry_run={}",
                        crate::publish_remote::STW033_PUBLISH_REMOTE_HEADLINE_PREFIX,
                        out.plan.bucket,
                        out.plan.prefix,
                        out.s3_objects.len(),
                        out.total_bytes,
                        out.bundle_sha256,
                        basename,
                        dry_run,
                    );
                    std::process::exit(0);
                }
                Err(e) => {
                    eprintln!("{e}");
                    std::process::exit(2);
                }
            }
        }
        // STW-033: also bypass the DB open for
        // `VerifyRemote`. The verifier is
        // read-only (no `DB_URL` /
        // `tokio_postgres::Client` use); an
        // empty `PathBuf` is the
        // missing-path-arg error the mode is
        // contractually required to convert
        // into exit 2.
        if let Self::VerifyRemote { path } = Self::from_args() {
            if path.as_os_str().is_empty() {
                eprintln!("Usage: trainer --verify-remote <path>");
                std::process::exit(2);
            }
            // The verifier reads the
            // `remote_receipt.json` + the local
            // files the receipt claims to have
            // uploaded; the parent publish
            // directory is `<remote_dir>.parent()`
            // (the verifier walks the receipt's
            // `s3_objects[].local_path` and
            // re-resolves relative paths against
            // the publish directory). For
            // absolute `local_path`s the parent
            // arg is unused.
            let parent = path
                .parent()
                .map(|p| p.to_path_buf())
                .unwrap_or_else(|| PathBuf::from("."));
            match crate::publish_remote::read_remote_receipt(&path) {
                Ok(receipt) => match receipt.verify(&parent) {
                    Ok(()) => {
                        println!(
                            "{} bucket={} prefix={} files={} bytes={} bundle_sha256={} basename={}",
                            crate::publish_remote::STW033_VERIFY_REMOTE_HEADLINE_PREFIX,
                            receipt.plan.bucket,
                            receipt.plan.prefix,
                            receipt.s3_objects.len(),
                            receipt.total_bytes,
                            receipt.bundle_sha256,
                            receipt.plan.receipt_basename,
                        );
                        std::process::exit(0);
                    }
                    Err(e) => {
                        eprintln!(
                            "{} {}",
                            crate::publish_remote::STW033_VERIFY_REMOTE_FAILURE_HEADLINE_PREFIX,
                            e
                        );
                        std::process::exit(2);
                    }
                },
                Err(e) => {
                    eprintln!(
                        "{} {}",
                        crate::publish_remote::STW033_VERIFY_REMOTE_FAILURE_HEADLINE_PREFIX,
                        e
                    );
                    std::process::exit(2);
                }
            }
        }
        let client = rbp_database::db().await;
        match Self::from_args() {
            Self::Fast => FastSession::new(client).await.train().await,
            // STW-017: v2 trained config. The
            // `Fast2Session` shape is the v1 shape
            // verbatim; the only divergence is the
            // table names the v2 `sync` writes
            // (BLUEPRINT2 / STAGING2 / EPOCH2).
            Self::Fast2 => Fast2Session::new(client).await.train().await,
            // STW-029: v3 trained config. The
            // `Fast3Session` shape is the v1 / v2
            // shape verbatim; the only divergence is
            // the table names the v3 `sync` writes
            // (BLUEPRINT3 / STAGING3 / EPOCH3).
            Self::Fast3 => Fast3Session::new(client).await.train().await,
            Self::Slow => SlowSession::new(client).await.train().await,
            Self::Reset => Self::reset(&client).await,
            Self::Status => {
                client.status().await;
                // STW-017: also print the v2 epoch +
                // blueprint row counts so a `--status`
                // run reports both the v1 + v2
                // trained-config state.
                client.status_v2().await;
                // STW-029: also print the v3 epoch +
                // blueprint row counts so a `--status`
                // run reports the v1 / v2 / v3
                // trained-config state.
                client.status_v3().await;
            }
            Self::Cluster => PreTraining::run(&client).await,
            Self::Smoke => Self::smoke(client).await,
            Self::Bench => crate::bench::run(client).await,
            // STW-018: head-to-head v1-vs-v2
            // trained-config bench. Mirrors the
            // `Self::Bench` arm — the compare is
            // structurally parallel to the bench
            // (one v1 + one v2 player, one `Room`
            // shell, one JSON report) so a v1
            // `trainer --bench` run and a v1
            // `trainer --compare` run can coexist in
            // the same database without colliding on
            // the v1 / v2 staging tables, the v1 / v2
            // blueprint tables, or the v1 / v2 epoch
            // rows. The compare reuses the same
            // `Room::play_hand_once` +
            // `Room::settlements` pair the bench
            // uses, so a regression in the per-hand
            // PnL math fails both the bench and
            // compare integration tests in the same
            // CI run.
            Self::Compare => crate::bench::run_compare(client).await,
            // STW-031: v1-vs-v2-vs-v3 three-way
            // trained-config compare. Mirrors the
            // `Self::Compare` arm — the compare3
            // runs three pairwise K-handed
            // heads-up rotations (v1 vs v2, v2 vs
            // v3, v3 vs v1) for K hands each and
            // prints a single-line JSON
            // `Compare3Report` declaring the
            // ranked winner. The compare3 is
            // structurally parallel to the
            // compare (one `Room` shell per
            // pair, three pairs, one JSON
            // report) so a v1 `trainer --compare`
            // run and a v1 `trainer --compare3`
            // run can coexist in the same
            // database without colliding on the
            // v1 / v2 / v3 staging tables, the
            // v1 / v2 / v3 blueprint tables, or
            // the v1 / v2 / v3 epoch rows. A
            // regression in the per-hand PnL
            // math fails both the compare and
            // compare3 integration tests in the
            // same CI run.
            Self::Compare3 => crate::bench::run_compare3(client).await,
            // The `Replay` / `VerifyReceipt` /
            // `Publish` / `VerifyBundle` arms are
            // handled above; the compiler still
            // requires an exhaustive match, so the
            // unreachable arms are defensive
            // `unreachable!()`s with messages a
            // future refactor will hit if the early
            // `if let`s are ever removed.
            Self::Replay { .. } => unreachable!(
                "Mode::Replay is dispatched before the DB open; the `Self::Replay {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
            Self::VerifyReceipt { .. } => unreachable!(
                "Mode::VerifyReceipt is dispatched before the DB open; the `Self::VerifyReceipt {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
            Self::Publish { .. } => unreachable!(
                "Mode::Publish is dispatched before the DB open; the `Self::Publish {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
            Self::VerifyBundle { .. } => unreachable!(
                "Mode::VerifyBundle is dispatched before the DB open; the `Self::VerifyBundle {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
            Self::PublishRemote { .. } => unreachable!(
                "Mode::PublishRemote is dispatched before the DB open; the `Self::PublishRemote {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
            Self::VerifyRemote { .. } => unreachable!(
                "Mode::VerifyRemote is dispatched before the DB open; the `Self::VerifyRemote {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
        }
    }
    async fn reset(client: &tokio_postgres::Client) {
        log::info!("Truncating blueprint (v1) table...");
        client
            .execute(<NlheProfile as Schema>::truncates(), &[])
            .await
            .expect("truncate blueprint");
        log::info!("Resetting epoch (v1) counter...");
        client
            .execute(<EpochMeta as Schema>::truncates(), &[])
            .await
            .expect("reset epoch");
        // STW-017: also reset the v2 trained config.
        // The v2 `'current_v2'` row is independent
        // of the v1 `'current'` row, so a v1 reset
        // (the `Mode::Reset` arm above) does not
        // affect the v2 counter. A v2 reset zeroes
        // the v2 row only — it does not touch the
        // v1 row, so a v1 `--fast` continuation
        // survives a v2 reset.
        log::info!("Truncating blueprint (v2) table...");
        client
            .execute(<NlheProfileV2 as Schema>::truncates(), &[])
            .await
            .expect("truncate blueprint_v2");
        log::info!("Resetting epoch (v2) counter...");
        client
            .execute(<EpochMetaV2 as Schema>::truncates(), &[])
            .await
            .expect("reset epoch_v2");
        // STW-029: also reset the v3 trained config.
        // The v3 `'current_v3'` row is independent
        // of the v1 `'current'` row and the v2
        // `'current_v2'` row, so a v1 / v2 reset
        // (the `Mode::Reset` arms above) does not
        // affect the v3 counter. A v3 reset zeroes
        // the v3 row only — it does not touch the
        // v1 / v2 rows, so a v1 `--fast` and a v2
        // `--fast2` continuation both survive a v3
        // reset.
        log::info!("Truncating blueprint (v3) table...");
        client
            .execute(<NlheProfileV3 as Schema>::truncates(), &[])
            .await
            .expect("truncate blueprint_v3");
        log::info!("Resetting epoch (v3) counter...");
        client
            .execute(<EpochMetaV3 as Schema>::truncates(), &[])
            .await
            .expect("reset epoch_v3");
        log::info!("Reset complete.");
    }
    /// One-shot smoke pipeline: pretraining + N training epochs
    /// (capped by `RBP_FAST_EPOCHS`) + sync + status, with a
    /// non-zero exit if the post-sync blueprint row count is `0`
    /// (a successful run leaves a non-empty blueprint that
    /// `trainer --status` can then report).
    ///
    /// The smoke mode is the testnet proof point the CEO roadmap
    /// demands: a small-abstraction, env-gated, end-to-end run
    /// that a worker can complete in seconds, with the result
    /// observable through the same `Check` queries that drive
    /// `trainer --status`.
    async fn smoke(client: std::sync::Arc<tokio_postgres::Client>) {
        let epochs = rbp_core::fast_epochs().unwrap_or(1);
        log::info!("smoke: pretraining + {epochs} epoch(s) + sync + status");
        let session = FastSession::new(client.clone()).await;
        session.train().await;
        // After the (env-capped) train loop the FastSession's
        // `sync` has already run inside `Trainer::train`. The
        // row count below reads what was actually persisted,
        // not what the in-memory profile thinks it has.
        let rows = client.blueprint().await;
        let epoch = client.epochs().await;
        log::info!("smoke complete: epochs={epoch} rows={rows}");
        if rows == 0 {
            log::error!("smoke failed: blueprint row count is 0");
            std::process::exit(2);
        }
    }
}
