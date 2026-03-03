use std::collections::HashMap;

use arb_core::{ArbType, ExecutionReport};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

/// Performance metrics: Brier score, PnL attribution, drawdown, execution quality.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    /// (predicted_probability, actual_outcome) pairs for Brier score.
    predictions: Vec<(f64, bool)>,
    /// PnL broken down by arb type.
    pnl_by_type: HashMap<String, Decimal>,
    /// Peak equity (for drawdown calculation).
    peak_equity: Decimal,
    /// Current equity.
    current_equity: Decimal,
    /// All execution reports for quality analysis.
    execution_reports: Vec<ExecutionReport>,
}

impl PerformanceMetrics {
    pub fn new(starting_equity: Decimal) -> Self {
        Self {
            predictions: Vec::new(),
            pnl_by_type: HashMap::new(),
            peak_equity: starting_equity,
            current_equity: starting_equity,
            execution_reports: Vec::new(),
        }
    }

    /// Brier score: mean((predicted - actual)²).
    ///
    /// Lower is better. 0.0 = perfect calibration. 0.25 = random guessing.
    /// Tracks how well our probability estimates match reality.
    pub fn brier_score(&self) -> f64 {
        if self.predictions.is_empty() {
            return 0.0;
        }

        let sum: f64 = self
            .predictions
            .iter()
            .map(|&(predicted, actual)| {
                let actual_val = if actual { 1.0 } else { 0.0 };
                (predicted - actual_val).powi(2)
            })
            .sum();

        sum / self.predictions.len() as f64
    }

    /// Record a probability prediction and its actual outcome.
    pub fn record_prediction(&mut self, predicted: f64, actual: bool) {
        self.predictions.push((predicted, actual));
    }

    /// Current drawdown as a percentage from peak equity.
    pub fn drawdown_pct(&self) -> f64 {
        if self.peak_equity <= Decimal::ZERO {
            return 0.0;
        }

        let drawdown = self.peak_equity - self.current_equity;
        if drawdown <= Decimal::ZERO {
            return 0.0;
        }

        // Convert to f64 for percentage
        let dd: f64 = drawdown.to_f64().unwrap_or(0.0);
        let peak: f64 = self.peak_equity.to_f64().unwrap_or(1.0);

        (dd / peak) * 100.0
    }

    /// Update equity and track peak.
    pub fn record_equity(&mut self, equity: Decimal) {
        self.current_equity = equity;
        if equity > self.peak_equity {
            self.peak_equity = equity;
        }
    }

    /// Execution quality: average(realized_edge / expected_edge) across trades.
    ///
    /// Values close to 1.0 mean we're executing as expected.
    /// Below 0.8 suggests systematic execution problems.
    pub fn execution_quality(&self) -> Decimal {
        let valid_reports: Vec<_> = self
            .execution_reports
            .iter()
            .filter(|r| r.realized_edge != Decimal::ZERO)
            .collect();

        if valid_reports.is_empty() {
            return Decimal::ZERO;
        }

        // We don't have expected_edge stored separately, so we use
        // realized_edge / (realized_edge + slippage + fees) as a proxy
        let sum: Decimal = valid_reports
            .iter()
            .map(|r| {
                let total = r.realized_edge + r.slippage + r.total_fees;
                if total > Decimal::ZERO {
                    r.realized_edge / total
                } else {
                    Decimal::ZERO
                }
            })
            .sum();

        sum / Decimal::from(valid_reports.len())
    }

    /// Record an execution and attribute PnL to arb type.
    pub fn record_execution(&mut self, report: &ExecutionReport, arb_type: ArbType) {
        let key = arb_type.to_string();
        let entry = self.pnl_by_type.entry(key).or_insert(Decimal::ZERO);
        *entry += report.realized_edge;

        self.current_equity += report.realized_edge;
        if self.current_equity > self.peak_equity {
            self.peak_equity = self.current_equity;
        }

        self.execution_reports.push(report.clone());
    }

    /// PnL for a specific arb type.
    pub fn pnl_for_type(&self, arb_type: ArbType) -> Decimal {
        self.pnl_by_type
            .get(&arb_type.to_string())
            .copied()
            .unwrap_or(Decimal::ZERO)
    }

    /// Total PnL across all types.
    pub fn total_pnl(&self) -> Decimal {
        self.pnl_by_type.values().sum()
    }

    /// Number of trades executed.
    pub fn trade_count(&self) -> usize {
        self.execution_reports.len()
    }

    /// Peak equity value (used for drawdown tracking).
    pub fn peak_equity(&self) -> Decimal {
        self.peak_equity
    }

    /// Current equity value.
    pub fn current_equity(&self) -> Decimal {
        self.current_equity
    }
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new(Decimal::ZERO)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_brier_score_perfect() {
        let mut metrics = PerformanceMetrics::default();
        // Perfect predictions
        metrics.record_prediction(1.0, true);
        metrics.record_prediction(0.0, false);
        assert!((metrics.brier_score() - 0.0).abs() < 1e-10);
    }

    #[test]
    fn test_brier_score_worst() {
        let mut metrics = PerformanceMetrics::default();
        // Perfectly wrong predictions
        metrics.record_prediction(1.0, false);
        metrics.record_prediction(0.0, true);
        assert!((metrics.brier_score() - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_brier_score_random() {
        let mut metrics = PerformanceMetrics::default();
        // Always predict 0.5 → Brier = 0.25
        for _ in 0..100 {
            metrics.record_prediction(0.5, true);
            metrics.record_prediction(0.5, false);
        }
        assert!((metrics.brier_score() - 0.25).abs() < 1e-10);
    }

    #[test]
    fn test_drawdown() {
        let mut metrics = PerformanceMetrics::new(dec!(1000));
        metrics.record_equity(dec!(1100)); // new peak
        metrics.record_equity(dec!(990)); // drawdown

        let dd = metrics.drawdown_pct();
        // (1100 - 990) / 1100 * 100 = 10%
        assert!((dd - 10.0).abs() < 0.1);
    }

    #[test]
    fn test_no_drawdown_at_peak() {
        let mut metrics = PerformanceMetrics::new(dec!(1000));
        metrics.record_equity(dec!(1100));
        assert!((metrics.drawdown_pct() - 0.0).abs() < 1e-10);
    }
}
