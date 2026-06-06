//! End-to-end persistence test: two policies with same (subgame,
//! bucket, choices) but different positions must NOT collapse.
//!
//! This is the slice-3 gate that proves the DEFAULT-0 collapse is
//! gone through the real read/write path.
//!
//! Requires `DB_URL` to be set; short-circuits with a skip notice
//! when unset, following the `crates/gameroom/tests/hand_roundtrip.rs`
//! pattern.

use rbp_cards::Street;
use rbp_gameplay::{Abstraction, Edge, Path};
use rbp_nlhe::{NlheInfo, Record, Sink, Source};

/// Build two deterministic `NlheInfo`s that share every key field
/// except position.
fn infos() -> (NlheInfo, NlheInfo) {
    let subgame = Path::default();
    let bucket = Abstraction::from(Street::Pref);
    let choices = Path::default();
    let info0 = NlheInfo::from((subgame, bucket, choices, 0usize));
    let info1 = NlheInfo::from((subgame, bucket, choices, 1usize));
    // They must differ only in position; everything else matches.
    assert_eq!(info0.subgame(), info1.subgame());
    assert_eq!(info0.bucket(), info1.bucket());
    assert_eq!(info0.choices(), info1.choices());
    assert_ne!(info0.position(), info1.position());
    (info0, info1)
}

/// Build a `Record` from an info set with a deterministic edge and
/// weight so the read-back assertions are stable.
fn record(info: NlheInfo, weight: f32) -> Record {
    let edge = Edge::Call;
    Record {
        info,
        edge,
        weight,
        regret: 0.1,
        evalue: 0.2,
        counts: 1,
    }
}

#[tokio::test]
async fn position_persistence_roundtrip() {
    let url = match std::env::var("DB_URL") {
        Ok(u) => u,
        Err(_) => {
            eprintln!("DB_URL not set; skipping position_persistence_roundtrip");
            return;
        }
    };

    let tls = tokio_postgres::tls::NoTls;
    let (client, connection) = tokio_postgres::connect(&url, tls)
        .await
        .expect("database connection failed");
    tokio::spawn(connection);

    // Use a temporary table so we do not touch production blueprint data.
    client
        .execute(
            "DROP TABLE IF EXISTS blueprint",
            &[],
        )
        .await
        .expect("drop temp blueprint");

    client
        .execute(
            "CREATE TEMP TABLE blueprint (
                edge       SMALLINT NOT NULL,
                past       BIGINT,
                present    SMALLINT,
                choices    BIGINT,
                position   SMALLINT,
                weight     REAL,
                regret     REAL,
                evalue     REAL,
                counts     INT DEFAULT 0,
                UNIQUE     (past, present, choices, position, edge)
            )",
            &[],
        )
        .await
        .expect("create temp blueprint");

    // Add the upsert index that Source::strategy relies on for fast
    // lookups (not strictly required for correctness, but matches prod).
    client
        .execute(
            "CREATE UNIQUE INDEX idx_blueprint_upsert ON blueprint (present, past, choices, position, edge)",
            &[],
        )
        .await
        .expect("create upsert index");

    let (info0, info1) = infos();
    let rec0 = record(info0.clone(), 0.30);
    let rec1 = record(info1.clone(), 0.70);

    // Write both records.
    Sink::submit(&client, vec![rec0, rec1]).await;

    // Read back position 0.
    let strat0 = Source::strategy(&client, info0).await;
    assert_eq!(strat0.len(), 1, "position 0 must have exactly one edge");
    let (edge0, weight0) = strat0[0];
    assert_eq!(edge0, Edge::Call);
    assert!((weight0 - 0.30).abs() < 1e-6, "position 0 weight must be 0.30, got {weight0}");

    // Read back position 1.
    let strat1 = Source::strategy(&client, info1).await;
    assert_eq!(strat1.len(), 1, "position 1 must have exactly one edge");
    let (edge1, weight1) = strat1[0];
    assert_eq!(edge1, Edge::Call);
    assert!((weight1 - 0.70).abs() < 1e-6, "position 1 weight must be 0.70, got {weight1}");

    // Verify the rows are physically distinct in the table.
    let rows = client
        .query(
            "SELECT past, present, choices, position, edge, weight FROM blueprint ORDER BY position",
            &[],
        )
        .await
        .expect("select all");
    assert_eq!(rows.len(), 2, "must be two distinct rows in the table");
    assert_eq!(rows[0].get::<_, i16>(3), info0.position() as i16);
    assert_eq!(rows[1].get::<_, i16>(3), info1.position() as i16);
}
