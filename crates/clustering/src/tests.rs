use super::*;
use rbp_cards::*;
use rbp_core::Energy;
use rbp_gameplay::*;

/// Test fixture for Elkan algorithm verification.
///
/// Clusters random Turn histograms with small fixed constants for
/// fast unit testing. Verifies that Elkan produces identical results
/// to naive k-means while demonstrating convergence properties.
const K: usize = 8;
const N: usize = 2048;

/// Test layer implementing Elkan trait for algorithm verification.
#[derive(Clone)]
pub struct TestLayer {
    metric: Metric,
    kmeans: [Histogram; K],
    points: Box<[Histogram; N]>,
    bounds: Box<[Bounds<K>; N]>,
}

impl TestLayer {
    /// Number of iterations for test runs.
    const fn t() -> usize {
        8
    }
    /// Creates a new test layer with random Turn histograms.
    pub fn new() -> Self {
        let points = (0..N)
            .map(|_| Histogram::from(Observation::from(Street::Turn)))
            .collect::<Vec<_>>()
            .try_into()
            .expect("N");
        let metric = Metric::default();
        let mut km = Self {
            metric,
            points,
            kmeans: std::array::from_fn(|_| Histogram::empty(Street::Rive)),
            bounds: vec![Bounds::default(); N].try_into().expect("N"),
        };
        km.kmeans = km.init_kmeans();
        km.bounds = km.init_bounds();
        km
    }

    /// Runs one Elkan iteration.
    pub fn step(&mut self) {
        let next = vec![Bounds::default(); N].try_into().expect("N");
        let ref mut curr = self.bounds;
        let ref mut prev = std::mem::replace(curr, next);
        self.kmeans = Elkan::step_elkan(self, prev);
        let ref mut curr = self.bounds;
        std::mem::swap(prev, curr);
        self.heal();
    }

    /// Runs one naive iteration.
    pub fn naive(&mut self) {
        self.kmeans = Elkan::step_naive(self);
        self.heal();
    }

    /// Replaces empty clusters with random histograms.
    pub fn heal(&mut self) {
        self.kmeans
            .iter_mut()
            .filter(|h| h.n() == 0)
            .map(|h| *h = Histogram::from(Observation::from(Street::Turn)))
            .count();
    }
}

impl Elkan<K, N> for TestLayer {
    type P = Histogram;
    fn t(&self) -> usize {
        Self::t()
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
        std::array::from_fn(|_| Histogram::from(Observation::from(Street::Turn)))
    }
    fn rms(&self) -> Energy {
        use rayon::prelude::*;
        ((0..N)
            .into_par_iter()
            .map(|i| self.neighbor(i).1)
            .map(|d| d * d)
            .sum::<Energy>()
            / N as Energy)
            .sqrt()
    }
}

/// STW-098 regression: the synthetic fast-mode flop lookup
/// helper must produce a *complete* flop lookup (one entry per
/// flop isomorphism) so the preflop `Layer::build` path's
/// `next_lookup.projections()` call has every preflop-iso's
/// flop-child in its key set. A regression that drops an iso
/// (or short-circuits to an empty map) re-introduces the
/// `lookup.rs:43` `"precomputed abstraction in lookup"` panic
/// the 2026-06-11 runbook receipts captured.
#[test]
fn synthetic_fast_flop_lookup_is_complete_and_degenerate() {
    use rbp_cards::IsomorphismIterator;
    use std::collections::BTreeMap;
    let lookup: Lookup = synthetic_fast_flop_lookup();
    let map: BTreeMap<Isomorphism, Abstraction> = lookup.into();
    // The synthesized lookup has one entry per flop iso. The
    // exact iso count is `N_FLOP` (the production compile-time
    // const); the test asserts the synthesized map is
    // *non-empty* and matches the full iso iterator's count so
    // a regression that returns an empty map (or a partial
    // map) fails here.
    let expected_n = IsomorphismIterator::from(Street::Flop).count();
    assert_eq!(
        map.len(),
        expected_n,
        "STW-098: synthetic fast-mode flop lookup must have one entry per flop iso; \
         got {} entries, expected {} (the production N_FLOP).",
        map.len(),
        expected_n
    );
    // Degenerate contract: every entry maps to
    // `Abstraction::from((Street::Flop, 0))`. The fast-mode
    // preflop build's degenerate-but-well-formed point pool
    // contract depends on this uniformity (a regression that
    // randomizes the abstraction value would still be
    // complete but would no longer be the canonical degenerate
    // shape the runbook smoke-test pins).
    for (iso, abs) in map.iter() {
        assert_eq!(
            iso.0.street(),
            Street::Flop,
            "STW-098: synthetic flop lookup must contain only flop isos; found iso on street {:?}",
            iso.0.street()
        );
        assert_eq!(
            *abs,
            Abstraction::from((Street::Flop, 0)),
            "STW-098: synthetic flop lookup must map every flop iso to bucket 0 \
             (the degenerate abstraction); found iso {:?} mapped to {:?}.",
            iso,
            abs
        );
    }
}

