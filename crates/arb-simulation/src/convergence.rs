use rand_distr::{Distribution, StandardNormal};
use serde::{Deserialize, Serialize};

use crate::monte_carlo::{MonteCarloParams, MonteCarloResult};

/// Diagnostics tracking convergence of a Monte Carlo simulation run in batches.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConvergenceDiagnostics {
    /// Running mean after each batch.
    pub running_mean: Vec<f64>,
    /// Running standard error after each batch.
    pub running_se: Vec<f64>,
    /// Gelman-Rubin R-hat from multiple chains (if computed).
    pub gelman_rubin: Option<f64>,
    /// Whether the simulation converged (SE < target_se).
    pub converged: bool,
    /// Total number of paths actually simulated.
    pub paths_used: usize,
}

/// Run Monte Carlo in batches of `batch_size`, checking convergence after each.
///
/// Stops early if SE < `target_se`, or flags non-convergent after `max_paths`.
/// Returns both the final `MonteCarloResult` and `ConvergenceDiagnostics`.
pub fn adaptive_monte_carlo(
    params: &MonteCarloParams,
    target_se: f64,
    max_paths: usize,
    batch_size: usize,
) -> (MonteCarloResult, ConvergenceDiagnostics) {
    let mut rng = rand::rng();
    let normal = StandardNormal;

    let drift_term = (params.drift - 0.5 * params.volatility.powi(2)) * params.time_horizon;
    let vol_term = params.volatility * params.time_horizon.sqrt();

    let mut total_hits: usize = 0;
    let mut total_paths: usize = 0;
    let mut running_mean = Vec::new();
    let mut running_se = Vec::new();
    let mut converged = false;

    while total_paths < max_paths {
        let paths_this_batch = batch_size.min(max_paths - total_paths);

        for _ in 0..paths_this_batch {
            let z: f64 = normal.sample(&mut rng);
            let s_t = params.initial_price * (drift_term + vol_term * z).exp();
            if s_t > params.strike {
                total_hits += 1;
            }
        }
        total_paths += paths_this_batch;

        let p_hat = total_hits as f64 / total_paths as f64;
        let se = (p_hat * (1.0 - p_hat) / total_paths as f64).sqrt();

        running_mean.push(p_hat);
        running_se.push(se);

        if se < target_se && total_paths >= batch_size {
            converged = true;
            break;
        }
    }

    let p_hat = total_hits as f64 / total_paths as f64;
    let se = (p_hat * (1.0 - p_hat) / total_paths as f64).sqrt();
    let z95 = 1.96;
    let ci = (
        (p_hat - z95 * se).max(0.0),
        (p_hat + z95 * se).min(1.0),
    );

    let result = MonteCarloResult {
        probability: p_hat,
        standard_error: se,
        confidence_interval: ci,
        n_paths: total_paths,
    };

    let diagnostics = ConvergenceDiagnostics {
        running_mean,
        running_se,
        gelman_rubin: None,
        converged,
        paths_used: total_paths,
    };

    (result, diagnostics)
}

/// Compute the Gelman-Rubin R-hat statistic from `n_chains` independent MC chains.
///
/// R-hat = sqrt(((n-1)/n * W + B/n) / W)
///
/// where:
/// - W = mean of within-chain variances
/// - B = between-chain variance of the chain means, scaled by n
/// - n = number of samples per chain
///
/// R-hat near 1.0 indicates convergence across chains.
pub fn gelman_rubin(
    params: &MonteCarloParams,
    n_chains: usize,
    paths_per_chain: usize,
) -> f64 {
    let chain_means = compute_chain_means(params, n_chains, paths_per_chain);
    let chain_variances = compute_chain_variances(params, n_chains, paths_per_chain, &chain_means);

    let n = paths_per_chain as f64;

    // W = mean of within-chain variances
    let w: f64 = chain_variances.iter().sum::<f64>() / n_chains as f64;

    // Grand mean
    let grand_mean: f64 = chain_means.iter().sum::<f64>() / n_chains as f64;

    // B = between-chain variance (scaled by n)
    let b: f64 = chain_means
        .iter()
        .map(|&m| (m - grand_mean).powi(2))
        .sum::<f64>()
        * n
        / (n_chains as f64 - 1.0);

    // R-hat
    if w < 1e-15 {
        // All chains identical, perfect convergence
        return 1.0;
    }

    let var_hat = (n - 1.0) / n * w + b / n;
    (var_hat / w).sqrt()
}

/// Simulate `n_chains` independent chains and return their means.
fn compute_chain_means(
    params: &MonteCarloParams,
    n_chains: usize,
    paths_per_chain: usize,
) -> Vec<f64> {
    let normal = StandardNormal;
    let drift_term = (params.drift - 0.5 * params.volatility.powi(2)) * params.time_horizon;
    let vol_term = params.volatility * params.time_horizon.sqrt();

    let mut means = Vec::with_capacity(n_chains);
    let mut rng = rand::rng();

    for _ in 0..n_chains {
        let mut hits = 0usize;
        for _ in 0..paths_per_chain {
            let z: f64 = normal.sample(&mut rng);
            let s_t = params.initial_price * (drift_term + vol_term * z).exp();
            if s_t > params.strike {
                hits += 1;
            }
        }
        means.push(hits as f64 / paths_per_chain as f64);
    }

    means
}

