use std::sync::{Arc, Mutex};

use arb_core::{
    ArbType, ExecutionReport, Opportunity, RiskDecision,
    config::RiskConfig,
    error::Result,
    traits::RiskManager,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
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

    /// Replace the internal position tracker with a previously persisted one.
    pub fn load_positions(&mut self, tracker: PositionTracker) {
        *self.positions.lock().unwrap() = tracker;
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
        // Compute notional per unit of size_available so we can scale correctly.
        // exposure_per_unit = sum(leg_vwap * leg_size) / opp.size_available
        let new_exposure: Decimal = opp
            .legs
            .iter()
            .map(|l| l.target_size * l.vwap_estimate)
            .sum();

        if current_exposure + new_exposure > self.config.max_total_exposure {
            // Try reducing size proportionally
            let available = self.config.max_total_exposure - current_exposure;
            if available > Decimal::ZERO && new_exposure > Decimal::ZERO {
                // Compute the per-unit notional exposure
                let exposure_per_unit = new_exposure / opp.size_available;
                let new_size = available / exposure_per_unit;
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

    fn record_execution(&mut self, report: &ExecutionReport, arb_type: ArbType) {
        self.check_daily_reset();
        self.daily_pnl += report.realized_edge;

        let mut tracker = self.positions.lock().unwrap();
        tracker.update(report);

        // Update performance metrics with PnL attribution and equity tracking
        self.metrics.record_execution(report, arb_type);
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

/// Result of Kelly criterion position sizing calculation.
#[derive(Debug, Clone)]
pub struct KellyResult {
    /// Raw Kelly fraction: f* = (p*b - q) / b
    pub kelly_fraction: f64,
    /// After applying the multiplier (e.g. 0.25 for quarter-Kelly)
    pub adjusted_fraction: f64,
    /// Suggested position size: adjusted_fraction * bankroll
    pub suggested_size: Decimal,
}

/// Compute optimal position size using the Kelly criterion.
///
/// The Kelly formula maximizes long-run log-wealth growth:
///   f* = (p * b - q) / b
/// where:
///   p = probability of winning (from opportunity confidence)
///   q = 1 - p (probability of losing)
///   b = win/loss ratio (net_edge / loss_if_wrong)
///
/// In practice, full Kelly is too aggressive, so we default to quarter-Kelly
/// (kelly_multiplier = 0.25) which sacrifices ~25% growth for ~75% variance reduction.
///
/// # Arguments
/// * `confidence` - Probability of winning (0.0 to 1.0)
/// * `net_edge` - Expected profit if the trade wins
/// * `loss_if_wrong` - Expected loss if the trade loses (positive amount)
/// * `bankroll` - Current total equity
/// * `kelly_multiplier` - Fraction of Kelly to use (0.25 = quarter-Kelly)
pub fn kelly_criterion(
    confidence: f64,
    net_edge: Decimal,
    loss_if_wrong: Decimal,
    bankroll: Decimal,
    kelly_multiplier: f64,
) -> KellyResult {
    // Edge cases
    if confidence <= 0.0 || loss_if_wrong <= Decimal::ZERO || bankroll <= Decimal::ZERO {
        return KellyResult {
            kelly_fraction: 0.0,
            adjusted_fraction: 0.0,
            suggested_size: Decimal::ZERO,
        };
    }

    if confidence >= 1.0 {
        // Certain win: Kelly says bet everything (capped to full bankroll)
        let adjusted = kelly_multiplier.min(1.0);
        return KellyResult {
            kelly_fraction: 1.0,
            adjusted_fraction: adjusted,
            suggested_size: Decimal::from_f64(adjusted).unwrap_or(Decimal::ZERO) * bankroll,
        };
    }

    let p = confidence;
    let q = 1.0 - p;

    // b = win/loss ratio
    let b = net_edge.to_f64().unwrap_or(0.0) / loss_if_wrong.to_f64().unwrap_or(1.0);

    if b <= 0.0 {
        return KellyResult {
            kelly_fraction: 0.0,
            adjusted_fraction: 0.0,
            suggested_size: Decimal::ZERO,
        };
    }

    // Kelly fraction: f* = (p*b - q) / b
    let f_star = (p * b - q) / b;

    // If Kelly fraction is negative or zero, don't bet
    if f_star <= 0.0 {
        return KellyResult {
            kelly_fraction: f_star,
            adjusted_fraction: 0.0,
            suggested_size: Decimal::ZERO,
        };
    }

    let adjusted = f_star * kelly_multiplier;
    let size = Decimal::from_f64(adjusted).unwrap_or(Decimal::ZERO) * bankroll;

    KellyResult {
        kelly_fraction: f_star,
        adjusted_fraction: adjusted,
        suggested_size: size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{ArbType, Side, StrategyType, TradeLeg};
    use chrono::Utc;
    use rust_decimal_macros::dec;
    use uuid::Uuid;

    fn default_config() -> RiskConfig {
        RiskConfig {
            max_position_per_market: dec!(1000),
            max_total_exposure: dec!(5000),
            daily_loss_limit: dec!(200),
            max_open_orders: 20,
            order_timeout_secs: 30,
        }
    }

    fn make_opp(size: Decimal) -> Opportunity {
        Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::IntraMarketArb,
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

    // --- Kelly criterion tests ---

    #[test]
    fn test_kelly_favorable_edge() {
        // 60% win probability, win $2 / lose $1 → f* = (0.6*2 - 0.4) / 2 = 0.4
        let result = kelly_criterion(0.60, dec!(2), dec!(1), dec!(10000), 1.0);
        let expected_f = (0.6 * 2.0 - 0.4) / 2.0; // 0.4
        assert!(
            (result.kelly_fraction - expected_f).abs() < 1e-10,
            "Expected f*={}, got {}",
            expected_f,
            result.kelly_fraction
        );
        assert!(result.suggested_size > Decimal::ZERO);
    }

    #[test]
    fn test_kelly_unfavorable_edge() {
        // 30% win probability, win $1 / lose $1 → f* = (0.3*1 - 0.7) / 1 = -0.4
        // Negative Kelly → don't bet
        let result = kelly_criterion(0.30, dec!(1), dec!(1), dec!(10000), 1.0);
        assert!(
            result.kelly_fraction < 0.0,
            "Unfavorable edge should give negative Kelly"
        );
        assert_eq!(
            result.adjusted_fraction, 0.0,
            "Adjusted fraction should be 0 for negative Kelly"
        );
        assert_eq!(
            result.suggested_size,
            Decimal::ZERO,
            "Suggested size should be 0"
        );
    }

    #[test]
    fn test_kelly_quarter_vs_full() {
        // Same favorable edge, compare quarter vs full Kelly
        let full = kelly_criterion(0.60, dec!(2), dec!(1), dec!(10000), 1.0);
        let quarter = kelly_criterion(0.60, dec!(2), dec!(1), dec!(10000), 0.25);

        assert!(
            (quarter.adjusted_fraction - full.adjusted_fraction * 0.25).abs() < 1e-10,
            "Quarter-Kelly adjusted fraction should be 1/4 of full Kelly"
        );
        assert!(
            quarter.suggested_size < full.suggested_size,
            "Quarter-Kelly size should be smaller than full Kelly"
        );
    }

    #[test]
    fn test_kelly_edge_cases() {
        // p = 0 → no bet
        let r0 = kelly_criterion(0.0, dec!(2), dec!(1), dec!(10000), 0.25);
        assert_eq!(r0.suggested_size, Decimal::ZERO);

        // p = 1 → certain win, fraction = kelly_multiplier
        let r1 = kelly_criterion(1.0, dec!(2), dec!(1), dec!(10000), 0.25);
        assert_eq!(r1.kelly_fraction, 1.0);
        assert!((r1.adjusted_fraction - 0.25).abs() < 1e-10);
        assert_eq!(r1.suggested_size, dec!(2500)); // 0.25 * 10000

        // Zero bankroll → no bet
        let rb = kelly_criterion(0.60, dec!(2), dec!(1), Decimal::ZERO, 0.25);
        assert_eq!(rb.suggested_size, Decimal::ZERO);

        // Zero loss_if_wrong → edge case, no bet
        let rl = kelly_criterion(0.60, dec!(2), Decimal::ZERO, dec!(10000), 0.25);
        assert_eq!(rl.suggested_size, Decimal::ZERO);
    }

    #[test]
    fn test_kelly_coin_flip() {
        // Fair coin, even payoff → f* = (0.5*1 - 0.5)/1 = 0 → don't bet
        let result = kelly_criterion(0.50, dec!(1), dec!(1), dec!(10000), 0.25);
        assert!(
            result.kelly_fraction.abs() < 1e-10,
            "Fair coin flip should give f*=0, got {}",
            result.kelly_fraction
        );
        assert_eq!(result.suggested_size, Decimal::ZERO);
    }
}
