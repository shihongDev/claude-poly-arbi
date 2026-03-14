use std::collections::HashMap;
use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
    config::{MarketMakingConfig, StrategyConfig},
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

/// Two-sided market making strategy.
///
/// Selects markets with sufficient volume but wide spreads, and generates
/// bid+ask quote opportunities. Inventory-aware: skews quotes away from
/// accumulated position to reduce risk.
///
/// This tick-based implementation generates quote opportunities each scan.
/// A full `LiveStrategy` implementation would maintain standing orders with
/// continuous requoting.
pub struct MarketMakingDetector {
    config: MarketMakingConfig,
    strategy_config: StrategyConfig,
    _slippage_estimator: Arc<dyn SlippageEstimator>,
    /// Current inventory per market (condition_id -> net position)
    inventory: std::sync::Mutex<HashMap<String, Decimal>>,
}

impl MarketMakingDetector {
    pub fn new(
        config: MarketMakingConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            _slippage_estimator: slippage_estimator,
            inventory: std::sync::Mutex::new(HashMap::new()),
        }
    }

    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        if !market.active {
            return Ok(opps);
        }

        // Volume threshold — only MM in liquid markets
        let volume = market.volume_24hr.unwrap_or(Decimal::ZERO);
        if volume < self.config.min_volume_24h {
            return Ok(opps);
        }

        let book = match market.orderbooks.first() {
            Some(b) if !b.bids.is_empty() && !b.asks.is_empty() => b,
            _ => return Ok(opps),
        };

        let best_bid = book.bids[0].price;
        let best_ask = book.asks[0].price;
        let current_spread_bps = ((best_ask - best_bid) * Decimal::from(10_000))
            .to_u64()
            .unwrap_or(0);

        // Only MM when spread is wide enough to be profitable
        if current_spread_bps < self.config.target_spread_bps {
            return Ok(opps);
        }

        let mid = (best_bid + best_ask) / dec!(2);

        // Inventory skew: lean away from existing position
        let net_pos = {
            let inventory = self.inventory.lock().unwrap();
            inventory
                .get(&market.condition_id)
                .copied()
                .unwrap_or(Decimal::ZERO)
        };

        // If we're long, widen bid (lower buy price) and tighten ask (lower sell price)
        // If we're short, tighten bid (higher buy price) and widen ask (higher sell price)
        let skew_factor = if self.config.max_inventory > Decimal::ZERO {
            (net_pos / self.config.max_inventory)
                .to_f64()
                .unwrap_or(0.0)
                .clamp(-0.5, 0.5)
        } else {
            0.0
        };

        let half_spread = Decimal::from(self.config.target_spread_bps) / Decimal::from(20_000);
        let skew = Decimal::try_from(skew_factor).unwrap_or(Decimal::ZERO) * half_spread;

        let our_bid = mid - half_spread - skew;
        let our_ask = mid + half_spread - skew;

        // Only quote if our prices are inside the current spread
        if our_bid >= best_ask || our_ask <= best_bid {
            return Ok(opps);
        }

        // Check inventory limit
        if net_pos.abs() >= self.config.max_inventory {
            return Ok(opps);
        }

        let quote_size = self.config.quote_size;
        let gross_edge = half_spread * dec!(2); // full spread capture
        let fee_estimate = mid * dec!(0.02) * dec!(2); // fees on both sides
        let net_edge = gross_edge - fee_estimate;
        let edge_bps = net_edge * Decimal::from(10_000);

        if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
            return Ok(opps);
        }

        let confidence = 0.50 + 0.20 * (current_spread_bps as f64 / 500.0).min(1.0);
        let confidence = confidence.min(0.75);

        let token_id = market.token_ids.first().cloned().unwrap_or_default();

        debug!(
            market = %market.condition_id,
            spread_bps = current_spread_bps,
            our_bid = %our_bid,
            our_ask = %our_ask,
            inventory = %net_pos,
            "Market making opportunity"
        );

        // Generate two-sided quote as a single opportunity
        opps.push(Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::MarketMaking,
            markets: vec![market.condition_id.clone()],
            legs: vec![
                TradeLeg {
                    token_id: token_id.clone(),
                    side: Side::Buy,
                    target_price: our_bid,
                    target_size: quote_size,
                    vwap_estimate: our_bid,
                },
                TradeLeg {
                    token_id,
                    side: Side::Sell,
                    target_price: our_ask,
                    target_size: quote_size,
                    vwap_estimate: our_ask,
                },
            ],
            gross_edge,
            net_edge,
            estimated_vwap: vec![our_bid, our_ask],
            confidence,
            size_available: quote_size,
            detected_at: Utc::now(),
        });

        Ok(opps)
    }
}

#[async_trait]
impl ArbDetector for MarketMakingDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking market making");
                }
            }
        }
        Ok(all_opps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{OrderbookLevel, OrderbookSnapshot, VwapEstimate};

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

    fn make_market(bid: Decimal, ask: Decimal, volume: Decimal) -> MarketState {
        MarketState {
            condition_id: "test".to_string(),
            question: "Test?".to_string(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["yes".into(), "no".into()],
            outcome_prices: vec![(bid + ask) / dec!(2), dec!(1.00) - (bid + ask) / dec!(2)],
            orderbooks: vec![OrderbookSnapshot {
                token_id: "yes".into(),
                bids: vec![OrderbookLevel {
                    price: bid,
                    size: dec!(1000),
                }],
                asks: vec![OrderbookLevel {
                    price: ask,
                    size: dec!(1000),
                }],
                timestamp: Utc::now(),
            }],
            volume_24hr: Some(volume),
            liquidity: None,
            active: true,
            neg_risk: false,
            best_bid: Some(bid),
            best_ask: Some(ask),
            spread: Some(ask - bid),
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
    async fn test_detects_wide_spread_opportunity() {
        let detector = MarketMakingDetector::new(
            MarketMakingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // Spread: 0.55 - 0.40 = 0.15 = 1500 bps > 200 bps threshold
        let market = make_market(dec!(0.40), dec!(0.55), dec!(20000));
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();

        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].strategy_type, StrategyType::MarketMaking);
        assert_eq!(opps[0].legs.len(), 2); // two-sided
        assert_eq!(opps[0].legs[0].side, Side::Buy);
        assert_eq!(opps[0].legs[1].side, Side::Sell);
    }

    #[tokio::test]
    async fn test_skips_tight_spread() {
        let detector = MarketMakingDetector::new(
            MarketMakingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // Spread: 0.505 - 0.495 = 0.01 = 100 bps < 200 bps threshold
        let market = make_market(dec!(0.495), dec!(0.505), dec!(20000));
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();
        assert!(opps.is_empty());
    }

    #[tokio::test]
    async fn test_skips_low_volume() {
        let detector = MarketMakingDetector::new(
            MarketMakingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // Wide spread but low volume
        let market = make_market(dec!(0.40), dec!(0.55), dec!(1000)); // < 10000
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();
        assert!(opps.is_empty());
    }
}
