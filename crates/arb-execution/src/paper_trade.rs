use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use arb_core::{
    ExecutionReport, FillStatus, LegReport, OpenOrder, Opportunity, Position, Side, TradingMode,
    error::Result, traits::TradeExecutor,
};
use arb_data::impact::MarketImpactEstimator;
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::info;
use uuid::Uuid;

/// Simulated trade executor using real orderbook data.
///
/// Fills at estimated VWAP (or worse, with configurable pessimism factor).
/// Tracks virtual positions and PnL. Logs all "trades" for backtesting analysis.
/// Same interface as the live executor — swap in production with no code changes.
pub struct PaperTradeExecutor {
    positions: Mutex<HashMap<String, Position>>,
    trade_log: Mutex<Vec<ExecutionReport>>,
    placed_orders: Mutex<HashMap<String, OpenOrder>>,
    /// Multiply VWAP slippage by this factor (e.g., 1.2 = assume 20% worse fills than estimated).
    pessimism_factor: Decimal,
    /// Optional market impact estimator for more realistic fill simulation.
    /// When orderbook context is available, this replaces the static pessimism factor
    /// with a model-based impact estimate.
    #[allow(dead_code)]
    impact_estimator: Option<Arc<MarketImpactEstimator>>,
}

impl PaperTradeExecutor {
    pub fn new(pessimism_factor: Decimal) -> Self {
        Self {
            positions: Mutex::new(HashMap::new()),
            trade_log: Mutex::new(Vec::new()),
            placed_orders: Mutex::new(HashMap::new()),
            pessimism_factor,
            impact_estimator: None,
        }
    }

    /// Default with 10% pessimism (fills are 10% worse than VWAP estimate).
    pub fn default_pessimism() -> Self {
        Self::new(dec!(1.10))
    }

    /// Attach a market impact estimator (builder pattern).
    ///
    /// When orderbook context becomes available in the execution path, the
    /// estimator will replace the static pessimism factor with model-based
    /// impact estimates for more realistic fill simulation.
    pub fn with_impact_estimator(mut self, estimator: Arc<MarketImpactEstimator>) -> Self {
        self.impact_estimator = Some(estimator);
        self
    }

    /// Get all current virtual positions.
    pub fn positions(&self) -> HashMap<String, Position> {
        self.positions.lock().unwrap().clone()
    }

    /// Get the full trade log.
    pub fn trade_log(&self) -> Vec<ExecutionReport> {
        self.trade_log.lock().unwrap().clone()
    }

    /// Calculate total PnL across all recorded trades.
    pub fn total_pnl(&self) -> Decimal {
        self.trade_log
            .lock()
            .unwrap()
            .iter()
            .map(|r| r.realized_edge)
            .sum()
    }

    /// Simulate the fill price with pessimism adjustment.
    ///
    /// Uses additive slippage for symmetry: both buy and sell experience
    /// the same absolute price impact (`vwap * (factor - 1)`), just in
    /// opposite directions. The old multiplicative approach (`* factor` vs
    /// `/ factor`) was asymmetric: +10% buy vs -9.1% sell.
    fn pessimistic_price(&self, vwap: Decimal, side: Side) -> Decimal {
        let slippage = vwap * (self.pessimism_factor - Decimal::ONE);
        match side {
            // Buying: pessimism means we pay MORE than VWAP
            Side::Buy => vwap + slippage,
            // Selling: pessimism means we receive LESS than VWAP
            Side::Sell => vwap - slippage,
        }
    }

    fn update_position(
        &self,
        token_id: &str,
        condition_id: &str,
        side: Side,
        size: Decimal,
        price: Decimal,
    ) {
        let mut positions = self.positions.lock().unwrap();
        let pos = positions
            .entry(token_id.to_string())
            .or_insert_with(|| Position {
                token_id: token_id.to_string(),
                condition_id: condition_id.to_string(),
                size: Decimal::ZERO,
                avg_entry_price: Decimal::ZERO,
                current_price: price,
                unrealized_pnl: Decimal::ZERO,
            });

        match side {
            Side::Buy => {
                let new_cost = pos.avg_entry_price * pos.size + price * size;
                pos.size += size;
                if pos.size > Decimal::ZERO {
                    pos.avg_entry_price = new_cost / pos.size;
                }
            }
            Side::Sell => {
                pos.size -= size;
                if pos.size <= Decimal::ZERO {
                    pos.size = Decimal::ZERO;
                    pos.avg_entry_price = Decimal::ZERO;
                }
            }
        }

        pos.current_price = price;
        pos.unrealized_pnl = (pos.current_price - pos.avg_entry_price) * pos.size;
    }
}

#[async_trait]
impl TradeExecutor for PaperTradeExecutor {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport> {
        let mut leg_reports = Vec::with_capacity(opp.legs.len());
        let mut total_slippage = Decimal::ZERO;
        let mut total_fees = Decimal::ZERO;

        let condition_id = opp.markets.first().cloned().unwrap_or_default();

