use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
    config::{ResolutionSnipingConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use tracing::debug;
use uuid::Uuid;

/// Detects resolution sniping (扫尾盘) opportunities.
///
/// Markets approaching their resolution date often have one outcome trading
/// near $1.00. If the leading outcome is priced between `min_price` and
/// `max_price` (e.g., $0.92–$0.98) and the market resolves within
/// `max_hours_to_resolution`, buying the leader at a discount captures the
/// spread to $1.00 at resolution.
///
/// This is directional (not risk-free) — confidence depends on how close
/// the price is to $1.00 and the market's volume.
pub struct ResolutionSnipingDetector {
    config: ResolutionSnipingConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

impl ResolutionSnipingDetector {
    pub fn new(
        config: ResolutionSnipingConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
        }
    }

    /// Parse `end_date_iso` and return hours until resolution.
    fn hours_to_resolution(market: &MarketState) -> Option<f64> {
        let end_str = market.end_date_iso.as_deref()?;
        let end_dt = end_str
            .parse::<DateTime<Utc>>()
            .or_else(|_| {
                // Try common ISO formats that chrono can parse
                DateTime::parse_from_rfc3339(end_str).map(|dt| dt.with_timezone(&Utc))
            })
            .ok()?;
        let remaining = end_dt - Utc::now();
        if remaining.num_seconds() <= 0 {
            return None; // already past
        }
        Some(remaining.num_minutes() as f64 / 60.0)
    }

    /// Check a single market for resolution sniping opportunity.
    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        if !market.active {
            return Ok(opps);
        }

        // Must have a resolution date within our window
        let hours_left = match Self::hours_to_resolution(market) {
            Some(h) if h <= self.config.max_hours_to_resolution as f64 && h > 0.0 => h,
            _ => return Ok(opps),
        };

        // Check volume threshold
        let volume = market.volume_24hr.unwrap_or(Decimal::ZERO);
        if volume < self.config.min_volume_24h {
            return Ok(opps);
        }

        // Check each outcome for high-price sniping
        for (i, price) in market.outcome_prices.iter().enumerate() {
            if *price < self.config.min_price || *price > self.config.max_price {
                continue;
            }

            // Need an orderbook for this outcome
            let book = match market.orderbooks.get(i) {
                Some(b) if !b.asks.is_empty() => b,
                _ => continue,
            };

            let token_id = match market.token_ids.get(i) {
                Some(t) => t.clone(),
                None => continue,
            };

            // VWAP check at our max position size
            let target_size = self.config.max_position;
            let vwap = match self.slippage_estimator.estimate_vwap(book, Side::Buy, target_size) {
                Ok(v) => v,
                Err(_) => continue,
            };

            // Net edge: $1.00 - VWAP buy price (per unit profit if resolves YES)
            let gross_edge = dec!(1.00) - *price;
            let net_edge_per_unit = dec!(1.00) - vwap.vwap;

            // Subtract fees (~2% on notional)
            let fee_estimate = vwap.vwap * dec!(0.02);
            let net_edge_after_fees = net_edge_per_unit - fee_estimate;

            let edge_bps = net_edge_after_fees * Decimal::from(10_000);
            if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
                continue;
            }

            // Confidence: higher price -> higher confidence, closer to resolution -> higher
            let price_f = price.to_f64().unwrap_or(0.5);
            let time_factor = 1.0 - (hours_left / self.config.max_hours_to_resolution as f64);
            let confidence = 0.5 + 0.3 * price_f + 0.2 * time_factor;
            let confidence = confidence.clamp(0.50, 0.95);

            let actual_size = target_size.min(vwap.max_available);
            if actual_size <= Decimal::ZERO {
                continue;
            }

            debug!(
                market = %market.condition_id,
                outcome = i,
                price = %price,
                hours_left = hours_left,
                net_edge_bps = %edge_bps,
                confidence = confidence,
                "Resolution sniping opportunity detected"
            );

            opps.push(Opportunity {
                id: Uuid::new_v4(),
                arb_type: ArbType::IntraMarket,
                strategy_type: StrategyType::ResolutionSniping,
                markets: vec![market.condition_id.clone()],
                legs: vec![TradeLeg {
                    token_id,
                    side: Side::Buy,
                    target_price: *price,
                    target_size: actual_size,
                    vwap_estimate: vwap.vwap,
                }],
                gross_edge,
                net_edge: net_edge_after_fees,
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
impl ArbDetector for ResolutionSnipingDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking resolution sniping");
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
                vwap: dec!(0.95),
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

    fn make_market(price: Decimal, hours_from_now: i64) -> MarketState {
        let end_date = Utc::now() + chrono::Duration::hours(hours_from_now);
        MarketState {
            condition_id: "test_market".to_string(),
            question: "Will X happen?".to_string(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["yes_tok".into(), "no_tok".into()],
            outcome_prices: vec![price, dec!(1.00) - price],
            orderbooks: vec![
                OrderbookSnapshot {
                    token_id: "yes_tok".into(),
                    bids: vec![OrderbookLevel {
                        price: price - dec!(0.01),
                        size: dec!(500),
                    }],
                    asks: vec![OrderbookLevel {
                        price,
                        size: dec!(500),
                    }],
                    timestamp: Utc::now(),
                },
                OrderbookSnapshot {
                    token_id: "no_tok".into(),
                    bids: vec![],
                    asks: vec![OrderbookLevel {
                        price: dec!(1.00) - price,
                        size: dec!(500),
                    }],
                    timestamp: Utc::now(),
                },
            ],
            volume_24hr: Some(dec!(10000)),
            liquidity: None,
            active: true,
            neg_risk: false,
            best_bid: Some(price - dec!(0.01)),
            best_ask: Some(price),
            spread: Some(dec!(0.01)),
            last_trade_price: Some(price),
            description: None,
            end_date_iso: Some(end_date.to_rfc3339()),
            slug: None,
            one_day_price_change: None,
            event_id: None,
            last_updated_gen: 0,
        }
    }

    #[tokio::test]
    async fn test_detects_high_price_near_resolution() {
        let detector = ResolutionSnipingDetector::new(
            ResolutionSnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        let market = make_market(dec!(0.95), 24);
        let markets: Vec<Arc<MarketState>> = vec![Arc::new(market)];

        let opps = detector.scan(&markets).await.unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].strategy_type, StrategyType::ResolutionSniping);
        assert!(opps[0].confidence > 0.5);
    }

    #[tokio::test]
    async fn test_skips_low_price() {
        let detector = ResolutionSnipingDetector::new(
            ResolutionSnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // Price too low (0.50 < min_price 0.92)
        let market = make_market(dec!(0.50), 24);
        let markets: Vec<Arc<MarketState>> = vec![Arc::new(market)];

        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }

    #[tokio::test]
    async fn test_skips_far_from_resolution() {
        let detector = ResolutionSnipingDetector::new(
            ResolutionSnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        // 100 hours > max_hours 48
        let market = make_market(dec!(0.95), 100);
        let markets: Vec<Arc<MarketState>> = vec![Arc::new(market)];

        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }

    #[tokio::test]
    async fn test_skips_low_volume() {
        let mut market = make_market(dec!(0.95), 24);
        market.volume_24hr = Some(dec!(100)); // below min_volume_24h (5000)

        let detector = ResolutionSnipingDetector::new(
            ResolutionSnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        let markets: Vec<Arc<MarketState>> = vec![Arc::new(market)];
        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }

    #[tokio::test]
    async fn test_confidence_increases_with_price() {
        let detector = ResolutionSnipingDetector::new(
            ResolutionSnipingConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
        );

        let m1 = make_market(dec!(0.92), 24);
        let m2 = make_market(dec!(0.97), 24);

        let opps1 = detector.scan(&[Arc::new(m1)]).await.unwrap();
        let opps2 = detector.scan(&[Arc::new(m2)]).await.unwrap();

        assert!(!opps1.is_empty());
        assert!(!opps2.is_empty());
        assert!(opps2[0].confidence > opps1[0].confidence);
    }
}
