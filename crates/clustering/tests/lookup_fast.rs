//! STW-091: fast-mode `Layer::lookup` sample cap regression
//! test.
//!
//! Pins the `RBP_TESTNET_FAST=1`-aware lookup prefix cap the
//! `Layer::lookup` path takes on a fresh DB. The test is no-DB
//! (it drives the `Layer::lookup` / `Layer::lookup_with_prefix`
//! entry points with a synthetic `Layer` built from random
//! `Histogram` points via the test-only
//! `Layer::synthetic_for_test` constructor) so it runs in the
//! default `cargo test -p rbp-clustering` invocation without a
//! Postgres sidecar.
//!
//! ## What the test pins
//!
//! 1. `lookup_fast_caps_sample_at_1024` — `Layer::lookup` on a
//!    synthetic 4K-row `Layer<K=4, N=4096>` for `Street::Flop`
//!    returns a `Lookup` of size 1024 (the cap is honored; the
//!    full N=4096 is not iterated) in under 2 s wall-clock
//!    (the same budget the `kmeans_fast.rs` sub-tests use;
//!    a regression to the full N path or a regression that
//!    bypasses the cap would either blow the budget or panic
//!    the rayon `par_iter` on a 1.3M-row pool in production).
//!
//! 2. `lookup_production_unchanged_when_fast_unset` — the
//!    `testnet_fast()` gate is `false` when `RBP_TESTNET_FAST`
//!    is unset (or set to any value other than `1`); the
//!    lookup cap helper returns the full N. A regression that
//!    accidentally caps production lookup fails here (the
//!    test drives the cap helper directly + asserts it returns
//!    N when the switch is unset).
//!
//! 3. `lookup_does_not_panic_on_underdetermined_input` —
//!    `Layer::lookup_with_prefix(0)` on a synthetic `Layer`
//!    returns a `Lookup` (potentially empty) without
//!    panicking on the BTreeMap insert. The lookup
//!    construction's `(0..prefix).into_par_iter().map(|i|
//!    self.neighbor(i))` path is guarded by a `prefix.min(N)`
//!    clamp; an out-of-range prefix must not panic.
//!
//! ## Background
//!
//! The 2026-06-10 11:20 runbook run
//! (`receipts/testnet-live-proof-20260610T112026Z/cluster/stdout.txt`)
//! captured a hang at `calculating lookup flop` AFTER kmeans
//! completed: STW-077's kmeans cap sub-sampled the kmeans
//! input to 1024 points, but the lookup construction
//! iterates the full `N = N_FLOP = 1_286_792` flop
//! isomorphism space (and `N = N_TURN = 13_960_050` for
//! turn) via `(0..N).into_par_iter()`. The kmeans cap was
//! the right *kind* of fix but the wrong *layer* — the
//! wall-clock is being spent building the lookup, not
//! running kmeans. STW-091 caps the lookup construction the
//! same way STW-077 caps the kmeans driver.

use rbp_cards::{Observation, Street};
use rbp_clustering::{Histogram, Layer, Lookup, Metric};
use rbp_core::{
    FAST_KMEANS_SAMPLE_DEFAULT, FAST_LOOKUP_SAMPLE_DEFAULT, fast_lookup_sample, testnet_fast,
};
use std::time::Instant;

