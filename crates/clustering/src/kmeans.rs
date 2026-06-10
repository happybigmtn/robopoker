//! Fast-mode kmeans driver (STW-077).
//!
//! The production kmeans driver in `Layer::cluster` is parameterized
//! by compile-time `K` (cluster count) and `N` (per-street isomorphism
//! count) — its iteration loop and bound initialization operate on the
//! full 1.3M-row flop / 14M-row turn point set, with 20 (flop) / 24
//! (turn) Lloyd iterations. On a *fresh* database the per-street
//! wall-clock dominates the testnet-live-proof runbook: receipts show
//! `river` kmeans running 17+ minutes before the runbook hits the
//! worker wall-clock cap. The `Check::clustered` deterministic fix
//! in STW-075 correctly *skips* kmeans on a *warmed* DB, but it
//! does not bound kmeans on a *fresh* DB. This module closes that
//! gap with a `RBP_TESTNET_FAST=1`-aware cap on the input point count
//! + the iteration count.
//!
//! ## Design
//!
//! The fast-mode driver operates on a dynamically-sized input
//! `&[Histogram]` (the production `Layer<K, N>` is fixed at
//! compile-time and cannot be re-sized at runtime), so the cap is
//! enforced *outside* the `Layer` type. `Layer::cluster` is
//! unchanged for production runs (no env knob is read unless
//! `RBP_TESTNET_FAST=1` is set); the fast-mode path:
//!
//! 1. Reads `RBP_FAST_KMEANS_SAMPLE` (default
//!    [`FAST_KMEANS_SAMPLE_DEFAULT`] = 1024) and
//!    `RBP_FAST_KMEANS_ITERATIONS` (default
//!    [`FAST_KMEANS_ITERATIONS_DEFAULT`] = 8).
//! 2. Sub-samples the input point slice to the first
//!    `min(N, sample)` points (a deterministic prefix; kmeans++
//!    uses the prefix for init + the prefix for Elkan updates).
//! 3. Caps the iteration count at `min(street.t(), iterations)` so
//!    the production default is the upper bound (the fast-mode cap
//!    never *raises* the iteration count; it only ever *lowers* it).
//!
//! The output K centroids remain compile-time const; the fast-mode
//! driver is `pub fn run_fast<const K: usize>(...)` and returns the
//! same `[Histogram; K]` shape the production `Layer::cluster`
//! produces. The downstream `lookup` / `metric` / `future`
//! post-processing is unchanged.
//!
//! ## Why an `&[Histogram]` slice and not the `Layer` type
//!
//! The `Layer<K, N>` struct stores `Box<[Histogram; N]>` for both
//! the point pool and the bound pool, so re-sizing the point pool
//! requires a new `Layer` instantiation with a different `N` —
//! which is not a runtime parameter. The clean solution is to
//! operate on the slice *before* it enters the `Layer` type. The
//! fast-mode driver in this module is a self-contained
//! `pub fn run_fast` that takes the slice, the metric, the
//! iteration cap, and the `K` compile-time const; it does not
//! touch the production `Layer::cluster` code path.

use rand::SeedableRng;
use rand::distr::Distribution;
use rand::distr::weighted::WeightedIndex;
use rand::rngs::SmallRng;
use rayon::prelude::*;
use rbp_cards::Street;
use rbp_core::{
    FAST_KMEANS_ITERATIONS_DEFAULT, FAST_KMEANS_SAMPLE_DEFAULT, fast_kmeans_iterations,
    fast_kmeans_sample,
};
use std::hash::DefaultHasher;
use std::hash::Hash;
use std::hash::Hasher;

use crate::absorb::Absorb;
use crate::bounds::Bounds;
use crate::histogram::Histogram;
use crate::metric::Metric;

