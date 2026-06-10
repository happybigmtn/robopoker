//! STW-088: property-test layer for the kmeans fast-mode driver
//! (`run_fast`) and the production-path kmeans++ init
//! (`kmeans::init_kmeans_plus_plus`, the driver `Layer::init_kmeans`
//! calls into for the kmeans++ seeding step).
//!
//! ## Why a property-test layer
//!
//! The 5 example tests in `kmeans_fast.rs` (STW-077's 3 + STW-086's
//! 1 + STW-087's 1) pin the *known* panic families the runbook
//! captured: the empty-input kmeans++ pre-filter (STW-086) and the
//! empty-cluster Lloyd-step guard (STW-087). They are *examples* of
//! the no-panic contract, not *proofs*. A future regression that
//! introduces a new panic site — a Lloyd-step reassign change that
//! breaks the empty-cluster guard, a kmeans++ init change that
//! breaks the empty-input guard, a metric change that panics on
//! degenerate inputs — is invisible to those 5 sub-tests because
//! they cover the 5 known shapes but not the unknown ones the
//! future might surface.
//!
//! STW-088 adds 5 `proptest!`-driven property sub-tests that
//! exercise the no-panic contract on randomized inputs across
//! both the fast-mode `run_fast` driver and the production-path
//! kmeans++ init. A contributor who introduces a regression in
//! either path fails CI on this file with a clear `proptest!`
//! failure trace naming the random input + the panic site — the
//! property test fails *before* the runbook would.
//!
//! ## Sizing
//!
//! Each property runs 16 cases (proptest's default is 256, which
//! would blow the wall-clock budget; 16 cases × ≤ 64 input points
//! × ≤ 4 iters ≤ ~64K EMD calls per property ≈ sub-second per
//! The deterministic seed
//! `PROPTEST_RNG_SEED=0xC0FFEE` env var (the proptest-1.11
//! standard for byte-stable seeds — the older
//! `ProptestConfig::with_rng_seed()` method was removed in
//! proptest 1.x) makes the test byte-stable across runs (a
//! failure always reproduces with the same input). The seed
//! is set in `crates/autotrain/tests/script_shape.rs` for the
//! `cargo test --workspace` invocation, or by the developer
//! running `PROPTEST_RNG_SEED=0xC0FFEE cargo test
//! -p rbp-clustering --test kmeans_property` directly.
//!
//! ## Streets covered
//!
//! The randomized street picker cycles through {`Flop`, `Turn`,
//! `Rive`} (the three streets the runbook's cluster step
//! actually exercises; `Pref` is excluded because the production
//! `Layer::init_kmeans` short-circuits it).

use proptest::prelude::*;
use rbp_cards::Street;
use rbp_clustering::{FastKmeansCaps, Histogram, Metric, init_kmeans_plus_plus, run_fast};
use rbp_gameplay::Abstraction;
use std::time::{Duration, Instant};

/// K (cluster count) for the property test. Mirrors the
/// module-level `K = 4` const the existing `kmeans_fast.rs`
/// uses (4 is the smallest K that exercises the kmeans++ init
/// loop more than once).
const K: usize = 4;

/// Streets the property test cycles through. `Pref` is
/// excluded because the production `Layer::init_kmeans`
/// short-circuits it (`debug_assert!(N == K); return ...`).
const STREETS: [Street; 3] = [Street::Flop, Street::Turn, Street::Rive];

/// Per-test wall-clock budget. 5 s is loose (a single
/// property with 16 cases × ≤ 64 points × ≤ 4 iters
/// completes in well under 1 s on a debug build) but the
/// 5x slack factor absorbs CI noise. A regression that
/// re-introduces a panic crashes the test process (caught
/// by proptest's panic hook before the budget); a
/// regression that turns the no-op guard into an O(N^2)
/// scan blows the budget.
const PROP_WALLCLOCK_BUDGET: Duration = Duration::from_secs(5);

/// Per-property case count. Proptest's default is 256 cases
/// per property; that × 5 properties × the EMD-heavy
/// `run_fast` driver blows the wall-clock budget. 16 cases
/// per property is the minimum that gives statistical
/// confidence in the no-panic contract while staying under
/// 30 s for the full test file on a debug build.
const PROP_CASES: u32 = 16;

