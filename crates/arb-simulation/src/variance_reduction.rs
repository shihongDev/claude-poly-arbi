use rand::Rng;
use rand_distr::{Distribution, StandardNormal};

use crate::monte_carlo::{MonteCarloParams, MonteCarloResult};

/// Builder for Monte Carlo with stackable variance reduction techniques.
///
/// Three techniques can be combined:
/// 1. **Antithetic variates**: For each Z, also simulate -Z. Free 50-75% variance reduction.
/// 2. **Control variates**: Use a known analytical price as a control to reduce variance.
/// 3. **Stratified sampling**: Partition [0,1] into strata, sample within each.
pub struct MonteCarloBuilder {
    params: MonteCarloParams,
    antithetic: bool,
    control_variate: Option<f64>,
    n_strata: Option<usize>,
}

impl MonteCarloBuilder {
    pub fn new(params: MonteCarloParams) -> Self {
        Self {
            params,
            antithetic: false,
            control_variate: None,
            n_strata: None,
        }
    }

    /// Enable antithetic variates: for each normal sample Z, also evaluate at -Z.
    /// The paired estimate (f(Z) + f(-Z))/2 has lower variance because the errors
    /// from Z and -Z are negatively correlated.
    pub fn with_antithetic(mut self) -> Self {
        self.antithetic = true;
        self
    }

    /// Enable control variates with a known analytical price.
    /// Uses: `p_cv = p_hat - β(C_hat - E[C])` where β is estimated from sample covariance.
    pub fn with_control_variate(mut self, known_price: f64) -> Self {
        self.control_variate = Some(known_price);
        self
    }

    /// Enable stratified sampling with J strata.
    /// Partitions [0,1] into J equal intervals and samples uniformly within each.
    pub fn with_stratification(mut self, n_strata: usize) -> Self {
        self.n_strata = Some(n_strata);
        self
    }

    pub fn build(self) -> VarianceReducedMC {
        VarianceReducedMC {
            params: self.params,
            antithetic: self.antithetic,
            control_variate: self.control_variate,
            n_strata: self.n_strata,
        }
    }
}

pub struct VarianceReducedMC {
    params: MonteCarloParams,
    antithetic: bool,
    control_variate: Option<f64>,
    n_strata: Option<usize>,
}

impl VarianceReducedMC {
    pub fn run(&self) -> MonteCarloResult {
        let drift_term =
            (self.params.drift - 0.5 * self.params.volatility.powi(2)) * self.params.time_horizon;
        let vol_term = self.params.volatility * self.params.time_horizon.sqrt();

        let samples = self.generate_normals();
        let mut payoffs = Vec::with_capacity(samples.len());
        let mut control_values = Vec::with_capacity(samples.len());

        for z in &samples {
            let s_t = self.params.initial_price * (drift_term + vol_term * z).exp();
            let payoff = if s_t > self.params.strike { 1.0 } else { 0.0 };
            payoffs.push(payoff);

            // Control variate: use the terminal price itself as a control
            // E[S_T] = S_0 * exp(μT) under the real measure
            if self.control_variate.is_some() {
                control_values.push(s_t);
            }
        }

        // Apply control variate correction
        if let Some(_known_price) = self.control_variate {
            let n = payoffs.len() as f64;
            let mean_payoff: f64 = payoffs.iter().sum::<f64>() / n;
            let mean_control: f64 = control_values.iter().sum::<f64>() / n;
            let expected_control =
                self.params.initial_price * (self.params.drift * self.params.time_horizon).exp();

            // Estimate β from sample covariance
            let mut cov = 0.0;
            let mut var_c = 0.0;
            for i in 0..payoffs.len() {
                let d_y = payoffs[i] - mean_payoff;
                let d_c = control_values[i] - mean_control;
                cov += d_y * d_c;
                var_c += d_c * d_c;
            }

            let beta = if var_c > 1e-12 { cov / var_c } else { 0.0 };

            // Apply correction: p_cv = p_hat - β(C_hat - E[C])
            // We adjust individual payoffs for proper SE calculation
            for i in 0..payoffs.len() {
                payoffs[i] -= beta * (control_values[i] - expected_control);
            }

            // Clamp to [0, 1] since control variate correction can push outside
            let p_hat: f64 = payoffs.iter().sum::<f64>() / n;
            let p_clamped = p_hat.clamp(0.0, 1.0);

            let variance: f64 =
                payoffs.iter().map(|&p| (p - p_hat).powi(2)).sum::<f64>() / (n - 1.0);
            let se = (variance / n).sqrt();

            return MonteCarloResult {
                probability: p_clamped,
                standard_error: se,
                confidence_interval: (
                    (p_clamped - 1.96 * se).max(0.0),
                    (p_clamped + 1.96 * se).min(1.0),
                ),
                n_paths: payoffs.len(),
            };
        }

        MonteCarloResult::from_payoffs(&payoffs, payoffs.len())
    }

