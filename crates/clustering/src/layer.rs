//! K-means clustering layer for poker hand abstraction.
//!
//! This module implements a single clustering layer that maps poker hand isomorphisms
//! to abstract buckets using the k-means algorithm with Elkan acceleration.

use super::*;
use rbp_cards::*;
use rbp_core::*;
use rbp_gameplay::*;
use std::collections::BTreeMap;

/// A clustering layer that maps poker hand isomorphisms to abstract buckets.
///
/// Each layer corresponds to a single betting street and maintains:
/// - The full dataset of hand histograms (one per isomorphism)
/// - K-means cluster centroids learned via the Elkan algorithm
/// - Distance bounds for acceleration during clustering
///
/// The layer produces three artifacts:
/// 1. A `Lookup` table mapping isomorphisms to abstractions
/// 2. A `Future` transition model mapping abstractions to next-street distributions
/// 3. A `Metric` defining distances between learned abstractions
pub struct Layer<const K: usize, const N: usize> {
    /// The betting street this layer represents
    street: Street,
    /// Distance metric for computing EMD between abstractions in the next street
    metric: Box<Metric>,
    /// Learned k-means cluster centroids, indexed by abstraction (K total)
    kmeans: Box<[Histogram; K]>,
    /// All poker hand histograms, indexed by isomorphism order (N total)
    points: Box<[Histogram; N]>,
    /// Distance bounds for each point, used by Elkan acceleration (not persisted)
    bounds: Box<[Bounds<K>; N]>,
}

impl<const K: usize, const N: usize> Layer<K, N> {
    /// Returns the betting street for this layer.
    fn street(&self) -> Street {
        self.street
    }

    /// Constructs an `Abstraction` from this layer's street and a cluster index.
    fn abstraction(&self, i: usize) -> Abstraction {
        Abstraction::from((self.street(), i))
    }
}

impl<const K: usize, const N: usize> Layer<K, N> {
    /// Builds a lookup table mapping each isomorphism to its nearest cluster abstraction.
    fn lookup(&self) -> Lookup
    where
        Self: Elkan<K, N>,
    {
        log::info!("{:<32}{:<32}", "calculating lookup", self.street());
        use rayon::iter::IntoParallelIterator;
        use rayon::iter::ParallelIterator;
        match self.street() {
            Street::Pref | Street::Rive => Lookup::grow(self.street()),
            Street::Flop | Street::Turn => (0..N)
                .into_par_iter()
                .map(|i| self.neighbor(i))
                .collect::<Vec<(usize, f32)>>()
                .into_iter()
                .map(|(k, _)| self.abstraction(k))
                .zip(IsomorphismIterator::from(self.street()))
                .map(|(abs, iso)| (iso, abs))
                .collect::<BTreeMap<Isomorphism, Abstraction>>()
                .into(),
        }
    }

    /// Computes pairwise distances between all learned cluster centroids.
    fn metric(&self) -> Metric {
        log::info!("{:<32}{:<32}", "calculating metric", self.street());
        let mut metric = BTreeMap::new();
        for (i, x) in self.kmeans.iter().enumerate() {
            for (j, y) in self.kmeans.iter().enumerate() {
                if i > j {
                    let ref a = self.abstraction(i);
                    let ref b = self.abstraction(j);
                    let index = Pair::from((a, b));
                    let distance = self.metric.emd(x, y) + self.metric.emd(y, x);
                    let distance = distance / 2.;
                    metric.insert(index, distance);
                }
            }
        }
        Metric::from(metric)
    }

    /// Builds the transition future hand mapping abstractions to their centroid histograms.
    fn future(&self) -> Future {
        log::info!("{:<32}{:<32}", "calculating transitions", self.street());
        self.kmeans()
            .iter()
            .cloned()
            .enumerate()
            .map(|(k, centroid)| (self.abstraction(k), centroid))
            .collect::<BTreeMap<Abstraction, Histogram>>()
            .into()
    }
}

/// Elkan k-means implementation for clustering poker hand abstractions.
impl<const K: usize, const N: usize> Elkan<K, N> for Layer<K, N> {
    type P = Histogram;

    fn t(&self) -> usize {
        self.street().t()
    }

    fn points(&self) -> &[Histogram; N] {
        &self.points
    }

    fn kmeans(&self) -> &[Histogram; K] {
        &self.kmeans
    }

    fn bounds(&self) -> &[Bounds<K>; N] {
        &self.bounds
    }

    fn distance(&self, h1: &Histogram, h2: &Histogram) -> Energy {
        self.metric.emd(h1, h2)
    }

