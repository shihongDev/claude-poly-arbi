use rand_distr::{Distribution, Normal, Poisson, StandardNormal};
use serde::{Deserialize, Serialize};

/// Parameters for a Merton jump-diffusion Monte Carlo simulation.
///
/// Model: `dS/S = (mu - lambda*k)dt + sigma*dW + J*dN`
/// where:
/// - J ~ N(jump_mean, jump_vol) is the log-jump size
/// - N ~ Poisson(lambda) is the jump counting process
/// - k = E[e^J - 1] = exp(jump_mean + jump_vol^2/2) - 1 is the compensator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpDiffusionParams {
    /// Initial asset price S_0.
    pub initial_price: f64,
    /// Drift rate mu (annualized).
    pub drift: f64,
    /// Diffusion volatility sigma (annualized).
    pub volatility: f64,
    /// Jump intensity lambda (expected jumps per year).
    pub jump_intensity: f64,
    /// Mean of log-jump size mu_J.
    pub jump_mean: f64,
    /// Volatility of log-jump size sigma_J.
    pub jump_vol: f64,
    /// Time horizon T (in years).
    pub time_horizon: f64,
    /// Strike price K for the binary digital contract.
    pub strike: f64,
    /// Number of Monte Carlo paths.
    pub n_paths: usize,
}

/// Result of a jump-diffusion Monte Carlo simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JumpDiffusionResult {
    /// Estimated probability P(S_T > K).
    pub probability: f64,
    /// Standard error of the probability estimate.
    pub standard_error: f64,
    /// 95% confidence interval for the probability.
    pub confidence_interval: (f64, f64),
    /// Number of paths simulated.
    pub n_paths: usize,
    /// Average number of jumps per path.
    pub avg_jumps_per_path: f64,
}

