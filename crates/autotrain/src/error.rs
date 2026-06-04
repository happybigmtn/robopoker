//! `TrainerError` — the dashboard-greppable, single
//! pinned-prefix error surface the `trainer` binary
//! emits on every error path. STW-038 lands the typed
//! enum so a downstream testnet dashboard scraper can
//! `grep ^trainer error kind=` every stderr line
//! regardless of which subcommand produced it, and so a
//! regression in a per-variant `to_pinned_line` shape
//! fails a single lib test (the testnet dashboard's
//! contract is "every error has a stable shape").
//!
//! The design contract is deliberately narrow:
//!
//! 1. `TrainerError::as_str` returns the pinned
//!    `kind` token a dashboard scraper greps
//!    (`"red_receipt"`, `"red_bundle"`, etc. — the
//!    11 tokens are stable forever and a regression
//!    in any of them fails a lib test).
//! 2. `TrainerError::to_pinned_line` returns a single
//!    stable
//!    `trainer error: kind=<kind> detail=<detail>`
//!    line. The `kind` is the `as_str()` output; the
//!    `detail` is the human-readable cause. The shape
//!    is the single dashboard-greppable contract.
//! 3. The existing per-arm `live_proof ...` error
//!    lines STW-032 / STW-033 / STW-034 / STW-035
//!    emit are *additionally* routed through
//!    `TrainerError::to_pinned_line` so a regression
//!    in either the legacy prefix or the new pinned
//!    shape fails CI.
//!
//! Scope boundary: the `kind` is the *only* pinned
//! piece. The `detail` is human-readable and a
//! dashboard scraper treats it as opaque prose (the
//! dashboard greps `^trainer error kind=`, never
//! `^trainer error detail=`).
use std::fmt;
use std::io;

