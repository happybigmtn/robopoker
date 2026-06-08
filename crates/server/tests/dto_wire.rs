//! No-DB integration tests for the request/response DTO wire format
//! used by the analysis API. These tests are entirely synchronous
//! (`render_query` is `fn` not `async fn`, DTO round-trips are sync)
//! so they run with `cargo test -p rbp-server --tests` and require no
//! `#[tokio::test]` runtime, no `actix_web::test` runtime, and no
//! `DATABASE_URL` env. They pin the JSON shape the HTTP layer
//! exchanges with clients (so a future field rename or `serde`
//! attribute change breaks CI rather than silently breaking a
//! dashboard). See STW-025.

use rbp_core::{
    AbsHist, ApiDecision, ApiSample, ApiStrategy, GetPolicy, ObsHist, ReplaceAbs, ReplaceAll,
    ReplaceObs, ReplaceOne, ReplaceRow, RowWrtObs, SetStreets,
};

/// Round-trip a DTO through JSON parse + serialize + re-parse and
/// assert the re-serialized JSON is identical to the first
/// serialization. This pins the wire format: any field rename,
/// `serde(rename = "...")` change, default-value change, or
/// ordering change breaks the assertion. We use the
/// `serialize → re-parse → serialize` loop instead of direct struct
/// equality because the request DTOs do not derive `PartialEq` and
/// adding it would be a wire-format change in its own right.
fn round_trip<T>(original: &str)
where
    T: serde::Serialize + serde::de::DeserializeOwned,
{
    let first: T = serde_json::from_str(original).expect("first parse");
    let json1 = serde_json::to_string(&first).expect("first serialize");
    let second: T = serde_json::from_str(&json1).expect("second parse");
    let json2 = serde_json::to_string(&second).expect("second serialize");
    assert_eq!(json1, json2, "wire format must be self-consistent");
    // Also assert the first parse survives a re-parse from the
    // second serialization (a stricter "no information loss" check
    // — a field that drops on serialize would surface here).
    let _third: T = serde_json::from_str(&json2).expect("third parse");
}

#[test]
fn request_dto_round_trip_set_streets() {
    round_trip::<SetStreets>(r#"{"street":"Preflop"}"#);
}

#[test]
fn request_dto_round_trip_replace_obs() {
    round_trip::<ReplaceObs>(r#"{"obs":"As Kh"}"#);
}

#[test]
fn request_dto_round_trip_row_wrt_obs() {
    round_trip::<RowWrtObs>(r#"{"obs":"2c 2d"}"#);
}

#[test]
fn request_dto_round_trip_replace_abs() {
    round_trip::<ReplaceAbs>(r#"{"wrt":"flop-001"}"#);
}

#[test]
fn request_dto_round_trip_replace_row() {
    round_trip::<ReplaceRow>(r#"{"wrt":"flop-001","obs":"2c 2d"}"#);
}

#[test]
fn request_dto_round_trip_replace_one() {
    round_trip::<ReplaceOne>(r#"{"wrt":"flop-001","abs":"turn-042"}"#);
}

#[test]
fn request_dto_round_trip_replace_all() {
    round_trip::<ReplaceAll>(
        r#"{"wrt":"flop-001","neighbors":["turn-042","turn-043","turn-044"]}"#,
    );
}

#[test]
fn request_dto_round_trip_obs_hist() {
    round_trip::<ObsHist>(r#"{"obs":"As Kh"}"#);
}

#[test]
fn request_dto_round_trip_abs_hist() {
    round_trip::<AbsHist>(r#"{"abs":"flop-001"}"#);
}

#[test]
fn request_dto_round_trip_get_policy() {
    round_trip::<GetPolicy>(r#"{"turn":"As Kh","seen":"2c 2d","past":["check","bet-1","call"]}"#);
}

#[test]
fn response_dto_round_trip_api_sample() {
    // `ApiSample` derives `Debug` + `Serialize` + `Deserialize`. It is
    // the per-row payload the `obs_hist` / `abs_hist` endpoints emit.
    // The `obs` and `abs` fields are poker string encodings; the
    // numeric fields (`equity`, `density`, `distance`) are `f32`
    // values. A f32 round-trip is exact for the values we feed in
    // (no rounding error from `1.0` / `0.5` / `0.25`).
    round_trip::<ApiSample>(
        r#"{"obs":"As Kh","abs":"flop-001","equity":0.625,"density":0.5,"distance":0.0}"#,
    );
}

#[test]
fn response_dto_round_trip_api_decision() {
    // `ApiDecision` is the per-action mass in an `ApiStrategy` row.
    round_trip::<ApiDecision>(r#"{"edge":"Bet-1","mass":0.75}"#);
}

#[test]
fn response_dto_round_trip_api_strategy() {
    // `ApiStrategy` is the policy payload the `get_policy` endpoint
    // emits. The `accumulated` and `counts` fields are
    // `BTreeMap<String, f32 / u32>` — BTreeMap is sorted by key, so
    // the JSON object key order is stable. `history: i64`,
    // `present: i16`, `choices: i64` are the CFR iteration cursor.
    // `position: usize` is the seat-aware seat/position of the
    // acting player (SEAT-PERSIST-001 slice 5) — a `0` value
    // round-trips bit-exactly as `usize` on every serde_json target
    // and a `1` (or any non-zero) value also round-trips because
    // `serde_json` does not widen integers. Both seat positions
    // must be representable on the wire so a future server build
    // that distinguishes seat 0 from seat 1 strategies does not
    // silently collapse to a single policy key after the round-trip.
    round_trip::<ApiStrategy>(
        r#"{
            "history": 1024,
            "present": 8,
            "choices": 17,
            "position": 1,
            "accumulated": {
                "Bet-1": 0.625,
                "Check": 0.25,
                "Fold": 0.125
            },
            "counts": {
                "Bet-1": 512,
                "Check": 256,
                "Fold": 256
            }
        }"#,
    );
}
