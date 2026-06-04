//! Deterministic tests for the concrete `rbp-transport` impls.
//!
//! These tests pin the trait surface (`Support`, `Density`, `Measure`,
//! `Coupling`) and the two concrete coupling impls (`Sinkhorn`,
//! `Greedy`) over `BTreeMap<usize, Probability>` + `UniformMetric`
//! against reproducible fixtures. There are no `unimplemented!()`
//! bodies, no random sources, and no env-var dependencies — every
//! assertion is fully deterministic across machines and CI runners.

use crate::Coupling;
use crate::Density;
use crate::Measure;
use crate::Support;
use crate::UniformMetric;
use crate::coupling::greedy::Greedy;
use crate::coupling::sinkhorn::Sinkhorn;
use rbp_core::Probability;
use std::collections::BTreeMap;
use std::collections::HashMap;

/// Helper: build a normalized 3-bucket BTreeMap fixture
/// `{0: 0.7, 1: 0.2, 2: 0.1}`.
fn btree_721() -> BTreeMap<usize, Probability> {
    let mut m: BTreeMap<usize, Probability> = BTreeMap::new();
    m.insert(0, 0.7);
    m.insert(1, 0.2);
    m.insert(2, 0.1);
    m
}

/// Helper: build a normalized 3-bucket HashMap fixture.
fn hashmap_721() -> HashMap<usize, Probability> {
    let mut m: HashMap<usize, Probability> = HashMap::new();
    m.insert(0, 0.7);
    m.insert(1, 0.2);
    m.insert(2, 0.1);
    m
}

/// Helper: build a normalized 3-bucket Vec<(usize, Probability)> fixture.
fn vec_721() -> Vec<(usize, Probability)> {
    vec![(0, 0.7), (1, 0.2), (2, 0.1)]
}

/// Helper: assert two `f32` values are within `eps` of each other.
fn assert_close(a: f32, b: f32, eps: f32, what: &str) {
    let diff = (a - b).abs();
    assert!(diff <= eps, "{what}: |{a} - {b}| = {diff} > {eps}",);
}

#[test]
fn support_usize_is_clone() {
    // The marker trait has no behavior beyond Clone; this test
    // pins the contract so a future relaxation breaks loudly.
    let x: usize = 5;
    let y: usize = x.clone();
    assert_eq!(x, y);
    fn requires_support<T: Support>(_: T) {}
    requires_support(7usize);
}

#[test]
fn density_btreemap_round_trips_known_probabilities() {
    let m = btree_721();
    assert!((m.density(&0) - 0.7).abs() < 1e-6);
    assert!((m.density(&1) - 0.2).abs() < 1e-6);
    assert!((m.density(&2) - 0.1).abs() < 1e-6);
    let support: Vec<usize> = m.support().collect();
    assert_eq!(support, vec![0, 1, 2]);
}

#[test]
fn density_hashmap_matches_btreemap_on_same_input() {
    let bt = btree_721();
    let hm = hashmap_721();
    for k in [0usize, 1, 2, 99] {
        assert_eq!(bt.density(&k), hm.density(&k), "mismatch at k={k}");
    }
}

#[test]
fn density_vec_assoc_list_matches_btreemap_on_same_input() {
    let bt = btree_721();
    let v = vec_721();
    for k in [0usize, 1, 2, 99] {
        assert_eq!(bt.density(&k), v.density(&k), "mismatch at k={k}");
    }
}

#[test]
fn density_unknown_point_returns_zero() {
    let bt = btree_721();
    let hm = hashmap_721();
    let v = vec_721();
    for k in [3usize, 7, 99] {
        assert_eq!(bt.density(&k), 0.0);
        assert_eq!(hm.density(&k), 0.0);
        assert_eq!(v.density(&k), 0.0);
    }
}

#[test]
fn density_total_mass_equals_one_for_normalized_input() {
    let bt = btree_721();
    let hm = hashmap_721();
    let v = vec_721();
    for (label, sum) in [
        (
            "btreemap",
            bt.support().map(|x| bt.density(&x)).sum::<f32>(),
        ),
        ("hashmap", hm.support().map(|x| hm.density(&x)).sum::<f32>()),
        ("vec", v.support().map(|x| v.density(&x)).sum::<f32>()),
    ] {
        assert_close(sum, 1.0, 1e-6, label);
    }
}

