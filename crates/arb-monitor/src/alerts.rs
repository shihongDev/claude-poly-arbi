use std::time::Instant;

use arb_core::{ExecutionReport, Opportunity};
use arb_core::config::AlertsConfig;
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
        let interval = std::time::Duration::from_secs(
            self.config.calibration_check_interval_mins * 60,
        );

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
