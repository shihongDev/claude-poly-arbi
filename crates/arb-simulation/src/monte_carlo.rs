use rand_distr::{Distribution, StandardNormal};
use serde::{Deserialize, Serialize};

/// Parameters for a binary contract Monte Carlo simulation.
///
/// Models the underlying as Geometric Brownian Motion:
/// `S_T = S_0 × exp((μ - σ²/2)T + σ√T × Z)`
///
/// The contract pays $1 if `S_T > strike`, else $0.
/// Probability estimate: `p_hat = mean(payoffs)`, SE = `sqrt(p(1-p)/N)`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloParams {
    pub initial_price: f64,
    pub drift: f64,
    pub volatility: f64,
    pub time_horizon: f64,
    pub strike: f64,
    pub n_paths: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonteCarloResult {
    pub probability: f64,
    pub standard_error: f64,
    pub confidence_interval: (f64, f64),
    pub n_paths: usize,
}

impl MonteCarloResult {
    /// Z-score for 95% confidence interval.
    const Z_95: f64 = 1.96;

    pub fn from_payoffs(payoffs: &[f64], n_paths: usize) -> Self {
        let p_hat: f64 = payoffs.iter().sum::<f64>() / n_paths as f64;
        let se = (p_hat * (1.0 - p_hat) / n_paths as f64).sqrt();
        let ci = (
            (p_hat - Self::Z_95 * se).max(0.0),
            (p_hat + Self::Z_95 * se).min(1.0),
        );
        Self {
            probability: p_hat,
            standard_error: se,
            confidence_interval: ci,
            n_paths,
        }
    }
}

/// Run a plain Monte Carlo simulation for a binary digital contract.
pub fn run_monte_carlo(params: &MonteCarloParams) -> MonteCarloResult {
    let mut rng = rand::rng();
    let normal = StandardNormal;

    let drift_term = (params.drift - 0.5 * params.volatility.powi(2)) * params.time_horizon;
    let vol_term = params.volatility * params.time_horizon.sqrt();

    let mut payoffs = Vec::with_capacity(params.n_paths);

    for _ in 0..params.n_paths {
        let z: f64 = normal.sample(&mut rng);
        let s_t = params.initial_price * (drift_term + vol_term * z).exp();
        let payoff = if s_t > params.strike { 1.0 } else { 0.0 };
        payoffs.push(payoff);
    }

    MonteCarloResult::from_payoffs(&payoffs, params.n_paths)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_monte_carlo_convergence() {
        // ATM option: set drift = σ²/2 so the GBM median equals S_0,
        // giving P(S_T > K) = 0.50 exactly.
        let sigma = 0.2;
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000,
        };

        let result = run_monte_carlo(&params);
        assert!(
            (result.probability - 0.5).abs() < 0.02,
            "Expected ~0.50, got {}",
            result.probability
        );
        assert!(result.standard_error < 0.01);
    }

    #[test]
    fn test_deep_itm() {
        // Strike far below → probability ~1.0
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.2,
            time_horizon: 1.0,
            strike: 0.1,
            n_paths: 10_000,
        };

        let result = run_monte_carlo(&params);
        assert!(result.probability > 0.95);
    }

    #[test]
    fn test_deep_otm() {
        // Strike far above → probability ~0.0
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.2,
            time_horizon: 1.0,
            strike: 5.0,
            n_paths: 10_000,
        };

        let result = run_monte_carlo(&params);
        assert!(result.probability < 0.05);
    }

    #[test]
    fn test_se_decreases_with_n() {
        let params_small = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 1_000,
        };
        let params_large = MonteCarloParams {
            n_paths: 100_000,
            ..params_small.clone()
        };

        let r_small = run_monte_carlo(&params_small);
        let r_large = run_monte_carlo(&params_large);

        // SE should decrease roughly as 1/√N → 10x more paths → ~3.16x smaller SE
        assert!(r_large.standard_error < r_small.standard_error);
    }
}