/// Effective fast-mode caps resolved from the env knobs + the
/// street's production iteration count. `sample` is the per-street
/// input point cap (the kmeans driver consumes at most this many
/// rows from the input slice); `iterations` is the per-street
/// Lloyd-iteration cap (the kmeans driver runs at most this many
/// `step_elkan` updates before returning). The values honor the
/// env knobs when set, fall back to the documented defaults
/// otherwise, and never *raise* the production iteration count
/// the `Street::t()` constant returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FastKmeansCaps {
    /// Per-street input point cap. Default
    /// [`FAST_KMEANS_SAMPLE_DEFAULT`] (1024); the env var
    /// `RBP_FAST_KMEANS_SAMPLE` overrides when set to a positive
    /// integer.
    pub sample: usize,
    /// Per-street Lloyd-iteration cap. Default
    /// [`FAST_KMEANS_ITERATIONS_DEFAULT`] (8); the env var
    /// `RBP_FAST_KMEANS_ITERATIONS` overrides when set to a
    /// positive integer. The cap never raises
    /// [`Street::t()`].
    pub iterations: usize,
}

impl FastKmeansCaps {
    /// Resolve the effective fast-mode caps from the env knobs.
    /// `street` is the street the kmeans driver is running on;
    /// the effective iteration cap is
    /// `min(street.t(), env_iteration_or_default)`. The sample
    /// cap is `env_sample_or_default` (no street-dependence).
    /// The function is `const`-callable in the sense that it
    /// performs no I/O — the env reads are the only side
    /// effect, and the result is fully determined by the env
    /// state at call time.
    pub fn resolve(street: Street) -> Self {
        let sample = fast_kmeans_sample().unwrap_or(FAST_KMEANS_SAMPLE_DEFAULT);
        let env_iters = fast_kmeans_iterations().unwrap_or(FAST_KMEANS_ITERATIONS_DEFAULT);
        let iterations = env_iters.min(street.t());
        Self { sample, iterations }
    }
}

