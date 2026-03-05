use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use serde::{Deserialize, Serialize};

/// Value-at-Risk estimate containing both VaR and CVaR at standard confidence levels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VarEstimate {
    /// 5% VaR (95th percentile loss) — we expect losses to exceed this 5% of the time
    pub var_95: Decimal,
    /// 1% VaR (99th percentile loss)
    pub var_99: Decimal,
    /// Conditional VaR / Expected Shortfall at 95% — average loss in the worst 5%
    pub cvar_95: Decimal,
    /// Method used to compute this estimate
    pub method: VarMethod,
}

/// Which VaR computation method was used.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub enum VarMethod {
    Historical,
    Parametric,
    MonteCarlo { paths: usize },
}

/// Compute VaR and CVaR from a historical PnL series using empirical percentiles.
///
/// Sorts the PnL series in ascending order (worst-to-best) and picks the value
/// at the `(1 - confidence)` percentile as VaR. CVaR is the mean of all values
/// at or below VaR.
///
/// Returns losses as positive numbers (negated PnL).
pub fn historical_var(pnl_series: &[Decimal], _confidence: f64) -> VarEstimate {
    if pnl_series.is_empty() {
        return VarEstimate {
            var_95: Decimal::ZERO,
            var_99: Decimal::ZERO,
            cvar_95: Decimal::ZERO,
            method: VarMethod::Historical,
        };
    }

    let mut sorted: Vec<Decimal> = pnl_series.to_vec();
    sorted.sort();

    let n = sorted.len();

    let var_95 = percentile_loss(&sorted, n, 0.95);
    let var_99 = percentile_loss(&sorted, n, 0.99);
    let cvar_95 = compute_cvar(&sorted, var_95);

    VarEstimate {
        var_95,
        var_99,
        cvar_95,
        method: VarMethod::Historical,
    }
}

/// Pick the loss at the (1 - confidence) percentile from a sorted PnL series.
/// Returns the loss as a positive number (negated PnL value).
///
/// Uses the floor method: VaR is the observation at index `floor(alpha * n)`.
/// With n=20 and alpha=0.05: floor(1.0)=1 → sorted[1] is the VaR boundary.
fn percentile_loss(sorted: &[Decimal], n: usize, confidence: f64) -> Decimal {
    let alpha = 1.0 - confidence; // e.g. 0.05 for 95%
    let raw = alpha * n as f64;
    // Use floor to get the index of the alpha-th quantile
    let idx = (raw.floor() as usize).min(n - 1);
    // VaR is reported as a positive loss amount, so negate the PnL
    -sorted[idx]
}

/// CVaR = mean of all PnL values at or below the -VaR threshold (i.e. in the tail).
/// Returns as a positive loss amount.
fn compute_cvar(sorted: &[Decimal], var: Decimal) -> Decimal {
    // threshold is the PnL value at VaR: pnl <= -var
    let threshold = -var;
    let tail: Vec<Decimal> = sorted.iter().copied().filter(|&x| x <= threshold).collect();
    if tail.is_empty() {
        return var; // If no values beyond VaR, CVaR = VaR
    }
    let sum: Decimal = tail.iter().sum();
    let mean = sum / Decimal::from(tail.len());
    -mean // return as positive loss
}

/// Compute parametric VaR assuming normal distribution.
///
/// VaR = -(mean - z_alpha * std_dev)  (as a positive loss)
/// where z_alpha is the z-score for the confidence level.
pub fn parametric_var(mean: Decimal, std_dev: Decimal, _confidence: f64) -> VarEstimate {
    // z-scores for standard confidence levels
    let z_95 = Decimal::from_f64(1.6449).unwrap(); // invnorm(0.95)
    let z_99 = Decimal::from_f64(2.3263).unwrap(); // invnorm(0.99)

    // VaR = -(mean - z * sigma) = z * sigma - mean
    let var_95 = z_95 * std_dev - mean;
    let var_99 = z_99 * std_dev - mean;

    // For normal distribution, CVaR = -mean + sigma * phi(z) / (1 - confidence)
    // phi(z_95) = 0.10314, alpha_95 = 0.05 -> phi(z)/alpha = 2.0628
    // Simplified: CVaR_95 = sigma * 2.0628 - mean
    let cvar_ratio_95 = Decimal::from_f64(2.0628).unwrap();
    let cvar_95 = cvar_ratio_95 * std_dev - mean;

    VarEstimate {
        var_95: var_95.max(Decimal::ZERO),
        var_99: var_99.max(Decimal::ZERO),
        cvar_95: cvar_95.max(Decimal::ZERO),
        method: VarMethod::Parametric,
    }
}

/// Compute Monte Carlo VaR by simulating N paths from N(mean, std_dev).
///
/// Generates `n_paths` random draws, sorts them, and computes percentile VaR.
pub fn monte_carlo_var(mean: f64, std_dev: f64, n_paths: usize) -> VarEstimate {
    use rand::Rng;
    use rand_distr::Normal;

    if n_paths == 0 {
        return VarEstimate {
            var_95: Decimal::ZERO,
            var_99: Decimal::ZERO,
            cvar_95: Decimal::ZERO,
            method: VarMethod::MonteCarlo { paths: 0 },
        };
    }

    let dist = Normal::new(mean, std_dev).expect("Invalid normal distribution parameters");
    let mut rng = rand::rng();

    let mut samples: Vec<f64> = (0..n_paths).map(|_| rng.sample(dist)).collect();
    samples.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let var_95 = mc_percentile_loss(&samples, 0.95);
    let var_99 = mc_percentile_loss(&samples, 0.99);
    let cvar_95 = mc_cvar(&samples, var_95);

    VarEstimate {
        var_95: Decimal::from_f64(var_95).unwrap_or(Decimal::ZERO),
        var_99: Decimal::from_f64(var_99).unwrap_or(Decimal::ZERO),
        cvar_95: Decimal::from_f64(cvar_95).unwrap_or(Decimal::ZERO),
        method: VarMethod::MonteCarlo { paths: n_paths },
    }
}

