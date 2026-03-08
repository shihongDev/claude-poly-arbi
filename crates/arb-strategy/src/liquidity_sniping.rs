use std::collections::VecDeque;
use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, OrderbookSnapshot, Side,
    StrategyType, TradeLeg,
    config::{LiquiditySnipingConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use tracing::debug;
use uuid::Uuid;

/// Detects sudden liquidity removal as a tick-based detector.
///
/// Maintains a sliding window of best-ask/best-bid snapshots per market.
/// When depth drops by >50% compared to the window average, a mean-reversion
/// opportunity is generated (the price spike is likely temporary).
///
/// This is a simplified tick-based version. A full WS-driven implementation
/// would use `LiveStrategy::run()` for sub-second reaction times.
pub struct LiquiditySnipingDetector {
    config: LiquiditySnipingConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
    /// Ring buffer of (total_ask_depth, total_bid_depth) per market per tick
    depth_history: std::sync::Mutex<std::collections::HashMap<String, VecDeque<(Decimal, Decimal)>>>,
}

impl LiquiditySnipingDetector {
    pub fn new(
        config: LiquiditySnipingConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
            depth_history: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }

    fn total_depth(book: &OrderbookSnapshot, levels: usize) -> (Decimal, Decimal) {
        let bid_depth: Decimal = book.bids.iter().take(levels).map(|l| l.size).sum();
        let ask_depth: Decimal = book.asks.iter().take(levels).map(|l| l.size).sum();
        (ask_depth, bid_depth)
    }

    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        if !market.active || market.orderbooks.is_empty() {
            return Ok(opps);
        }

        let book = &market.orderbooks[0];
        let (ask_depth, bid_depth) = Self::total_depth(book, 3);

        // Compute averages under lock, then drop it before building opportunities
        let (avg_ask, avg_bid) = {
            let mut history = self.depth_history.lock().unwrap();
            let window = history
                .entry(market.condition_id.clone())
                .or_default();

            window.push_back((ask_depth, bid_depth));

            // Keep last 30 snapshots
            if window.len() > 30 {
                window.pop_front();
            }

            // Need at least 5 snapshots for comparison
            if window.len() < 5 {
                return Ok(opps);
            }

            // Average depth over window (excluding current)
            let n = window.len() - 1;
            let (sum_ask, sum_bid) = window
                .iter()
                .take(n)
                .fold((Decimal::ZERO, Decimal::ZERO), |(a, b), (ask, bid)| {
                    (a + ask, b + bid)
                });
            (sum_ask / Decimal::from(n as u64), sum_bid / Decimal::from(n as u64))
        };

        let min_depth_pct = self.config.min_depth_change_pct;

        // Check ask-side wipeout (mean-reversion: price spiked up, buy the dip)
        if avg_ask > Decimal::ZERO {
            let ask_drop_pct = ((avg_ask - ask_depth) / avg_ask * dec!(100))
                .to_f64()
                .unwrap_or(0.0);

            if ask_drop_pct >= min_depth_pct
                && let Some(opp) = self.build_opp(market, Side::Buy, avg_ask, ask_depth)?
            {
                opps.push(opp);
            }
        }

        // Check bid-side wipeout (price crashed, sell into recovery)
        if avg_bid > Decimal::ZERO {
            let bid_drop_pct = ((avg_bid - bid_depth) / avg_bid * dec!(100))
                .to_f64()
                .unwrap_or(0.0);

            if bid_drop_pct >= min_depth_pct
                && let Some(opp) = self.build_opp(market, Side::Sell, avg_bid, bid_depth)?
            {
                opps.push(opp);
            }
        }

        Ok(opps)
    }

    fn build_opp(
        &self,
        market: &MarketState,
        side: Side,
        avg_depth: Decimal,
        current_depth: Decimal,
    ) -> Result<Option<Opportunity>> {
        let book = match market.orderbooks.first() {
            Some(b) => b,
            None => return Ok(None),
        };

        let has_liquidity = match side {
            Side::Buy => !book.asks.is_empty(),
            Side::Sell => !book.bids.is_empty(),
        };
        if !has_liquidity {
            return Ok(None);
        }

        let target_size = self.config.max_position;
        let vwap = match self.slippage_estimator.estimate_vwap(book, side, target_size) {
            Ok(v) => v,
            Err(_) => return Ok(None),
        };

        // Estimate edge from depth change magnitude
        let depth_ratio = if avg_depth > Decimal::ZERO {
            (avg_depth - current_depth) / avg_depth
        } else {
            Decimal::ZERO
        };

        // Edge proportional to depth wipeout: more wipeout = bigger expected reversion
        // Scale: 50% wipeout -> ~2% edge, 90% wipeout -> ~5% edge
        let gross_edge = depth_ratio * dec!(0.05);
        let fee_estimate = vwap.vwap * dec!(0.002); // taker rebate for post-only
        let net_edge = gross_edge - fee_estimate;
        let edge_bps = net_edge * Decimal::from(10_000);

        if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
            return Ok(None);
        }

        let confidence = (0.45 + 0.20 * depth_ratio.to_f64().unwrap_or(0.0)).min(0.75);

        let actual_size = target_size.min(vwap.max_available);
        if actual_size <= Decimal::ZERO {
            return Ok(None);
        }

        let token_id = market.token_ids.first().cloned().unwrap_or_default();
        let price = match side {
            Side::Buy => book.asks.first().map(|l| l.price).unwrap_or_default(),
            Side::Sell => book.bids.first().map(|l| l.price).unwrap_or_default(),
        };

        debug!(
            market = %market.condition_id,
            side = ?side,
            depth_drop_pct = %depth_ratio * dec!(100),
            "Liquidity sniping opportunity detected"
        );

        Ok(Some(Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::LiquiditySniping,
            markets: vec![market.condition_id.clone()],
            legs: vec![TradeLeg {
                token_id,
                side,
                target_price: price,
                target_size: actual_size,
                vwap_estimate: vwap.vwap,
            }],
            gross_edge,
            net_edge,
            estimated_vwap: vec![vwap.vwap],
            confidence,
            size_available: actual_size,
            detected_at: Utc::now(),
        }))
    }
}