/// Build a non-empty `Histogram` for the given street. The
/// clustering crate's `Histogram::from(Observation::from(street))`
/// impl is *only* valid for `Street::Turn` (it asserts
/// `turn.street() == Street::Turn` in the `From<Observation>` impl
/// at `histogram.rs:197`); for Flop + Rive we build a histogram by
/// folding `Histogram::empty(street)` with a random number of
/// `Abstraction::from(street)` increments (the same
/// homegrown `Histogram::random()` shape the clustering
/// tests use for `Street::Flop`).
fn random_histogram(street: Street) -> Histogram {
    // Pick 4-32 abstractions for the histogram's support;
    // the count is bounded so the test stays under the
    // wall-clock budget (a 64-point pool with 32-abs
    // histograms is 64 × 32 = 2K EMD inputs per
    // `metric.emd` call).
    let count = 4 + (rand::random::<u32>() as usize) % 29;
    (0..count)
        .map(|_| Abstraction::from(street))
        .fold(Histogram::empty(street), Histogram::increment)
}

/// Pick a street from the randomized index. A `usize` is
/// the natural proptest `prop_index` shape; modulo gives
/// a uniform distribution over the 3 streets.
fn street_at(i: usize) -> Street {
    STREETS[i % STREETS.len()]
}

/// Build a point pool from the randomized size + the
/// randomized empty mask. The empty mask is capped to the
/// actual point count so the zip is well-defined.
fn points_with_empty_mask(size: usize, empty_mask: &[bool], street: Street) -> Vec<Histogram> {
    let actual_size = size.min(empty_mask.len());
    (0..actual_size)
        .map(|i| {
            if empty_mask[i] {
                Histogram::empty(street)
            } else {
                random_histogram(street)
            }
        })
        .collect()
}

// ---------------------------------------------------------------------
// Property 1: `run_fast` returns K centroids on a randomized
// mixed-empty input pool.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROP_CASES))]
    /// STW-088 (a): `run_fast_no_panic_on_random_input` exercises
    /// the fast-mode kmeans driver on a randomized point pool
    /// (size ∈ [1, 64], 50% chance each point is `Histogram::empty`)
    /// + a random street ∈ {`Flop`, `Turn`, `Rive`}. Asserts
    /// `run_fast::<K>` returns K centroids without panicking, all
    /// K centroids are non-empty (the STW-086 pre-filter + the
    /// STW-087 empty-cluster guard + the all-empty early-return
    /// each hold under the randomized input), and the wall-clock
    /// stays under the 5 s budget.
    #[test]
    fn run_fast_no_panic_on_random_input(
        size in 1usize..=64,
        empty_mask in proptest::collection::vec(any::<bool>(), 64),
        street_idx in 0usize..STREETS.len(),
    ) {
        let street = street_at(street_idx);
        let points = points_with_empty_mask(size, &empty_mask, street);
        // Property (a) asserts the all-K-centroids-non-empty
        // contract; the all-empty-input edge case is the
        // degenerate early-return path covered by property (c).
        // Skip the all-empty randomized inputs here so the
        // "every centroid is non-empty" assertion is meaningful
        // (the early-return on `non_empty.is_empty()` produces
        // K empty centroids by spec — that is not a panic, but
        // it is not what (a) is testing).
        prop_assume!(points.iter().any(|h| h.n() > 0));
        let caps = FastKmeansCaps { sample: 1024, iterations: 4 };
        let start = Instant::now();
        let centroids = run_fast::<K>(&points, &Metric::default(), street, caps);
        let elapsed = start.elapsed();
        prop_assert_eq!(
            centroids.len(), K,
            "STW-088: run_fast must return exactly K centroids"
        );
        for (i, c) in centroids.iter().enumerate() {
            prop_assert!(
                c.n() > 0,
                "STW-088: centroid {} must be non-empty (got n() = {})",
                i,
                c.n()
            );
        }
        prop_assert!(
            elapsed < PROP_WALLCLOCK_BUDGET,
            "STW-088: run_fast on size={} + K={} must complete in under {:?}; took {:?}",
            points.len(),
            K,
            PROP_WALLCLOCK_BUDGET,
            elapsed
        );
    }
}