/// The single typed error surface the `trainer`
/// binary emits. Every error variant has a stable
/// `kind` token (the `as_str()` return value) and a
/// stable `to_pinned_line` shape
/// (`trainer error: kind=<kind> detail=<detail>`).
///
/// The 11 variants and their `kind` tokens are:
///
/// | variant                | `kind` token    |
/// |------------------------|-----------------|
/// | `NoBlueprint`          | `no_blueprint`  |
/// | `NoDatabase`           | `no_database`   |
/// | `NoBucket`             | `no_bucket`     |
/// | `RedReceipt(String)`   | `red_receipt`   |
/// | `RedBundle(String)`    | `red_bundle`    |
/// | `RedIndex(String)`     | `red_index`     |
/// | `MissingArg(&str)`     | `missing_arg`   |
/// | `BadArg { kind, .. }`  | `bad_arg`       |
/// | `Io(io::Error)`        | `io`            |
/// | `AwsCli`               | `aws_cli`       |
/// | `Internal(String)`     | `internal`      |
///
/// The tokens are stable forever; a regression in any
/// of them fails the
/// `tests::as_str_is_stable_per_variant` test, which
/// is the single source of truth for the dashboard
/// scraper's grep regex.
///
/// Note: `PartialEq` / `Eq` / `Clone` are *not* derived
/// because the `Io(io::Error)` variant wraps a
/// `std::io::Error`, which does not implement those
/// traits on stable Rust. Tests compare via
/// `as_str()` + `to_pinned_line()` instead (the
/// dashboard's contract is the string shape, not
/// `==`-equality on the wrapped error).
#[derive(Debug)]
pub enum TrainerError {
    /// A training pipeline tried to read the
    /// `BLUEPRINT` table but the database open
    /// succeeded with no blueprint rows. The trainer
    /// refuses to run `fast` / `slow` against an empty
    /// blueprint (the
    /// "no-blueprint-no-train" invariant).
    NoBlueprint,
    /// The trainer tried to open the database but the
    /// `DATABASE_URL` env is unset or the open failed.
    /// Distinct from `NoBlueprint`: a present-but-empty
    /// database is `NoBlueprint`; an absent database
    /// is `NoDatabase`.
    NoDatabase,
    /// The `trainer --publish-remote` / `--publish-index-remote`
    /// arm was run without a `--bucket` value. The
    /// arms refuse to plan an upload without a bucket
    /// URI (the "no-bucket-no-upload" invariant).
    NoBucket,
    /// STW-033: a `remote_receipt.json` failed the
    /// STW-023 verifier. The publisher-remote refuses
    /// to plan an upload for a red receipt. The
    /// `String` payload is the verifier's reason
    /// (e.g. `"step_failed: cluster"`).
    RedReceipt(String),
    /// STW-032: a receipt failed the STW-023
    /// verifier. The publisher refuses to bundle a
    /// red receipt. The `String` payload is the
    /// verifier's reason.
    RedBundle(String),
    /// STW-034 / STW-035: a `remote_receipt.json`
    /// (STW-034) or an `INDEX.json` (STW-035) failed
    /// the corresponding verifier. The aggregator /
    /// index-remote refuse to plan an upload for a
    /// red entry / red index. The `String` payload is
    /// the verifier's reason.
    RedIndex(String),
    /// The operator passed a subcommand that requires
    /// a value (e.g. `trainer --replay`) without one.
    /// The `&'static str` is the subcommand name the
    /// arm printed in its one-line usage + exit 2
    /// path (e.g. `"--replay"`, `"--publish"`).
    MissingArg(&'static str),
    /// The operator passed a flag with a malformed
    /// value (e.g. `--bucket <empty>` or
    /// `--prefix <unparseable>`). The `kind` is the
    /// flag name; the `detail` is the human-readable
    /// parse failure.
    BadArg {
        /// The flag name the operator passed
        /// (e.g. `"--bucket"`, `"--prefix"`).
        kind: &'static str,
        /// Human-readable parse failure (e.g.
        /// `"empty value"`, `"missing s3:// prefix"`).
        detail: String,
    },
    /// A file or directory operation failed (open,
    /// read, write, etc.). The wrapped
    /// `std::io::Error` is the cause; the
    /// `to_pinned_line` shape collapses it to a
    /// `kind=io` line with the `Display` form of the
    /// error in `detail`.
    Io(io::Error),
    /// The live `aws s3 cp` step failed (CLI missing,
    /// no creds, network error). Only fired when
    /// `--no-dry-run` is set. The `Display` form of
    /// the underlying error string is in `detail`
    /// (the arm writes a `String` payload it
    /// constructs from the `aws` stderr; the typed
    /// variant deliberately does not carry the
    /// payload — the dashboard scraper only cares
    /// about the `kind=aws_cli` token).
    AwsCli,
    /// A catch-all for any other failure that does
    /// not map to the 10 typed variants. The
    /// `String` payload is the human-readable cause.
    /// Production code prefers the typed variants;
    /// `Internal` is the last-resort shape.
    Internal(String),
}

impl TrainerError {
    /// The pinned `kind` token a dashboard scraper
    /// greps. The 11 tokens are stable forever and
    /// the `tests::as_str_is_stable_per_variant`
    /// test fails on any regression.
    pub fn as_str(&self) -> &'static str {
        match self {
            TrainerError::NoBlueprint => "no_blueprint",
            TrainerError::NoDatabase => "no_database",
            TrainerError::NoBucket => "no_bucket",
            TrainerError::RedReceipt(_) => "red_receipt",
            TrainerError::RedBundle(_) => "red_bundle",
            TrainerError::RedIndex(_) => "red_index",
            TrainerError::MissingArg(_) => "missing_arg",
            TrainerError::BadArg { .. } => "bad_arg",
            TrainerError::Io(_) => "io",
            TrainerError::AwsCli => "aws_cli",
            TrainerError::Internal(_) => "internal",
        }
    }

