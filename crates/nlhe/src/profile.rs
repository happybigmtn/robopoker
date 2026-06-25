use super::*;
use rbp_core::Probability;
use rbp_core::Utility;
use rbp_mccfr::*;
use std::collections::BTreeMap;

/// Profile storing accumulated strategies, regrets, and expected values for NLHE.
///
/// Maintains a nested map: `Info → NlheEdge → Encounter`.
/// The outer key is the information set, inner key is the action,
/// and [`Encounter`] contains cumulative strategy weight, regret, and EV.
///
/// # Iteration Tracking
///
/// `iterations` tracks the current training epoch. The walker (traversing
/// player) alternates each iteration: even=player0, odd=player1.
///
/// # Expected Value Storage
///
/// The `evalue` field in each [`Encounter`] accumulates counterfactual action values,
/// enabling depth-limited search and safe subgame solving. These values are
/// weighted by iteration (matching the policy weighting scheme).
///
/// # Database Persistence
///
/// With the `database` feature, supports loading/saving to PostgreSQL
/// via [`Hydrate`] and [`Schema`] implementations.
#[derive(Default)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NlheProfile {
    /// Current training iteration (epoch).
    pub iterations: usize,
    /// Nested map: Info → NlheEdge → Encounter (weight, regret, evalue).
    pub encounters: BTreeMap<NlheInfo, BTreeMap<NlheEdge, Encounter>>,
    /// Training metrics collector.
    #[cfg_attr(feature = "serde", serde(skip))]
    pub metrics: Metrics,
}

impl Profile for NlheProfile {
    type T = NlheTurn;
    type E = NlheEdge;
    type G = NlheGame;
    type I = NlheInfo;

    fn increment(&mut self) {
        self.iterations += 1;
    }
    fn walker(&self) -> Self::T {
        NlheTurn::from(self.epochs() % 2)
    }
    fn epochs(&self) -> usize {
        self.iterations
    }
    fn metrics(&self) -> Option<&Metrics> {
        Some(&self.metrics)
    }
    fn cum_weight(&self, info: &Self::I, edge: &Self::E) -> Probability {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|e| e.weight)
            .unwrap_or_default()
    }
    fn cum_regret(&self, info: &Self::I, edge: &Self::E) -> Utility {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|e| e.regret)
            .unwrap_or_else(|| edge.default_regret())
    }
    fn cum_evalue(&self, info: &Self::I, edge: &Self::E) -> Utility {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|e| e.evalue)
            .unwrap_or_default()
    }
    fn cum_counts(&self, info: &Self::I, edge: &Self::E) -> u32 {
        self.encounters
            .get(info)
            .and_then(|memory| memory.get(edge))
            .map(|e| e.counts)
            .unwrap_or_default()
    }
}

#[cfg(feature = "database")]
impl rbp_database::Schema for NlheProfile {
    fn name() -> &'static str {
        rbp_database::BLUEPRINT
    }
    fn creates() -> &'static str {
        const_format::concatcp!(
            "CREATE TABLE IF NOT EXISTS ",
            rbp_database::BLUEPRINT,
            " (
                edge       BIGINT,
                past       BIGINT,
                present    SMALLINT,
                choices    BIGINT,
                position   SMALLINT,
                weight     REAL,
                regret     REAL,
                evalue     REAL,
                counts     INT DEFAULT 0,
                UNIQUE     (past, present, choices, position, edge)
            );"
        )
    }
    fn indices() -> &'static str {
        const_format::concatcp!(
            "CREATE UNIQUE INDEX IF NOT EXISTS idx_blueprint_upsert  ON ",
            rbp_database::BLUEPRINT,
            " (present, past, choices, position, edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_bucket  ON ",
            rbp_database::BLUEPRINT,
            " (present, past, choices);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_present ON ",
            rbp_database::BLUEPRINT,
            " (present);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_edge    ON ",
            rbp_database::BLUEPRINT,
            " (edge);
             CREATE        INDEX IF NOT EXISTS idx_blueprint_past    ON ",
            rbp_database::BLUEPRINT,
            " (past);"
        )
    }
    fn truncates() -> &'static str {
        const_format::concatcp!("TRUNCATE TABLE ", rbp_database::BLUEPRINT, ";")
    }
    fn freeze() -> &'static str {
        const_format::concatcp!(
            "ALTER TABLE ",
            rbp_database::BLUEPRINT,
            " SET (fillfactor = 100);
            ALTER TABLE ",
            rbp_database::BLUEPRINT,
            " SET (autovacuum_enabled = false);"
        )
    }
}

#[cfg(feature = "database")]
impl rbp_database::BulkSchema for NlheProfile {
    fn columns() -> &'static [tokio_postgres::types::Type] {
        &[
            tokio_postgres::types::Type::INT8,   // past (subgame path)
            tokio_postgres::types::Type::INT2,   // present (abstraction bucket)
            tokio_postgres::types::Type::INT8,   // choices (available edges)
            tokio_postgres::types::Type::INT2,   // position (acting player)
            tokio_postgres::types::Type::INT8,   // edge (action taken)
            tokio_postgres::types::Type::FLOAT4, // weight
            tokio_postgres::types::Type::FLOAT4, // regret
            tokio_postgres::types::Type::FLOAT4, // evalue
            tokio_postgres::types::Type::INT4,   // counts
        ]
    }
    fn copy() -> &'static str {
        const_format::concatcp!(
            "COPY ",
            rbp_database::BLUEPRINT,
            " (past, present, choices, position, edge, weight, regret, evalue, counts) FROM STDIN BINARY"
        )
    }
}

