use std::collections::VecDeque;
use std::time::Instant;

use arb_core::config::AlertsConfig;
use arb_core::{ExecutionReport, Opportunity};
use serde::{Deserialize, Serialize};
use tracing::{error, info, warn};

/// Alert manager for monitoring drawdown, calibration drift, and trade events.
pub struct AlertManager {
    config: AlertsConfig,
    last_calibration_check: Instant,
}

impl AlertManager {
    pub fn new(config: AlertsConfig) -> Self {
        Self {
            config,
            last_calibration_check: Instant::now(),
        }
    }

    /// Check drawdown level and emit alerts.
    pub fn check_drawdown(&self, drawdown_pct: f64) {
        if drawdown_pct >= self.config.drawdown_critical_pct {
            error!(
                drawdown_pct = drawdown_pct,
                threshold = self.config.drawdown_critical_pct,
                "CRITICAL: Drawdown exceeds critical threshold"
            );
        } else if drawdown_pct >= self.config.drawdown_warning_pct {
            warn!(
                drawdown_pct = drawdown_pct,
                threshold = self.config.drawdown_warning_pct,
                "WARNING: Drawdown exceeds warning threshold"
            );
        }
    }

    /// Periodically check Brier score calibration.
    pub fn check_calibration(&mut self, brier_score: f64) {
        let interval =
            std::time::Duration::from_secs(self.config.calibration_check_interval_mins * 60);

        if self.last_calibration_check.elapsed() >= interval {
            self.last_calibration_check = Instant::now();

            if brier_score > 0.30 {
                warn!(
                    brier_score = brier_score,
                    "Calibration degraded: Brier score above 0.30 (random = 0.25)"
                );
            } else {
                info!(brier_score = brier_score, "Calibration check OK");
            }
        }
    }

    /// Log a detected opportunity.
    pub fn log_opportunity(&self, opp: &Opportunity) {
        info!(
            event = "opportunity_detected",
            id = %opp.id,
            arb_type = %opp.arb_type,
            gross_edge = %opp.gross_edge,
            net_edge = %opp.net_edge,
            edge_bps = %opp.net_edge_bps(),
            confidence = opp.confidence,
            size = %opp.size_available,
            markets = ?opp.markets,
            legs = opp.legs.len(),
            "Opportunity detected"
        );
    }

    /// Log a trade execution.
    pub fn log_execution(&self, report: &ExecutionReport) {
        info!(
            event = "trade_executed",
            opportunity_id = %report.opportunity_id,
            mode = ?report.mode,
            legs = report.legs.len(),
            realized_edge = %report.realized_edge,
            slippage = %report.slippage,
            fees = %report.total_fees,
            "Trade executed"
        );
    }

    /// Log a rejected opportunity.
    pub fn log_rejected(&self, opp: &Opportunity, reason: &str) {
        info!(
            event = "opportunity_rejected",
            id = %opp.id,
            arb_type = %opp.arb_type,
            net_edge = %opp.net_edge,
            reason = reason,
            "Opportunity rejected by risk manager"
        );
    }

    /// Log kill switch activation.
    pub fn log_kill_switch(&self, reason: &str) {
        error!(
            event = "kill_switch_activated",
            reason = reason,
            "KILL SWITCH ACTIVATED — all trading halted"
        );
    }
}

// ---------------------------------------------------------------------------
// Model Health / Drift Detection
// ---------------------------------------------------------------------------

/// A timestamped prediction-outcome pair for Brier score computation.
#[derive(Debug, Clone)]
struct PredictionRecord {
    /// Predicted probability (0.0 to 1.0)
    predicted: f64,
    /// Actual outcome (true = event occurred)
    actual: bool,
    /// When this prediction was recorded
    timestamp: Instant,
}

/// Tracks model prediction quality via rolling Brier scores and detects drift.
///
/// Brier score = mean((predicted - actual)^2) over a rolling window.
/// - Perfect predictions: Brier = 0.0
/// - Random (coin flip at 0.5): Brier = 0.25
/// - Always wrong: Brier = 1.0
///
/// Confidence scaling based on 30-minute Brier:
/// - brier_30m > 0.45: confidence = 0.0 (pause trading)
/// - brier_30m > 0.35: confidence = 0.5 (half-size)
/// - brier_30m < 0.20: confidence = 1.0 (full size)
/// - Linear interpolation between thresholds
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelHealth {
    /// Rolling 30-minute Brier score
    pub brier_score_30m: f64,
    /// Rolling 24-hour Brier score
    pub brier_score_24h: f64,
    /// Whether drift has been detected (brier_30m > 0.35)
    pub drift_detected: bool,

    /// Internal ring buffer of predictions (not serialized)
    #[serde(skip)]
    records: VecDeque<PredictionRecord>,
}

impl ModelHealth {
    /// Duration constants for rolling windows
    const WINDOW_30M: std::time::Duration = std::time::Duration::from_secs(30 * 60);
    const WINDOW_24H: std::time::Duration = std::time::Duration::from_secs(24 * 60 * 60);

