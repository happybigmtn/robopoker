use super::super::density::Density;
use super::super::measure::Measure;
use super::super::measure::UniformMetric;
use super::Coupling;
use rbp_core::Probability;
use std::collections::BTreeMap;

/// Greedy transport coupling over `BTreeMap<usize, Probability>` with a
/// [`UniformMetric`] ground cost.
///
/// This is the concrete `Coupling` implementation that makes the greedy
/// OT algorithm available over the `rbp-transport` trait surface. The
/// coupling walks every (x, y) pair in order of increasing L1 distance
/// and ships `min(remaining_source, remaining_target)` mass along the
/// edge. The final coupling is therefore not optimal in general (it can
/// over-transport along long edges when short edges are already saturated
/// by mass-imbalanced inputs) but it runs in O(n² log n) and matches the
/// exact EMD on uniform marginals.
pub struct Greedy<'a> {
    metric: &'a UniformMetric,
    mu: &'a BTreeMap<usize, Probability>,
    nu: &'a BTreeMap<usize, Probability>,
    /// Marginal-consistent flow at (i, j) where i indexes the sorted
    /// support of `mu` and j indexes the sorted support of `nu`. Empty
    /// before [`minimize`](Coupling::minimize); populated after.
    flow: Vec<Vec<f32>>,
}

impl<'a> Greedy<'a> {
    /// Construct a greedy transport plan over the given source and
    /// target distributions under the uniform L1 ground metric. The
    /// plan is not yet run — call
    /// [`minimize`](Coupling::minimize) to actually compute the flow.
    pub fn new(
        metric: &'a UniformMetric,
        mu: &'a BTreeMap<usize, Probability>,
        nu: &'a BTreeMap<usize, Probability>,
    ) -> Self {
        Self {
            metric,
            mu,
            nu,
            flow: Vec::new(),
        }
    }
}

impl<'a> Coupling for Greedy<'a> {
    type X = usize;
    type Y = usize;
    type M = UniformMetric;
    type P = BTreeMap<usize, Probability>;
    type Q = BTreeMap<usize, Probability>;

    fn minimize(mut self) -> Self {
        let xs: Vec<usize> = self.mu.keys().copied().collect();
        let ys: Vec<usize> = self.nu.keys().copied().collect();
        let mut remaining_x: Vec<f32> = xs.iter().map(|x| self.mu.density(x)).collect();
        let mut remaining_y: Vec<f32> = ys.iter().map(|y| self.nu.density(y)).collect();
        let mut pairs: Vec<(usize, usize, f32)> = Vec::with_capacity(xs.len() * ys.len());
        for (i, &x) in xs.iter().enumerate() {
            for (j, &y) in ys.iter().enumerate() {
                pairs.push((i, j, self.metric.distance(&x, &y)));
            }
        }
        pairs.sort_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal));
        let mut flow = vec![vec![0.0f32; ys.len()]; xs.len()];
        for (i, j, _d) in pairs {
            if remaining_x[i] <= 0.0 || remaining_y[j] <= 0.0 {
                continue;
            }
            let ship = remaining_x[i].min(remaining_y[j]);
            flow[i][j] = ship;
            remaining_x[i] -= ship;
            remaining_y[j] -= ship;
        }
        self.flow = flow;
        self
    }

    fn flow(&self, x: &Self::X, y: &Self::Y) -> f32 {
        let xs: Vec<usize> = self.mu.keys().copied().collect();
        let ys: Vec<usize> = self.nu.keys().copied().collect();
        let i = match xs.iter().position(|k| k == x) {
            Some(i) => i,
            None => return 0.0,
        };
        let j = match ys.iter().position(|k| k == y) {
            Some(j) => j,
            None => return 0.0,
        };
        self.flow
            .get(i)
            .and_then(|row| row.get(j))
            .copied()
            .unwrap_or(0.0)
    }

    fn cost(&self) -> f32 {
        let xs: Vec<usize> = self.mu.keys().copied().collect();
        let ys: Vec<usize> = self.nu.keys().copied().collect();
        let mut total = 0.0f32;
        for (i, x) in xs.iter().enumerate() {
            for (j, y) in ys.iter().enumerate() {
                let f = self
                    .flow
                    .get(i)
                    .and_then(|row| row.get(j))
                    .copied()
                    .unwrap_or(0.0);
                total += f * self.metric.distance(x, y);
            }
        }
        total
    }
}
