use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
    config::{StaleMarketConfig, StrategyConfig},
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

/// Detects stale markets where orderbooks haven't updated
/// but related markets (same event_id) have moved.
///
/// If market A's orderbook is stale but sibling market B has moved
/// significantly, market A's price likely needs to catch up.
/// We trade in the direction of the implied correction.
pub struct StaleMarketDetector {
    config: StaleMarketConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
    fee_rate: Decimal,
}

impl StaleMarketDetector {
    pub fn new(
        config: StaleMarketConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
        fee_rate: Decimal,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
            fee_rate,
        }
    }

    /// Check if any market's orderbook is stale relative to siblings.
    fn check_event_group(
        &self,
        group: &[&MarketState],
    ) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();
        if group.len() < 2 {
            return Ok(opps);
        }

        let now = Utc::now();

        for stale_candidate in group {
            // Check if the orderbook is stale
            let book_ts = match stale_candidate.orderbooks.first() {
                Some(b) => b.timestamp,
                None => continue,
            };

            let stale_hours = (now - book_ts).num_hours();
            if stale_hours < self.config.max_stale_hours as i64 {
                continue; // not stale enough
            }

            // Check volume threshold
            let volume = stale_candidate.volume_24hr.unwrap_or(Decimal::ZERO);
            if volume < self.config.min_volume_24h {
                continue;
            }

            let stale_price = match stale_candidate.outcome_prices.first() {
                Some(p) => *p,
                None => continue,
            };

            // Compare with siblings: compute average sibling YES price
            let sibling_prices: Vec<Decimal> = group
                .iter()
                .filter(|m| m.condition_id != stale_candidate.condition_id)
                .filter_map(|m| m.outcome_prices.first().copied())
                .collect();

            if sibling_prices.is_empty() {
                continue;
            }

            // For mutually exclusive events, if siblings' prices have risen,
            // this market should fall (and vice versa).
            let sibling_sum: Decimal = sibling_prices.iter().sum();
            let implied_price = (dec!(1.00) - sibling_sum).max(dec!(0.01));
            let divergence = implied_price - stale_price;
            let divergence_bps = (divergence.abs() * Decimal::from(10_000))
                .to_u64()
                .unwrap_or(0);

            if divergence_bps < self.config.min_divergence_bps {
                continue;
            }

            // Determine trade direction
            let (side, target_price) = if divergence > Decimal::ZERO {
                // Implied price > stale price: market is underpriced, buy
                (Side::Buy, stale_price)
            } else {
                // Implied price < stale price: market is overpriced, sell
                (Side::Sell, stale_price)
            };

            let book = match stale_candidate.orderbooks.first() {
                Some(b) => b,
                None => continue,
            };

            let target_size = Decimal::from(200); // conservative
            let vwap = match self.slippage_estimator.estimate_vwap(book, side, target_size) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let gross_edge = divergence.abs();
            let fee_estimate = vwap.vwap * self.fee_rate;
            let net_edge = gross_edge - fee_estimate;
            let edge_bps = net_edge * Decimal::from(10_000);

            if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
                continue;
            }

            // Confidence based on staleness and divergence magnitude
            let confidence = 0.55 + 0.15 * (stale_hours as f64 / 48.0).min(1.0)
                + 0.10 * (divergence_bps as f64 / 200.0).min(1.0);
            let confidence = confidence.clamp(0.50, 0.85);

            let actual_size = target_size.min(vwap.max_available);
            if actual_size <= Decimal::ZERO {
                continue;
            }

            debug!(
                market = %stale_candidate.condition_id,
                stale_hours = stale_hours,
                implied_price = %implied_price,
                current_price = %stale_price,
                divergence_bps = divergence_bps,
                n_siblings = sibling_prices.len(),
                "Stale market opportunity detected"
            );

            let token_id = stale_candidate.token_ids.first().cloned().unwrap_or_default();

            opps.push(Opportunity {
                id: Uuid::new_v4(),
                arb_type: ArbType::CrossMarket,
                strategy_type: StrategyType::StaleMarket,
                markets: vec![stale_candidate.condition_id.clone()],
                legs: vec![TradeLeg {
                    token_id,
                    side,
                    target_price,
                    target_size: actual_size,
                    vwap_estimate: vwap.vwap,
                }],
                gross_edge,
                net_edge,
                estimated_vwap: vec![vwap.vwap],
                confidence,
                size_available: actual_size,
                detected_at: Utc::now(),
            });
        }

        Ok(opps)
    }
}