fn mc_percentile_loss(sorted: &[f64], confidence: f64) -> f64 {
    let alpha = 1.0 - confidence;
    let idx = (alpha * sorted.len() as f64).floor() as usize;
    let idx = idx.min(sorted.len() - 1);
    let val = -sorted[idx]; // negate PnL to get positive loss
    val.max(0.0)
}

fn mc_cvar(sorted: &[f64], var: f64) -> f64 {
    let threshold = -var;
    let tail: Vec<f64> = sorted.iter().copied().filter(|&x| x <= threshold).collect();
    if tail.is_empty() {
        return var;
    }
    let sum: f64 = tail.iter().sum();
    let mean = sum / tail.len() as f64;
    (-mean).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_historical_var_known_data() {
        // 20 data points: -10, -9, ..., -1, 0, 1, ..., 9
        // Sorted ascending: [-10, -9, -8, ..., 9]
        // alpha_95 = 0.05, floor(0.05 * 20) = 1 → sorted[1] = -9 → VaR_95 = 9
        // alpha_99 = 0.01, floor(0.01 * 20) = 0 → sorted[0] = -10 → VaR_99 = 10
        let pnl: Vec<Decimal> = (-10..=9).map(|x| Decimal::from(x)).collect();
        assert_eq!(pnl.len(), 20);

        let result = historical_var(&pnl, 0.95);
        assert!(result.var_95 > Decimal::ZERO, "VaR95 should be positive");
        assert!(result.var_99 >= result.var_95, "VaR99 >= VaR95");
        assert_eq!(result.var_95, dec!(9)); // floor(0.05*20)=1 → sorted[1]=-9 → VaR=9
        assert_eq!(result.var_99, dec!(10)); // floor(0.01*20)=0 → sorted[0]=-10 → VaR=10
    }

    #[test]
    fn test_historical_var_all_positive() {
        // If all PnL are positive, VaR should still be computed from worst
        let pnl: Vec<Decimal> = (1..=100).map(|x| Decimal::from(x)).collect();
        let result = historical_var(&pnl, 0.95);
        // Worst 5% starts at pnl=1..5, VaR = -sorted[4] = -5 → but these are all positive
        // so var = -1 through -5, but we take -sorted[idx] which is -(positive) = negative → clamped?
        // Actually our code doesn't clamp historical. Let's see:
        // sorted[0]=1, index for 95%: ceil(0.05*100)-1 = 4, sorted[4] = 5, var = -5
        // This means "negative VaR" = we're making money even in worst case
        // This is valid: negative VaR means no risk of loss
        assert!(result.var_95 < Decimal::ZERO);
    }

    #[test]
    fn test_parametric_var_known_z_scores() {
        // mean=0, std=1 → VaR_95 = 1.6449, VaR_99 = 2.3263
        let result = parametric_var(Decimal::ZERO, Decimal::ONE, 0.95);
        let expected_95 = Decimal::from_f64(1.6449).unwrap();
        let expected_99 = Decimal::from_f64(2.3263).unwrap();

        assert_eq!(result.var_95, expected_95);
        assert_eq!(result.var_99, expected_99);
    }

    #[test]
    fn test_parametric_cvar_greater_than_var() {
        let result = parametric_var(Decimal::ZERO, dec!(10), 0.95);
        assert!(
            result.cvar_95 > result.var_95,
            "CVaR should exceed VaR: cvar={} var={}",
            result.cvar_95,
            result.var_95
        );
    }

    #[test]
    fn test_empty_series_handling() {
        let result = historical_var(&[], 0.95);
        assert_eq!(result.var_95, Decimal::ZERO);
        assert_eq!(result.var_99, Decimal::ZERO);
        assert_eq!(result.cvar_95, Decimal::ZERO);
    }

    #[test]
    fn test_monte_carlo_convergence() {
        // With N(0, 1) and large sample, VaR_95 should be ~1.645
        let result = monte_carlo_var(0.0, 1.0, 100_000);

        let var_95_f = result.var_95.to_f64().unwrap();
        assert!(
            (var_95_f - 1.645).abs() < 0.1,
            "MC VaR_95 should converge near 1.645, got {}",
            var_95_f
        );
    }

    #[test]
    fn test_var_99_exceeds_var_95() {
        let pnl: Vec<Decimal> = (-50..=50).map(|x| Decimal::from(x)).collect();
        let result = historical_var(&pnl, 0.95);
        assert!(
            result.var_99 >= result.var_95,
            "99% VaR ({}) should >= 95% VaR ({})",
            result.var_99,
            result.var_95
        );
    }

    #[test]
    fn test_monte_carlo_zero_paths() {
        let result = monte_carlo_var(0.0, 1.0, 0);
        assert_eq!(result.var_95, Decimal::ZERO);
    }

    #[test]
    fn test_historical_cvar_exceeds_var() {
        // Generate a series with a fat left tail
        let mut pnl: Vec<Decimal> = vec![];
        // 5 extreme losses
        for _ in 0..5 {
            pnl.push(dec!(-100));
        }
        // 95 moderate gains
        for _ in 0..95 {
            pnl.push(dec!(5));
        }

        let result = historical_var(&pnl, 0.95);
        assert!(
            result.cvar_95 >= result.var_95,
            "CVaR ({}) should >= VaR ({})",
            result.cvar_95,
            result.var_95
        );
    }
}