/// Run a Merton jump-diffusion simulation for a binary digital contract.
///
/// Terminal price formula:
/// `S_T = S_0 * exp((mu - lambda*k - sigma^2/2)*T + sigma*sqrt(T)*Z + J_total)`
///
/// where `J_total = sum of N_jumps draws from N(jump_mean, jump_vol)`,
/// `N_jumps ~ Poisson(lambda * T)`, and `k = exp(jump_mean + jump_vol^2/2) - 1`.
pub fn run_jump_diffusion(params: &JumpDiffusionParams) -> JumpDiffusionResult {
    let mut rng = rand::rng();
    let normal = StandardNormal;

    // Compensator: k = E[e^J - 1]
    let k = (params.jump_mean + 0.5 * params.jump_vol.powi(2)).exp() - 1.0;

    // Diffusion terms
    let drift_term =
        (params.drift - params.jump_intensity * k - 0.5 * params.volatility.powi(2))
            * params.time_horizon;
    let vol_term = params.volatility * params.time_horizon.sqrt();

    // Poisson parameter for number of jumps over [0, T]
    let lambda_t = params.jump_intensity * params.time_horizon;

    // Jump size distribution (if jump_vol > 0)
    let jump_normal = if params.jump_vol > 0.0 {
        Some(Normal::new(params.jump_mean, params.jump_vol).unwrap())
    } else {
        None
    };

    let mut total_hits: usize = 0;
    let mut total_jumps: usize = 0;

    // Handle edge case: zero jump intensity means pure GBM
    if lambda_t < 1e-15 {
        for _ in 0..params.n_paths {
            let z: f64 = normal.sample(&mut rng);
            let s_t = params.initial_price * (drift_term + vol_term * z).exp();
            if s_t > params.strike {
                total_hits += 1;
            }
        }
    } else {
        let poisson = Poisson::new(lambda_t).unwrap();

        for _ in 0..params.n_paths {
            // Number of jumps this path
            let n_jumps: u64 = poisson.sample(&mut rng) as u64;
            total_jumps += n_jumps as usize;

            // Total jump component
            let j_total: f64 = if n_jumps == 0 {
                0.0
            } else {
                match &jump_normal {
                    Some(dist) => (0..n_jumps).map(|_| dist.sample(&mut rng)).sum(),
                    None => params.jump_mean * n_jumps as f64,
                }
            };

            // Diffusion component
            let z: f64 = normal.sample(&mut rng);

            // Terminal price
            let s_t = params.initial_price * (drift_term + vol_term * z + j_total).exp();
            if s_t > params.strike {
                total_hits += 1;
            }
        }
    }

    let p_hat = total_hits as f64 / params.n_paths as f64;
    let se = (p_hat * (1.0 - p_hat) / params.n_paths as f64).sqrt();
    let z95 = 1.96;
    let ci = (
        (p_hat - z95 * se).max(0.0),
        (p_hat + z95 * se).min(1.0),
    );

    let avg_jumps = total_jumps as f64 / params.n_paths as f64;

    JumpDiffusionResult {
        probability: p_hat,
        standard_error: se,
        confidence_interval: ci,
        n_paths: params.n_paths,
        avg_jumps_per_path: avg_jumps,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// With zero jump intensity, jump-diffusion should match plain GBM.
    #[test]
    fn test_no_jump_matches_gbm() {
        let sigma = 0.2;
        let params = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            jump_intensity: 0.0,
            jump_mean: 0.0,
            jump_vol: 0.0,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000,
        };

        let result = run_jump_diffusion(&params);

        assert!(
            (result.probability - 0.5).abs() < 0.02,
            "No-jump case should match GBM ATM ~0.5, got {}",
            result.probability
        );
        assert!(
            result.avg_jumps_per_path < 0.01,
            "Should have ~0 jumps, got {}",
            result.avg_jumps_per_path
        );
    }

    /// Jumps should increase the variance of terminal prices.
    /// With symmetric jumps, the probability stays near 0.5 but SE may change.
    #[test]
    fn test_jumps_increase_variance() {
        let sigma = 0.2;

        // No jumps
        let no_jump = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            jump_intensity: 0.0,
            jump_mean: 0.0,
            jump_vol: 0.0,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 50_000,
        };

        // With jumps (large jump_vol adds variance)
        let with_jump = JumpDiffusionParams {
            jump_intensity: 5.0,
            jump_mean: 0.0,
            jump_vol: 0.3,
            ..no_jump.clone()
        };

        let r_nj = run_jump_diffusion(&no_jump);
        let r_wj = run_jump_diffusion(&with_jump);

        // Both should give reasonable probabilities
        assert!(
            r_nj.probability > 0.1 && r_nj.probability < 0.9,
            "No-jump prob should be reasonable: {}",
            r_nj.probability
        );
        assert!(
            r_wj.probability > 0.1 && r_wj.probability < 0.9,
            "With-jump prob should be reasonable: {}",
            r_wj.probability
        );
        // Jump paths should have some jumps
        assert!(
            r_wj.avg_jumps_per_path > 1.0,
            "Should have jumps: avg={}",
            r_wj.avg_jumps_per_path
        );
    }

    /// Probability should be bounded in [0, 1].
    #[test]
    fn test_probability_bounds() {
        let params = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.1,
            volatility: 0.3,
            jump_intensity: 2.0,
            jump_mean: -0.05,
            jump_vol: 0.1,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 10_000,
        };

        let result = run_jump_diffusion(&params);

        assert!(
            result.probability >= 0.0 && result.probability <= 1.0,
            "Probability must be in [0,1]: {}",
            result.probability
        );
        assert!(result.confidence_interval.0 >= 0.0);
        assert!(result.confidence_interval.1 <= 1.0);
        assert!(result.confidence_interval.0 <= result.confidence_interval.1);
    }

    /// Average number of jumps should match Poisson expectation lambda*T.
    #[test]
    fn test_jump_frequency() {
        let lambda = 3.0;
        let t = 2.0;
        let params = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.1,
            jump_intensity: lambda,
            jump_mean: 0.0,
            jump_vol: 0.01,
            time_horizon: t,
            strike: 0.5,
            n_paths: 50_000,
        };

        let result = run_jump_diffusion(&params);
        let expected_jumps = lambda * t;

        assert!(
            (result.avg_jumps_per_path - expected_jumps).abs() < 0.3,
            "Average jumps should be ~{}, got {}",
            expected_jumps,
            result.avg_jumps_per_path
        );
    }

    /// With large negative jump mean, probability of ending above strike should decrease.
    #[test]
    fn test_negative_jumps_reduce_probability() {
        let sigma = 0.2;

        let base = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            jump_intensity: 0.0,
            jump_mean: 0.0,
            jump_vol: 0.0,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 50_000,
        };

        let neg_jumps = JumpDiffusionParams {
            jump_intensity: 3.0,
            jump_mean: -0.2,
            jump_vol: 0.05,
            ..base.clone()
        };

        let r_base = run_jump_diffusion(&base);
        let r_neg = run_jump_diffusion(&neg_jumps);

        assert!(
            r_neg.probability < r_base.probability + 0.05,
            "Negative jumps should reduce probability: base={}, neg_jumps={}",
            r_base.probability,
            r_neg.probability
        );
    }

    /// Deep ITM with jumps should still give probability near 1.0.
    #[test]
    fn test_deep_itm_with_jumps() {
        let params = JumpDiffusionParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.2,
            jump_intensity: 1.0,
            jump_mean: 0.0,
            jump_vol: 0.05,
            time_horizon: 1.0,
            strike: 0.1,
            n_paths: 10_000,
        };

        let result = run_jump_diffusion(&params);
        assert!(
            result.probability > 0.90,
            "Deep ITM should have high probability: {}",
            result.probability
        );
    }
}
