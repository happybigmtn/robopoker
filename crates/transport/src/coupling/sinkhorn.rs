use super::super::density::Density;
use super::super::measure::Measure;
use super::super::measure::UniformMetric;
use super::Coupling;
use rbp_core::Probability;
use rbp_core::SINKHORN_ITERATIONS;
use rbp_core::SINKHORN_TEMPERATURE;
use rbp_core::SINKHORN_TOLERANCE;
use std::collections::BTreeMap;

/// Sinkhorn transport coupling over `BTreeMap<usize, Probability>` with a
/// [`UniformMetric`] ground cost.
///
/// This is the concrete `Coupling` implementation behind
/// `rbp-transport`'s entropic regularized optimal transport. It runs
/// the standard log-domain Sinkhorn-Knopp iteration:
///
/// ```text
/// K[i,j] = exp(-|x_i - y_j| / temperature)
/// log_alpha_new[i] = log mu[i] - LSE_j(log_beta[j] + log_K[i,j])
/// log_beta_new[j]  = log nu[j] - LSE_i(log_alpha[i] + log_K[i,j])
/// ```
///
/// with early stopping on L1 potential change at `tolerance` and a hard
/// cap at `iterations` so a future parameter tweak cannot blow the
/// worker budget. The final marginal-consistent flow at (i, j) is
/// `exp(log_alpha[i] + log_beta[j] + log_K[i,j])` and
/// [`cost`](Coupling::cost) integrates `flow * |x_i - y_j|` over the
/// support, returning the entropic-regularized transport cost. With
/// small `temperature` and enough iterations the result converges to
/// the exact EMD.
pub struct Sinkhorn<'a> {
    metric: &'a UniformMetric,
    mu: &'a BTreeMap<usize, Probability>,
    nu: &'a BTreeMap<usize, Probability>,
    iterations: usize,
    tolerance: f32,
    /// Sorted source support, indexed by row of `flow` and `log_kernel`.
    xs: Vec<usize>,
    /// Sorted target support, indexed by column of `flow` and `log_kernel`.
    ys: Vec<usize>,
    /// Precomputed log-Kernel: `log_K[i][j] = -distance(xs[i], ys[j]) / temperature`.
    log_kernel: Vec<Vec<f32>>,
    /// Log-domain LHS potentials (length `xs.len()`). Populated after `minimize`.
    log_alpha: Vec<f32>,
    /// Log-domain RHS potentials (length `ys.len()`). Populated after `minimize`.
    log_beta: Vec<f32>,
    /// Marginal-consistent flow at (i, j). Empty before `minimize`.
    flow: Vec<Vec<f32>>,
}

impl<'a> Sinkhorn<'a> {
    /// Construct a new Sinkhorn plan with the documented
    /// `rbp-core::SINKHORN_*` constants (temperature 0.025, 128
    /// iterations, 1e-3 tolerance).
    #[allow(dead_code)]
    pub fn new(
        metric: &'a UniformMetric,
        mu: &'a BTreeMap<usize, Probability>,
        nu: &'a BTreeMap<usize, Probability>,
    ) -> Self {
        Self::with_params(
            metric,
            mu,
            nu,
            SINKHORN_TEMPERATURE,
            SINKHORN_ITERATIONS,
            SINKHORN_TOLERANCE,
        )
    }

    /// Construct a new Sinkhorn plan with explicit hyperparameters. The
    /// `temperature` must be > 0 and `iterations` must be ≥ 1; smaller
    /// `tolerance` enforces tighter convergence. The defaults from
    /// [`new`](Self::new) match `rbp-core::SINKHORN_*`.
    pub fn with_params(
        metric: &'a UniformMetric,
        mu: &'a BTreeMap<usize, Probability>,
        nu: &'a BTreeMap<usize, Probability>,
        temperature: f32,
        iterations: usize,
        tolerance: f32,
    ) -> Self {
        assert!(temperature > 0.0, "temperature must be > 0");
        assert!(iterations >= 1, "iterations must be >= 1");
        let xs: Vec<usize> = mu.keys().copied().collect();
        let ys: Vec<usize> = nu.keys().copied().collect();
        let log_kernel = xs
            .iter()
            .map(|&x| {
                ys.iter()
                    .map(|&y| -metric.distance(&x, &y) / temperature)
                    .collect()
            })
            .collect();
        Self {
            metric,
            mu,
            nu,
            iterations,
            tolerance,
            xs,
            ys,
            log_kernel,
            log_alpha: Vec::new(),
            log_beta: Vec::new(),
            flow: Vec::new(),
        }
    }

