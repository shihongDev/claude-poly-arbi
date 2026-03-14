//! Probability estimator combining Monte Carlo and Particle Filter into an ensemble.
//!
//! Implements `ProbabilityEstimator` from arb-core, providing calibrated probability
//! estimates for each market by running variance-reduced MC and particle filter
//! in parallel, then inverse-variance-weighting the results.

use std::collections::HashMap;
use std::sync::Mutex;

use arb_core::error::{ArbError, Result};
use arb_core::traits::ProbabilityEstimator;
use arb_core::types::{MarketState, ProbEstimate};
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

use crate::monte_carlo::{MonteCarloParams, run_monte_carlo};
use crate::particle_filter::ParticleFilter;

/// A combined probability estimate from one or more simulation methods.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CombinedEstimate {
    /// Weighted average probability across methods.
    pub probability: f64,
    /// Combined standard error (inverse-variance weighted).
    pub standard_error: f64,
    /// 95% confidence interval.
    pub confidence_interval: (f64, f64),
    /// Number of individual estimates combined.
    pub n_estimates: usize,
}

/// A single estimate from one simulation method.
#[derive(Debug, Clone)]
pub struct SingleEstimate {
    pub probability: f64,
    pub standard_error: f64,
}

/// Combine multiple probability estimates using inverse-variance weighting.
///
/// Each estimate is weighted by 1/SE^2, so more precise estimates have more influence.
/// Returns `None` if no valid estimates are provided.
pub fn combine_estimates(estimates: &[SingleEstimate]) -> Option<CombinedEstimate> {
    let valid: Vec<&SingleEstimate> = estimates
        .iter()
        .filter(|e| e.standard_error > 0.0 && e.probability.is_finite())
        .collect();

    if valid.is_empty() {
        return None;
    }

    let weights: Vec<f64> = valid
        .iter()
        .map(|e| 1.0 / e.standard_error.powi(2))
        .collect();
    let total_weight: f64 = weights.iter().sum();

    let p_hat: f64 = valid
        .iter()
        .zip(weights.iter())
        .map(|(e, &w)| e.probability * w)
        .sum::<f64>()
        / total_weight;

    let combined_se = (1.0 / total_weight).sqrt();
    let z95 = 1.96;
    let ci = (
        (p_hat - z95 * combined_se).max(0.0),
        (p_hat + z95 * combined_se).min(1.0),
    );

    Some(CombinedEstimate {
        probability: p_hat,
        standard_error: combined_se,
        confidence_interval: ci,
        n_estimates: valid.len(),
    })
}

/// Ensemble estimator combining Monte Carlo + Particle Filter.
///
/// Uses interior mutability (`Mutex`) so it can implement the `&self`-based
/// `ProbabilityEstimator` trait while maintaining per-market particle filter state.
pub struct EnsembleEstimator {
    mc_paths: usize,
    particle_count: usize,
    process_noise: f64,
    observation_noise: f64,
    /// Per-market particle filters, lazily initialized.
    filters: Mutex<HashMap<String, ParticleFilter>>,
}

impl EnsembleEstimator {
    pub fn new(mc_paths: usize, particle_count: usize) -> Self {
        Self {
            mc_paths,
            particle_count,
            process_noise: 0.03,
            observation_noise: 0.02,
            filters: Mutex::new(HashMap::new()),
        }
    }

    pub fn from_config(mc_paths: usize, particle_count: usize) -> Self {
        Self::new(mc_paths, particle_count)
    }

    fn get_initial_price(market: &MarketState) -> f64 {
        market
            .outcome_prices
            .first()
            .and_then(|p| p.to_f64())
            .unwrap_or(0.5)
    }
}