    fn init_kmeans(&self) -> [Histogram; K] {
        use rand::SeedableRng;
        use rand::distr::Distribution;
        use rand::distr::weighted::WeightedIndex;
        use rand::rngs::SmallRng;
        use rayon::iter::IntoParallelRefIterator;
        use rayon::iter::ParallelIterator;
        use std::hash::DefaultHasher;
        use std::hash::Hash;
        use std::hash::Hasher;
        // don't do any abstraction on preflop or river
        if matches!(self.street(), Street::Pref | Street::Rive) {
            debug_assert!(N == K);
            return std::array::from_fn(|i| self.points()[i]);
        }
        // STW-086: zero out the weight slot for any input
        // point whose support is empty. The kmeans++ loop
        // below calls `self.distance(&x, &h)` on the *next*
        // iteration's picked centroid; `self.distance` is
        // `Metric::emd` (layer.rs:124), which dispatches
        // via `source.peek().street()` (metric.rs:108), and
        // `Bins::peek` (bins.rs:95) panics on an empty
        // support with `"non empty histogram"`. The
        // production 1.3M-row pool is large enough that
        // the empty-prefix luck-out has not fired yet, but
        // a future production run on a fresh DB with
        // sparse observations will hit it (the fast-mode
        // path that *did* hit it is `kmeans.rs`, fixed
        // with the parallel pre-filter in
        // `init_kmeans_plus_plus`). Mirroring the guard
        // here keeps both paths bounded.
        let mut potentials = vec![1.; N];
        for (i, h) in self.points().iter().enumerate() {
            if h.n() == 0 {
                potentials[i] = 0.;
            }
        }
        // All-zero weights would panic `WeightedIndex::new`
        // (it requires at least one positive weight). An
        // input where every point is empty is a degenerate
        // case the same way `kmeans.rs` handles it: return
        // K empty centroids so a downstream consumer sees
        // a well-formed (degenerate) result instead of a
        // crash. (This branch is unreachable in practice
        // on a non-degenerate production pool — the guard
        // is here for the same "don't panic on sparse
        // data" reason `kmeans.rs` ships the mirror.)
        if potentials.iter().all(|&p| p == 0.) {
            return std::array::from_fn(|_| Histogram::empty(self.street().next()));
        }
        // deterministic pseudo-random clustering
        let ref mut hasher = DefaultHasher::default();
        self.street().hash(hasher);
        let ref mut rng = SmallRng::seed_from_u64(hasher.finish());
        // kmeans++ initialization
        let mut histograms = Vec::with_capacity(K);
        while histograms.len() < K {
            let i = WeightedIndex::new(potentials.iter())
                .expect("valid weights array")
                .sample(rng);
            let x = self.points()[i];
            histograms.push(x);
            potentials[i] = 0.;
            potentials = self
                .points()
                .par_iter()
                .map(|h| self.distance(&x, &h))
                .map(|p| p * p)
                .collect::<Vec<Energy>>()
                .iter()
                .zip(potentials.iter())
                .map(|(d0, d1)| Energy::min(*d0, *d1))
                .collect::<Vec<Energy>>();
        }
        histograms.try_into().expect("K")
    }
}