/// Run the fast-mode kmeans driver on a dynamically-sized input
/// point slice. The function is a self-contained, no-I/O kmeans
/// implementation that mirrors the production `Layer::cluster`
/// path (kmeans++ init + Elkan-accelerated Lloyd iterations) but
/// operates on a slice (so the input size is a runtime parameter
/// — the production `Layer<K, N>` is fixed at compile-time).
///
/// `street` is used for the seed hash (kmeans++ is deterministic
/// per street) and the iteration cap (the effective cap is
/// `min(caps.iterations, street.t())`). `metric` is the
/// street-specific EMD metric the Elkan step uses for
/// point-centroid distance computation. The output K centroids
/// are returned as a compile-time-fixed `[Histogram; K]` array;
/// the downstream `lookup` / `metric` / `future` post-processing
/// is unchanged.
///
/// ## Algorithm
///
/// 1. **Sub-sample.** If `points.len() > caps.sample`, the slice
///    is truncated to its first `caps.sample` elements (a
///    deterministic prefix — the kmeans driver is byte-stable
///    across runs with the same input slice, satisfying the
///    "re-running kmeans on an unchanged blueprint returns the
///    same centroids" contract).
/// 2. **Init (kmeans++).** The kmeans++ seeding picks the first
///    centroid uniformly at random, then each subsequent
///    centroid with probability proportional to the squared
///    distance from the closest already-chosen centroid. The
///    init is deterministic per street (a `SmallRng` is seeded
///    from `DefaultHasher` over the street).
/// 3. **Elkan steps.** Each iteration runs the Elkan
///    triangle-inequality-accelerated Lloyd step (bound
///    initialization, neighbor reassignment with pruning,
///    centroid recomputation, bound update). The iteration
///    count is capped at `min(caps.iterations, street.t())` so
///    the production default is the upper bound.
pub fn run_fast<const K: usize>(
    points: &[Histogram],
    metric: &Metric,
    street: Street,
    caps: FastKmeansCaps,
) -> [Histogram; K] {
    // Sub-sample to the per-street cap. A slice shorter than
    // `caps.sample` is used as-is (no padding, no wrap-around);
    // a slice longer than the cap is truncated to its prefix
    // (deterministic, byte-stable).
    let truncated: &[Histogram] = if points.len() > caps.sample {
        &points[..caps.sample]
    } else {
        points
    };
    // STW-086: pre-filter the truncated point set to drop
    // empty histograms. The kmeans++ init path calls
    // `metric.emd(centroid, h)` on the *next* iteration's
    // picked centroid; `Metric::emd` dispatches via
    // `source.peek().street()` (metric.rs:108), and
    // `Bins::peek` (bins.rs:95) panics on an empty
    // support with `"non empty histogram"`. The first
    // 1024 turn projections of the flop point pool in
    // production include empty turn isomorphisms (turn
    // isomorphisms with no observed flops) — if the
    // first centroid picked by
    // `WeightedIndex::new(potentials.iter()).sample(rng)`
    // happens to be one of them, the next iteration's
    // `metric.emd(empty_centroid, h)` panics. The same
    // guard also has to apply to the Lloyd iterations
    // below: `step_elkan_slice` calls `metric.emd`
    // between a (non-empty) centroid and every point in
    // `truncated`, so leaving empty points in `truncated`
    // after the init fix would still cause `peek()` to
    // be reached on the empty target's Bin (producing
    // NaN in Sinkhorn, not a panic, but a degenerate
    // result that is not what the spec promises).
    // Filtering at the `run_fast` entry point keeps the
    // kmeans++ init *and* the Lloyd iterations on a
    // consistent (non-empty) point set.
    let non_empty: Vec<Histogram> = truncated.iter().filter(|h| h.n() > 0).cloned().collect();
    // Effective iteration cap: never raises `street.t()`.
    let iters = caps.iterations.min(street.t());
    log::info!(
        "{:<32}sample={} iters={} (street.t()={})",
        "kmeans fast",
        non_empty.len(),
        iters,
        street.t()
    );
    if non_empty.is_empty() {
        // An empty point pool cannot drive kmeans++ init; the
        // production driver would panic on `WeightedIndex::new`
        // with an empty weights array. The fast-mode driver
        // returns K empty centroids (one per cluster slot) so
        // a downstream `lookup` / `metric` / `future` consumer
        // sees a well-formed (degenerate) result instead of a
        // crash. The street argument is the next-street (the
        // centroid's own street), not the street being
        // clustered.
        return std::array::from_fn(|_| Histogram::empty(street));
    }
    // Kmeans++ initialization on the (non-empty) point set.
    let mut centroids = init_kmeans_plus_plus::<K>(&non_empty, metric, street);
    if iters == 0 {
        return centroids;
    }
    // Elkan-accelerated Lloyd iterations. Each iteration
    // reassigns points to the nearest centroid (with bound
    // pruning) and recomputes centroids as the absorb-merge of
    // the assigned points. The bound update at the end of each
    // iteration lowers the per-point upper bound and
    // subtracts from each per-point lower bound in
    // `Bounds::update`, so the pruning in the next
    // iteration has tighter bounds to work with.
    let mut bounds: Vec<Bounds<K>> = (0..non_empty.len())
        .into_par_iter()
        .map(|i| neighbor::<K>(&non_empty, &centroids, metric, i))
        .map(Bounds::from)
        .collect();
    for _ in 0..iters {
        centroids = step_elkan_slice::<K>(&non_empty, &centroids, &mut bounds, metric);
    }
    centroids
}

