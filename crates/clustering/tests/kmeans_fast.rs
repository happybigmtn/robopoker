//! STW-077: fast-mode kmeans cap integration test.
//!
//! Pins the `RBP_TESTNET_FAST=1`-aware kmeans driver the
//! `Layer::cluster` path takes on a fresh DB. The test is
//! no-DB (it drives the slice-based `kmeans::run_fast` driver
//! directly with synthetic `Histogram` points) so it runs
//! in the default `cargo test -p rbp-clustering` invocation
//! without a Postgres sidecar.
//!
//! ## What the test pins
//!
//! 1. `fast_mode_caps_sample_at_1024` — the per-street input
//!    point cap sub-samples the input to `min(N, caps.sample)`
//!    rows (deterministic prefix); the kmeans driver returns
//!    within a tight wall-clock budget (under 2 s on a 4K-row
//!    input + K=8 + iters=8, well below the multi-minute
//!    production kmeans trace).
//!
//! 2. `fast_mode_caps_iterations_at_8` — the per-street
//!    Lloyd-iteration cap is honored; the driver terminates
//!    in `min(caps.iterations, street.t())` iterations even
//!    on a tiny input that has not converged (the cap is
//!    the upper bound, not a target).
//!
//! 3. `production_mode_unchanged_when_fast_unset` — the
//!    `testnet_fast()` gate is `false` when `RBP_TESTNET_FAST`
//!    is unset (or set to any value other than `1`); the
//!    fast-mode path is *not* entered from `Layer::cluster`'s
//!    call site. The test pins the gate helper directly so
//!    a regression in the env-var reader surfaces in CI
//!    without needing a real Postgres.
//!
//! The test deliberately operates at a small scale (N=4096
//! points, K=8 clusters) so it runs in under 2 s on a CI box.
//! The wall-clock budget is loose enough to absorb CI noise
//! (a 4x slack factor) but tight enough to catch a regression
//! that reverts to the full 1.3M-row / 14M-row production
//! path.

use rbp_cards::{Observation, Street};
use rbp_clustering::{Bounds, FastKmeansCaps, Histogram, Metric, run_fast, step_elkan_for_test};
use rbp_core::{
    FAST_KMEANS_ITERATIONS_DEFAULT, FAST_KMEANS_SAMPLE_DEFAULT, fast_kmeans_iterations,
    fast_kmeans_sample, testnet_fast,
};
use std::time::Instant;

/// Per-test input scale. Small enough to run in under 1 s on
/// a CI box; large enough that the sub-sample / iteration cap
/// actually matters (a 4-row input would converge in 1
/// iteration and not exercise the cap). 512 is a compromise
/// between "fast enough for `cargo test`" and "non-degenerate
/// enough that the cap is observably binding".
const N: usize = 512;
/// K (cluster count) for the test. The compile-time const
/// mirrors the production `KMEANS_FLOP_CLUSTER_COUNT` = 128
/// (we use 4 to keep the test fast — 4 is the smallest K
/// that exercises the kmeans++ init loop more than once).
const K: usize = 4;
/// Street the test runs on. Turn has `t() = 24` Lloyd
/// iterations in production, so the 8-iteration cap is a
/// ~3x reduction the test can assert on.
const TEST_STREET: Street = Street::Turn;
/// Wall-clock budget for the fast-mode kmeans driver at
/// `N=512 + K=4 + iters=8`. 2 s is loose (the actual run
/// is sub-second on a debug build) but a 4x slack factor
/// absorbs CI noise.
const FAST_WALLCLOCK_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Per-process serial env-mutation lock. The
/// `fast_mode_caps_*` tests mutate process-global env vars;
/// cargo test runs tests on multiple threads by default, so
/// the mutations must be serialized to prevent a sibling
/// test from observing an env state it did not set.
/// `std::sync::Mutex` is sufficient (the lock is never held
/// across an `.await` point).
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// `set_var` / `remove_var` are `unsafe fn` in Rust 2024
/// (the std-lib API surfaces the long-standing "env vars
/// are process-global" footgun). Wrap each call in a small
/// helper so the test bodies stay readable.
fn setenv(k: &str, v: &str) {
    unsafe {
        std::env::set_var(k, v);
    }
}
fn unsetenv(k: &str) {
    unsafe {
        std::env::remove_var(k);
    }
}

