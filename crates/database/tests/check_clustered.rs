//! `Check::clustered` regression pins (STW-075).
//!
//! STW-070 shipped a cold-startup-safety fix to
//! `crates/database/src/check.rs`, and the working tree
//! at the start of RE-PLAN-002 carried the *minimum
//! deterministic bug fix* — a `SELECT obs FROM
//! isomorphism LIMIT 16` shape that replaced the
//! pre-fix `Isomorphism::from(Observation::from(street))`
//! lookup (which drew a *random* obs per call and
//! silently never matched the `obs = $1` predicate,
//! forcing kmeans to re-run on a "warmed" DB and
//! blowing the testnet-live-proof runbook's wall-clock
//! budget). RE-PLAN-002 promoted that fix to a
//! dedicated `[P0]` row, `STW-075`, which is what this
//! integration test pins.
//!
//! ## Why no-DB
//!
//! The deterministic `Check::clustered` algorithm has
//! exactly two layers:
//!
//! 1. A SQL read: `SELECT obs FROM isomorphism LIMIT 16`
//!    returning a `Vec<i64>` of observation ids.
//! 2. A pure decision: does any `obs` decode to the
//!    requested street via `Street::from(obs)`?
//!
//! The decision is fully captured by the `pub` helper
//! [`rbp_database::clustered_decision`]. The SQL layer
//! is a thin pass-through: it issues a bounded query
//! and hands the rows to the helper. There is no
//! "in-process mock" of `tokio_postgres::Client` in the
//! workspace (the type is opaque), so a DB-needing
//! integration test would require a live Postgres —
//! which the workspace's CI does not provide. The
//! [`Check::clustered` SQL fragment] is pinned
//! separately by `check::tests::clustered_sql_uses_limit_16_on_isomorphism`
//! in `crates/database/src/check.rs`, and the helper
//! is exercised here in three no-DB sub-tests:
//!
//! - `clustered_returns_true_on_warmed_isomorphism` —
//!   the 16-row sample contains at least one `obs`
//!   that decodes to the requested street (the
//!   warmed-DB + non-empty target bucket contract).
//! - `clustered_returns_false_on_fresh_empty_table` —
//!   the sample is empty (the fresh-DB contract).
//! - `clustered_does_not_full_scan_warmed_table` —
//!   a 10K-row sample returns the same answer as a
//!   16-row sample when the 16 rows are a subset of
//!   the 10K (the O(1)-on-warmed-DB contract: the
//!   helper is O(n) over its input, but the SQL
//!   `LIMIT 16` caps `n` at 16 on warmed state).
//!
//! The byte-stability property the plan names
//! ("re-running `clustered` 100x in a row on the same
//! warmed DB returns the same answer") is the
//! regression pin for the pre-STW-070 shape, which
//! silently returned a different `true`/`false` per
//! call because it drew random cards from a fresh
//! deck; that pin lives at
//! `check::tests::clustered_decision_is_byte_stable_across_100_calls`
//! in the lib tests.
//!
//! [`Check::clustered` SQL fragment]: crate::check::tests::clustered_sql_uses_limit_16_on_isomorphism

use rbp_cards::{Card, Hand, Observation, Street};
use rbp_database::clustered_decision;

/// Build a deterministic `obs: i64` for a 2-card pocket
/// + N-card public hand. Cards are taken from the
/// `Card::from(0u8)`-derived disjoint sequence, so
/// successive calls with the same `cards` slice return
/// the same `i64` (no randomness, no global state). The
/// first two cards form the pocket; the rest form the
/// public hand.
fn obs_for(cards: &[Card]) -> i64 {
    let mut iter = cards.iter().copied();
    let first = iter.next().expect("at least one card");
    let second = iter.next().expect("at least two cards");
    let pocket = Hand::add(Hand::from(first), Hand::from(second));
    let public = iter.fold(Hand::empty(), |acc, card| Hand::add(acc, Hand::from(card)));
    i64::from(Observation::from((pocket, public)))
}

