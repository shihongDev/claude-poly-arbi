use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
    config::{StrategyConfig, VolumeSpikeConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use tracing::debug;
use uuid::Uuid;

/// Tracks rolling volume per market and detects spikes.
///
/// When current volume > `spike_multiplier` x rolling average,
/// generates an opportunity in the direction of price movement
/// (positive price change = buy, negative = sell).
pub struct VolumeSpikeDetector {
    config: VolumeSpikeConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
    fee_rate: Decimal,
    /// Rolling volume history: condition_id -> ring buffer of recent 24h volumes
    volume_history: Mutex<HashMap<String, VecDeque<Decimal>>>,
}

impl VolumeSpikeDetector {
    pub fn new(
        config: VolumeSpikeConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
        fee_rate: Decimal,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
            fee_rate,
            volume_history: Mutex::new(HashMap::new()),
        }
    }

    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        if !market.active {
            return Ok(opps);
        }

        let current_volume = match market.volume_24hr {
            Some(v) if v >= self.config.min_absolute_volume => v,
            _ => return Ok(opps),
        };

        // Update rolling history and compute average
        let avg_volume = {
            let mut history = self.volume_history.lock().unwrap();
            let entries = history
                .entry(market.condition_id.clone())
                .or_default();

            entries.push_back(current_volume);

            // Keep last 10 observations for rolling average
            if entries.len() > 10 {
                entries.pop_front();
            }

            if entries.len() < 2 {
                return Ok(opps); // need at least 2 observations
            }

            // Average excludes the current reading
            let n = entries.len() - 1;
            let sum: Decimal = entries.iter().take(n).sum();
            sum / Decimal::from(n as u64)
        };

        if avg_volume <= Decimal::ZERO {
            return Ok(opps);
        }

        let ratio = current_volume
            .to_f64()
            .unwrap_or(0.0)
            / avg_volume.to_f64().unwrap_or(1.0);

        if ratio < self.config.spike_multiplier {
            return Ok(opps); // no spike
        }

        // Determine direction from price change
        let price_change = market.one_day_price_change.unwrap_or(Decimal::ZERO);
        let (side, outcome_idx) = if price_change > Decimal::ZERO {
            (Side::Buy, 0) // buying pressure -> buy YES
        } else if price_change < Decimal::ZERO {
            (Side::Sell, 0) // selling pressure -> sell YES
        } else {
            return Ok(opps); // no directional signal
        };

        let book = match market.orderbooks.get(outcome_idx) {
            Some(b) if (!b.asks.is_empty() && side == Side::Buy)
                    || (!b.bids.is_empty() && side == Side::Sell) => b,
            _ => return Ok(opps),
        };

        let target_size = self.config.max_position;
        let vwap = match self.slippage_estimator.estimate_vwap(book, side, target_size) {
            Ok(v) => v,
            Err(_) => return Ok(opps),
        };

        // Gross edge: proportional to price change magnitude
        let gross_edge = price_change.abs();
        let fee_estimate = vwap.vwap * self.fee_rate;
        let net_edge = gross_edge - fee_estimate;
        let edge_bps = net_edge * Decimal::from(10_000);

        if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
            return Ok(opps);
        }

        // Confidence based on spike magnitude
        let confidence = (0.50 + 0.10 * (ratio - self.config.spike_multiplier).min(5.0)).min(0.80);

        let actual_size = target_size.min(vwap.max_available);
        if actual_size <= Decimal::ZERO {
            return Ok(opps);
        }

        let token_id = market.token_ids.get(outcome_idx).cloned().unwrap_or_default();

        debug!(
            market = %market.condition_id,
            volume_ratio = ratio,
            direction = ?side,
            net_edge_bps = %edge_bps,
            "Volume spike opportunity detected"
        );

        opps.push(Opportunity {
            id: Uuid::new_v4(),
            arb_type: ArbType::IntraMarket,
            strategy_type: StrategyType::VolumeSpike,
            markets: vec![market.condition_id.clone()],
            legs: vec![TradeLeg {
                token_id,
                side,
                target_price: market.outcome_prices.get(outcome_idx).copied().unwrap_or_default(),
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

        Ok(opps)
    }
}

#[async_trait]
impl ArbDetector for VolumeSpikeDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking volume spike");
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
    use rust_decimal_macros::dec;

    struct MockSlippage;
    impl SlippageEstimator for MockSlippage {
        fn estimate_vwap(
            &self,
            _book: &OrderbookSnapshot,
            _side: Side,
            _size: Decimal,
        ) -> Result<VwapEstimate> {
            Ok(VwapEstimate {
                vwap: dec!(0.60),
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

    fn make_market(volume: Decimal, price_change: Decimal) -> MarketState {
        MarketState {
            condition_id: "test".to_string(),
            question: "Test?".to_string(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["yes_tok".into(), "no_tok".into()],
            outcome_prices: vec![dec!(0.60), dec!(0.40)],
            orderbooks: vec![OrderbookSnapshot {
                token_id: "yes_tok".into(),
                bids: vec![OrderbookLevel { price: dec!(0.59), size: dec!(500) }],
                asks: vec![OrderbookLevel { price: dec!(0.60), size: dec!(500) }],
                timestamp: Utc::now(),
            }],
            volume_24hr: Some(volume),
            liquidity: None,
            active: true,
            neg_risk: false,
            best_bid: Some(dec!(0.59)),
            best_ask: Some(dec!(0.60)),
            spread: Some(dec!(0.01)),
            last_trade_price: Some(dec!(0.60)),
            description: None,
            end_date_iso: None,
            slug: None,
            one_day_price_change: Some(price_change),
            event_id: None,
            last_updated_gen: 0,
        }
    }

    #[tokio::test]
    async fn test_detects_volume_spike() {
        let detector = VolumeSpikeDetector::new(
            VolumeSpikeConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            dec!(0.02),
        );

        // Seed history with low volumes, then hit a spike
        let normal = make_market(dec!(2000), dec!(0.05));
        let spike = make_market(dec!(10000), dec!(0.05));

        let markets_normal: Vec<Arc<MarketState>> = vec![Arc::new(normal)];
        // First 3 scans seed the average
        for _ in 0..3 {
            let _ = detector.scan(&markets_normal).await;
        }

        // Spike scan: 10000 / avg(2000) = 5.0 > 3.0
        let markets_spike: Vec<Arc<MarketState>> = vec![Arc::new(spike)];
        let opps = detector.scan(&markets_spike).await.unwrap();
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].strategy_type, StrategyType::VolumeSpike);
        assert_eq!(opps[0].legs[0].side, Side::Buy); // positive price change
    }

    #[tokio::test]
    async fn test_skips_no_spike() {
        let detector = VolumeSpikeDetector::new(
            VolumeSpikeConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            dec!(0.02),
        );

        let normal = make_market(dec!(2000), dec!(0.05));
        let markets: Vec<Arc<MarketState>> = vec![Arc::new(normal)];

        // Need at least 2 scans, no spike
        let _ = detector.scan(&markets).await;
        let opps = detector.scan(&markets).await.unwrap();
        assert!(opps.is_empty());
    }
}