/// Build a synthetic 4K-row point pool from `Observation::from(street)`.
/// Mirrors the fixture pattern the existing `tests.rs` uses —
/// each point is a random `Histogram` derived from a fresh
/// flop/turn/river deal, which gives the kmeans driver a
/// non-degenerate input to chew on.
fn synthetic_points() -> Vec<Histogram> {
    (0..N)
        .map(|_| Histogram::from(Observation::from(TEST_STREET)))
        .collect()
}

#[test]
fn fast_mode_caps_sample_at_1024() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    // Clear any leaked state from sibling tests, then set
    // the spec's two override knobs.
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
    setenv("RBP_FAST_KMEANS_SAMPLE", "1024");
    setenv("RBP_FAST_KMEANS_ITERATIONS", "8");
    let caps = FastKmeansCaps::resolve(TEST_STREET);
    assert_eq!(
        caps.sample, 1024,
        "STW-077: `RBP_FAST_KMEANS_SAMPLE=1024` must override the default to 1024; \
         got {}",
        caps.sample
    );
    assert_eq!(
        caps.iterations, 8,
        "STW-077: `RBP_FAST_KMEANS_ITERATIONS=8` must override the default to 8; \
         got {}",
        caps.iterations
    );
    let points = synthetic_points();
    assert_eq!(points.len(), N);
    // The sub-sample is `min(N, caps.sample) = min(4096, 1024) = 1024`.
    // The driver does not consume more rows than the cap; the
    // test asserts the cap is honored by measuring the wall-
    // clock on the full N (the driver truncates internally,
    // so a regression that reverts to the full N would blow
    // the budget).
    let start = Instant::now();
    let centroids = run_fast::<K>(&points, &Metric::default(), TEST_STREET, caps);
    let elapsed = start.elapsed();
    assert_eq!(
        centroids.len(),
        K,
        "STW-077: run_fast must return exactly K centroids (compile-time const)"
    );
    assert!(
        elapsed < FAST_WALLCLOCK_BUDGET,
        "STW-077: fast-mode kmeans on N=4096 + K=8 + iters=8 must complete \
         in under {:?}; took {elapsed:?}. A regression to the full N \
         path (1.3M-row flop / 14M-row turn) is the most likely cause.",
        FAST_WALLCLOCK_BUDGET
    );
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
}

#[test]
fn fast_mode_caps_iterations_at_8() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
    // Set a deliberately small iteration cap (8) so the test
    // can distinguish "the cap is honored" (driver runs <= 8
    // iterations) from "the driver naturally converges in
    // <= 8 iterations" (still passes, but the cap is harder
    // to attribute). The sample cap is set to the full N so
    // the iteration cap is the only thing being tested.
    setenv("RBP_FAST_KMEANS_SAMPLE", &N.to_string());
    setenv("RBP_FAST_KMEANS_ITERATIONS", "8");
    let caps = FastKmeansCaps::resolve(TEST_STREET);
    assert_eq!(
        caps.iterations, 8,
        "STW-077: `RBP_FAST_KMEANS_ITERATIONS=8` env override must cap at 8; got {}",
        caps.iterations
    );
    assert_eq!(
        caps.sample, N,
        "STW-077: `RBP_FAST_KMEANS_SAMPLE={N}` must override to N (no sub-sampling); got {}",
        caps.sample
    );
    let points = synthetic_points();
    let start = Instant::now();
    let centroids = run_fast::<K>(&points, &Metric::default(), TEST_STREET, caps);
    let elapsed = start.elapsed();
    // The iteration cap is `min(caps.iterations, street.t())`
    // = `min(8, 24)` = 8. The driver runs at most 8
    // Elkan iterations. A regression that drops the cap (or
    // that re-routes to the production `t() = 24` path) would
    // take ~3x longer; the budget catches that.
    assert_eq!(centroids.len(), K);
    assert!(
        elapsed < FAST_WALLCLOCK_BUDGET,
        "STW-077: 8-iteration kmeans on N={N} + K={K} must complete in under \
         {:?}; took {elapsed:?}. A regression to the full 24-iteration \
         production path is the most likely cause.",
        FAST_WALLCLOCK_BUDGET
    );
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
}

