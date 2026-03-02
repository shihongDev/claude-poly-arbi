use rand_distr::{Distribution, Normal};

use crate::monte_carlo::{MonteCarloParams, MonteCarloResult};

/// Importance sampling for tail-risk binary contracts (probability < 1%).
///
/// Standard Monte Carlo is inefficient for rare events: with P = 0.001,
/// you need ~1M paths just to get ~1000 "hits". Importance sampling shifts
/// the sampling distribution toward the rare event and corrects with
/// likelihood ratios, achieving 100-10,000x variance reduction.
///
/// Method: shift the drift by `tilt` so that the rare event becomes common
/// under the tilted distribution, then weight each path by f(x)/g(x).
pub struct ImportanceSampler {
    params: MonteCarloParams,
    tilt: f64,
}

impl ImportanceSampler {
    pub fn new(params: MonteCarloParams) -> Self {
        let tilt = Self::compute_optimal_tilt(&params);
        Self { params, tilt }
    }

    /// Create with an explicit tilt parameter.
    pub fn with_tilt(params: MonteCarloParams, tilt: f64) -> Self {
        Self { params, tilt }
    }

    /// Compute the optimal exponential tilt that maximizes the probability
    /// of the rare event under the tilted distribution.
    ///
    /// For a digital option with strike K, we want to shift the mean
    /// of log(S_T) so that it's centered near log(K).
    fn compute_optimal_tilt(params: &MonteCarloParams) -> f64 {
        let log_moneyness = (params.strike / params.initial_price).ln();
        let t = params.time_horizon;
        let sigma = params.volatility;

        // Tilt to center the log-normal distribution at the strike
        // θ* ≈ (log(K/S_0) - (μ - σ²/2)T) / (σ²T)
        let drift_term = (params.drift - 0.5 * sigma * sigma) * t;
        let tilt = (log_moneyness - drift_term) / (sigma * sigma * t);

        tilt
    }

    /// Run importance sampling simulation.
    pub fn run(&self) -> MonteCarloResult {
        let mut rng = rand::rng();
        let tilted_mean = self.tilt * self.params.volatility * self.params.time_horizon.sqrt();

        let tilted_dist = Normal::new(tilted_mean, 1.0).unwrap();

        let drift_term = (self.params.drift - 0.5 * self.params.volatility.powi(2))
            * self.params.time_horizon;
        let vol_term = self.params.volatility * self.params.time_horizon.sqrt();

        let mut weighted_payoffs = Vec::with_capacity(self.params.n_paths);

        for _ in 0..self.params.n_paths {
            // Sample from tilted distribution
            let z: f64 = tilted_dist.sample(&mut rng);
            let s_t = self.params.initial_price * (drift_term + vol_term * z).exp();
            let payoff = if s_t > self.params.strike { 1.0 } else { 0.0 };

            // Likelihood ratio: f(z) / g(z)
            // f = N(0, 1), g = N(tilted_mean, 1)
            let log_ratio = -0.5 * z * z + 0.5 * (z - tilted_mean).powi(2);
            let ratio = log_ratio.exp();

            weighted_payoffs.push(payoff * ratio);
        }

        let n = self.params.n_paths as f64;
        let p_hat: f64 = weighted_payoffs.iter().sum::<f64>() / n;
        let p_clamped = p_hat.clamp(0.0, 1.0);

        // Variance of the weighted estimator
        let variance: f64 =
            weighted_payoffs.iter().map(|&w| (w - p_hat).powi(2)).sum::<f64>() / (n - 1.0);
        let se = (variance / n).sqrt();

        MonteCarloResult {
            probability: p_clamped,
            standard_error: se,
            confidence_interval: (
                (p_clamped - 1.96 * se).max(0.0),
                (p_clamped + 1.96 * se).min(1.0),
            ),
            n_paths: self.params.n_paths,
        }
    }

    /// Effective sample size for importance sampling.
    /// ESS = (Σw_i)² / Σ(w_i²) — measures how many "effective" samples we have.
    pub fn effective_sample_size(&self) -> f64 {
        // Run a quick simulation to estimate ESS
        let mut rng = rand::rng();
        let tilted_mean = self.tilt * self.params.volatility * self.params.time_horizon.sqrt();
        let tilted_dist = Normal::new(tilted_mean, 1.0).unwrap();

        let n = 1000.min(self.params.n_paths);
        let mut weights = Vec::with_capacity(n);

        for _ in 0..n {
            let z: f64 = tilted_dist.sample(&mut rng);
            let log_ratio = -0.5 * z * z + 0.5 * (z - tilted_mean).powi(2);
            weights.push(log_ratio.exp());
        }

        let sum_w: f64 = weights.iter().sum();
        let sum_w2: f64 = weights.iter().map(|w| w * w).sum();

        if sum_w2 > 0.0 {
            (sum_w * sum_w) / sum_w2
        } else {
            0.0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::monte_carlo::run_monte_carlo;

    #[test]
    fn test_importance_sampling_rare_event() {
        // Deep OTM: probability should be very small
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 2.5, // ~3.3σ move needed
            n_paths: 50_000,
        };

        let is_result = ImportanceSampler::new(params.clone()).run();
        let mc_result = run_monte_carlo(&params);

        // IS should give a non-zero estimate even for rare events
        // Plain MC might give 0 or very noisy estimate
        assert!(
            is_result.probability > 0.0,
            "IS should detect the rare event"
        );

        // IS SE should be lower than plain MC for rare events
        // (when MC doesn't find any hits, its SE is 0 but that's misleading)
        if mc_result.probability > 0.0 {
            // Both found it — IS should have better SE
            assert!(
                is_result.standard_error <= mc_result.standard_error * 2.0,
                "IS SE ({}) should be comparable or better than MC SE ({})",
                is_result.standard_error,
                mc_result.standard_error
            );
        }
    }

    #[test]
    fn test_importance_sampling_moderate_event() {
        // Moderate event: IS and MC should agree
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 1.2,
            n_paths: 50_000,
        };

        let is_result = ImportanceSampler::new(params.clone()).run();
        let mc_result = run_monte_carlo(&params);

        assert!(
            (is_result.probability - mc_result.probability).abs() < 0.05,
            "IS ({}) and MC ({}) should agree for moderate events",
            is_result.probability,
            mc_result.probability
        );
    }

    #[test]
    fn test_ess_reasonable() {
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 2.0,
            n_paths: 10_000,
        };

        let sampler = ImportanceSampler::new(params);
        let ess = sampler.effective_sample_size();
        assert!(ess > 10.0, "ESS too low: {}", ess);
    }
}