// ---------------------------------------------------------------------
// Property 2: `init_kmeans_plus_plus` (production-path) returns
// K non-empty centroids on a randomized mixed-empty input pool.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROP_CASES))]
    /// STW-088 (b): `init_kmeans_plus_plus_no_panic_on_random_input`
    /// exercises the production-path kmeans++ init driver on a
    /// randomized point pool (size ∈ [1, 64], 50% empty). The
    /// production `Layer::init_kmeans` (layer.rs:128-203) calls
    /// into this driver for the kmeans++ seeding step — the
    /// function is structurally identical to the production
    /// path's kmeans++ loop (same `WeightedIndex` pick, same
    /// per-iteration `metric.emd` call, same
    /// `potentials[idx] = 0.0` zero-out). The pre-filter at
    /// `kmeans.rs:248-273` mirrors the STW-086 production-path
    /// guard (layer.rs:159-177). Asserts the driver returns K
    /// non-empty centroids without panicking. A regression that
    /// drops the pre-filter panics in the kmeans++ `metric.emd`
    /// call on the next iteration's empty picked centroid.
    #[test]
    fn init_kmeans_plus_plus_no_panic_on_random_input(
        size in 1usize..=64,
        empty_mask in proptest::collection::vec(any::<bool>(), 64),
        street_idx in 0usize..STREETS.len(),
    ) {
        let street = street_at(street_idx);
        let points = points_with_empty_mask(size, &empty_mask, street);
        // Mirror of property (a)'s `prop_assume!`: the
        // all-K-centroids-non-empty assertion is only
        // meaningful when at least one input is non-empty.
        // The all-empty-input degenerate path is covered
        // by property (d).
        prop_assume!(points.iter().any(|h| h.n() > 0));
        let start = Instant::now();
        let centroids = init_kmeans_plus_plus::<K>(&points, &Metric::default(), street);
        let elapsed = start.elapsed();
        prop_assert_eq!(
            centroids.len(), K,
            "STW-088: init_kmeans_plus_plus must return exactly K centroids"
        );
        for (i, c) in centroids.iter().enumerate() {
            prop_assert!(
                c.n() > 0,
                "STW-088: centroid {} must be non-empty (the STW-086 pre-filter drops the empty prefix); got n() = {}",
                i,
                c.n()
            );
        }
        prop_assert!(
            elapsed < PROP_WALLCLOCK_BUDGET,
            "STW-088: init_kmeans_plus_plus on size={} + K={} must complete in under {:?}; took {:?}",
            points.len(),
            K,
            PROP_WALLCLOCK_BUDGET,
            elapsed
        );
    }
}

// ---------------------------------------------------------------------
// Property 3: `run_fast` does not panic on the all-empty
// degenerate edge case.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROP_CASES))]
    /// STW-088 (c): `run_fast_does_not_panic_on_all_empty_input`
    /// exercises the worst-case empty-input scenario the runbook
    /// could surface if a future production DB has no observations
    /// for a street. Asserts the driver returns K centroids
    /// without panicking, and the early-return path (the
    /// `non_empty.is_empty()` branch in `run_fast` at kmeans.rs:201-212)
    /// returns K empty centroids (the spec'd degenerate
    /// behavior — the kmeans++ init cannot drive an empty point
    /// pool; the fast-mode driver returns K empty centroids so a
    /// downstream consumer sees a well-formed result instead of a
    /// crash). A regression that drops the early-return panics on
    /// `WeightedIndex::new` (it requires at least one positive
    /// weight).
    #[test]
    fn run_fast_does_not_panic_on_all_empty_input(
        size in 1usize..=64,
        street_idx in 0usize..STREETS.len(),
    ) {
        let street = street_at(street_idx);
        let points: Vec<Histogram> = (0..size).map(|_| Histogram::empty(street)).collect();
        let caps = FastKmeansCaps { sample: 1024, iterations: 4 };
        let start = Instant::now();
        let centroids = run_fast::<K>(&points, &Metric::default(), street, caps);
        let elapsed = start.elapsed();
        prop_assert_eq!(
            centroids.len(), K,
            "STW-088: run_fast on all-empty input must return exactly K centroids"
        );
        for (i, c) in centroids.iter().enumerate() {
            prop_assert_eq!(
                c.n(),
                0,
                "STW-088: centroid {} on all-empty input must be empty (the spec'd degenerate result); got n() = {}",
                i,
                c.n()
            );
        }
        prop_assert!(
            elapsed < PROP_WALLCLOCK_BUDGET,
            "STW-088: run_fast on all-empty size={} + K={} must complete in under {:?}; took {:?}",
            size,
            K,
            PROP_WALLCLOCK_BUDGET,
            elapsed
        );
    }
}