#[cfg(feature = "database")]
impl<const K: usize, const N: usize> Layer<K, N> {
    /// Internal clustering implementation for a specific K, N.
    pub async fn cluster(street: Street, client: &tokio_postgres::Client) -> Artifacts {
        // STW-077: detect fast-mode *before* the production
        // hydrating/initializing/bounding path so a fresh-DB
        // receipt runbook run (`RBP_TESTNET_FAST=1`) does not
        // build a 1.3M-row flop / 14M-row turn bound table
        // (the `init_bounds` step alone is the dominant
        // per-street wall-clock cost on a fresh DB; the
        // production 20/24 Lloyd iterations are a smaller
        // multiplier on top of it). The fast-mode gate is
        // `RBP_TESTNET_FAST=1` (the single switch the runbook
        // flips); the `RBP_FAST_KMEANS_SAMPLE` +
        // `RBP_FAST_KMEANS_ITERATIONS` knobs override the
        // default 1024/8 caps. The production path is
        // unchanged when the switch is unset (the call site is
        // argv-unchanged — the spec `RBP_TESTNET_FAST=1 is the
        // only switch` contract holds).
        if rbp_core::testnet_fast() {
            let caps = crate::kmeans::FastKmeansCaps::resolve(street);
            // The fast-mode cap is honored only when the
            // resolved caps would actually cap the work (a
            // cap larger than production is a no-op, not a
            // production re-route).
            let fast_active = caps.iterations < street.t() || caps.sample < N;
            if fast_active {
                log::info!(
                    "{:<32}sample={} iters={} (production would be sample={} iters={})",
                    "STW-077 fast-mode",
                    caps.sample,
                    caps.iterations,
                    N,
                    street.t()
                );
                return Self::cluster_fast(street, client, caps).await;
            }
        }
        log::info!("{:<32}{:<32}", "kmeans hydrating", street);
        let mut layer = Self::build(street, client).await;
        log::info!("{:<32}{:<32}", "kmeans initializing", street);
        layer.kmeans = Box::new(layer.init_kmeans());
        log::info!("{:<32}{:<32}", "kmeans bounding", street);
        layer.bounds = layer.init_bounds();
        log::info!("{:<32}{:<32}", "kmeans iterating", street);
        let new = vec![Bounds::default(); N].try_into().expect("N");
        let ref mut old = layer.bounds;
        let ref mut old = std::mem::replace(old, new);
        for i in 0..layer.t() {
            layer.kmeans = Box::new(layer.step_elkan(old));
            log::debug!("{:3}", i);
        }
        let ref mut new = layer.bounds;
        std::mem::swap(new, old);
        Artifacts {
            lookup: layer.lookup(),
            metric: layer.metric(),
            future: layer.future(),
        }
    }
    /// STW-077 fast-mode clustering path. Mirrors the production
    /// `cluster` body but (a) sub-samples the input point pool to
    /// `caps.sample` rows (a deterministic prefix of the
    /// hydration output), (b) runs the kmeans driver from
    /// [`crate::kmeans::run_fast`] (a slice-based kmeans++ +
    /// Elkan implementation that takes a runtime-sized input),
    /// (c) caps the iteration count at `caps.iterations`, and
    /// (d) reconstructs the `lookup` / `metric` / `future`
    /// artifacts from the sub-sampled output centroids. The
    /// production `init_kmeans` / `init_bounds` / `step_elkan`
    /// methods are not called (they would build the full N
    /// point + bound tables, defeating the fast-mode wall-
    /// clock goal).
    async fn cluster_fast(
        street: Street,
        client: &tokio_postgres::Client,
        caps: crate::kmeans::FastKmeansCaps,
    ) -> Artifacts {
        log::info!("{:<32}{:<32}", "kmeans fast hydrating", street);
        // Build the layer so we can read its points + metric
        // + street tag; the points are then handed to the
        // slice-based fast-mode kmeans driver, and the
        // returned centroids are written back into the layer
        // for the artifact-computation pass. The bounds
        // table is left at its default (the fast-mode
        // driver builds its own bounds internally).
        let mut layer = Self::build(street, client).await;
        // Sub-sample to a deterministic prefix. The slice
        // length is `min(N, caps.sample)`; an out-of-range
        // `caps.sample` is clamped by the prefix
        // construction below.
        let sample_n = caps.sample.min(N);
        let points_slice: &[Histogram] = &layer.points[..sample_n];
        log::info!(
            "{:<32}points={} caps.sample={} caps.iters={}",
            "kmeans fast driving",
            points_slice.len(),
            caps.sample,
            caps.iterations
        );
        let centroids = crate::kmeans::run_fast::<K>(
            points_slice,
            &layer.metric,
            street,
            crate::kmeans::FastKmeansCaps {
                sample: sample_n,
                iterations: caps.iterations,
            },
        );
        layer.kmeans = Box::new(centroids);
        Artifacts {
            lookup: layer.lookup(),
            metric: layer.metric(),
            future: layer.future(),
        }
    }
    /// Build layer dependencies from postgres (not disk).
    async fn build(street: Street, client: &tokio_postgres::Client) -> Self {
        if street == Street::Rive {
            Self {
                street,
                metric: Box::new(Metric::default()),
                kmeans: Box::new(std::array::from_fn(|_| Histogram::empty(Street::Rive))),
                bounds: vec![Bounds::default(); N].try_into().expect("N"),
                points: vec![Histogram::empty(Street::Rive); N]
                    .try_into()
                    .expect("N"),
            }
        } else {
            Self {
                street,
                metric: Box::new(Metric::from_street(client, street.next()).await),
                kmeans: Box::new(std::array::from_fn(|_| Histogram::empty(street.next()))),
                bounds: vec![Bounds::default(); N].try_into().expect("N"),
                points: Lookup::from_street(client, street.next())
                    .await
                    .projections()
                    .try_into()
                    .expect("projections.len() == N"),
            }
        }
    }
}
