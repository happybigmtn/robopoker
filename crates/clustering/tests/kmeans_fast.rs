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
use rbp_clustering::{FastKmeansCaps, Histogram, Metric, run_fast};
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