#[test]
fn uniform_metric_zero_self_distance() {
    let m = UniformMetric::new();
    for x in [0usize, 1, 2, 5, 17] {
        assert_eq!(m.distance(&x, &x), 0.0);
    }
}

#[test]
fn uniform_metric_symmetry_holds() {
    let m = UniformMetric::new();
    for &(x, y) in &[(0usize, 5usize), (3, 9), (12, 4), (7, 7), (100, 0)] {
        assert_eq!(m.distance(&x, &y), m.distance(&y, &x));
    }
}

#[test]
fn uniform_metric_triangle_inequality() {
    let m = UniformMetric::new();
    // d(x, z) ≤ d(x, y) + d(y, z) over a 5-point grid.
    let grid: [usize; 5] = [0, 1, 2, 3, 4];
    for &x in &grid {
        for &y in &grid {
            for &z in &grid {
                let lhs = m.distance(&x, &z);
                let rhs = m.distance(&x, &y) + m.distance(&y, &z);
                assert!(
                    lhs <= rhs + 1e-5,
                    "triangle inequality failed: d({x},{z})={lhs} > d({x},{y})+d({y},{z})={rhs}",
                );
            }
        }
    }
}

fn normalize(mut m: BTreeMap<usize, Probability>) -> BTreeMap<usize, Probability> {
    let z: f32 = m.values().sum();
    if z > 0.0 {
        for v in m.values_mut() {
            *v /= z;
        }
    }
    m
}

fn run_sinkhorn<'a>(
    mu: &'a BTreeMap<usize, Probability>,
    nu: &'a BTreeMap<usize, Probability>,
    iterations: usize,
    tolerance: f32,
    temperature: f32,
) -> Sinkhorn<'a> {
    static METRIC: UniformMetric = UniformMetric::new();
    Sinkhorn::with_params(&METRIC, mu, nu, temperature, iterations, tolerance).minimize()
}

#[test]
fn sinkhorn_identity_cost_is_zero() {
    // μ = ν ⇒ cost = 0 (the only way to match mass is on-diagonal at
    // distance 0 for any 1-1 mapping of identical marginals).
    let mu = btree_721();
    let nu = btree_721();
    let cost = run_sinkhorn(&mu, &nu, 256, 1e-6, 0.025).cost();
    assert_close(cost, 0.0, 1e-3, "identity");
}

#[test]
fn sinkhorn_self_transport_cost_is_zero() {
    // Single unit-mass bucket in both μ and ν ⇒ cost = 0.
    let mut mu: BTreeMap<usize, Probability> = BTreeMap::new();
    mu.insert(7, 1.0);
    let mut nu: BTreeMap<usize, Probability> = BTreeMap::new();
    nu.insert(7, 1.0);
    let cost = run_sinkhorn(&mu, &nu, 64, 1e-6, 0.025).cost();
    assert_close(cost, 0.0, 1e-6, "self");
}

#[test]
fn sinkhorn_preserves_marginals_within_tolerance() {
    // 3x3 uniform fixture, run 50 iterations at low temperature, then
    // assert that the row sums of `flow` ≈ μ and the column sums ≈ ν.
    let mut mu: BTreeMap<usize, Probability> = BTreeMap::new();
    mu.insert(0, 1.0 / 3.0);
    mu.insert(1, 1.0 / 3.0);
    mu.insert(2, 1.0 / 3.0);
    let nu = mu.clone();
    let s = run_sinkhorn(&mu, &nu, 50, 1e-8, 0.025);
    for &x in &[0usize, 1, 2] {
        let row_sum: f32 = (0..3).map(|j| s.flow(&x, &[0, 1, 2][j])).sum();
        assert_close(row_sum, 1.0 / 3.0, 1e-2, &format!("row {x}"));
    }
    for &y in &[0usize, 1, 2] {
        let col_sum: f32 = (0..3).map(|i| s.flow(&[0, 1, 2][i], &y)).sum();
        assert_close(col_sum, 1.0 / 3.0, 1e-2, &format!("col {y}"));
    }
}