/// kmeans++ seeding on a slice. Picks the first centroid
/// uniformly at random; each subsequent centroid is picked with
/// probability proportional to the squared distance to the
/// closest already-chosen centroid. Deterministic per street
/// (the `SmallRng` is seeded from a `DefaultHasher` of the
/// street).
///
/// **Public API (STW-088):** the function is exposed to the
/// `kmeans_property.rs` integration test as the production-path
/// kmeans++ init driver the property test exercises. The
/// production `Layer::init_kmeans` (layer.rs:128-203) is
/// structurally identical (same `WeightedIndex` pick + same
/// per-iteration `metric.emd` call + same `potentials[idx] = 0.0`
/// zero-out), so the property test on this function gives the
/// same defensive coverage the production-path kmeans++ init
/// needs. The function is `pub` (not `pub(crate)`) so the
/// integration test in `tests/` (which compiles as a separate
/// crate) can reach it.
pub fn init_kmeans_plus_plus<const K: usize>(
    points: &[Histogram],
    metric: &Metric,
    street: Street,
) -> [Histogram; K] {
    // STW-086: pre-filter the input slice to drop empty
    // histograms. The kmeans++ loop below calls
    // `metric.emd(centroid, h)` on the *next* iteration's
    // picked centroid; `Metric::emd` dispatches via
    // `source.peek().street()` (metric.rs:108), and
    // `Bins::peek` (bins.rs:95) panics on an empty support
    // with `"non empty histogram"`. The first 1024 turn
    // projections of the flop point pool include empty
    // turn isomorphisms (turn isomorphisms with no
    // observed flops) — if the first centroid picked by
    // `WeightedIndex::new(potentials.iter()).sample(rng)`
    // happens to be one of them, the next iteration's
    // `metric.emd(empty_centroid, h)` panics. Mirrors
    // the production `Layer::init_kmeans` defensive
    // guard (layer.rs) so both paths are bounded.
    let non_empty: Vec<Histogram> = points.iter().filter(|h| h.n() > 0).cloned().collect();
    if non_empty.is_empty() {
        // No non-empty input — mirror the
        // `truncated.is_empty()` early-return on kmeans.rs:175
        // and return K empty centroids so a downstream
        // `lookup` / `metric` / `future` consumer sees a
        // well-formed (degenerate) result instead of a
        // crash. Deterministic per-street seed: the empty
        // return path is independent of the seed.
        return std::array::from_fn(|_| Histogram::empty(street));
    }
    // Deterministic per-street seed.
    let ref mut hasher = DefaultHasher::default();
    street.hash(hasher);
    let ref mut rng = SmallRng::seed_from_u64(hasher.finish());
    let n = non_empty.len();
    let mut potentials = vec![1.0_f32; n];
    let mut centroids: Vec<Histogram> = Vec::with_capacity(K);
    while centroids.len() < K {
        // If we've already picked every non-empty input
        // point (a degenerate `n < K` case — the
        // kmeans++ init cannot seed more centroids than
        // there are non-empty inputs), the `potentials`
        // array is all zeros and `WeightedIndex::new`
        // would panic with `InsufficientNonZero`. Pad
        // the remaining centroid slots with a clone of
        // the most-recently-picked centroid (the kmeans
        // literature's standard "fewer-than-K points"
        // handling — duplicate the last picked point
        // so the resulting K-tuple has K-n duplicates
        // at the tail). Duplicates are non-empty by
        // construction (the picked centroid was in the
        // non-empty pool), so the downstream Lloyd step
        // in `step_elkan_slice` (which calls
        // `metric.emd(centroid_i, centroid_j)` on every
        // pair) is well-defined: `Bins::peek` on the
        // `source` side of the EMD call never reaches
        // the empty-support panic at `bins.rs:95`. The
        // empty-cluster guard at `step_elkan_slice`'s
        // recompute step keeps the duplicates in place
        // across Lloyd iterations (a centroid with no
        // reassigned points this iteration keeps its
        // old value, which is the duplicate), so the
        // K-n duplicate slots are stable across the
        // iteration loop. The early-return at
        // kmeans.rs:276-285 (the all-empty input case)
        // is the only path that returns truly empty
        // centroids — that path is reached *only* when
        // `n == 0`, and the corresponding `run_fast`
        // branch short-circuits the Lloyd step on the
        // same condition.
        if potentials.iter().all(|p| *p == 0.0) {
            // SAFETY: this branch is reachable only
            // after at least one `centroids.push` has
            // fired (potentials started as `vec![1.0; n]`
            // so the all-zeros check can fire only after
            // the n-th pick has zeroed its slot). The
            // most-recently-picked centroid is the last
            // element of the `centroids` Vec.
            let last = centroids
                .last()
                .cloned()
                .expect("K>N guard reachable only after first pick");
            while centroids.len() < K {
                centroids.push(last.clone());
            }
            break;
        }
        let idx = WeightedIndex::new(potentials.iter())
            .expect("valid weights array")
            .sample(rng);
        let centroid = non_empty[idx].clone();
        centroids.push(centroid);
        potentials[idx] = 0.0;
        // Update potentials: each point's potential is
        // min(potential, d^2 to the just-chosen centroid).
        potentials = non_empty
            .par_iter()
            .enumerate()
            .map(|(i, h)| {
                let d = metric.emd(&centroids[centroids.len() - 1], h);
                potentials[i].min(d * d)
            })
            .collect();
    }
    let len = centroids.len();
    centroids
        .try_into()
        .unwrap_or_else(|_| panic!("kmeans++ produced {len} centroids, expected {K}"))
}