impl ProbabilityEstimator for EnsembleEstimator {
    fn estimate(&self, market: &MarketState) -> Result<ProbEstimate> {
        let initial_price = Self::get_initial_price(market);

        // 1. Monte Carlo estimate
        let mc_params = MonteCarloParams {
            initial_price,
            drift: 0.0,
            volatility: 0.3,
            time_horizon: 1.0,
            strike: 0.5,
            n_paths: self.mc_paths,
        };
        let mc_result = run_monte_carlo(&mc_params);

        // 2. Particle filter estimate
        let pf_estimate = {
            let mut filters = self
                .filters
                .lock()
                .map_err(|e| ArbError::Simulation(format!("Particle filter lock: {e}")))?;

            let pf = filters
                .entry(market.condition_id.clone())
                .or_insert_with(|| {
                    ParticleFilter::new(
                        self.particle_count,
                        initial_price,
                        self.process_noise,
                        self.observation_noise,
                    )
                });

            pf.update(initial_price);
            pf.estimate()
        };

        let pf_prob = pf_estimate.probabilities.first().copied().unwrap_or(0.5);

        // 3. Inverse-variance weighted combination
        let mc_se = mc_result.standard_error.max(1e-6);
        let pf_se = 0.05_f64; // Particle filters don't naturally produce SE; use conservative default

        let estimates = vec![
            SingleEstimate {
                probability: mc_result.probability,
                standard_error: mc_se,
            },
            SingleEstimate {
                probability: pf_prob,
                standard_error: pf_se,
            },
        ];

        let combined = combine_estimates(&estimates).unwrap_or(CombinedEstimate {
            probability: initial_price,
            standard_error: 0.1,
            confidence_interval: (
                (initial_price - 0.196).max(0.0),
                (initial_price + 0.196).min(1.0),
            ),
            n_estimates: 0,
        });

        Ok(ProbEstimate {
            probabilities: vec![combined.probability],
            confidence_interval: vec![combined.confidence_interval],
            method: format!("ensemble(mc={},pf={})", self.mc_paths, self.particle_count),
        })
    }

    fn update(&mut self, market: &MarketState, _new_data: &MarketState) {
        let price = Self::get_initial_price(market);
        if let Ok(mut filters) = self.filters.lock()
            && let Some(pf) = filters.get_mut(&market.condition_id)
        {
            pf.update(price);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn mock_market(condition_id: &str, price: rust_decimal::Decimal) -> MarketState {
        MarketState {
            condition_id: condition_id.to_string(),
            question: "Test?".into(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["t1".into(), "t2".into()],
            outcome_prices: vec![price],
            orderbooks: vec![],
            volume_24hr: None,
            liquidity: None,
            active: true,
            neg_risk: false,
            best_bid: None,
            best_ask: None,
            spread: None,
            last_trade_price: None,
            description: None,
            end_date_iso: None,
            slug: None,
            one_day_price_change: None,
            event_id: None,
            last_updated_gen: 0,
        }
    }

    #[test]
    fn test_combine_single_estimate() {
        let estimates = vec![SingleEstimate {
            probability: 0.5,
            standard_error: 0.01,
        }];
        let result = combine_estimates(&estimates).unwrap();
        assert!((result.probability - 0.5).abs() < 1e-10);
        assert!((result.standard_error - 0.01).abs() < 1e-10);
        assert_eq!(result.n_estimates, 1);
    }

    #[test]
    fn test_combine_favors_precise() {
        let estimates = vec![
            SingleEstimate {
                probability: 0.4,
                standard_error: 0.1,
            },
            SingleEstimate {
                probability: 0.6,
                standard_error: 0.01,
            },
        ];
        let result = combine_estimates(&estimates).unwrap();
        assert!(
            (result.probability - 0.6).abs() < 0.05,
            "Combined should favor precise estimate: {}",
            result.probability
        );
    }

    #[test]
    fn test_combine_empty() {
        let result = combine_estimates(&[]);
        assert!(result.is_none());
    }

    #[test]
    fn test_ensemble_estimator_returns_probability() {
        let estimator = EnsembleEstimator::new(1_000, 100);
        let market = mock_market("test-1", dec!(0.65));
        let result = estimator.estimate(&market).unwrap();
        assert!(!result.probabilities.is_empty());
        let p = result.probabilities[0];
        assert!((0.0..=1.0).contains(&p), "probability {p} out of range");
        assert!(result.method.contains("ensemble"));
    }

    #[test]
    fn test_ensemble_estimator_consistent() {
        let estimator = EnsembleEstimator::new(5_000, 200);
        let market = mock_market("test-2", dec!(0.50));
        let r1 = estimator.estimate(&market).unwrap();
        let r2 = estimator.estimate(&market).unwrap();
        let diff = (r1.probabilities[0] - r2.probabilities[0]).abs();
        assert!(
            diff < 0.15,
            "Two estimates should be reasonably close: {diff}"
        );
    }

    #[test]
    fn test_ensemble_estimator_different_markets() {
        let estimator = EnsembleEstimator::new(1_000, 100);
        let m1 = mock_market("mkt-a", dec!(0.20));
        let m2 = mock_market("mkt-b", dec!(0.80));
        let r1 = estimator.estimate(&m1).unwrap();
        let r2 = estimator.estimate(&m2).unwrap();
        // Different initial prices should produce meaningfully different estimates
        assert!(
            (r1.probabilities[0] - r2.probabilities[0]).abs() > 0.01,
            "Different prices should give different estimates"
        );
    }
}