/// Simulate chains and compute within-chain variances.
/// For Bernoulli payoffs, within-chain variance = p_hat * (1 - p_hat).
fn compute_chain_variances(
    _params: &MonteCarloParams,
    n_chains: usize,
    _paths_per_chain: usize,
    chain_means: &[f64],
) -> Vec<f64> {
    let mut variances = Vec::with_capacity(n_chains);
    for &p in chain_means {
        // Bernoulli variance = p * (1 - p)
        variances.push(p * (1.0 - p));
    }
    variances
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Easy problem: ATM option should converge quickly.
    #[test]
    fn test_convergence_on_easy_problem() {
        let sigma = 0.2;
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000, // not used by adaptive
        };

        let (result, diag) = adaptive_monte_carlo(&params, 0.005, 200_000, 10_000);

        assert!(diag.converged, "Easy ATM problem should converge");
        assert!(
            (result.probability - 0.5).abs() < 0.03,
            "Probability should be ~0.5, got {}",
            result.probability
        );
        assert!(
            result.standard_error < 0.005,
            "SE should be below target: {}",
            result.standard_error
        );
    }

    /// Hard problem with extremely tight target SE: should not converge within max_paths.
    #[test]
    fn test_non_convergence_on_hard_target() {
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000,
        };

        // Require impossibly tight SE with very few paths
        let (_, diag) = adaptive_monte_carlo(&params, 0.0001, 5_000, 1_000);

        assert!(
            !diag.converged,
            "Should not converge with 5k paths and SE target 0.0001"
        );
        assert_eq!(diag.paths_used, 5_000);
    }

    /// Adaptive scaling: more batches reduce SE progressively.
    #[test]
    fn test_adaptive_scaling() {
        let sigma = 0.3;
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000,
        };

        let (_, diag) = adaptive_monte_carlo(&params, 0.001, 100_000, 5_000);

        // SE should generally decrease across batches
        assert!(
            diag.running_se.len() >= 2,
            "Should have at least 2 batches"
        );
        let first_se = diag.running_se[0];
        let last_se = *diag.running_se.last().unwrap();
        assert!(
            last_se < first_se,
            "SE should decrease: first={}, last={}",
            first_se,
            last_se
        );
    }

    /// Gelman-Rubin R-hat should be near 1.0 for a well-behaved simulation.
    #[test]
    fn test_gelman_rubin_near_one() {
        let sigma = 0.2;
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 50_000,
        };

        let r_hat = gelman_rubin(&params, 4, 50_000);

        assert!(
            (r_hat - 1.0).abs() < 0.05,
            "R-hat should be near 1.0, got {}",
            r_hat
        );
    }

    /// Batch accumulation: running_mean length matches number of batches.
    #[test]
    fn test_batch_accumulation() {
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.0,
            volatility: 0.2,
            time_horizon: 1.0,
            strike: 0.5,
            n_paths: 10_000,
        };

        let batch_size = 2_000;
        let max_paths = 10_000;
        let (_, diag) = adaptive_monte_carlo(&params, 0.0001, max_paths, batch_size);

        // Without early convergence, we expect max_paths / batch_size = 5 batches
        // With early convergence, fewer. Either way, check consistency.
        assert_eq!(
            diag.running_mean.len(),
            diag.running_se.len(),
            "Mean and SE vectors should have same length"
        );
        assert!(
            diag.running_mean.len() >= 1,
            "Should have at least one batch"
        );
        assert_eq!(
            diag.paths_used,
            diag.running_mean.len() * batch_size,
            "Paths used should equal batches * batch_size (or less if residual)"
        );
    }

    /// Running means should stabilize as more batches are added.
    #[test]
    fn test_running_mean_stabilizes() {
        let sigma = 0.2;
        let params = MonteCarloParams {
            initial_price: 1.0,
            drift: 0.5 * sigma * sigma,
            volatility: sigma,
            time_horizon: 1.0,
            strike: 1.0,
            n_paths: 100_000,
        };

        let (_, diag) = adaptive_monte_carlo(&params, 0.001, 100_000, 5_000);

        if diag.running_mean.len() >= 4 {
            // Variance of the last half of running means should be small
            let mid = diag.running_mean.len() / 2;
            let late_means = &diag.running_mean[mid..];
            let late_avg: f64 = late_means.iter().sum::<f64>() / late_means.len() as f64;
            let late_var: f64 = late_means
                .iter()
                .map(|&m| (m - late_avg).powi(2))
                .sum::<f64>()
                / late_means.len() as f64;
            assert!(
                late_var < 0.001,
                "Late running means should be stable, variance={}",
                late_var
            );
        }
    }
}
