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
    // Effective iteration cap: never raises `street.t()`.
    let iters = caps.iterations.min(street.t());
    log::info!(
        "{:<32}sample={} iters={} (street.t()={})",
        "kmeans fast",
        truncated.len(),
        iters,
        street.t()
    );
    if truncated.is_empty() {
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
    // Kmeans++ initialization on the truncated point set.
    let mut centroids = init_kmeans_plus_plus::<K>(truncated, metric, street);
    if iters == 0 {
        return centroids;
    }
    // Elkan-accelerated Lloyd iterations. Each iteration
    // reassigns points to the nearest centroid (with bound
    // pruning) and recomputes centroids as the absorb-merge of
    // the assigned points. The bound update at the end of each
    // iteration lowers the per-point distance bounds by the
    // centroid movement (so a future iteration can prune more
    // work).
    let mut bounds: Vec<Bounds<K>> = (0..truncated.len())
        .into_par_iter()
        .map(|i| neighbor::<K>(truncated, &centroids, metric, i))
        .map(Bounds::from)
        .collect();
    for _ in 0..iters {
        centroids = step_elkan_slice::<K>(truncated, &centroids, &mut bounds, metric);
    }
    centroids
}

/// kmeans++ seeding on a slice. Picks the first centroid
/// uniformly at random; each subsequent centroid is picked with
/// probability proportional to the squared distance to the
/// closest already-chosen centroid. Deterministic per street
/// (the `SmallRng` is seeded from a `DefaultHasher` of the
/// street).
fn init_kmeans_plus_plus<const K: usize>(
    points: &[Histogram],
    metric: &Metric,
    street: Street,
) -> [Histogram; K] {
    // Deterministic per-street seed.
    let ref mut hasher = DefaultHasher::default();
    street.hash(hasher);
    let ref mut rng = SmallRng::seed_from_u64(hasher.finish());
    let n = points.len();
    let mut potentials = vec![1.0_f32; n];
    let mut centroids: Vec<Histogram> = Vec::with_capacity(K);
    while centroids.len() < K {
        // Pick the next centroid index from the weighted
        // distribution. `WeightedIndex::new` panics on an
        // empty / all-zero weights array, but the loop
        // maintains `potentials[i] = 0.0` for every already-
        // chosen index `i` so the weights array is never
        // all-zero after the first iteration (and is `1.0`-
        // uniform on the first iteration).
        let idx = WeightedIndex::new(potentials.iter())
            .expect("valid weights array")
            .sample(rng);
        let centroid = points[idx].clone();
        centroids.push(centroid);
        potentials[idx] = 0.0;
        // Update potentials: each point's potential is
        // min(potential, d^2 to the just-chosen centroid).
        potentials = points
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
    let new_centroids: [Histogram; K] = std::array::from_fn(|j| {
        let identity = centroids[j].identity();
        bounds
            .iter()
            .enumerate()
            .filter(|(_, b)| b.j() == j)
            .map(|(i, _)| points[i].clone())
            .fold(identity, |acc, h| acc.absorb(&h))
    });
    // Drift: how far each centroid moved this iteration.
    // The drift is added to the per-point upper bound and
    // subtracted from each per-point lower bound in
    // `Bounds::update`, so the pruning in the next
    // iteration has tighter bounds to work with.
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
