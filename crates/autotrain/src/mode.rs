//! Training mode selection from command line arguments.
use crate::*;
use rbp_database::Check;
use rbp_database::Schema;
use rbp_nlhe::NlheProfile;
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
                "--slow" => return Self::Slow,
                "--reset" => return Self::Reset,
                "--smoke" => return Self::Smoke,
                "--bench" => return Self::Bench,
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
            "Usage: trainer --status | --cluster | --fast | --slow | --reset | --smoke | --bench | --replay <path>"
        );
        std::process::exit(1);
    }

    pub async fn run() {
        // The dispatch opens a `tokio_postgres::Client`
        // *before* the match because every variant
        // other than `Replay` is a database-backed
        // training pipeline. STW-016 deliberately
        // bypasses the DB open for `Replay`: the
        // whole point of the slice is "no DB
        // needed" (a downstream tool runs this
        // without `DATABASE_URL` set). The early
        // `match` arm keeps the cost out of the
        // hot path.
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
        let client = rbp_database::db().await;
        match Self::from_args() {
            Self::Fast => FastSession::new(client).await.train().await,
            Self::Slow => SlowSession::new(client).await.train().await,
            Self::Reset => Self::reset(&client).await,
            Self::Status => client.status().await,
            Self::Cluster => PreTraining::run(&client).await,
            Self::Smoke => Self::smoke(client).await,
            Self::Bench => crate::bench::run(client).await,
            // The `Replay` arm was handled above; the
            // compiler still requires an exhaustive
            // match, so the unreachable arm is a
            // defensive `unreachable!()` with a
            // message a future refactor will hit if
            // the early `match` is ever removed.
            Self::Replay { .. } => unreachable!(
                "Mode::Replay is dispatched before the DB open; the `Self::Replay {{ .. }}` \
                 arm here is the compiler-required exhaustive-match catch-all"
            ),
        }
    }
    async fn reset(client: &tokio_postgres::Client) {
        log::info!("Truncating blueprint table...");
        client
            .execute(<NlheProfile as Schema>::truncates(), &[])
            .await
            .expect("truncate blueprint");
        log::info!("Resetting epoch counter...");
        client
            .execute(<EpochMeta as Schema>::truncates(), &[])
            .await
            .expect("reset epoch");
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