/// Find the nearest centroid (by EMD) for the i-th point in
/// the slice. Returns `(centroid_index, distance)` so the
/// caller can build a `Bounds<K>` from the result. The slice
/// + centroids + metric + index signature mirrors the
/// production `Elkan::neighbor` trait method.
fn neighbor<const K: usize>(
    points: &[Histogram],
    centroids: &[Histogram; K],
    metric: &Metric,
    i: usize,
) -> (usize, f32) {
    let x = &points[i];
    centroids
        .iter()
        .enumerate()
        .map(|(j, c)| (j, metric.emd(c, x)))
        .inspect(|(_, d)| debug_assert!(d.is_finite()))
        .min_by(|(_, d1), (_, d2)| d1.partial_cmp(d2).unwrap())
        .unwrap_or((0, 0.0))
}

/// One Elkan step on a slice + a mutable bounds vector.
/// Computes pairwise inter-centroid distances, the midpoint
/// vector s(c) = (1/2) min_{c'≠c} d(c, c'), the per-point
/// neighbor reassignment with triangle-inequality pruning,
/// the new centroids (absorb-merged assigned points), and the
/// bound update for the next iteration. Returns the new
/// centroids.
fn step_elkan_slice<const K: usize>(
    points: &[Histogram],
    centroids: &[Histogram; K],
    bounds: &mut [Bounds<K>],
    metric: &Metric,
) -> [Histogram; K] {
    let pairwise: [[f32; K]; K] = std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            if i == j {
                0.0
            } else {
                metric.emd(&centroids[i], &centroids[j])
            }
        })
    });
    let midpoints: [f32; K] = std::array::from_fn(|i| {
        (0..K)
            .filter(|j| *j != i)
            .map(|j| 0.5 * pairwise[i][j])
            .fold(f32::MAX, f32::min)
    });
    // Reassign each point to the nearest centroid with
    // triangle-inequality pruning. The per-point work is
    // O(K) EMD calls in the worst case, but the pruning
    // typically drops the constant to a handful of calls
    // per point per iteration.
    bounds.par_iter_mut().enumerate().for_each(|(i, b)| {
        if !b.can_exclude(&midpoints) {
            let x = &points[i];
            // Refresh stale upper bound (only if a
            // centroid has moved since the last
            // bound update).
            if b.stale() {
                b.refresh(metric.emd(x, &centroids[b.j()]));
            }
            for j in 0..K {
                if b.has_shifted(&pairwise, j) {
                    b.witness(metric.emd(x, &centroids[j]), j);
                }
            }
        }
    });
    // Recompute centroids: for each cluster slot j, the
    // new centroid is the absorb-merge of every point
    // currently assigned to j. The `Absorb` impl on
    // `Histogram` is associative + commutative, so the
    // fold order is irrelevant.
    //
    // STW-087: if a cluster slot j received zero points
    // in the reassign pass, the fold returns the
    // `centroids[j].identity()` (= `Histogram::empty(...)`)
    // and the subsequent drift calculation
    // `metric.emd(empty, &centroids[j])` would panic
    // on `Bins::peek` ("non empty histogram" at
    // bins.rs:95). This is the classical k-means
    // "empty cluster" pathology: a Lloyd step
    // concentrates all points into fewer than K
    // clusters (production evidence: the flop
    // `RBP_TESTNET_FAST=1` runbook receipt
    // `receipts/testnet-live-proof-20260610T032421Z/`
    // panicked at bins.rs:95 on the flop street
    // after the kmeans++ init returned K non-empty
    // centroids + the first Lloyd step left at least
    // one cluster slot with zero assignments). The
    // classical fix is to keep the OLD centroid for any
    // cluster slot that landed zero points this
    // iteration (mirrors the production `TestLayer::heal`
    // in `tests.rs:64-70`, which replaces empty
    // centroids with a random histogram after each
    // step; the slice-based fast-mode path uses the
    // old centroid instead of a fresh sample because
    // the fast-mode driver is byte-stable and a
    // fresh sample would introduce a second source
    // of non-determinism). The new centroid is
    // therefore *guaranteed* non-empty for every
    // cluster slot — the drift calculation is
    // well-defined.
    let new_centroids: [Histogram; K] = std::array::from_fn(|j| {
        let mut assigned = 0usize;
        let new = bounds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.j() == j)
            .map(|(i, _)| {
                assigned += 1;
                points[i].clone()
            })
            .fold(centroids[j].identity(), |acc, h| acc.absorb(&h));
        if assigned == 0 {
            // Empty cluster: keep the old centroid
            // (the classical k-means "empty cluster"
            // fix). The old centroid is non-empty by
            // invariant (kmeans++ seeds centroids
            // from the non-empty input pool, and
            // every prior step's empty-cluster guard
            // preserved non-emptiness), so the
            // returned `[Histogram; K]` is uniformly
            // non-empty and the subsequent drift
            // calculation never panics on
            // `Bins::peek`.
            centroids[j]
        } else {
            new
        }
    });
    // Drift: how far each centroid moved this iteration.
    // The drift is added to the per-point upper bound and
    // subtracted from each per-point lower bound in
    // `Bounds::update`, so the pruning in the next
    // iteration has tighter bounds to work with. The
    // STW-087 empty-cluster guard above guarantees
    // `new_centroids[i]` is non-empty for every i, so
    // the `metric.emd` call's `Bins::peek` dispatch
    // (`metric.rs:108` → `bins.rs:95`) never reaches
    // the empty-support panic.
    let drift: [f32; K] = std::array::from_fn(|i| metric.emd(&new_centroids[i], &centroids[i]));
    bounds.par_iter_mut().for_each(|b| b.update(&drift));
    new_centroids
}