/// STW-098 regression: the synthetic fast-mode flop lookup's
/// `projections()` call must produce a `Vec<Histogram>` of
/// length `N_PREF` (one histogram per preflop iso) without
/// panicking on a missing key. A regression that returns a
/// partial lookup (e.g. dropping entries) re-introduces the
/// `lookup.rs:43` panic the 2026-06-11 runbook receipts
/// captured (`receipts/testnet-live-proof-20260611T000534Z_v3/cluster/stderr.txt`).
#[test]
fn synthetic_fast_flop_lookup_projections_satisfy_preflop_shape() {
    use rbp_cards::IsomorphismIterator;
    let lookup: Lookup = synthetic_fast_flop_lookup();
    let projections: Vec<Histogram> = lookup.projections();
    // The preflop iso iterator enumerates N_PREF isos; the
    // `Lookup::projections()` arm for a complete lookup
    // returns one histogram per prev-street iso (preflop is
    // prev-of-flop). The exact count is the production
    // `N_PREF` const; the test asserts `projections.len() ==
    // N_PREF` and every projection is well-formed (its `n()`
    // is the per-iso flop-child count, which is well-defined
    // for a preflop iso on a complete flop lookup).
    let expected_n = IsomorphismIterator::from(Street::Pref).count();
    assert_eq!(
        projections.len(),
        expected_n,
        "STW-098: synthetic flop lookup's projections() must return one histogram per \
         preflop iso (N_PREF); got {} projections, expected {}.",
        projections.len(),
        expected_n
    );
    // Every projection is a Histogram of flop abstractions.
    // The degenerate fast-mode lookup maps every flop iso to
    // bucket 0, so every preflop iso's projection has its
    // support (`n()`) equal to 1 (a single distinct
    // abstraction across all children — every child's lookup
    // returns the same single abstraction). A regression that
    // returns a Histogram with `n() == 0` (empty support)
    // trips the `Bins::peek` `expect("non empty histogram")`
    // guard during the preflop kmeans driver's distance call;
    // a regression that drops entries from the synthetic
    // lookup (so children miss the partial key set) panics at
    // `lookup.rs:43` before reaching this test.
    for (i, h) in projections.iter().enumerate() {
        assert_eq!(
            h.n(),
            1,
            "STW-098: preflop iso {} projection's n() (the per-histogram support count, \
             the number of distinct abstraction buckets with non-zero count) must be 1 \
             on the degenerate lookup (every flop iso maps to bucket 0, so every preflop \
             iso's projection has exactly one distinct abstraction in its support). A \
             regression that randomizes the synthetic lookup's abstraction values would \
             change this number; a regression that drops entries would either panic at \
             `lookup.rs:43` (caught by the production runbook) or change the support \
             count.",
            i
        );
        // The histogram's first (and only) support entry is
        // `Abstraction::from((Street::Flop, 0))` — the
        // degenerate bucket every flop iso maps to. A
        // regression that uses a different bucket (e.g. a
        // randomized hash) would surface here.
        let first = h.support().next().expect("non empty histogram");
        assert_eq!(
            first,
            Abstraction::from((Street::Flop, 0)),
            "STW-098: preflop iso {} projection's first (and only) support entry must \
             be `Abstraction::from((Street::Flop, 0))` on the degenerate lookup; got \
             {:?}. A regression that randomizes the synthetic lookup's abstraction \
             values would change this.",
            i,
            first
        );
    }
}