        for leg in &opp.legs {
            let fill_price = self.pessimistic_price(leg.vwap_estimate, leg.side);
            let slippage = (fill_price - leg.vwap_estimate).abs();

            // Fee: 2% on notional
            let fee = leg.target_size * fill_price * dec!(0.02);

            let order_id = Uuid::new_v4().to_string();

            leg_reports.push(LegReport {
                order_id,
                token_id: leg.token_id.clone(),
                condition_id: condition_id.clone(),
                side: leg.side,
                expected_vwap: leg.vwap_estimate,
                actual_fill_price: fill_price,
                filled_size: leg.target_size,
                status: FillStatus::FullyFilled,
            });

            // Note: paper trades fill immediately (FullyFilled), so they are
            // NOT tracked as open orders. Only unfilled/partial orders would be
            // tracked here in a more sophisticated simulation.

            total_slippage += slippage * leg.target_size;
            total_fees += fee;

            // Update virtual positions
            self.update_position(
                &leg.token_id,
                &condition_id,
                leg.side,
                leg.target_size,
                fill_price,
            );
        }

        // Realized edge = gross edge - actual slippage - fees
        let total_realized_edge = opp.gross_edge * opp.size_available - total_slippage - total_fees;

        let report = ExecutionReport {
            opportunity_id: opp.id,
            legs: leg_reports,
            realized_edge: total_realized_edge,
            slippage: total_slippage,
            total_fees,
            timestamp: Utc::now(),
            mode: TradingMode::Paper,
        };

        info!(
            mode = "paper",
            opportunity_id = %opp.id,
            arb_type = %opp.arb_type,
            legs = opp.legs.len(),
            realized_edge = %total_realized_edge,
            slippage = %total_slippage,
            fees = %total_fees,
            "Paper trade executed"
        );

        self.trade_log.lock().unwrap().push(report.clone());

        Ok(report)
    }

    async fn cancel_order(&self, order_id: &str) -> Result<()> {
        self.placed_orders.lock().unwrap().remove(order_id);
        info!(mode = "paper", order_id, "Paper order cancelled");
        Ok(())
    }

    async fn cancel_all(&self) -> Result<()> {
        let count = {
            let mut orders = self.placed_orders.lock().unwrap();
            let n = orders.len();
            orders.clear();
            n
        };
        info!(mode = "paper", count, "All paper orders cancelled");
        Ok(())
    }

    async fn open_orders(&self) -> Result<Vec<OpenOrder>> {
        Ok(self
            .placed_orders
            .lock()
            .unwrap()
            .values()
            .cloned()
            .collect())
    }

    async fn execute_batch(&self, opps: &[Opportunity]) -> Result<Vec<ExecutionReport>> {
        let mut reports = Vec::with_capacity(opps.len());
        for opp in opps {
            reports.push(self.execute_opportunity(opp).await?);
        }
        Ok(reports)
    }

    fn mode(&self) -> TradingMode {
        TradingMode::Paper
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{ArbType, StrategyType, TradeLeg};
    use rust_decimal_macros::dec;

    fn make_opp() -> Opportunity {
        Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::IntraMarketArb,
            markets: vec!["cond_abc".into()],
            legs: vec![
                TradeLeg {
                    token_id: "yes_token".into(),
                    side: Side::Buy,
                    target_price: dec!(0.48),
                    target_size: dec!(100),
                    vwap_estimate: dec!(0.48),
                },
                TradeLeg {
                    token_id: "no_token".into(),
                    side: Side::Buy,
                    target_price: dec!(0.49),
                    target_size: dec!(100),
                    vwap_estimate: dec!(0.49),
                },
            ],
            gross_edge: dec!(0.03),
            net_edge: dec!(0.02),
            estimated_vwap: vec![dec!(0.48), dec!(0.49)],
            confidence: 1.0,
            size_available: dec!(100),
            detected_at: Utc::now(),
        }
    }

    #[tokio::test]
    async fn test_paper_execution() {
        let executor = PaperTradeExecutor::default_pessimism();
        let opp = make_opp();

        let report = executor.execute_opportunity(&opp).await.unwrap();
        assert_eq!(report.legs.len(), 2);
        assert_eq!(report.mode, TradingMode::Paper);
        assert!(report.total_fees > Decimal::ZERO);

        // All legs should be fully filled
        for leg in &report.legs {
            assert_eq!(leg.status, FillStatus::FullyFilled);
            assert_eq!(leg.filled_size, dec!(100));
        }
    }

    #[tokio::test]
    async fn test_pessimism_makes_fills_worse() {
        let executor = PaperTradeExecutor::new(dec!(1.20)); // 20% pessimism
        let opp = make_opp();

        let report = executor.execute_opportunity(&opp).await.unwrap();

        // Buy fills should be at higher-than-VWAP prices
        let buy_leg = &report.legs[0];
        assert!(buy_leg.actual_fill_price > buy_leg.expected_vwap);
    }

    #[tokio::test]
    async fn test_position_tracking() {
        let executor = PaperTradeExecutor::default_pessimism();
        let opp = make_opp();

        executor.execute_opportunity(&opp).await.unwrap();

        let positions = executor.positions();
        assert_eq!(positions.len(), 2);
        assert!(positions.contains_key("yes_token"));
        assert!(positions.contains_key("no_token"));
    }
}