    /// Brier score thresholds for confidence scaling
    const BRIER_PAUSE_THRESHOLD: f64 = 0.45; // confidence = 0.0
    const BRIER_HALF_THRESHOLD: f64 = 0.35; // confidence = 0.5
    const BRIER_FULL_THRESHOLD: f64 = 0.20; // confidence = 1.0
    const DRIFT_THRESHOLD: f64 = 0.35;

    pub fn new() -> Self {
        Self {
            brier_score_30m: 0.0,
            brier_score_24h: 0.0,
            drift_detected: false,
            records: VecDeque::new(),
        }
    }

    /// Record a new prediction-outcome pair and update rolling Brier scores.
    ///
    /// # Arguments
    /// * `predicted` - Model's predicted probability (0.0 to 1.0)
    /// * `actual` - Whether the event actually occurred
    /// * `timestamp` - When this prediction was made
    pub fn record(&mut self, predicted: f64, actual: bool, timestamp: Instant) {
        let predicted = predicted.clamp(0.0, 1.0);

        self.records.push_back(PredictionRecord {
            predicted,
            actual,
            timestamp,
        });

        // Prune records older than 24h
        let cutoff_24h = timestamp.checked_sub(Self::WINDOW_24H);
        if let Some(cutoff) = cutoff_24h {
            while self.records.front().is_some_and(|r| r.timestamp < cutoff) {
                self.records.pop_front();
            }
        }

        // Recompute rolling Brier scores
        self.recompute_brier_scores(timestamp);

        // Update drift flag
        self.drift_detected = self.is_drift_detected();
    }

    /// Recompute both rolling Brier scores from the record buffer.
    fn recompute_brier_scores(&mut self, now: Instant) {
        let cutoff_30m = now.checked_sub(Self::WINDOW_30M);

        let mut sum_30m = 0.0;
        let mut count_30m = 0usize;
        let mut sum_24h = 0.0;
        let mut count_24h = 0usize;

        for rec in &self.records {
            let actual_val = if rec.actual { 1.0 } else { 0.0 };
            let sq_err = (rec.predicted - actual_val).powi(2);

            // 24h window (all records that survived pruning)
            sum_24h += sq_err;
            count_24h += 1;

            // 30m window
            if cutoff_30m.is_none() || rec.timestamp >= cutoff_30m.unwrap() {
                sum_30m += sq_err;
                count_30m += 1;
            }
        }

        self.brier_score_30m = if count_30m > 0 {
            sum_30m / count_30m as f64
        } else {
            0.0
        };

        self.brier_score_24h = if count_24h > 0 {
            sum_24h / count_24h as f64
        } else {
            0.0
        };
    }

    /// Compute the confidence level based on the 30-minute Brier score.
    ///
    /// Returns a value between 0.0 and 1.0 that should be used to scale position sizes:
    /// - 1.0 = full confidence, use normal sizing
    /// - 0.5 = degraded, use half-size positions
    /// - 0.0 = model is broken, pause trading
    ///
    /// Uses linear interpolation between the thresholds:
    /// - brier >= 0.45 -> 0.0
    /// - 0.35 <= brier < 0.45 -> linear from 0.5 to 0.0
    /// - 0.20 <= brier < 0.35 -> linear from 1.0 to 0.5
    /// - brier < 0.20 -> 1.0
    pub fn confidence_level(&self) -> f64 {
        let b = self.brier_score_30m;

        if b >= Self::BRIER_PAUSE_THRESHOLD {
            // 0.45+ → pause
            0.0
        } else if b >= Self::BRIER_HALF_THRESHOLD {
            // 0.35..0.45 → linear from 0.5 down to 0.0
            let t = (Self::BRIER_PAUSE_THRESHOLD - b)
                / (Self::BRIER_PAUSE_THRESHOLD - Self::BRIER_HALF_THRESHOLD);
            0.5 * t
        } else if b >= Self::BRIER_FULL_THRESHOLD {
            // 0.20..0.35 → linear from 1.0 down to 0.5
            let t = (Self::BRIER_HALF_THRESHOLD - b)
                / (Self::BRIER_HALF_THRESHOLD - Self::BRIER_FULL_THRESHOLD);
            0.5 + 0.5 * t
        } else {
            // < 0.20 → full confidence
            1.0
        }
    }

    /// Returns true if the model shows signs of drift (30m Brier > 0.35).
    pub fn is_drift_detected(&self) -> bool {
        self.brier_score_30m > Self::DRIFT_THRESHOLD
    }
}

impl Default for ModelHealth {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// Helper: record N predictions all at `now` and return the ModelHealth.
    fn make_model_with_predictions(pairs: &[(f64, bool)]) -> ModelHealth {
        let mut model = ModelHealth::new();
        let now = Instant::now();
        for (i, (predicted, actual)) in pairs.iter().enumerate() {
            // Small offset so timestamps differ
            let ts = now + Duration::from_millis(i as u64);
            model.record(*predicted, *actual, ts);
        }
        model
    }