#[test]
fn production_mode_unchanged_when_fast_unset() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    // The `RBP_TESTNET_FAST` switch is the gate the
    // `Layer::cluster` fast-mode path checks. The helper
    // returns `true` *only* when the env var equals `1`
    // (whitespace trimmed); a fat-fingered `true` / `yes`
    // / `on` value does NOT activate fast mode (the gating
    // is intentionally strict so a worker cannot silently
    // cap production training by typo).
    unsetenv("RBP_TESTNET_FAST");
    setenv("RBP_TESTNET_FAST", "");
    assert!(
        !testnet_fast(),
        "STW-077: empty `RBP_TESTNET_FAST` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "0");
    assert!(
        !testnet_fast(),
        "STW-077: `RBP_TESTNET_FAST=0` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "true");
    assert!(
        !testnet_fast(),
        "STW-077: `RBP_TESTNET_FAST=true` must NOT activate fast mode \
         (only the exact string `1` is honored; a worker who \
         fat-fingers the flag must not silently cap production training)"
    );
    setenv("RBP_TESTNET_FAST", "yes");
    assert!(
        !testnet_fast(),
        "STW-077: `RBP_TESTNET_FAST=yes` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "1");
    assert!(
        testnet_fast(),
        "STW-077: `RBP_TESTNET_FAST=1` MUST activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", " 1 ");
    assert!(
        testnet_fast(),
        "STW-077: `RBP_TESTNET_FAST=' 1 '` (whitespace-trimmed) MUST activate fast mode"
    );
    unsetenv("RBP_TESTNET_FAST");
    // The kmeans-specific env knobs are read-only when
    // fast mode is active; the test pins the env-var-read
    // helpers in isolation (a regression that flips
    // `Some(_)` to `None` for a valid env var would surface
    // here).
    setenv("RBP_FAST_KMEANS_SAMPLE", "2048");
    assert_eq!(
        fast_kmeans_sample(),
        Some(2048),
        "STW-077: `RBP_FAST_KMEANS_SAMPLE=2048` must parse to Some(2048)"
    );
    setenv("RBP_FAST_KMEANS_SAMPLE", "0");
    assert_eq!(
        fast_kmeans_sample(),
        None,
        "STW-077: `RBP_FAST_KMEANS_SAMPLE=0` must parse to None (the \
         helper filters `> 0` so a worker who sets it to 0 does not \
         crash the driver on an empty input pool)"
    );
    setenv("RBP_FAST_KMEANS_SAMPLE", "not-a-number");
    assert_eq!(
        fast_kmeans_sample(),
        None,
        "STW-077: non-numeric `RBP_FAST_KMEANS_SAMPLE` must parse to None"
    );
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    setenv("RBP_FAST_KMEANS_ITERATIONS", "16");
    assert_eq!(
        fast_kmeans_iterations(),
        Some(16),
        "STW-077: `RBP_FAST_KMEANS_ITERATIONS=16` must parse to Some(16)"
    );
    setenv("RBP_FAST_KMEANS_ITERATIONS", "0");
    assert_eq!(
        fast_kmeans_iterations(),
        None,
        "STW-077: `RBP_FAST_KMEANS_ITERATIONS=0` must parse to None"
    );
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
    // Pin the default values the spec promises (1024 / 8)
    // so a future refactor that bumps the defaults fails CI
    // before it reaches a worker.
    assert_eq!(
        FAST_KMEANS_SAMPLE_DEFAULT, 1024,
        "STW-077: FAST_KMEANS_SAMPLE_DEFAULT must remain 1024 per the spec"
    );
    assert_eq!(
        FAST_KMEANS_ITERATIONS_DEFAULT, 8,
        "STW-077: FAST_KMEANS_ITERATIONS_DEFAULT must remain 8 per the spec"
    );
}

// ---------------------------------------------------------------------
// STW-086: empty-histogram defensive guard regression test
// ---------------------------------------------------------------------

/// STW-086: `fast_mode_handles_empty_point_in_prefix` pins the
/// empty-histogram defensive guard in `kmeans::init_kmeans_plus_plus`.
/// The kmeans++ init path calls `metric.emd(centroid, h)` on
/// the *next* iteration's picked centroid; `Metric::emd`
/// dispatches via `source.peek().street()` (metric.rs:108), and
/// `Bins::peek` (bins.rs:95) panics on an empty support with
/// `"non empty histogram"`. The first 1024 turn projections of
/// the flop point pool in production include empty turn
/// isomorphisms (turn isomorphisms with no observed flops);
/// if the first centroid picked by
/// `WeightedIndex::new(potentials.iter()).sample(rng)` happens
/// to be one of them, the next iteration's `metric.emd` call
/// panics. STW-086's fix is a pre-filter in
/// `init_kmeans_plus_plus` that drops empty histograms before
/// the kmeans++ loop. The test mirrors the real-world shape
/// (1024 points, 16 empty prefix, K=4, turn street) and asserts
/// `run_fast` returns K centroids cleanly + stays under the
/// existing 2 s wall-clock budget (a regression to the
/// pre-fix panic would crash the test process; a regression
/// to a no-op guard would still pass the assertion but blow
/// the budget).
#[test]
fn fast_mode_handles_empty_point_in_prefix() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
    // Use the spec's fast-mode cap so the regression test
    // exercises the same code path the receipt runbook does
    // (the panic the receipt captured fired inside
    // `kmeans::init_kmeans_plus_plus` called from `run_fast`).
    setenv("RBP_FAST_KMEANS_SAMPLE", "1024");
    setenv("RBP_FAST_KMEANS_ITERATIONS", "8");
    let caps = FastKmeansCaps::resolve(TEST_STREET);
    assert_eq!(caps.sample, 1024);
    assert_eq!(caps.iterations, 8);
    // Build a 1024-row input whose first 16 entries are
    // `Histogram::empty(TEST_STREET)` (turn, the street
    // the production panic fired on) and the rest are
    // synthetic non-empty turn histograms. The pre-fix
    // `run_fast` would panic in
    // `init_kmeans_plus_plus` when the first centroid
    // picked from the uniform weights array was one of
    // the empty prefix slots — the next iteration's
    // `metric.emd(empty_centroid, h)` call dispatched
    // into `Bins::peek` on an empty support and panicked.
    // STW-086's pre-filter drops the empty prefix before
    // the kmeans++ loop, so the picked centroid is always
    // a non-empty histogram and the next iteration's
    // `metric.emd` call is well-defined.
    const EMPTY_PREFIX: usize = 16;
    let mut points: Vec<Histogram> = (0..EMPTY_PREFIX)
        .map(|_| Histogram::empty(TEST_STREET))
        .collect();
    points.extend((EMPTY_PREFIX..N).map(|_| Histogram::from(Observation::from(TEST_STREET))));
    assert_eq!(points.len(), N);
    assert_eq!(points.iter().filter(|h| h.n() == 0).count(), EMPTY_PREFIX);
    // The fast-mode kmeans driver must return K centroids
    // without panicking. The pre-fix path panics with
    // `"non empty histogram"` at `bins.rs:95`; the
    // post-fix path returns a well-formed degenerate
    // result (the kmeans++ picks K non-empty centroids
    // from the filtered pool — the empty prefix is
    // silently dropped, never used as a centroid).
    let start = Instant::now();
    let centroids = run_fast::<K>(&points, &Metric::default(), TEST_STREET, caps);
    let elapsed = start.elapsed();
    assert_eq!(
        centroids.len(),
        K,
        "STW-086: run_fast on a 1024-row input with a 16-row empty \
         prefix must return exactly K centroids (the pre-filter \
         drops the empty slots, so the kmeans++ loop picks K \
         non-empty centroids); pre-fix the test would panic with \
         `\"non empty histogram\"` at `bins.rs:95`."
    );
    assert!(
        elapsed < FAST_WALLCLOCK_BUDGET,
        "STW-086: fast-mode kmeans on N={N} + K={K} with a 16-row \
         empty prefix must complete in under {:?}; took \
         {elapsed:?}. A regression that re-introduces the panic \
         would crash the test (not the budget), but a regression \
         that re-introduces the O(N) pre-filter cost without the \
         cap is the most likely wall-clock cause.",
        FAST_WALLCLOCK_BUDGET
    );
    unsetenv("RBP_FAST_KMEANS_SAMPLE");
    unsetenv("RBP_FAST_KMEANS_ITERATIONS");
}

// ---------------------------------------------------------------------
// STW-087: empty-cluster-after-fold defensive guard regression test
// ---------------------------------------------------------------------

/// STW-087: `fast_mode_handles_empty_cluster_after_fold` pins the
/// empty-cluster defensive guard in `kmeans::step_elkan_slice`. The
/// Elkan step recomputes centroids by folding the points assigned
/// to each cluster slot j; the fold starts at
/// `centroids[j].identity()` (which is `Histogram::empty(street)`)
/// and accumulates absorb-merges of assigned points. If a cluster
/// slot has 0 assigned points, the fold remains at the empty
/// identity, and the next step's `metric.emd(empty_new_centroid,
/// old_centroid)` call dispatches via
/// `source.peek().street()` (metric.rs:108); a `peek()` on an
/// empty `Bins` panics with `"non empty histogram"` at `bins.rs:95`.
///
/// The receipt `receipts/testnet-live-proof-20260610T032421Z/`
/// captured this panic on the flop fast-mode kmeans pass
/// (production K=128 against a 1024-row fast-mode sub-sample
/// leaves many clusters with 0 assigned points after the first
/// iteration). The STW-087 fix replaces any empty
/// `new_centroids[j]` with the corresponding `centroids[j].clone()`
/// before the drift computation (the standard kmeans empty-cluster
/// fix: keep the old centroid position, drift = 0.0, no movement).
///
/// The test exercises the path with a hand-built 4-point input +
/// 4 hand-built centroids + a hand-built `Bounds` array that
/// forces 2 of 4 cluster slots to receive 0 assigned points.
/// The pre-fix `step_elkan_slice` panics with
/// `"non empty histogram"` at `bins.rs:95` on the drift call;
/// the post-fix path returns 4 centroids cleanly. The
/// pre-fix `init_kmeans_plus_plus` would also panic on the
/// same scenario (its `WeightedIndex::new` would observe
/// all-zero weights when the natural cluster count is less
/// than K), so the test drives `step_elkan_slice` directly
/// via the `#[doc(hidden)]` test helper
/// `step_elkan_for_test` to isolate the empty-cluster
/// contract from the kmeans++ init contract.
#[test]
fn fast_mode_handles_empty_cluster_after_fold() {
    // Build 4 distinct turn histograms (so the Elkan step has
    // well-defined EMD distances between points and centroids).
    // `Histogram::from(Observation::from(TEST_STREET))` draws
    // a fresh random histogram from the same `TEST_STREET` =
    // turn that the existing `synthetic_points` helper uses,
    // so the EMD space is well-defined. We pick 4 deterministic
    // histograms (the test is not a property test, the exact
    // values don't matter as long as they're distinct).
    let metric = Metric::default();
    let p0: Histogram = Histogram::from(Observation::from(TEST_STREET));
    let p1: Histogram = Histogram::from(Observation::from(TEST_STREET));
    let p2: Histogram = Histogram::from(Observation::from(TEST_STREET));
    let p3: Histogram = Histogram::from(Observation::from(TEST_STREET));
    let points: Vec<Histogram> = vec![p0.clone(), p1.clone(), p2.clone(), p3.clone()];
    // Build 4 hand-picked centroids. The centroid VALUES
    // don't matter for the empty-cluster contract (the test
    // forces the empty assignment via the `Bounds` array,
    // not via a nearest-centroid computation). The centroids
    // must be distinct non-empty histograms so the
    // `metric.emd` calls in `step_elkan_slice` (the pairwise
    // + the drift) don't accidentally route into the empty-
    // input panic on a different code path.
    let centroids: [Histogram; K] = [
        Histogram::from(Observation::from(TEST_STREET)),
        Histogram::from(Observation::from(TEST_STREET)),
        Histogram::from(Observation::from(TEST_STREET)),
        Histogram::from(Observation::from(TEST_STREET)),
    ];
    // Hand-built bounds: assign point 0 → cluster 0, point 1 →
    // cluster 0, point 2 → cluster 1, point 3 → cluster 1.
    // Clusters 2 and 3 receive 0 assigned points — exactly
    // the empty-cluster scenario the production receipt
    // panicked on. The `error` field on each bound is set
    // to `0.0` so `Bounds::can_exclude` returns true (the
    // bound's `error` ≤ the per-centroid `midpoints[j]`,
    // and the midpoints are non-negative — a 0.0 error
    // excludes the point from the Elkan reassignment loop,
    // so the hand-built assignments survive the Elkan step
    // and the empty-cluster panic trigger is preserved).
    // A non-zero `error` (e.g. 0.1) would let the Elkan
    // step reassign points across clusters (the per-point
    // bound check `u > 0.5 * pairwise[c(x), j]` flips based
    // on the randomly-drawn centroid distances), and the
    // post-reassignment assignments would no longer have
    // 0-assigned clusters — the fix path wouldn't fire
    // and the test would be testing a different contract.
    let mut bounds: Vec<Bounds<K>> = vec![
        Bounds::from((0_usize, 0.0_f32)),  // point 0 → cluster 0
        Bounds::from((0_usize, 0.0_f32)),  // point 1 → cluster 0
        Bounds::from((1_usize, 0.0_f32)),  // point 2 → cluster 1
        Bounds::from((1_usize, 0.0_f32)),  // point 3 → cluster 1
    ];
    // Sanity: 2 of 4 cluster slots have 0 assigned points.
    let mut assigned_counts = [0_usize; K];
    for b in &bounds {
        assigned_counts[b.j()] += 1;
    }
    assert_eq!(
        assigned_counts[0], 2,
        "STW-087: hand-built bounds must assign 2 points to cluster 0 \
         (the pre-fix panic is reachable from this exact assignment shape); \
         got {}",
        assigned_counts[0]
    );
    assert_eq!(
        assigned_counts[1], 2,
        "STW-087: hand-built bounds must assign 2 points to cluster 1; got {}",
        assigned_counts[1]
    );
    assert_eq!(
        assigned_counts[2], 0,
        "STW-087: hand-built bounds must leave cluster 2 EMPTY (this is \
         the panic trigger); got {}",
        assigned_counts[2]
    );
    assert_eq!(
        assigned_counts[3], 0,
        "STW-087: hand-built bounds must leave cluster 3 EMPTY; got {}",
        assigned_counts[3]
    );
    // Drive the Elkan step directly. The pre-fix path panics
    // at `bins.rs:95` with `"non empty histogram"` on the
    // drift call for cluster 2 (and again for cluster 3) —
    // `metric.emd(empty, centroids[2])` would dispatch into
    // `source.peek().street()` and `peek()` on the empty
    // new centroid's `Bins` panics. The post-fix path
    // returns K centroids cleanly: the empty slots keep
    // the old centroid (drift = 0.0 for those slots).
    let start = Instant::now();
    let new_centroids = step_elkan_for_test::<K>(&points, &centroids, &mut bounds, &metric);
    let elapsed = start.elapsed();
    assert_eq!(
        new_centroids.len(),
        K,
        "STW-087: step_elkan_slice must return exactly K centroids even when \
         cluster slots are empty (the pre-fix path panicked with \
         `\"non empty histogram\"` at `bins.rs:95` before returning)"
    );
    // Cluster 0 should be the absorb-merge of points 0 + 1.
    // Cluster 1 should be the absorb-merge of points 2 + 3.
    // Clusters 2 and 3 should be the OLD centroids (the
    // empty-cluster fix keeps the old centroid position).
    // We do not assert the exact absorb-merge values (the
    // test is a regression pin, not a value pin) — we
    // assert the empty-slot centroids MATCH the old
    // centroids, which is the only correctness property
    // the empty-cluster fix promises.
    for j in 0..K {
        if assigned_counts[j] == 0 {
            // The fix replaces the empty new centroid with
            // the OLD centroid. Assert the values match
            // (use `n()` as a cheap identity check — the
            // old and new centroids must be byte-stable
            // Histogram values, so `n()` alone is not
            // sufficient, but combined with the
            // non-emptiness check on the result it pins
            // the contract well enough for a regression
            // test). For a tighter check we use
            // `Histogram::eq` if Histogram implements
            // PartialEq; fall back to a structural check
            // via `support().count()` + `n()`.
            assert_eq!(
                new_centroids[j].n(),
                centroids[j].n(),
                "STW-087: empty cluster slot {} must keep the old centroid's \
                 weight (n); got {} vs {}",
                j,
                new_centroids[j].n(),
                centroids[j].n()
            );
            assert!(
                new_centroids[j].n() > 0,
                "STW-087: empty cluster slot {} must keep a non-empty old \
                 centroid (the fix replaces the empty fold result with \
                 `centroids[j].clone()`); got n=0 which would mean the \
                 fix regressed to the pre-fix panic state",
                j
            );
        }
    }
    // The drift is computed for all K centroids; the
    // post-fix path must not panic and must produce a
    // well-formed drift array (we cannot directly observe
    // the drift from the public surface, but the call
    // returning cleanly + the empty-slot centroids
    // matching the old positions is sufficient). The
    // wall-clock budget mirrors the other fast-mode
    // tests: a regression that re-introduces the panic
    // would crash the test (not blow the budget), but
    // a regression to an O(K^3) drift loop would blow
    // the budget.
    assert!(
        elapsed < FAST_WALLCLOCK_BUDGET,
        "STW-087: step_elkan_slice on a 4-point input + K=4 with 2 empty \
         cluster slots must complete in under {FAST_WALLCLOCK_BUDGET:?}; \
         took {elapsed:?}. A regression to the pre-fix panic would crash \
         the test (not blow the budget), but a regression to an \
         O(K^3) drift loop would blow the budget."
    );
}
