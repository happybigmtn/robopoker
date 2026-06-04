//! STW-017: fast in-memory training session for the v2
//! trained config (`Flagship2`).
//!
//! Parallels [`super::FastSession`] (the v1 fast session
//! that trains [`rbp_nlhe::Flagship`]) but targets the v2
//! tables ([`rbp_database::BLUEPRINT2`] /
//! [`rbp_database::STAGING2`] / [`rbp_database::EPOCH2`]
//! + [`rbp_database::EPOCH2_KEY`]). The v1
//! `FastSession::sync` writes the v1 staging table + the
//! v1 `'current'` epoch row; this v2 `Fast2Session::sync`
//! writes the v2 staging table + the v2 `'current_v2'`
//! epoch row. The two sessions can run sequentially (or
//! interleaved across the same database) without
//! colliding on the staging table or the epoch counter
//! row.
//!
//! The training loop itself is identical to v1: the v2
//! difference is the regret/policy combination
//! ([`rbp_nlhe::Flagship2`] = `DiscountedRegret` +
//! `QuadraticWeight` + `PluribusSampling`), so the
//! `Trainer` trait impl is the v1 shape verbatim —
//! `step` / `epoch` / `checkpoint` / `summary` all
//! delegate to the wrapped solver, and `sync` is the
//! only method that knows it's writing v2 tables.

use crate::*;
use rbp_database::*;
use rbp_mccfr::*;
use rbp_nlhe::Flagship2;
use rbp_nlhe::NlheProfileV2;
use std::sync::Arc;
use tokio_postgres::Client;
use tokio_postgres::binary_copy::BinaryCopyInWriter;

/// STW-017: v2 trained-config fast in-memory training
/// session using `DiscountedRegret` + `QuadraticWeight` +
/// `PluribusSampling` (the [`rbp_nlhe::Flagship2`]
/// combination).
///
/// Mirrors [`FastSession`] (the v1 fast session) so a
/// `Mode::Fast2` dispatch can call
/// `Fast2Session::new(client).await.train().await` the
/// same way `Mode::Fast` calls
/// `FastSession::new(client).await.train().await`. The
/// only divergence from v1 is the table names the `sync`
/// step writes: v1 → `STAGING` / `BLUEPRINT` / `EPOCH`;
/// v2 → `STAGING2` / `BLUEPRINT2` / `EPOCH2`
/// (`'current_v2'` row).
pub struct Fast2Session {
    client: Arc<Client>,
    solver: Flagship2,
}

impl Fast2Session {
    /// Build a v2 training session against a live
    /// Postgres. The call order is the v1 order verbatim
    /// (run pretraining to ensure the v1 clustering
    /// tables exist, then hydrate the v2 solver from the
    /// v2 tables). The v2 tables
    /// (`BLUEPRINT2` / `EPOCH2`) are created lazily by
    /// the v2 `Schema::creates()` DDL; a fresh DB that
    /// has never seen a `--fast2` run lands in the
    /// `NlheProfileV2::hydrate` empty-blueprint
    /// fallback (the bench's `blueprint_trained: false`
    /// path), which is the documented post-`--reset`
    /// state.
    pub async fn new(client: Arc<Client>) -> Self {
        // `PreTraining::run` is the v1 clustering bootstrap.
        // The v2 path doesn't re-cluster (the v1 clustering
        // is what the v2 profile's info-set indexing is built
        // on); the v2 bootstrap is just creating the v2
        // tables if they don't exist. The clustering side
        // effect is harmless on a fresh DB (it seeds the
        // abstraction / metric tables that are already a no-op
        // when clustered) and idempotent on a warm DB.
        PreTraining::run(&client).await;
        Self {
            solver: rbp_nlhe::hydrate_flagship2(client.clone()).await,
            client,
        }
    }
}

#[async_trait::async_trait]
impl Trainer for Fast2Session {
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
        // The v2 sync is the v1 sync shape verbatim
        // (consume the solver, stage → write → merge →
        // stamp), but every database call targets the v2
        // tables:
        //  - `client.stage2()` recreates the
        //    `staging_v2` `UNLOGGED` table (the v1
        //    `Stage::stage` writes the v1 `staging`
        //    table, which is a different physical
        //    table; the v1 + v2 staging tables can
        //    coexist).
        //  - The `COPY staging_v2 ...` header is the
        //    `NlheProfileV2::copy()` literal (the v2
        //    `BulkSchema::copy` impl).
        //  - `client.merge2()` upserts the staging rows
        //    into `BLUEPRINT2` and drops the staging
        //    table.
        //  - `client.stamp2(epochs)` adds `epochs` to
        //    the `'current_v2'` row of `EPOCH2`. The
        //    v1 `Stage::stamp` updates the v1
        //    `'current'` row, so a v1 sync and a v2
        //    sync cannot collide on the epoch row.
        let client = self.client;
        let epochs = self.solver.profile.epochs();
        // The v2 `NlheProfileV2::rows()` is a
        // type-aliased row iterator that hands back
        // `(i64, i16, i64, i64, f32, f32, f32, i32)`
        // tuples — the v1 `NlheProfile::rows()` shape
        // verbatim. The `BinaryCopyInWriter` is
        // parameterised on `NlheProfileV2::columns()`,
        // which is the v2 `BulkSchema::columns` impl
        // (same arity + same Postgres types as the v1
        // `NlheProfile::columns()`).
        let profile = self.solver.profile;
        client.stage2().await;
        let writer = BinaryCopyInWriter::new(
            client
                .copy_in(NlheProfileV2::copy())
                .await
                .expect("copy_in v2"),
            NlheProfileV2::columns(),
        );
        futures::pin_mut!(writer);
        for row in profile.rows() {
            row.write(writer.as_mut()).await;
        }
        writer.finish().await.expect("finish v2 stream");
        client.merge2().await;
        client.stamp2(epochs).await;
        log::info!("profile v2 sync complete (epoch {})", epochs);
    }
}