/// Per-test input scale. Small enough to run in under 1 s on
/// a CI box; large enough that the sub-sample cap actually
/// matters (a 4-row input would not exercise the cap). 4096
/// mirrors the test-local `pub const FAST_TEST_FLOP_N: usize
/// = 4096; const FAST_TEST_FLOP_K: usize = 4;` shape the
/// STW-091 spec calls out.
const N: usize = 4096;
/// K (cluster count) for the test. The compile-time const
/// mirrors the production `KMEANS_FLOP_CLUSTER_COUNT` = 128
/// (we use 4 to keep the test fast — 4 is the smallest K
/// that exercises the kmeans++ init loop more than once).
const K: usize = 4;
/// Street the test runs on. Flop has `N = N_FLOP =
/// 1_286_792` in production — the 1024-row cap is a ~1256x
/// reduction the test pins. The metric the lookup drives
/// is a Turn-metric (Flop histograms are over Turn
/// abstractions; `Metric::emd` dispatches to Sinkhorn on
/// the `source.peek().street()` arm of the metric).
const TEST_STREET: Street = Street::Flop;
/// Wall-clock budget for the fast-mode `Layer::lookup` at
/// `N=4096 + K=4 + cap=1024`. 2 s is loose (the actual run
/// is sub-second on a debug build) but a 4x slack factor
/// absorbs CI noise.
const FAST_WALLCLOCK_BUDGET: std::time::Duration = std::time::Duration::from_secs(2);

/// Per-process serial env-mutation lock. The
/// `lookup_*` tests mutate process-global env vars;
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

/// Build a synthetic 4K-row `Layer<K, N>` for the test
/// street, with random non-empty Histograms + a small
/// K-sized kmeans centroid pool seeded from the same random
/// point pool. Mirrors the fixture pattern the existing
/// `tests.rs::TestLayer::new` uses — each point is a random
/// `Histogram` derived from a fresh deal, which gives the
/// `Layer::neighbor` EMD computation a non-degenerate
/// input to chew on.
fn synthetic_layer() -> Layer<K, N> {
    // The Metric + Histogram streets for a Flop layer are
    // Turn (per the production `build` constructor at
    // `layer.rs:420-430`: metric is the next-street
    // metric, kmeans is `Histogram::empty(street.next())`).
    // However, `Histogram::from(Observation::from(Street))`
    // is only defined for `Street::Turn` (it produces a
    // Rive histogram). To build a non-degenerate synthetic
    // Layer in a no-DB test we use a Rive metric + Rive
    // histograms (the kmeans + points shapes are
    // well-defined and the `Metric::emd` River arm —
    // `Equity::variation` — does NOT panic on a
    // zero-distance table). The test exercises the
    // `Street::Flop | Street::Turn` lookup arm (the cap
    // gate), not the metric correctness (a metric
    // correctness regression fails the existing
    // `kmeans_fast.rs` sub-tests).
    let metric = Metric::new(Street::Rive);
    // Build 4096 random non-empty Rive histograms. The
    // random draws come from `Observation::from(Turn)`,
    // which mirrors the production hydration step. Each
    // point is guaranteed non-empty (`Histogram::from`
    // reads the deal's children into the bin array; a
    // random Turn deal has at least one child River with
    // non-zero count).
    let points: Box<[Histogram; N]> = (0..N)
        .map(|_| Histogram::from(Observation::from(Street::Turn)))
        .collect::<Vec<_>>()
        .try_into()
        .expect("N");
    // Build K=4 kmeans centroids from a small prefix of
    // the same point pool. The centroids are non-empty
    // (they are slices of the same non-empty point pool);
    // the `neighbor` function's EMD computation reads
    // `source.peek()` (bins.rs:95) which panics on empty
    // support — keeping the centroids non-empty keeps the
    // EMD call well-defined.
    let kmeans: Box<[Histogram; K]> = points[..K].to_vec().try_into().expect("K");
    assert_eq!(points.len(), N);
    assert_eq!(kmeans.len(), K);
    assert!(points.iter().all(|h| h.n() > 0));
    assert!(kmeans.iter().all(|h| h.n() > 0));
    Layer::synthetic_for_test(TEST_STREET, points, kmeans, metric)
}

