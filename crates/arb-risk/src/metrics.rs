use std::collections::{HashMap, VecDeque};

use arb_core::{ArbType, ExecutionReport};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use serde::{Deserialize, Serialize};

/// Maximum number of execution reports to retain in memory.
/// Oldest reports are evicted when this limit is reached.
const MAX_EXECUTION_REPORTS: usize = 10_000;

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
    /// Fix T1-32: Bounded to MAX_EXECUTION_REPORTS using VecDeque for O(1) eviction.
    execution_reports: VecDeque<ExecutionReport>,
    /// Number of currently open (partially filled) orders.
    /// Fix T1-27: Tracked here so callers can query it.
    open_order_count: usize,
}

impl PerformanceMetrics {
    pub fn new(starting_equity: Decimal) -> Self {
        Self {
            predictions: Vec::new(),
            pnl_by_type: HashMap::new(),
            peak_equity: starting_equity,
            current_equity: starting_equity,
            execution_reports: VecDeque::new(),
            open_order_count: 0,
        }
    }

    /// Brier score: mean((predicted - actual)^2).
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

        // Fix T1-32: Evict oldest report if at capacity
        if self.execution_reports.len() >= MAX_EXECUTION_REPORTS {
            self.execution_reports.pop_front();
        }
        self.execution_reports.push_back(report.clone());

        // Fix T1-27: Track open order count from leg statuses
        let new_legs = report.legs.len();
        let completed = report
            .legs
            .iter()
            .filter(|l| {
                matches!(
                    l.status,
                    arb_core::FillStatus::FullyFilled
                        | arb_core::FillStatus::Rejected
                        | arb_core::FillStatus::Cancelled
                )
            })
            .count();
        self.open_order_count = self
            .open_order_count
            .saturating_add(new_legs)
            .saturating_sub(completed);
    }

    /// Record the completion of an order that was previously partially filled.
    ///
    /// Fix T1-27: Provides a decrement path for open_order_count.
    pub fn record_order_completion(&mut self, completed_count: usize) {
        self.open_order_count = self.open_order_count.saturating_sub(completed_count);
    }

    /// Number of currently open (in-flight) orders.
    pub fn open_order_count(&self) -> usize {
        self.open_order_count
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
    use arb_core::{FillStatus, LegReport, Side, TradingMode};
    use rust_decimal_macros::dec;
    use uuid::Uuid;

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
        // Always predict 0.5 -> Brier = 0.25
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

    fn make_execution_report(edge: Decimal) -> ExecutionReport {
        ExecutionReport {
            opportunity_id: Uuid::new_v4(),
            legs: vec![LegReport {
                order_id: "o1".into(),
                token_id: "tok".into(),
                condition_id: "c1".into(),
                side: Side::Buy,
                expected_vwap: dec!(0.50),
                actual_fill_price: dec!(0.50),
                filled_size: dec!(100),
                status: FillStatus::FullyFilled,
            }],
            realized_edge: edge,
            slippage: Decimal::ZERO,
            total_fees: Decimal::ZERO,
            timestamp: chrono::Utc::now(),
            mode: TradingMode::Paper,
        }
    }

    #[test]
    fn test_execution_reports_bounded() {
        // Fix T1-32: Verify reports are capped at MAX_EXECUTION_REPORTS
        let mut metrics = PerformanceMetrics::new(dec!(10000));

        for _i in 0..MAX_EXECUTION_REPORTS + 100 {
            let report = make_execution_report(dec!(0.01));
            metrics.record_execution(&report, ArbType::IntraMarket);
        }

        assert_eq!(
            metrics.execution_reports.len(),
            MAX_EXECUTION_REPORTS,
            "Reports should be capped at {}",
            MAX_EXECUTION_REPORTS
        );
    }

    #[test]
    fn test_open_order_count_tracking() {
        // Fix T1-27: Verify open_order_count is maintained in metrics
        let mut metrics = PerformanceMetrics::new(dec!(10000));
        assert_eq!(metrics.open_order_count(), 0);

        // Fully filled leg: added and immediately completed
        let report = make_execution_report(dec!(0.01));
        metrics.record_execution(&report, ArbType::IntraMarket);
        assert_eq!(metrics.open_order_count(), 0);

        // Partially filled leg: stays open
        let partial_report = ExecutionReport {
            opportunity_id: Uuid::new_v4(),
            legs: vec![LegReport {
                order_id: "o2".into(),
                token_id: "tok".into(),
                condition_id: "c1".into(),
                side: Side::Buy,
                expected_vwap: dec!(0.50),
                actual_fill_price: dec!(0.50),
                filled_size: dec!(50),
                status: FillStatus::PartiallyFilled,
            }],
            realized_edge: Decimal::ZERO,
            slippage: Decimal::ZERO,
            total_fees: Decimal::ZERO,
            timestamp: chrono::Utc::now(),
            mode: TradingMode::Paper,
        };
        metrics.record_execution(&partial_report, ArbType::IntraMarket);
        assert_eq!(metrics.open_order_count(), 1);

        // Manually complete the open order
        metrics.record_order_completion(1);
        assert_eq!(metrics.open_order_count(), 0);
    }
}
