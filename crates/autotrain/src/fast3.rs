//! STW-029: fast in-memory training session for the v3
//! trained config (`Flagship3`).
//!
//! Parallels [`super::FastSession`] (v1) and
//! [`super::Fast2Session`] (v2) but targets the v3
//! tables ([`rbp_database::BLUEPRINT3`] /
//! [`rbp_database::STAGING3`] / [`rbp_database::EPOCH3`]
//! + [`rbp_database::EPOCH3_KEY`]). The v1
//! `FastSession::sync` writes the v1 staging table +
//! the v1 `'current'` epoch row; the v2
//! `Fast2Session::sync` writes the v2 staging table +
//! the v2 `'current_v2'` epoch row; this v3
//! `Fast3Session::sync` writes the v3 staging table +
//! the v3 `'current_v3'` epoch row. The three sessions
//! can run sequentially (or interleaved across the same
//! database) without colliding on the staging table or
//! the epoch counter row.
//!
//! The training loop itself is identical to v1 / v2: the
//! v3 difference is the regret / policy combination
//! ([`rbp_nlhe::Flagship3`] = `DiscountedRegret` +
//! `LinearWeight` + `PluribusSampling` — the missing
//! cross-product cell of the v1 / v2 regret × policy
//! matrix the CEO roadmap names as the "third
//! DCFR-with-LinearWeight variant"), so the `Trainer`
//! trait impl is the v1 / v2 shape verbatim — `step` /
//! `epoch` / `checkpoint` / `summary` all delegate to
//! the wrapped solver, and `sync` is the only method
//! that knows it's writing v3 tables.

use crate::*;
use rbp_database::*;
use rbp_mccfr::*;
use rbp_nlhe::Flagship3;
use rbp_nlhe::NlheProfileV3;
use std::sync::Arc;
use tokio_postgres::Client;
use tokio_postgres::binary_copy::BinaryCopyInWriter;

/// STW-029: v3 trained-config fast in-memory training
/// session using `DiscountedRegret` + `LinearWeight` +
/// `PluribusSampling` (the [`rbp_nlhe::Flagship3`]
/// combination — the "third DCFR-with-LinearWeight
/// variant" the CEO testnet roadmap names as the v6
/// next slice after STW-017's `Flagship2`).
///
/// Mirrors [`FastSession`] (v1) and [`Fast2Session`]
/// (v2) so a `Mode::Fast3` dispatch can call
/// `Fast3Session::new(client).await.train().await` the
/// same way `Mode::Fast` calls
/// `FastSession::new(client).await.train().await`. The
/// only divergence from v1 / v2 is the table names the
/// `sync` step writes: v1 → `STAGING` / `BLUEPRINT` /
/// `EPOCH`; v2 → `STAGING2` / `BLUEPRINT2` / `EPOCH2`;
/// v3 → `STAGING3` / `BLUEPRINT3` / `EPOCH3`
/// (`'current_v3'` row).
pub struct Fast3Session {
    client: Arc<Client>,
    solver: Flagship3,
}

impl Fast3Session {
    /// Build a v3 training session against a live
    /// Postgres. The call order is the v1 / v2 order
    /// verbatim (run pretraining to ensure the v1
    /// clustering tables exist, then hydrate the v3
    /// solver from the v3 tables). The v3 tables
    /// (`BLUEPRINT3` / `EPOCH3`) are created lazily by
    /// the v3 `Schema::creates()` DDL; a fresh DB
    /// that has never seen a `--fast3` run lands in
    /// the `NlheProfileV3::hydrate` empty-blueprint
    /// fallback (the bench's `blueprint_trained: false`
    /// path), which is the documented post-`--reset`
    /// state.
    pub async fn new(client: Arc<Client>) -> Self {
        // `PreTraining::run` is the v1 clustering
        // bootstrap. The v3 path doesn't re-cluster
        // (the v1 clustering is what the v3 profile's
        // info-set indexing is built on); the v3
        // bootstrap is just creating the v3 tables
        // if they don't exist. The clustering side
        // effect is harmless on a fresh DB (it seeds
        // the abstraction / metric tables that are
        // already a no-op when clustered) and
        // idempotent on a warm DB.
        PreTraining::run(&client).await;
        Self {
            solver: rbp_nlhe::hydrate_flagship3(client.clone()).await,
            client,
        }
    }
}

#[async_trait::async_trait]
impl Trainer for Fast3Session {
    fn client(&self) -> &Arc<Client> {
        &self.client
    }
    async fn step(&mut self) {
        self.solver.step();
    }
    async fn epoch(&self) -> usize {
        self.solver.profile().epochs()
    }
    async fn checkpoint(&self) -> Option<String> {
        self.solver.profile().metrics().and_then(|m| m.checkpoint())
    }
    async fn summary(&self) -> String {
        self.solver
            .profile()
            .metrics()
            .map(|m| m.summary())
            .unwrap_or_else(|| "training stopped".to_string())
    }
    async fn sync(self) {
        // The v3 sync is the v1 / v2 sync shape
        // verbatim (consume the solver, stage → write
        // → merge → stamp), but every database call
        // targets the v3 tables:
        //  - `client.stage3()` recreates the
        //    `staging_v3` `UNLOGGED` table (the v1
        //    `Stage::stage` writes the v1 `staging`
        //    table and the v2 `Stage2::stage2` writes
        //    the v2 `staging_v2` table; all three are
        //    separate physical tables).
        //  - The `COPY staging_v3 ...` header is the
        //    `NlheProfileV3::copy()` literal (the v3
        //    `BulkSchema::copy` impl).
        //  - `client.merge3()` upserts the staging
        //    rows into `BLUEPRINT3` and drops the
        //    staging table.
        //  - `client.stamp3(epochs)` adds `epochs` to
        //    the `'current_v3'` row of `EPOCH3`. The
        //    v1 `Stage::stamp` updates the v1
        //    `'current'` row, the v2 `Stage2::stamp2`
        //    updates the v2 `'current_v2'` row, and
        //    the v3 `Stage3::stamp3` updates the v3
        //    `'current_v3'` row, so a v1 / v2 / v3
        //    sync cannot collide on the epoch row.
        let client = self.client;
        let epochs = self.solver.profile.epochs();
        let profile = self.solver.profile;
        if let Err(e) = crate::check_integrity(&profile) {
            log::error!("integrity gate failed (v3): {}", e);
            std::process::exit(2);
        }
        client.stage3().await;
        let writer = BinaryCopyInWriter::new(
            client
                .copy_in(NlheProfileV3::copy())
                .await
                .expect("copy_in v3"),
            NlheProfileV3::columns(),
        );
        futures::pin_mut!(writer);
        for row in profile.rows() {
            row.write(writer.as_mut()).await;
        }
        writer.finish().await.expect("finish v3 stream");
        client.merge3().await;
        client.stamp3(epochs).await;
        log::info!("profile v3 sync complete (epoch {})", epochs);
    }
}
