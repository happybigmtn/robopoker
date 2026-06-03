//! Training mode selection from command line arguments.
use crate::*;
use rbp_database::Check;
use rbp_database::Schema;
use rbp_nlhe::NlheProfile;

/// Training mode parsed from command line arguments
pub enum Mode {
    Status,
    Cluster,
    Fast,
    Slow,
    Reset,
    Smoke,
}

impl Mode {
    pub fn from_args() -> Self {
        std::env::args()
            .find_map(|a| match a.as_str() {
                "--cluster" => Some(Self::Cluster),
                "--status" => Some(Self::Status),
                "--fast" => Some(Self::Fast),
                "--slow" => Some(Self::Slow),
                "--reset" => Some(Self::Reset),
                "--smoke" => Some(Self::Smoke),
                _ => None,
            })
            .unwrap_or_else(|| {
                eprintln!(
                    "Usage: trainer --status | --cluster | --fast | --slow | --reset | --smoke"
                );
                std::process::exit(1);
            })
    }

    pub async fn run() {
        let client = rbp_database::db().await;
        match Self::from_args() {
            Self::Fast => FastSession::new(client).await.train().await,
            Self::Slow => SlowSession::new(client).await.train().await,
            Self::Reset => Self::reset(&client).await,
            Self::Status => client.status().await,
            Self::Cluster => PreTraining::run(&client).await,
            Self::Smoke => Self::smoke(client).await,
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