#[cfg(feature = "database")]
#[async_trait::async_trait]
impl rbp_database::Hydrate for NlheProfile {
    async fn hydrate(client: std::sync::Arc<tokio_postgres::Client>) -> Self {
        log::info!("{:<32}{:<32}", "loading blueprint", "from database");
        const EPOCH_SQL: &str = const_format::concatcp!(
            "SELECT value FROM ",
            rbp_database::EPOCH,
            " WHERE key = 'current'"
        );
        let iterations = client
            .query_opt(EPOCH_SQL, &[])
            .await
            .ok()
            .flatten()
            .map(|r| r.get::<_, i64>(0) as usize)
            .expect("to have already created epoch metadata");
        const BLUEPRINT_SQL: &str = const_format::concatcp!(
            "SELECT past, present, choices, position, edge, weight, regret, evalue, counts FROM ",
            rbp_database::BLUEPRINT
        );
        let mut encounters = BTreeMap::new();
        for row in client
            .query(BLUEPRINT_SQL, &[])
            .await
            .expect("to have already created blueprint")
        {
            let subgame = rbp_gameplay::Path::from(row.get::<_, i64>(0) as u64);
            let present = rbp_gameplay::Abstraction::from(row.get::<_, i16>(1));
            let choices = rbp_gameplay::Path::from(row.get::<_, i64>(2) as u64);
            let position = row.get::<_, i16>(3) as usize;
            let edge = NlheEdge::from(row.get::<_, i64>(4) as u64);
            let weight = row.get::<_, f32>(5);
            let regret = row.get::<_, f32>(6);
            let evalue = row.get::<_, f32>(7);
            let counts = row.get::<_, i32>(8) as u32;
            let bucket = NlheInfo::from((subgame, present, choices, position));
            encounters
                .entry(bucket)
                .or_insert_with(BTreeMap::default)
                .entry(edge)
                .or_insert(Encounter::new(weight, regret, evalue, counts));
        }
        log::info!(
            "{:<32}{:<32}",
            format!("{} infos", encounters.len()),
            "from database"
        );
        log::info!(
            "{:<32}{:<32}",
            format!("{} iters", iterations),
            "from database"
        );
        Self {
            iterations,
            encounters,
            metrics: Metrics::with_epoch(iterations),
        }
    }
}

impl NlheProfile {
    pub fn rows(self) -> impl Iterator<Item = (i64, i16, i64, i16, i64, f32, f32, f32, i32)> {
        self.encounters.into_iter().flat_map(|(info, edges)| {
            let subgame = i64::from(info.subgame());
            let present = i16::from(info.bucket());
            let choices = i64::from(info.choices());
            let position = info.position() as i16;
            edges.into_iter().map(move |(edge, encounter)| {
                (
                    subgame,
                    present,
                    choices,
                    position,
                    u64::from(edge) as i64,
                    encounter.weight,
                    encounter.regret,
                    encounter.evalue,
                    encounter.counts as i32,
                )
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rbp_cards::*;
    use rbp_gameplay::*;
    use rbp_mccfr::Encounter;

    #[cfg(feature = "database")]
    use rbp_database::{BulkSchema, Schema};

    #[test]
    fn position_roundtrip_through_rows() {
        let mut profile = NlheProfile::default();

        let subgame = Path::default();
        let bucket = NlheSecret::from(Abstraction::from(Street::Pref));
        let choices = Path::default();

        // Two info sets with identical subgame, bucket, choices but different positions
        let info0 = NlheInfo::from((subgame, bucket.into(), choices, 0));
        let info1 = NlheInfo::from((subgame, bucket.into(), choices, 1));

        let edge = NlheEdge::from(Edge::Call);

        // Different weights for each position
        profile
            .encounters
            .entry(info0)
            .or_default()
            .insert(edge, Encounter::new(0.3, 0.1, 0.2, 1));
        profile
            .encounters
            .entry(info1)
            .or_default()
            .insert(edge, Encounter::new(0.7, 0.2, 0.3, 2));

        let rows: Vec<_> = profile.rows().collect();

        // Must produce 2 distinct rows (not collapsed into 1)
        assert_eq!(
            rows.len(),
            2,
            "position difference must produce distinct rows"
        );

        let row0 = rows.iter().find(|r| r.3 == 0).expect("row for position 0");
        let row1 = rows.iter().find(|r| r.3 == 1).expect("row for position 1");

        // row shape: (past, present, choices, position, edge, weight, regret, evalue, counts)
        assert_eq!(row0.3, 0);
        assert_eq!(row0.5, 0.3); // weight
        assert_eq!(row0.8, 1); // counts

        assert_eq!(row1.3, 1);
        assert_eq!(row1.5, 0.7); // weight
        assert_eq!(row1.8, 2); // counts
    }

    #[test]
    #[cfg(feature = "database")]
    fn nlhe_profile_creates_includes_position() {
        let sql = NlheProfile::creates();
        assert!(
            sql.contains("position"),
            "creates() must include position column; got: {sql}"
        );
        assert!(
            sql.contains("UNIQUE     (past, present, choices, position, edge)"),
            "creates() must include position in UNIQUE constraint; got: {sql}"
        );
    }

    #[test]
    #[cfg(feature = "database")]
    fn nlhe_profile_indices_include_position() {
        let sql = NlheProfile::indices();
        assert!(
            sql.contains("position"),
            "indices() must include position in upsert index; got: {sql}"
        );
    }

    #[test]
    #[cfg(feature = "database")]
    fn nlhe_profile_copy_includes_position() {
        let sql = NlheProfile::copy();
        assert!(
            sql.contains("position"),
            "copy() must include position column; got: {sql}"
        );
    }
}