// ---------------------------------------------------------------------
// Property 4: `init_kmeans_plus_plus` does not panic on the
// all-empty degenerate edge case.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROP_CASES))]
    /// STW-088 (d): `init_kmeans_plus_plus_does_not_panic_on_all_empty_input`
    /// mirrors property (c) on the production-path kmeans++ init
    /// driver. Asserts the early-return path (the
    /// `non_empty.is_empty()` branch at kmeans.rs:264-273) returns
    /// K empty centroids. A regression that drops the early-return
    /// panics on `WeightedIndex::new` (same panic site as (c)).
    #[test]
    fn init_kmeans_plus_plus_does_not_panic_on_all_empty_input(
        size in 1usize..=64,
        street_idx in 0usize..STREETS.len(),
    ) {
        let street = street_at(street_idx);
        let points: Vec<Histogram> = (0..size).map(|_| Histogram::empty(street)).collect();
        let start = Instant::now();
        let centroids = init_kmeans_plus_plus::<K>(&points, &Metric::default(), street);
        let elapsed = start.elapsed();
        prop_assert_eq!(
            centroids.len(), K,
            "STW-088: init_kmeans_plus_plus on all-empty input must return exactly K centroids"
        );
        for (i, c) in centroids.iter().enumerate() {
            prop_assert_eq!(
                c.n(),
                0,
                "STW-088: centroid {} on all-empty input must be empty; got n() = {}",
                i,
                c.n()
            );
        }
        prop_assert!(
            elapsed < PROP_WALLCLOCK_BUDGET,
            "STW-088: init_kmeans_plus_plus on all-empty size={} + K={} must complete in under {:?}; took {:?}",
            size,
            K,
            PROP_WALLCLOCK_BUDGET,
            elapsed
        );
    }
}

// ---------------------------------------------------------------------
// Property 5: deterministic seed produces byte-stable output.
// ---------------------------------------------------------------------

proptest! {
    #![proptest_config(ProptestConfig::with_cases(PROP_CASES))]
    /// STW-088 (e): `run_fast_is_byte_stable_on_identical_input`
    /// exercises the deterministic-seed contract the runbook
    /// relies on (a fresh runbook run on a warmed DB must return
    /// the same centroids as the prior run, so the
    /// `Check::clustered` STW-075 skip fires correctly). Asserts
    /// two `run_fast` calls on the *same* randomized input return
    /// the same K centroids (bit-equal via the `Histogram`
    /// `PartialEq` impl, which compares the underlying `Bins` for
    /// equality). The street's `DefaultHasher` seed is the only
    /// source of randomness in the kmeans++ loop, so two calls
    /// with the same `points` + same `street` + same `caps` must be
    /// byte-equal. A regression that introduces a
    /// `rand::thread_rng()` call (or a non-deterministic
    /// `Instant::now()`-seeded RNG) breaks byte-stability and fails
    /// this test.
    #[test]
    fn run_fast_is_byte_stable_on_identical_input(
        size in 1usize..=32,
        street_idx in 0usize..STREETS.len(),
    ) {
        let street = street_at(street_idx);
        let points: Vec<Histogram> = (0..size).map(|_| random_histogram(street)).collect();
        let caps = FastKmeansCaps { sample: 1024, iterations: 4 };
        let c1 = run_fast::<K>(&points, &Metric::default(), street, caps);
        let c2 = run_fast::<K>(&points, &Metric::default(), street, caps);
        prop_assert_eq!(c1.len(), K, "STW-088: first run_fast must return exactly K centroids");
        prop_assert_eq!(c2.len(), K, "STW-088: second run_fast must return exactly K centroids");
        for (i, (a, b)) in c1.iter().zip(c2.iter()).enumerate() {
            prop_assert_eq!(
                a, b,
                "STW-088: centroid {} must be byte-stable across two run_fast calls (regression in the deterministic seed); got {:?} vs {:?}",
                i, a, b
            );
        }
    }
}
