//! Pretraining - hierarchical clustering pipeline for poker abstractions.
//!
//! Manages clustering from scratch to postgres without disk I/O:
//! 1. River: equity-based abstractions (computed from scratch)
//! 2. Turn: k-means on river distributions (hydrates river data)
//! 3. Flop: k-means on turn distributions (hydrates turn data)
//! 4. Preflop: 1:1 isomorphism enumeration (computed from scratch)
use rbp_cards::*;
use rbp_clustering::*;
use rbp_database::*;
use rbp_gameplay::*;
use rbp_nlhe::EpochMetaV2;
use rbp_nlhe::EpochMetaV3;
use rbp_nlhe::NlheProfile;
use rbp_nlhe::NlheProfileV2;
use rbp_nlhe::NlheProfileV3;
use std::sync::Arc;
use tokio_postgres::Client;

use crate::EpochMeta;

type PrefLayer = Layer<{ Street::Pref.k() }, { Street::Pref.n_isomorphisms() }>;
type FlopLayer = Layer<{ Street::Flop.k() }, { Street::Flop.n_isomorphisms() }>;
type TurnLayer = Layer<{ Street::Turn.k() }, { Street::Turn.n_isomorphisms() }>;

/// Zero-sized orchestrator for the clustering pipeline.
/// Encapsulates all clustering logic so Trainer stays clean.
pub struct PreTraining;

impl PreTraining {
    /// Run the complete clustering pipeline if needed.
    /// Always runs finalize to ensure derived tables exist.
    pub async fn run(client: &Arc<Client>) {
        let streets = Self::pending(client).await;
        for street in streets.iter().cloned() {
            log::info!("{:<32}{:<32}", "beginning clustering", street);
            Self::cluster(street, client).await.stream(client).await;
        }
        if streets.len() > 0 {
            Self::index(client).await;
        }
        Self::derive::<Abstraction>(client).await;
        Self::derive::<Street>(client).await;
        // The blueprint and epoch tables are created lazily by
        // the `Schema::creates()` DDL on first use. A fresh
        // operator environment (or a CI worker that just stood
        // up Postgres) lands here with neither table present, so
        // the very first `trainer --fast` / `--smoke` call would
        // panic on `truncate blueprint` (in `--reset`) or on
        // `CREATE UNLOGGED TABLE staging (LIKE blueprint)` (in
        // `Stage::stage`). Bootstrapping the tables here keeps
        // the `--reset` / `--smoke` paths idempotent without
        // forcing the operator to run a separate `init.sql`
        // script before the first training run.
        Self::ensure::<NlheProfile>(client).await;
        Self::ensure::<EpochMeta>(client).await;
        // STW-017: bootstrap the v2 trained-config tables
        // (`blueprint_v2` / `epoch_v2`) the same way the
        // v1 tables are bootstrapped. A fresh DB that
        // has never seen a `--fast2` / `--reset` run
        // needs the v2 tables present before the first
        // `Fast2Session::sync` (which would otherwise
        // crash on `CREATE UNLOGGED TABLE staging_v2
        // (LIKE blueprint_v2)` against a missing
        // `blueprint_v2`) and before the first
        // `Mode::Reset` v2 arm (which would otherwise
        // crash on `truncate blueprint_v2`). The
        // `EpochMetaV2::creates()` DDL also seeds the
        // `'current_v2'` row to 0 in an
        // `ON CONFLICT DO NOTHING` block, so a v1
        // reset that doesn't touch the v2 row leaves
        // the v2 row intact.
        Self::ensure::<NlheProfileV2>(client).await;
        Self::ensure::<EpochMetaV2>(client).await;
        // STW-029: bootstrap the v3 trained-config tables
        // (`blueprint_v3` / `epoch_v3`) the same way the
        // v1 / v2 tables are bootstrapped. A fresh DB
        // that has never seen a `--fast3` / `--reset`
        // run needs the v3 tables present before the
        // first `Fast3Session::sync` (which would
        // otherwise crash on `CREATE UNLOGGED TABLE
        // staging_v3 (LIKE blueprint_v3)` against a
        // missing `blueprint_v3`) and before the first
        // `Mode::Reset` v3 arm (which would otherwise
        // crash on `truncate blueprint_v3`). The
        // `EpochMetaV3::creates()` DDL also seeds the
        // `'current_v3'` row to 0 in an `ON CONFLICT
        // DO NOTHING` block, so a v1 / v2 reset that
        // doesn't touch the v3 row leaves the v3 row
        // intact.
        Self::ensure::<NlheProfileV3>(client).await;
        Self::ensure::<EpochMetaV3>(client).await;
        log::info!("{:<32}{:<32}", "vacuum analyze", "all tables");
        client
            .batch_execute("VACUUM ANALYZE;")
            .await
            .expect("vacuum analyze");
    }