#[async_trait]
impl ArbDetector for LiquiditySnipingDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking liquidity sniping");
                }
            }
        }
        Ok(all_opps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{OrderbookLevel, VwapEstimate};

    struct MockSlippage;
    impl SlippageEstimator for MockSlippage {
        fn estimate_vwap(
            &self,
            _book: &OrderbookSnapshot,
            _side: Side,
            _size: Decimal,
        ) -> Result<VwapEstimate> {
            Ok(VwapEstimate {
                vwap: dec!(0.50),
                total_size: dec!(200),
                levels_consumed: 2,
                max_available: dec!(500),
                slippage_bps: dec!(10),
            })
        }

        fn split_order(
            &self,
            _book: &OrderbookSnapshot,
            _side: Side,
            _total_size: Decimal,
            _max_slippage_bps: Decimal,
        ) -> Result<Vec<arb_core::OrderChunk>> {
            Ok(vec![])
        }
    }

    fn make_market(ask_size: Decimal) -> MarketState {
        MarketState {
            condition_id: "test".to_string(),
            question: "Test?".to_string(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["yes".into(), "no".into()],
            outcome_prices: vec![dec!(0.50), dec!(0.50)],
            orderbooks: vec![OrderbookSnapshot {
                token_id: "yes".into(),
                bids: vec![OrderbookLevel { price: dec!(0.49), size: dec!(500) }],
                asks: vec![OrderbookLevel { price: dec!(0.50), size: ask_size }],
                timestamp: Utc::now(),
            }],
            volume_24hr: Some(dec!(10000)),
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

    #[tokio::test]
    async fn test_detects_depth_wipeout() {
        let detector = LiquiditySnipingDetector::new(
            LiquiditySnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // Seed with normal depth (500 ask size)
        let normal = make_market(dec!(500));
        let markets = vec![Arc::new(normal)];
        for _ in 0..6 {
            let _ = detector.scan(&markets).await;
        }

        // Now depth drops to 50 (90% wipeout > 50% threshold)
        let wiped = make_market(dec!(50));
        let opps = detector.scan(&[Arc::new(wiped)]).await.unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].strategy_type, StrategyType::LiquiditySniping);
        assert_eq!(opps[0].legs[0].side, Side::Buy); // mean reversion buy
    }

    #[tokio::test]
    async fn test_skips_normal_depth() {
        let detector = LiquiditySnipingDetector::new(
            LiquiditySnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        let normal = make_market(dec!(500));
        let markets = vec![Arc::new(normal)];
        for _ in 0..6 {
            let _ = detector.scan(&markets).await;
        }

        // Same depth — no wipeout
        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }
}