    #[test]
    fn test_perfect_predictions_full_confidence() {
        // Perfect predictions: predicted=1.0 when actual=true, predicted=0.0 when actual=false
        let pairs: Vec<(f64, bool)> = (0..50)
            .map(|i| {
                if i % 2 == 0 {
                    (1.0, true)
                } else {
                    (0.0, false)
                }
            })
            .collect();

        let model = make_model_with_predictions(&pairs);

        assert!(
            model.brier_score_30m < 0.001,
            "Perfect predictions should have Brier ~0, got {}",
            model.brier_score_30m
        );
        assert!(
            (model.confidence_level() - 1.0).abs() < 0.001,
            "Perfect predictions should give confidence=1.0, got {}",
            model.confidence_level()
        );
        assert!(!model.is_drift_detected());
    }

    #[test]
    fn test_random_predictions_half_confidence() {
        // Random predictions: always predict 0.5, outcomes are 50/50
        // Brier = mean((0.5 - actual)^2) = mean(0.25) = 0.25
        let pairs: Vec<(f64, bool)> = (0..100).map(|i| (0.5, i % 2 == 0)).collect();

        let model = make_model_with_predictions(&pairs);

        assert!(
            (model.brier_score_30m - 0.25).abs() < 0.01,
            "Random predictions should have Brier ~0.25, got {}",
            model.brier_score_30m
        );

        // Brier 0.25 is between 0.20 and 0.35 → confidence between 0.5 and 1.0
        let conf = model.confidence_level();
        assert!(
            (0.45..=0.95).contains(&conf),
            "Random predictions should give confidence ~0.67, got {}",
            conf
        );
        assert!(!model.is_drift_detected(), "0.25 < 0.35 → no drift");
    }

    #[test]
    fn test_terrible_predictions_zero_confidence() {
        // Always wrong: predict 1.0 when actual=false, predict 0.0 when actual=true
        let pairs: Vec<(f64, bool)> = (0..50)
            .map(|i| {
                if i % 2 == 0 {
                    (1.0, false) // predicted 1.0, was false → error = 1.0
                } else {
                    (0.0, true) // predicted 0.0, was true → error = 1.0
                }
            })
            .collect();

        let model = make_model_with_predictions(&pairs);

        assert!(
            (model.brier_score_30m - 1.0).abs() < 0.01,
            "Terrible predictions should have Brier ~1.0, got {}",
            model.brier_score_30m
        );
        assert!(
            model.confidence_level() < 0.001,
            "Terrible predictions should give confidence=0.0, got {}",
            model.confidence_level()
        );
    }

    #[test]
    fn test_drift_detection_threshold() {
        // Brier exactly at 0.35 should NOT trigger drift (need > 0.35)
        // Brier at 0.36 should trigger drift

        // To get Brier = 0.36: mean((p - actual)^2) = 0.36
        // predict 0.6 for all, outcome = false → (0.6 - 0)^2 = 0.36
        let pairs: Vec<(f64, bool)> = (0..20).map(|_| (0.6, false)).collect();
        let model = make_model_with_predictions(&pairs);

        assert!(
            (model.brier_score_30m - 0.36).abs() < 0.01,
            "Expected Brier ~0.36, got {}",
            model.brier_score_30m
        );
        assert!(model.is_drift_detected(), "0.36 > 0.35 → drift detected");
        assert!(model.drift_detected); // field should be set too
    }

    #[test]
    fn test_confidence_scales_linearly() {
        // Test several Brier scores and verify linear interpolation
        let test_cases = vec![
            // (brier, expected_confidence_approx)
            (0.0, 1.0),    // perfect → full
            (0.10, 1.0),   // below 0.20 → full
            (0.20, 1.0),   // boundary → full
            (0.275, 0.75), // midpoint of [0.20, 0.35] → 0.75
            (0.35, 0.50),  // boundary → half
            (0.40, 0.25),  // midpoint of [0.35, 0.45] → 0.25
            (0.45, 0.0),   // boundary → pause
            (0.60, 0.0),   // above pause → 0.0
        ];

        for (brier, expected) in test_cases {
            let mut model = ModelHealth::new();
            model.brier_score_30m = brier;
            let conf = model.confidence_level();
            assert!(
                (conf - expected).abs() < 0.01,
                "Brier={}: expected confidence={}, got {}",
                brier,
                expected,
                conf
            );
        }
    }

    #[test]
    fn test_empty_model_defaults() {
        let model = ModelHealth::new();
        assert_eq!(model.brier_score_30m, 0.0);
        assert_eq!(model.brier_score_24h, 0.0);
        assert!(!model.drift_detected);
        assert!((model.confidence_level() - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_prediction_clamping() {
        // Predictions outside [0, 1] should be clamped
        let mut model = ModelHealth::new();
        let now = Instant::now();

        model.record(1.5, true, now); // clamped to 1.0
        assert!(
            model.brier_score_30m < 0.01,
            "Clamped to 1.0, actual=true → Brier ~0"
        );

        model.record(-0.5, false, now + Duration::from_millis(1)); // clamped to 0.0
        assert!(
            model.brier_score_30m < 0.01,
            "Clamped to 0.0, actual=false → Brier ~0"
        );
    }
}