#[test]
fn lookup_fast_caps_sample_at_1024() {
    let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Clear any leaked state from sibling tests, then set
    // the spec's override knob.
    unsetenv("RBP_TESTNET_FAST");
    unsetenv("RBP_FAST_LOOKUP_SAMPLE");
    setenv("RBP_TESTNET_FAST", "1");
    setenv("RBP_FAST_LOOKUP_SAMPLE", "1024");
    assert!(
        testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=1` must activate fast mode (the lookup cap gate)"
    );
    assert_eq!(
        fast_lookup_sample(),
        Some(1024),
        "STW-091: `RBP_FAST_LOOKUP_SAMPLE=1024` must parse to Some(1024)"
    );
    // Build the synthetic 4K-row Layer and drive `lookup`
    // (the production entry point). The cap resolves to
    // `min(N, RBP_FAST_LOOKUP_SAMPLE) = min(4096, 1024) =
    // 1024`; the lookup prefix is `cap.min(N) = 1024`; the
    // returned `Lookup` should have 1024 entries.
    let layer = synthetic_layer();
    let start = Instant::now();
    let lookup: Lookup = layer.lookup();
    let elapsed = start.elapsed();
    let lookup_size = Into::<std::collections::BTreeMap<_, _>>::into(lookup).len();
    assert_eq!(
        lookup_size, 1024,
        "STW-091: `Layer::lookup` on a 4K-row Flop Layer in fast mode must return a \
         Lookup of size 1024 (the cap is honored; the full N=4096 is not iterated). A \
         regression to the pre-fix full-N path would return a Lookup of size {N}; a \
         regression that bypasses the cap would return a different size. Got \
         {lookup_size}."
    );
    assert!(
        elapsed < FAST_WALLCLOCK_BUDGET,
        "STW-091: fast-mode `Layer::lookup` on N={N} + K={K} + cap=1024 must complete \
         in under {:?}; took {elapsed:?}. A regression to the full 1.3M-row production \
         path is the most likely wall-clock cause on production-sized inputs.",
        FAST_WALLCLOCK_BUDGET
    );
    unsetenv("RBP_TESTNET_FAST");
    unsetenv("RBP_FAST_LOOKUP_SAMPLE");
}

#[test]
fn lookup_production_unchanged_when_fast_unset() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    // The `RBP_TESTNET_FAST` switch is the gate the
    // `Layer::lookup_prefix` fast-mode path checks. The
    // helper returns `true` *only* when the env var
    // equals `1` (whitespace trimmed); a fat-fingered
    // `true` / `yes` / `on` value does NOT activate
    // fast mode (the gating is intentionally strict so a
    // worker cannot silently cap production lookup by
    // typo).
    unsetenv("RBP_TESTNET_FAST");
    setenv("RBP_TESTNET_FAST", "");
    assert!(
        !testnet_fast(),
        "STW-091: empty `RBP_TESTNET_FAST` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "0");
    assert!(
        !testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=0` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "true");
    assert!(
        !testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=true` must NOT activate fast mode (only the \
         exact string `1` is honored; a worker who fat-fingers the flag must not \
         silently cap production lookup)"
    );
    setenv("RBP_TESTNET_FAST", "yes");
    assert!(
        !testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=yes` must NOT activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", "1");
    assert!(
        testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=1` MUST activate fast mode"
    );
    setenv("RBP_TESTNET_FAST", " 1 ");
    assert!(
        testnet_fast(),
        "STW-091: `RBP_TESTNET_FAST=' 1 '` (whitespace-trimmed) MUST activate fast mode"
    );
    unsetenv("RBP_TESTNET_FAST");
    // The lookup-specific env knob is read-only when fast
    // mode is active; the test pins the env-var-read
    // helper in isolation (a regression that flips
    // `Some(_)` to `None` for a valid env var would
    // surface here).
    setenv("RBP_FAST_LOOKUP_SAMPLE", "2048");
    assert_eq!(
        fast_lookup_sample(),
        Some(2048),
        "STW-091: `RBP_FAST_LOOKUP_SAMPLE=2048` must parse to Some(2048)"
    );
    setenv("RBP_FAST_LOOKUP_SAMPLE", "0");
    assert_eq!(
        fast_lookup_sample(),
        None,
        "STW-091: `RBP_FAST_LOOKUP_SAMPLE=0` must parse to None (the helper \
         filters `> 0` so a worker who sets it to 0 does not crash the lookup \
         construction on an empty input pool)"
    );
    setenv("RBP_FAST_LOOKUP_SAMPLE", "not-a-number");
    assert_eq!(
        fast_lookup_sample(),
        None,
        "STW-091: non-numeric `RBP_FAST_LOOKUP_SAMPLE` must parse to None"
    );
    unsetenv("RBP_FAST_LOOKUP_SAMPLE");
    // Pin the default values the spec promises (1024) so
    // a future refactor that bumps the defaults fails CI
    // before it reaches a worker.
    assert_eq!(
        FAST_LOOKUP_SAMPLE_DEFAULT, 1024,
        "STW-091: FAST_LOOKUP_SAMPLE_DEFAULT must remain 1024 per the spec"
    );
    // Pin the parity with the kmeans cap (both default
    // to 1024 — the lookup cap and the kmeans cap are
    // structurally parallel).
    assert_eq!(
        FAST_LOOKUP_SAMPLE_DEFAULT, FAST_KMEANS_SAMPLE_DEFAULT,
        "STW-091: FAST_LOOKUP_SAMPLE_DEFAULT ({}) must match \
         FAST_KMEANS_SAMPLE_DEFAULT ({}) per the spec parity contract",
        FAST_LOOKUP_SAMPLE_DEFAULT, FAST_KMEANS_SAMPLE_DEFAULT
    );
    // Drive the lookup with the switch unset and assert
    // the production path returns the full N. A
    // regression that accidentally caps production
    // lookup (e.g. an unconditional `prefix = 1024`
    // without the gate) fails here.
    let layer = synthetic_layer();
    let lookup: Lookup = layer.lookup();
    let lookup_size = Into::<std::collections::BTreeMap<_, _>>::into(lookup).len();
    assert_eq!(
        lookup_size, N,
        "STW-091: `Layer::lookup` with `RBP_TESTNET_FAST` unset must return a \
         Lookup of size N={N} (the production path is byte-identical when the \
         switch is unset). Got {lookup_size}; a regression that caps production \
         lookup would return < N."
    );
}

#[test]
fn lookup_does_not_panic_on_underdetermined_input() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    unsetenv("RBP_TESTNET_FAST");
    setenv("RBP_TESTNET_FAST", "1");
    setenv("RBP_FAST_LOOKUP_SAMPLE", "1024");
    let layer = synthetic_layer();
    // Drive the lookup with a prefix larger than the
    // actual point count (N=4096) and a prefix of 0.
    // Both must return a Lookup without panicking on the
    // BTreeMap insert or the `prefix.min(N)` clamp.
    let lookup_zero: Lookup = layer.lookup_with_prefix(0);
    let lookup_size_zero = Into::<std::collections::BTreeMap<_, _>>::into(lookup_zero).len();
    assert_eq!(
        lookup_size_zero, 0,
        "STW-091: `Layer::lookup_with_prefix(0)` must return an empty Lookup \
         (the prefix is clamped to `min(N, 0) = 0`; the par_iter does not run; \
         the BTreeMap insert does not fire). Got size {lookup_size_zero}."
    );
    // A prefix larger than N clamps to N. The lookup
    // construction does not allocate a N+1 array; the
    // `prefix.min(N)` clamp in `lookup_with_prefix` is
    // the safety net.
    let lookup_huge: Lookup = layer.lookup_with_prefix(usize::MAX);
    let lookup_size_huge = Into::<std::collections::BTreeMap<_, _>>::into(lookup_huge).len();
    assert_eq!(
        lookup_size_huge, N,
        "STW-091: `Layer::lookup_with_prefix(usize::MAX)` must clamp to N={N} \
         (the prefix is clamped to `min(N, usize::MAX) = N`). Got size \
         {lookup_size_huge}; a regression that drops the `prefix.min(N)` \
         clamp would either panic on the par_iter or return a different size."
    );
    unsetenv("RBP_TESTNET_FAST");
    unsetenv("RBP_FAST_LOOKUP_SAMPLE");
}