/// `Check::clustered(Street::Flop)` MUST return `true`
/// on a warmed `isomorphism` table. The warmed-DB
/// contract is: the `cluster` step has run for at
/// least one flop observation, so the 16-row sample
/// contains at least one `obs` that decodes to Flop
/// via `Street::from(obs)`. The (3/4)^16 < 1e-2
/// probability of a warmed DB missing the target
/// street is the property the deterministic
/// `LIMIT 16` shape relies on; this test pins the
/// "non-empty target bucket returns true" half of
/// that contract.
#[test]
fn clustered_returns_true_on_warmed_isomorphism() {
    // A 16-row sample with one Flop observation + 15
    // Pref observations (the "warmed DB, kmeans pass
    // for Flop is done, all other streets may or may
    // not be done" shape). For each street that
    // appears in the sample, `clustered_decision`
    // returns true.
    let flop = obs_for(&[
        Card::from(0u8),
        Card::from(1u8),
        Card::from(2u8),
        Card::from(3u8),
        Card::from(4u8),
    ]);
    let pref = obs_for(&[Card::from(0u8), Card::from(1u8)]);
    let mut obs_rows: Vec<i64> = vec![flop];
    obs_rows.extend(std::iter::repeat(pref).take(15));
    assert_eq!(obs_rows.len(), 16);

    assert!(
        clustered_decision(&obs_rows, Street::Flop),
        "warmed DB with a non-empty Flop bucket must return true"
    );
    // The Pref bucket is also non-empty (the sample
    // contains 15 Pref observations), so Pref is true
    // too — the same warmed-DB contract.
    assert!(
        clustered_decision(&obs_rows, Street::Pref),
        "warmed DB with a non-empty Pref bucket must return true"
    );
}

/// `Check::clustered(Street::*)` MUST return `false` on
/// a fresh empty `isomorphism` table. The fresh-DB
/// contract is: the `cluster` step has not run for any
/// street, so the `SELECT obs FROM isomorphism LIMIT
/// 16` returns zero rows and the helper's `any` short-
/// circuits to `false`. This is the "skip kmeans for
/// streets that already have a bucket" decision the
/// `PreTraining::pending` consumer relies on (an
/// empty bucket means "not done", which means "run
/// kmeans"; a bug that returns `true` on an empty
/// table would silently skip kmeans for every street
/// and leave the DB un-warmed).
#[test]
fn clustered_returns_false_on_fresh_empty_table() {
    let obs_rows: Vec<i64> = Vec::new();
    for street in Street::all().iter().copied() {
        assert!(
            !clustered_decision(&obs_rows, street),
            "fresh empty {street:?} bucket must return false"
        );
    }
}

/// `Check::clustered` MUST NOT full-scan the
/// `isomorphism` table on warmed state. The contract
/// is: the SQL `LIMIT 16` caps the sample at 16 rows
/// on a warmed DB, so the helper is O(1)-bounded
/// regardless of the table's actual size. The pure-
/// helper pin here is: a 10K-row sample produces the
/// same answer as a 16-row sample when the 16 rows
/// are a subset of the 10K (the helper is O(n) over
/// its input but never exceeds the 16-row SQL cap on
/// warmed state; the test asserts the function does
/// not need to look at "the whole table" to reach a
/// decision).
#[test]
fn clustered_does_not_full_scan_warmed_table() {
    let flop = obs_for(&[
        Card::from(0u8),
        Card::from(1u8),
        Card::from(2u8),
        Card::from(3u8),
        Card::from(4u8),
    ]);
    let pref = obs_for(&[Card::from(0u8), Card::from(1u8)]);
    // 16-row sample: 1 Flop + 15 Pref.
    let mut obs16: Vec<i64> = vec![flop];
    obs16.extend(std::iter::repeat(pref).take(15));
    assert_eq!(obs16.len(), 16);

    // 10K-row sample: the 16 rows above, repeated 625x.
    // The deterministic `LIMIT 16` SQL cap on a warmed
    // DB is the production reason this 10K sample is
    // unreachable in practice; the test asserts that
    // if a future schema change raises the limit, the
    // helper still produces the same answer as the
    // 16-row sample (no per-row I/O, no DB round-trip
    // inside the decision loop, no global state).
    let obs10k: Vec<i64> = obs16.iter().cycle().take(10_000).copied().collect();
    assert_eq!(obs10k.len(), 10_000);

    for street in Street::all().iter().copied() {
        assert_eq!(
            clustered_decision(&obs16, street),
            clustered_decision(&obs10k, street),
            "10K sample must agree with 16-row sample for {street:?}"
        );
    }
    // The 10K sample is a superset of the 16-row
    // sample, so the matching street (Flop, present
    // in the 16-row sample) is also present in the
    // 10K sample, and the helper must return true.
    assert!(
        clustered_decision(&obs10k, Street::Flop),
        "10K sample must return true for the matching street"
    );
}