/// Naive (non-Elkan) kmeans on a slice. Used as the
/// reference implementation in tests; the production
/// `Layer::cluster` is Elkan-accelerated, but the slice-
/// based fast-mode driver is tested against this naive
/// shape to pin the "fast-mode produces the same centroids
/// as naive kmeans modulo iteration cap" property. The
/// `metric` + `street` args match the `run_fast` signature
/// so tests can call them interchangeably.
#[allow(dead_code)]
pub fn run_naive<const K: usize>(
    points: &[Histogram],
    metric: &Metric,
    street: Street,
    iterations: usize,
) -> [Histogram; K] {
    if points.is_empty() {
        return std::array::from_fn(|_| Histogram::empty(street));
    }
    let mut centroids = init_kmeans_plus_plus::<K>(points, metric, street);
    let iters = iterations.min(street.t());
    for _ in 0..iters {
        // O(N * K) EMD calls per iteration (no pruning);
        // the naive driver is the slow reference, not the
        // fast-mode driver.
        let assignments: Vec<usize> = (0..points.len())
            .into_par_iter()
            .map(|i| neighbor::<K>(points, &centroids, metric, i).0)
            .collect();
        centroids = std::array::from_fn(|j| {
            let identity = centroids[j].identity();
            assignments
                .iter()
                .enumerate()
                .filter(|(_, k)| **k == j)
                .map(|(i, _)| points[i].clone())
                .fold(identity, |acc, h| acc.absorb(&h))
        });
    }
    centroids
}