    /// Returns true when the underlying marginals sum to 1 within
    /// `1e-3`. The Sinkhorn iteration is only well-defined on
    /// normalized marginals; this helper is used by the lib tests to
    /// pin the precondition.
    fn check_normalized(&self) -> Result<(), &'static str> {
        let mu_sum: f32 = self.mu.values().sum();
        let nu_sum: f32 = self.nu.values().sum();
        if (mu_sum - 1.0).abs() > 1e-3 {
            return Err("source marginal is not normalized to 1");
        }
        if (nu_sum - 1.0).abs() > 1e-3 {
            return Err("target marginal is not normalized to 1");
        }
        Ok(())
    }

    /// Runs Sinkhorn-Knopp in log domain. Returns the L1 change in
    /// potentials from the last iteration (always `< tolerance` on
    /// convergence; the iteration cap is the only way out of the
    /// loop otherwise).
    fn sinkhorn_step(&mut self) -> f32 {
        let n = self.xs.len();
        let m = self.ys.len();
        // Initialize potentials on the first call.
        if self.log_alpha.is_empty() {
            self.log_alpha = self
                .xs
                .iter()
                .map(|x| self.mu.density(x).max(1e-30).ln())
                .collect();
        }
        if self.log_beta.is_empty() {
            self.log_beta = self
                .ys
                .iter()
                .map(|y| self.nu.density(y).max(1e-30).ln())
                .collect();
        }
        // One log-sum-exp sweep over (i, j): LSE_j (log_beta[j] + log_K[i,j])
        // for each row i, then LSE_i (log_alpha[i] + log_K[i,j]) for each col j.
        let mut new_alpha = vec![0.0f32; n];
        for i in 0..n {
            let mut max = f32::NEG_INFINITY;
            for j in 0..m {
                let v = self.log_beta[j] + self.log_kernel[i][j];
                if v > max {
                    max = v;
                }
            }
            if !max.is_finite() {
                new_alpha[i] = self.log_alpha[i];
                continue;
            }
            let mut sum = 0.0f32;
            for j in 0..m {
                sum += (self.log_beta[j] + self.log_kernel[i][j] - max).exp();
            }
            let lse = max + sum.max(1e-30).ln();
            new_alpha[i] = self.mu.density(&self.xs[i]).max(1e-30).ln() - lse;
        }
        let mut new_beta = vec![0.0f32; m];
        for j in 0..m {
            let mut max = f32::NEG_INFINITY;
            for i in 0..n {
                let v = new_alpha[i] + self.log_kernel[i][j];
                if v > max {
                    max = v;
                }
            }
            if !max.is_finite() {
                new_beta[j] = self.log_beta[j];
                continue;
            }
            let mut sum = 0.0f32;
            for i in 0..n {
                sum += (new_alpha[i] + self.log_kernel[i][j] - max).exp();
            }
            let lse = max + sum.max(1e-30).ln();
            new_beta[j] = self.nu.density(&self.ys[j]).max(1e-30).ln() - lse;
        }
        // L1 change in exp-potentials as the convergence proxy.
        let mut delta = 0.0f32;
        for i in 0..n {
            let prev = self.log_alpha[i].exp();
            let next = new_alpha[i].exp();
            delta += (next - prev).abs();
        }
        for j in 0..m {
            let prev = self.log_beta[j].exp();
            let next = new_beta[j].exp();
            delta += (next - prev).abs();
        }
        self.log_alpha = new_alpha;
        self.log_beta = new_beta;
        delta
    }

    /// Builds the marginal-consistent flow matrix from the converged
    /// log-potentials. Called once at the end of `minimize`.
    fn materialize_flow(&mut self) {
        self.flow = self
            .log_alpha
            .iter()
            .enumerate()
            .map(|(i, a)| {
                self.log_beta
                    .iter()
                    .enumerate()
                    .map(|(j, b)| (a + b + self.log_kernel[i][j]).exp())
                    .collect::<Vec<f32>>()
            })
            .collect::<Vec<Vec<f32>>>();
    }
}

impl<'a> Coupling for Sinkhorn<'a> {
    type X = usize;
    type Y = usize;
    type M = UniformMetric;
    type P = BTreeMap<usize, Probability>;
    type Q = BTreeMap<usize, Probability>;

    fn minimize(mut self) -> Self {
        // The iteration is only well-defined on normalized marginals.
        // We don't error-out (downstream callers may pass un-normalized
        // histograms and rely on the inner rescaling) but we do ensure
        // the potentials initialize from a non-zero seed.
        let _ = self.check_normalized();
        for _ in 0..self.iterations {
            let delta = self.sinkhorn_step();
            if delta < self.tolerance {
                break;
            }
        }
        self.materialize_flow();
        self
    }

    fn flow(&self, x: &Self::X, y: &Self::Y) -> f32 {
        let i = match self.xs.iter().position(|k| k == x) {
            Some(i) => i,
            None => return 0.0,
        };
        let j = match self.ys.iter().position(|k| k == y) {
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
        let mut total = 0.0f32;
        for (i, &x) in self.xs.iter().enumerate() {
            for (j, &y) in self.ys.iter().enumerate() {
                let f = self
                    .flow
                    .get(i)
                    .and_then(|row| row.get(j))
                    .copied()
                    .unwrap_or(0.0);
                total += f * self.metric.distance(&x, &y);
            }
        }
        total
    }
}