    /// Idempotently create a [`Schema`]'s table. The same DDL
    /// is exposed by [`Streamable::finalize`] for the clustering
    /// tables, but `finalize` also rebuilds indices and applies
    /// the `freeze` settings — those are unnecessary on a
    /// never-populated blueprint/epoch table and would just
    /// churn the catalog. This helper runs `creates()` (which is
    /// `CREATE TABLE IF NOT EXISTS`) and leaves the rest of the
    /// lifecycle to the train loop.
    async fn ensure<S>(client: &Arc<Client>)
    where
        S: Schema,
    {
        let absent = client
            .query(
                &format!(
                    "SELECT 1 FROM information_schema.tables WHERE table_name = '{}'",
                    S::name()
                ),
                &[],
            )
            .await
            .map(|rows| rows.is_empty())
            .unwrap_or(true);
        if absent {
            log::info!("{:<32}{:<32}", "creating table", S::name());
            client.batch_execute(S::creates()).await.expect("creates");
        } else {
            log::info!("{:<32}{:<32}", "table already exists", S::name());
        }
    }

    /// Cluster a street via k-means. Dependencies loaded from postgres.
    /// Dispatches to the appropriate const-generic Layer based on street.
    async fn cluster(street: Street, client: &Arc<Client>) -> Artifacts {
        match street {
            Street::Rive => Artifacts::from(Lookup::grow(street)),
            Street::Turn => TurnLayer::cluster(street, client).await,
            Street::Flop => FlopLayer::cluster(street, client).await,
            Street::Pref => PrefLayer::cluster(street, client).await,
        }
    }

    /// Collect unclustered streets in reverse order (river first).
    async fn pending(client: &Arc<Client>) -> Vec<Street> {
        let mut pending = Vec::new();
        for street in Street::all().iter().rev().cloned() {
            if client.clustered(street).await {
                log::info!("{:<32}{:<32}", "skipping clustering", street);
            } else {
                pending.push(street);
            }
        }
        pending
    }

    /// Prepare tables for streaming (truncate if needed).
    #[allow(unused)]
    async fn truncate(client: &Arc<Client>) {
        client
            .batch_execute(&Metric::truncates())
            .await
            .expect("truncate table metric");
        client
            .batch_execute(&Future::truncates())
            .await
            .expect("truncate table transitions");
        client
            .batch_execute(&Lookup::truncates())
            .await
            .expect("truncate table isomorphism");
    }

    /// Index tables after data is streamed.
    async fn index(client: &Arc<Client>) {
        Lookup::finalize(client).await;
        Metric::finalize(client).await;
        Future::finalize(client).await;
    }

    /// Derive a table from existing data using SQL functions.
    async fn derive<D>(client: &Arc<Client>)
    where
        D: Derive,
    {
        let absent = client
            .query(
                &format!(
                    "SELECT 1 FROM information_schema.tables WHERE table_name = '{}'",
                    D::name()
                ),
                &[],
            )
            .await
            .map(|rows| rows.is_empty())
            .unwrap_or(true);
        if absent {
            log::info!("{:<32}{:<32}", "creating table", D::name());
            client.batch_execute(D::creates()).await.expect("creates");
        }
        if client
            .query(&format!("SELECT 1 FROM {} LIMIT 1 ", D::name()), &[])
            .await
            .map(|rows| rows.is_empty())
            .unwrap_or(true)
        {
            log::info!("{:<32}{:<32}", "deriving table", D::name());
            client.batch_execute(&D::derives()).await.expect("derives");
            log::info!("{:<32}{:<32}", "indexing table", D::name());
            client.batch_execute(D::indices()).await.expect("indices");
            log::info!("{:<32}{:<32}", "freezing table", D::name());
            client.batch_execute(D::freeze()).await.expect("freeze");
        } else {
            log::info!("{:<32}{:<32}", "table already derived", D::name());
        }
    }
}