    /// The pinned single-line shape a dashboard
    /// scraper greps. The shape is
    /// `trainer error: kind=<kind> detail=<detail>`
    /// where `<kind>` is the `as_str()` return value
    /// and `<detail>` is the human-readable cause.
    /// The shape is stable forever; the
    /// `tests::to_pinned_line_is_stable_per_variant`
    /// test fails on any regression.
    pub fn to_pinned_line(&self) -> String {
        match self {
            TrainerError::NoBlueprint => {
                "trainer error: kind=no_blueprint detail=BLUEPRINT table is empty; run trainer --cluster first".to_string()
            }
            TrainerError::NoDatabase => {
                "trainer error: kind=no_database detail=DATABASE_URL is unset or database open failed".to_string()
            }
            TrainerError::NoBucket => {
                "trainer error: kind=no_bucket detail=--bucket <s3://...> is required for the publish-remote arm".to_string()
            }
            TrainerError::RedReceipt(s) => {
                format!("trainer error: kind=red_receipt detail={s}")
            }
            TrainerError::RedBundle(s) => {
                format!("trainer error: kind=red_bundle detail={s}")
            }
            TrainerError::RedIndex(s) => {
                format!("trainer error: kind=red_index detail={s}")
            }
            TrainerError::MissingArg(flag) => {
                format!("trainer error: kind=missing_arg detail=required value for {flag} was not provided")
            }
            TrainerError::BadArg { kind, detail } => {
                format!("trainer error: kind=bad_arg detail={kind}: {detail}")
            }
            TrainerError::Io(e) => {
                format!("trainer error: kind=io detail={e}")
            }
            TrainerError::AwsCli => {
                "trainer error: kind=aws_cli detail=aws s3 cp failed (missing binary, no creds, or network error)".to_string()
            }
            TrainerError::Internal(s) => {
                format!("trainer error: kind=internal detail={s}")
            }
        }
    }

    /// The 11 pinned `kind` tokens in stable
    /// alphabetical order. The
    /// `trainer --error-shape-test` argv flag prints
    /// one token per line in this order so a CI
    /// dashboard scraper can `grep ^trainer error
    /// kind=` the shape without exercising every
    /// error path. The function returns a `'static`
    /// slice so the order is fixed at compile time
    /// (the alphabetical order is the dashboard's
    /// contract).
    pub fn all_kinds_alphabetical() -> &'static [&'static str] {
        &[
            "aws_cli",
            "bad_arg",
            "internal",
            "io",
            "missing_arg",
            "no_blueprint",
            "no_bucket",
            "no_database",
            "red_bundle",
            "red_index",
            "red_receipt",
        ]
    }
}

impl fmt::Display for TrainerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The `Display` impl is the *pinned* shape a
        // dashboard scraper greps. The legacy per-arm
        // `live_proof ...` lines are *additionally*
        // emitted on the same stderr line (see the
        // `to_pinned_line` calls in `publish.rs` /
        // `publish_remote.rs` / `publish_index.rs` /
        // `publish_index_remote.rs`).
        f.write_str(&self.to_pinned_line())
    }
}

impl std::error::Error for TrainerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            TrainerError::Io(e) => Some(e),
            _ => None,
        }
    }
}

impl From<io::Error> for TrainerError {
    fn from(e: io::Error) -> Self {
        TrainerError::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::error::Error;

    /// STW-038: the 11 pinned `as_str` tokens are
    /// stable forever. A regression in any of them
    /// fails this test (the dashboard scraper's grep
    /// regex is `^trainer error kind=` and the regex
    /// is generated from this list).
    #[test]
    fn as_str_is_stable_per_variant() {
        assert_eq!(TrainerError::NoBlueprint.as_str(), "no_blueprint");
        assert_eq!(TrainerError::NoDatabase.as_str(), "no_database");
        assert_eq!(TrainerError::NoBucket.as_str(), "no_bucket");
        assert_eq!(
            TrainerError::RedReceipt("x".to_string()).as_str(),
            "red_receipt"
        );
        assert_eq!(
            TrainerError::RedBundle("x".to_string()).as_str(),
            "red_bundle"
        );
        assert_eq!(
            TrainerError::RedIndex("x".to_string()).as_str(),
            "red_index"
        );
        assert_eq!(TrainerError::MissingArg("--replay").as_str(), "missing_arg");
        assert_eq!(
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "x".to_string()
            }
            .as_str(),
            "bad_arg"
        );
        assert_eq!(
            TrainerError::Io(io::Error::new(io::ErrorKind::Other, "x")).as_str(),
            "io"
        );
        assert_eq!(TrainerError::AwsCli.as_str(), "aws_cli");
        assert_eq!(TrainerError::Internal("x".to_string()).as_str(), "internal");
    }