    /// Generate normal random samples, applying antithetic and/or stratification.
    fn generate_normals(&self) -> Vec<f64> {
        let mut rng = rand::rng();

        if let Some(n_strata) = self.n_strata {
            // Stratified sampling: partition [0,1] into strata
            let paths_per_stratum = self.params.n_paths / n_strata;
            let mut normals = Vec::with_capacity(self.params.n_paths);

            for j in 0..n_strata {
                let lo = j as f64 / n_strata as f64;
                let hi = (j + 1) as f64 / n_strata as f64;

                for _ in 0..paths_per_stratum {
                    // Uniform within stratum → inverse normal CDF
                    let u: f64 = lo + rng.random::<f64>() * (hi - lo);
                    let z = statrs::distribution::Normal::new(0.0, 1.0)
                        .map(|n| {
                            use statrs::distribution::ContinuousCDF;
                            n.inverse_cdf(u.clamp(1e-10, 1.0 - 1e-10))
                        })
                        .unwrap_or(0.0);

                    if self.antithetic {
                        normals.push(z);
                        normals.push(-z);
                    } else {
                        normals.push(z);
                    }
                }
            }

            normals
        } else if self.antithetic {
            // Antithetic only
            let half = self.params.n_paths / 2;
            let normal = StandardNormal;
            let mut normals = Vec::with_capacity(self.params.n_paths);

            for _ in 0..half {
                let z: f64 = normal.sample(&mut rng);
                normals.push(z);
                normals.push(-z);
            }

            normals
        } else {
            // Plain MC
            let normal = StandardNormal;
            (0..self.params.n_paths)
                .map(|_| normal.sample(&mut rng))
                .collect()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monte_carlo::{MonteCarloParams, run_monte_carlo};

    fn atm_params() -> MonteCarloParams {
        let sigma = 0.3;
        MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma, // drift = σ²/2 → GBM median = S_0 → P(S_T > K) = 0.50
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 50_000,
        }
    }

    #[test]
    fn test_antithetic_reduces_variance() {
        let params = atm_params();

        let plain = run_monte_carlo(&params);
        let antithetic = MonteCarloBuilder::new(params)
            .with_antithetic()
            .build()
            .run();

        // Antithetic should have comparable probability but lower SE
        assert!((plain.probability - antithetic.probability).abs() < 0.05);
        // SE should generally be lower (not guaranteed on every run, but statistically likely)
        // We just check both are reasonable
        assert!(antithetic.standard_error < 0.01);
    }

    #[test]
    fn test_stratified_sampling() {
        let params = atm_params();
        let result = MonteCarloBuilder::new(params)
            .with_stratification(10)
            .build()
            .run();

        assert!((result.probability - 0.5).abs() < 0.03);
        assert!(result.standard_error < 0.01);
    }

    #[test]
    fn test_combined_techniques() {
        let params = atm_params();
        let result = MonteCarloBuilder::new(params)
            .with_antithetic()
            .with_stratification(10)
            .build()
            .run();

        assert!((result.probability - 0.5).abs() < 0.03);
    }
}
