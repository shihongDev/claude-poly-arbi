use std::sync::{Arc, Mutex};

use arb_core::{
    ExecutionReport, Opportunity, RiskDecision,
    config::RiskConfig,
    error::Result,
    traits::RiskManager,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tracing::info;

use crate::kill_switch::KillSwitch;
use crate::metrics::PerformanceMetrics;
use crate::position_tracker::PositionTracker;

/// Risk limit enforcement implementing the RiskManager trait.
///
/// Checks per-market position limits, total exposure cap, daily loss limit,
/// and max open orders before approving any opportunity.
pub struct RiskLimits {
    config: RiskConfig,
    positions: Arc<Mutex<PositionTracker>>,
    kill_switch: KillSwitch,
    metrics: PerformanceMetrics,
    daily_pnl: Decimal,
    daily_reset: DateTime<Utc>,
    open_order_count: usize,
}

impl RiskLimits {
    pub fn new(config: RiskConfig, starting_equity: Decimal) -> Self {
        Self {
            config,
            positions: Arc::new(Mutex::new(PositionTracker::new())),
            kill_switch: KillSwitch::new(),
            metrics: PerformanceMetrics::new(starting_equity),
            daily_pnl: Decimal::ZERO,
            daily_reset: Utc::now(),
            open_order_count: 0,
        }
    }

    /// Get a reference to the position tracker (for status display).
    pub fn positions(&self) -> Arc<Mutex<PositionTracker>> {
        self.positions.clone()
    }

    /// Get a reference to the performance metrics.
    pub fn metrics(&self) -> &PerformanceMetrics {
        &self.metrics
    }

    /// Reset daily PnL counter (called at midnight UTC).
    fn check_daily_reset(&mut self) {
        let now = Utc::now();
        if now.date_naive() != self.daily_reset.date_naive() {
            info!(
                previous_pnl = %self.daily_pnl,
                "Daily PnL reset"
            );
            self.daily_pnl = Decimal::ZERO;
            self.daily_reset = now;
        }
    }
}

impl RiskManager for RiskLimits {
    fn check_opportunity(&self, opp: &Opportunity) -> Result<RiskDecision> {
        // Check kill switch first
        if self.kill_switch.is_active() {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Kill switch active: {}",
                    self.kill_switch.reason().unwrap_or("unknown")
                ),
            });
        }

        // Check daily loss limit
        if self.daily_pnl < -self.config.daily_loss_limit {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Daily loss limit exceeded: {} < -{}",
                    self.daily_pnl, self.config.daily_loss_limit
                ),
            });
        }

        // Check max open orders
        if self.open_order_count + opp.legs.len() > self.config.max_open_orders {
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Would exceed max open orders: {} + {} > {}",
                    self.open_order_count,
                    opp.legs.len(),
                    self.config.max_open_orders
                ),
            });
        }

        let tracker = self.positions.lock().unwrap();

        // Check total exposure
        let current_exposure = tracker.total_exposure();
        let new_exposure: Decimal = opp
            .legs
            .iter()
            .map(|l| l.target_size * l.vwap_estimate)
            .sum();

        if current_exposure + new_exposure > self.config.max_total_exposure {
            // Try reducing size
            let available = self.config.max_total_exposure - current_exposure;
            if available > Decimal::ZERO {
                let ratio = available / new_exposure;
                let new_size = opp.size_available * ratio;
                return Ok(RiskDecision::ReduceSize {
                    new_size,
                    reason: format!(
                        "Total exposure would exceed {}: reducing size to {}",
                        self.config.max_total_exposure, new_size
                    ),
                });
            }
            return Ok(RiskDecision::Reject {
                reason: format!(
                    "Total exposure at cap: {} >= {}",
                    current_exposure, self.config.max_total_exposure
                ),
            });
        }

        // Check per-market position limits
        for market_cid in &opp.markets {
            let market_exposure = tracker.market_exposure(market_cid);
            if market_exposure + new_exposure > self.config.max_position_per_market {
                let available = self.config.max_position_per_market - market_exposure;
                if available > Decimal::ZERO {
                    return Ok(RiskDecision::ReduceSize {
                        new_size: available,
                        reason: format!(
                            "Per-market limit for {}: reducing to {}",
                            market_cid, available
                        ),
                    });
                }
                return Ok(RiskDecision::Reject {
                    reason: format!(
                        "Per-market position limit hit for {}: {} >= {}",
                        market_cid, market_exposure, self.config.max_position_per_market
                    ),
                });
            }
        }

        Ok(RiskDecision::Approve {
            max_size: opp.size_available,
        })
    }

    fn record_execution(&mut self, report: &ExecutionReport) {
        self.check_daily_reset();
        self.daily_pnl += report.realized_edge;

        let mut tracker = self.positions.lock().unwrap();
        tracker.update(report);

        // We don't know the arb_type here, so attribute as generic
        // The daemon should call metrics.record_execution() with the type
    }

    fn is_kill_switch_active(&self) -> bool {
        self.kill_switch.is_active()
    }

    fn activate_kill_switch(&mut self, reason: &str) {
        self.kill_switch.activate(reason);
    }

    fn daily_pnl(&self) -> Decimal {
        self.daily_pnl
    }

    fn current_exposure(&self) -> Decimal {
        self.positions.lock().unwrap().total_exposure()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{ArbType, Side, TradeLeg};
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn default_config() -> RiskConfig {
        RiskConfig {
            max_position_per_market: dec!(1000),
            max_total_exposure: dec!(5000),
            daily_loss_limit: dec!(200),
            max_open_orders: 20,
        }
    }

    fn make_opp(size: Decimal) -> Opportunity {
        Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            markets: vec!["cond_abc".into()],
            legs: vec![
                TradeLeg {
                    token_id: "yes".into(),
                    side: Side::Buy,
                    target_price: dec!(0.50),
                    target_size: size,
                    vwap_estimate: dec!(0.50),
                },
                TradeLeg {
                    token_id: "no".into(),
                    side: Side::Buy,
                    target_price: dec!(0.48),
                    target_size: size,
                    vwap_estimate: dec!(0.48),
                },
            ],
            gross_edge: dec!(0.02),
            net_edge: dec!(0.01),
            estimated_vwap: vec![dec!(0.50), dec!(0.48)],
            confidence: 1.0,
            size_available: size,
            detected_at: Utc::now(),
        }
    }

    #[test]
    fn test_approve_within_limits() {
        // Ensure no kill switch file from other tests
        let mut ks = crate::kill_switch::KillSwitch::new();
        if ks.is_active() {
            ks.deactivate();
        }

        let rm = RiskLimits::new(default_config(), dec!(5000));
        let opp = make_opp(dec!(100));

        match rm.check_opportunity(&opp).unwrap() {
            RiskDecision::Approve { max_size } => {
                assert_eq!(max_size, dec!(100));
            }
            other => panic!("Expected Approve, got {:?}", other),
        }
    }

    #[test]
    fn test_reject_over_exposure() {
        let config = RiskConfig {
            max_total_exposure: dec!(50), // very low
            ..default_config()
        };
        let rm = RiskLimits::new(config, dec!(5000));
        let opp = make_opp(dec!(100)); // would need 100 * 0.50 + 100 * 0.48 = 98

        match rm.check_opportunity(&opp).unwrap() {
            RiskDecision::ReduceSize { new_size, .. } => {
                assert!(new_size < dec!(100));
            }
            RiskDecision::Reject { .. } => {} // also acceptable
            other => panic!("Expected ReduceSize or Reject, got {:?}", other),
        }
    }

    #[test]
    fn test_reject_kill_switch() {
        let mut rm = RiskLimits::new(default_config(), dec!(5000));
        rm.activate_kill_switch("test reason");

        let opp = make_opp(dec!(100));
        match rm.check_opportunity(&opp).unwrap() {
            RiskDecision::Reject { reason } => {
                assert!(reason.contains("Kill switch"));
            }
            other => panic!("Expected Reject, got {:?}", other),
        }

        // Clean up: deactivate so other tests aren't affected
        let mut ks = crate::kill_switch::KillSwitch::new();
        ks.deactivate();
    }
}