#[test]
fn sinkhorn_cost_is_nonnegative() {
    // A non-negative cost is the cheapest assertion that the
    // distance weighting + flow sign convention aren't producing a
    // sign-flipped result.
    let mu: BTreeMap<usize, Probability> = btree_721();
    let mut nu: BTreeMap<usize, Probability> = BTreeMap::new();
    nu.insert(0, 0.1);
    nu.insert(1, 0.2);
    nu.insert(2, 0.7);
    let cost = run_sinkhorn(&mu, &nu, 128, 1e-4, 0.025).cost();
    assert!(cost >= -1e-3, "cost went negative: {cost}");
}

#[test]
fn sinkhorn_uniform_metric_matches_known_emd_on_1d() {
    // 1-point shift: μ = δ_0, ν = δ_1 ⇒ exact EMD = |0 - 1| = 1.0.
    let mut mu: BTreeMap<usize, Probability> = BTreeMap::new();
    mu.insert(0, 1.0);
    let mut nu: BTreeMap<usize, Probability> = BTreeMap::new();
    nu.insert(1, 1.0);
    let cost = run_sinkhorn(&mu, &nu, 256, 1e-6, 0.005).cost();
    assert_close(cost, 1.0, 1e-2, "1-point shift EMD");
}

#[test]
fn sinkhorn_handles_disjoint_supports() {
    // Disjoint supports force every unit of mass to travel the
    // inter-support L1 distance, so cost is at least the L1 gap.
    let mu: BTreeMap<usize, Probability> = normalize({
        let mut m: BTreeMap<usize, Probability> = BTreeMap::new();
        m.insert(0, 0.6);
        m.insert(1, 0.4);
        m
    });
    let nu: BTreeMap<usize, Probability> = normalize({
        let mut m: BTreeMap<usize, Probability> = BTreeMap::new();
        m.insert(2, 0.6);
        m.insert(3, 0.4);
        m
    });
    let cost = run_sinkhorn(&mu, &nu, 256, 1e-6, 0.005).cost();
    assert!(
        cost >= 2.0,
        "disjoint-support EMD should be ≥ 2.0 (L1 gap), got {cost}",
    );
}

#[test]
fn sinkhorn_respects_iteration_cap() {
    // The `iterations` cap must be honored. We assert that on a
    // fixture that needs more than `cap` iterations to converge at
    // tight tolerance, the call still returns in time (we just
    // measure the resulting cost; the upper bound is enforced by
    // the `for _ in 0..iterations` loop, not by post-hoc).
    let mu = btree_721();
    let nu = btree_721();
    let cap = 5usize;
    let s = run_sinkhorn(&mu, &nu, cap, 1e-12, 0.025);
    // After only 5 iterations at tight tolerance the cost should
    // still be finite and the marginal constraints at least
    // approximately satisfied (within 0.1).
    let cost = s.cost();
    assert!(cost.is_finite(), "cost must be finite after cap");
    for (i, &x) in [0usize, 1, 2].iter().enumerate() {
        let row_sum: f32 = (0..3).map(|j| s.flow(&x, &[0, 1, 2][j])).sum();
        assert_close(row_sum, mu[&[0, 1, 2][i]], 0.1, &format!("row {x}"));
    }
}

#[test]
fn greedy_uniform_metric_matches_sinkhorn_on_uniform_marginals() {
    // On identical uniform marginals the greedy algorithm happens
    // to find the optimal coupling (mass ships on-diagonal), so its
    // cost must be ≈ 0.
    let mu: BTreeMap<usize, Probability> = normalize({
        let mut m: BTreeMap<usize, Probability> = BTreeMap::new();
        m.insert(0, 1.0);
        m.insert(1, 1.0);
        m.insert(2, 1.0);
        m
    });
    let nu = mu.clone();
    let metric = UniformMetric::new();
    let cost = Greedy::new(&metric, &mu, &nu).minimize().cost();
    assert_close(cost, 0.0, 1e-6, "greedy uniform");
}

#[test]
fn greedy_one_point_shift_cost_is_one() {
    // Same 1-point shift as the Sinkhorn test, so a downstream
    // consumer can pin the greedy impl against the sinkhorn impl.
    let mut mu: BTreeMap<usize, Probability> = BTreeMap::new();
    mu.insert(0, 1.0);
    let mut nu: BTreeMap<usize, Probability> = BTreeMap::new();
    nu.insert(1, 1.0);
    let metric = UniformMetric::new();
    let cost = Greedy::new(&metric, &mu, &nu).minimize().cost();
    assert_close(cost, 1.0, 1e-6, "greedy 1-point shift");
}