    /// STW-038: the pinned `to_pinned_line` shape is
    /// `trainer error: kind=<kind> detail=<detail>`
    /// and is stable per variant. A regression in
    /// any variant's shape fails this test.
    #[test]
    fn to_pinned_line_is_stable_per_variant() {
        assert_eq!(
            TrainerError::NoBlueprint.to_pinned_line(),
            "trainer error: kind=no_blueprint detail=BLUEPRINT table is empty; run trainer --cluster first"
        );
        assert_eq!(
            TrainerError::NoDatabase.to_pinned_line(),
            "trainer error: kind=no_database detail=DATABASE_URL is unset or database open failed"
        );
        assert_eq!(
            TrainerError::NoBucket.to_pinned_line(),
            "trainer error: kind=no_bucket detail=--bucket <s3://...> is required for the publish-remote arm"
        );
        assert_eq!(
            TrainerError::RedReceipt("step_failed: cluster".to_string()).to_pinned_line(),
            "trainer error: kind=red_receipt detail=step_failed: cluster"
        );
        assert_eq!(
            TrainerError::RedBundle("step_failed: cluster".to_string()).to_pinned_line(),
            "trainer error: kind=red_bundle detail=step_failed: cluster"
        );
        assert_eq!(
            TrainerError::RedIndex("index_red: missing_object".to_string()).to_pinned_line(),
            "trainer error: kind=red_index detail=index_red: missing_object"
        );
        assert_eq!(
            TrainerError::MissingArg("--replay").to_pinned_line(),
            "trainer error: kind=missing_arg detail=required value for --replay was not provided"
        );
        assert_eq!(
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "empty value".to_string()
            }
            .to_pinned_line(),
            "trainer error: kind=bad_arg detail=--bucket: empty value"
        );
        assert_eq!(
            TrainerError::AwsCli.to_pinned_line(),
            "trainer error: kind=aws_cli detail=aws s3 cp failed (missing binary, no creds, or network error)"
        );
        assert_eq!(
            TrainerError::Internal("oops".to_string()).to_pinned_line(),
            "trainer error: kind=internal detail=oops"
        );
    }

    /// STW-038: the `Io` variant's `to_pinned_line`
    /// shape wraps the underlying `std::io::Error`'s
    /// `Display` form. The test pins the shape on a
    /// canonical "file not found" error so a
    /// regression in the wrapping shape fails CI.
    #[test]
    fn to_pinned_line_io_wraps_underlying_error() {
        let e = io::Error::new(io::ErrorKind::NotFound, "fixture: not_found");
        let pinned = TrainerError::Io(e).to_pinned_line();
        assert!(
            pinned.starts_with("trainer error: kind=io detail="),
            "pinned line must start with the pinned prefix: {pinned}"
        );
        assert!(
            pinned.contains("fixture: not_found"),
            "pinned line must contain the underlying error's Display form: {pinned}"
        );
    }

    /// STW-038: the `Display` impl is the *pinned*
    /// shape (the legacy per-arm `live_proof ...`
    /// line is *additionally* emitted by the calling
    /// arm; the `Display` is the dashboard-greppable
    /// contract).
    #[test]
    fn display_eq_to_pinned_line() {
        for variant in [
            TrainerError::NoBlueprint,
            TrainerError::NoDatabase,
            TrainerError::NoBucket,
            TrainerError::RedReceipt("x".to_string()),
            TrainerError::RedBundle("x".to_string()),
            TrainerError::RedIndex("x".to_string()),
            TrainerError::MissingArg("--replay"),
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "x".to_string(),
            },
            TrainerError::AwsCli,
            TrainerError::Internal("x".to_string()),
        ] {
            assert_eq!(
                variant.to_string(),
                variant.to_pinned_line(),
                "Display must equal to_pinned_line for {variant:?}"
            );
        }
    }

    /// STW-038: every `to_pinned_line` output starts
    /// with the single pinned prefix
    /// `trainer error: kind=`. The dashboard scraper
    /// greps this exact prefix. A regression in the
    /// prefix (typo, change of case, change of
    /// separator) fails this test.
    #[test]
    fn to_pinned_line_starts_with_pinned_prefix_for_every_variant() {
        let variants: Vec<TrainerError> = vec![
            TrainerError::NoBlueprint,
            TrainerError::NoDatabase,
            TrainerError::NoBucket,
            TrainerError::RedReceipt("x".to_string()),
            TrainerError::RedBundle("x".to_string()),
            TrainerError::RedIndex("x".to_string()),
            TrainerError::MissingArg("--replay"),
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "x".to_string(),
            },
            TrainerError::Io(io::Error::new(io::ErrorKind::Other, "x")),
            TrainerError::AwsCli,
            TrainerError::Internal("x".to_string()),
        ];
        for v in variants {
            let line = v.to_pinned_line();
            assert!(
                line.starts_with("trainer error: kind="),
                "every pinned line must start with the pinned prefix; got: {line}"
            );
            let kind = v.as_str();
            assert!(
                line.contains(&format!("kind={kind}")),
                "pinned line must contain kind={kind}; got: {line}"
            );
        }
    }

    /// STW-038: the `all_kinds_alphabetical` list is
    /// the single source of truth the
    /// `--error-shape-test` argv flag prints. The
    /// list is in stable alphabetical order; a
    /// regression in the order (a sort regression, a
    /// typo in a token) fails this test.
    #[test]
    fn all_kinds_alphabetical_is_stable() {
        let kinds = TrainerError::all_kinds_alphabetical();
        assert_eq!(kinds.len(), 11, "must have 11 kinds");
        let expected: &[&str] = &[
            "aws_cli",
            "bad_arg",
            "internal",
            "io",
            "missing_arg",
            "no_blueprint",
            "no_bucket",
            "no_database",
            "red_bundle",
            "red_index",
            "red_receipt",
        ];
        assert_eq!(kinds, expected, "alphabetical order must match");
        // Belt + suspenders: verify the slice is
        // actually sorted (a regression in
        // `all_kinds_alphabetical`'s definition
        // producing an unsorted slice fails this).
        let mut sorted = kinds.to_vec();
        sorted.sort_unstable();
        assert_eq!(kinds, &sorted[..], "kinds must be sorted ascending");
    }

    /// STW-038: every `as_str()` token is present in
    /// the `all_kinds_alphabetical` list (the
    /// `Display` / `as_str` / `all_kinds` triple is
    /// the dashboard's contract; a token in one but
    /// not the other is a contract drift that fails
    /// this test).
    #[test]
    fn as_str_token_appears_in_all_kinds_list() {
        let kinds = TrainerError::all_kinds_alphabetical();
        for variant in [
            TrainerError::NoBlueprint,
            TrainerError::NoDatabase,
            TrainerError::NoBucket,
            TrainerError::RedReceipt("x".to_string()),
            TrainerError::RedBundle("x".to_string()),
            TrainerError::RedIndex("x".to_string()),
            TrainerError::MissingArg("--replay"),
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "x".to_string(),
            },
            TrainerError::Io(io::Error::new(io::ErrorKind::Other, "x")),
            TrainerError::AwsCli,
            TrainerError::Internal("x".to_string()),
        ] {
            let token = variant.as_str();
            assert!(
                kinds.contains(&token),
                "as_str() token `{token}` must appear in all_kinds_alphabetical()"
            );
        }
    }

    /// STW-038: the `From<io::Error>` impl is the
    /// "?"-operator-friendly bridge the calling arms
    /// use to convert a `std::io::Error` from
    /// `read_to_string` / `File::open` into a
    /// `TrainerError`. The test pins the conversion
    /// shape.
    #[test]
    fn from_io_error_yields_io_variant() {
        let e = io::Error::new(io::ErrorKind::PermissionDenied, "denied");
        let te: TrainerError = e.into();
        assert_eq!(te.as_str(), "io");
        assert!(
            te.to_pinned_line().contains("denied"),
            "underlying io::Error Display must appear in detail"
        );
    }

    /// STW-038: the `Error::source` impl is the
    /// "unwrap the underlying io::Error" bridge a
    /// dashboard can use to walk the cause chain.
    /// The test pins that the `Io` variant's
    /// `source()` returns `Some(&io::Error)` and the
    /// 10 payload-free variants return `None`.
    #[test]
    fn source_returns_underlying_io_error_for_io_variant() {
        let e = io::Error::new(io::ErrorKind::Other, "x");
        let te = TrainerError::Io(e);
        assert!(te.source().is_some(), "Io variant must expose a source");
        for variant in [
            TrainerError::NoBlueprint,
            TrainerError::NoDatabase,
            TrainerError::NoBucket,
            TrainerError::RedReceipt("x".to_string()),
            TrainerError::RedBundle("x".to_string()),
            TrainerError::RedIndex("x".to_string()),
            TrainerError::MissingArg("--replay"),
            TrainerError::BadArg {
                kind: "--bucket",
                detail: "x".to_string(),
            },
            TrainerError::AwsCli,
            TrainerError::Internal("x".to_string()),
        ] {
            assert!(
                variant.source().is_none(),
                "non-Io variant must not expose a source: {variant:?}"
            );
        }
    }

    /// STW-038: the `RedReceipt` / `RedBundle` /
    /// `RedIndex` payload-bearing variants produce
    /// distinct pinned lines for distinct payloads
    /// (a regression that collapses payloads fails
    /// this test; the dashboard greps the
    /// `kind=` prefix, but a future operator UX
    /// upgrade might want to extract the
    /// `detail=` cause).
    #[test]
    fn red_variants_payload_is_reflected_in_pinned_line() {
        assert_ne!(
            TrainerError::RedReceipt("a".to_string()).to_pinned_line(),
            TrainerError::RedReceipt("b".to_string()).to_pinned_line()
        );
        assert_ne!(
            TrainerError::RedBundle("a".to_string()).to_pinned_line(),
            TrainerError::RedBundle("b".to_string()).to_pinned_line()
        );
        assert_ne!(
            TrainerError::RedIndex("a".to_string()).to_pinned_line(),
            TrainerError::RedIndex("b".to_string()).to_pinned_line()
        );
        // The `kind` token is the same for any
        // payload of the same variant (the
        // dashboard's grep target).
        assert_eq!(
            TrainerError::RedReceipt("a".to_string()).as_str(),
            TrainerError::RedReceipt("b".to_string()).as_str()
        );
    }

    /// STW-038: the `BadArg` variant's
    /// `to_pinned_line` includes both the flag
    /// `kind` (the arg-name token) and the
    /// `detail` (the parse-failure prose). A
    /// regression that drops either piece fails
    /// this test.
    #[test]
    fn bad_arg_pinned_line_includes_kind_and_detail() {
        let line = TrainerError::BadArg {
            kind: "--prefix",
            detail: "missing s3:// prefix".to_string(),
        }
        .to_pinned_line();
        assert!(
            line.contains("kind=bad_arg"),
            "pinned line must contain kind=bad_arg: {line}"
        );
        assert!(
            line.contains("--prefix"),
            "pinned line must contain the flag name: {line}"
        );
        assert!(
            line.contains("missing s3:// prefix"),
            "pinned line must contain the detail prose: {line}"
        );
    }

    /// STW-038: the `MissingArg` variant's pinned
    /// line names the flag the operator forgot
    /// (the legacy per-arm `Usage: trainer --replay
    /// <path>` line is *additionally* emitted; the
    /// pinned line is the dashboard-greppable
    /// contract). A regression that drops the flag
    /// name from the detail fails this test.
    #[test]
    fn missing_arg_pinned_line_names_the_flag() {
        assert!(
            TrainerError::MissingArg("--replay")
                .to_pinned_line()
                .contains("--replay"),
            "pinned line must name the missing-arg flag"
        );
        assert!(
            TrainerError::MissingArg("--publish")
                .to_pinned_line()
                .contains("--publish"),
            "pinned line must name the missing-arg flag"
        );
    }
}