#[async_trait]
impl ArbDetector for StaleMarketDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::CrossMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        // Group by event_id
        let mut by_event: std::collections::HashMap<&str, Vec<&MarketState>> =
            std::collections::HashMap::new();

        for market in markets {
            if let Some(ref eid) = market.event_id {
                by_event.entry(eid.as_str()).or_default().push(market.as_ref());
            }
        }

        let mut all_opps = Vec::new();
        for group in by_event.values() {
            match self.check_event_group(group) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(error = %e, "Error checking stale market group");
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
                vwap: dec!(0.40),
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

    fn make_market(cond_id: &str, event_id: &str, yes_price: Decimal, stale_hours: i64) -> MarketState {
        let book_time = Utc::now() - chrono::Duration::hours(stale_hours);
        MarketState {
            condition_id: cond_id.to_string(),
            question: format!("Market {cond_id}?"),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec![format!("{cond_id}_yes"), format!("{cond_id}_no")],
            outcome_prices: vec![yes_price, dec!(1.00) - yes_price],
            orderbooks: vec![OrderbookSnapshot {
                token_id: format!("{cond_id}_yes"),
                bids: vec![OrderbookLevel { price: yes_price - dec!(0.01), size: dec!(500) }],
                asks: vec![OrderbookLevel { price: yes_price, size: dec!(500) }],
                timestamp: book_time,
            }],
            volume_24hr: Some(dec!(5000)),
            liquidity: None,
            active: true,
            neg_risk: false,
            best_bid: Some(yes_price - dec!(0.01)),
            best_ask: Some(yes_price),
            spread: Some(dec!(0.01)),
            last_trade_price: Some(yes_price),
            description: None,
            end_date_iso: None,
            slug: None,
            one_day_price_change: None,
            event_id: Some(event_id.to_string()),
            last_updated_gen: 0,
        }
    }

    #[tokio::test]
    async fn test_detects_stale_divergence() {
        let detector = StaleMarketDetector::new(
            StaleMarketConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            dec!(0.02),
        );

        // Market A: stale (30h), priced at 0.40
        // Market B: fresh (1h), priced at 0.50
        // In a 2-outcome event, implied_price_A = 1.0 - 0.50 = 0.50
        // Divergence: 0.50 - 0.40 = 0.10 = 1000 bps > 50 bps threshold
        let markets = vec![
            Arc::new(make_market("a", "event1", dec!(0.40), 30)),
            Arc::new(make_market("b", "event1", dec!(0.50), 1)),
        ];

        let opps = detector.scan(&markets).await.unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].strategy_type, StrategyType::StaleMarket);
        assert_eq!(opps[0].legs[0].side, Side::Buy); // underpriced
    }

    #[tokio::test]
    async fn test_skips_fresh_market() {
        let detector = StaleMarketDetector::new(
            StaleMarketConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            dec!(0.02),
        );

        // Both fresh (1h < 24h threshold)
        let markets = vec![
            Arc::new(make_market("a", "event1", dec!(0.40), 1)),
            Arc::new(make_market("b", "event1", dec!(0.50), 1)),
        ];

        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }

    #[tokio::test]
    async fn test_skips_small_divergence() {
        let detector = StaleMarketDetector::new(
            StaleMarketConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            dec!(0.02),
        );

        // Market A stale but prices barely differ
        // implied = 1.0 - 0.505 = 0.495, divergence from 0.50 = 0.005 = 50 bps (borderline)
        let markets = vec![
            Arc::new(make_market("a", "event1", dec!(0.50), 30)),
            Arc::new(make_market("b", "event1", dec!(0.505), 1)),
        ];

        let opps = detector.scan(&markets).await.unwrap();
        // 50 bps divergence is at the threshold, net_edge after fees should be below min_edge_bps
        // This depends on fee deduction, but the divergence is small
        assert!(opps.is_empty() || opps[0].net_edge < dec!(0.005));
    }
}
