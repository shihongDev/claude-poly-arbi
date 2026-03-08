use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
    config::{ProbModelConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, ProbabilityEstimator, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use rust_decimal_macros::dec;
use tracing::debug;
use uuid::Uuid;

/// Uses the ensemble probability estimator to find model-vs-market divergences.
///
/// When the model's fair value diverges from market price by more than
/// `min_deviation_bps` with confidence above `min_confidence`,
/// generates a directional opportunity.
pub struct ProbModelDetector {
    config: ProbModelConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
    estimator: Arc<dyn ProbabilityEstimator>,
}

impl ProbModelDetector {
    pub fn new(
        config: ProbModelConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
        estimator: Arc<dyn ProbabilityEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
            estimator,
        }
    }

    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        if !market.active || market.outcome_prices.is_empty() {
            return Ok(opps);
        }

        let estimate = match self.estimator.estimate(market) {
            Ok(e) => e,
            Err(_) => return Ok(opps),
        };

        // Compare model probability vs market price for each outcome
        for (i, model_prob) in estimate.probabilities.iter().enumerate() {
            let market_price = match market.outcome_prices.get(i) {
                Some(p) => *p,
                None => continue,
            };

            let model_price = Decimal::try_from(*model_prob).unwrap_or(Decimal::ZERO);
            let deviation = model_price - market_price;
            let deviation_bps = (deviation.abs() * Decimal::from(10_000))
                .to_u64()
                .unwrap_or(0);

            if deviation_bps < self.config.min_deviation_bps {
                continue;
            }

            // Check confidence from the estimate's confidence interval
            let ci_width = estimate
                .confidence_interval
                .get(i)
                .map(|(lo, hi)| hi - lo)
                .unwrap_or(1.0);
            let confidence = (1.0 - ci_width).max(0.0);

            if confidence < self.config.min_confidence {
                continue;
            }

            // Direction: if model says higher than market, buy; if lower, sell
            let (side, outcome_idx) = if deviation > Decimal::ZERO {
                (Side::Buy, i) // underpriced by market
            } else {
                (Side::Sell, i) // overpriced by market
            };

            let book = match market.orderbooks.get(outcome_idx) {
                Some(b) => b,
                None => continue,
            };

            // Verify the book has liquidity on our side
            let has_liquidity = match side {
                Side::Buy => !book.asks.is_empty(),
                Side::Sell => !book.bids.is_empty(),
            };
            if !has_liquidity {
                continue;
            }

            let target_size = self.config.max_position;
            let vwap = match self.slippage_estimator.estimate_vwap(book, side, target_size) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let gross_edge = deviation.abs();
            let fee_estimate = vwap.vwap * dec!(0.02);
            let net_edge = gross_edge - fee_estimate;
            let edge_bps = net_edge * Decimal::from(10_000);

            if edge_bps < Decimal::from(self.strategy_config.min_edge_bps) {
                continue;
            }

            let actual_size = target_size.min(vwap.max_available);
            if actual_size <= Decimal::ZERO {
                continue;
            }

            let token_id = market.token_ids.get(outcome_idx).cloned().unwrap_or_default();

            debug!(
                market = %market.condition_id,
                outcome = i,
                model_prob = model_prob,
                market_price = %market_price,
                deviation_bps = deviation_bps,
                confidence = confidence,
                "Probability model opportunity detected"
            );

            opps.push(Opportunity {
                id: Uuid::new_v4(),
                arb_type: ArbType::IntraMarket,
                strategy_type: StrategyType::ProbabilityModel,
                markets: vec![market.condition_id.clone()],
                legs: vec![TradeLeg {
                    token_id,
                    side,
                    target_price: market_price,
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
impl ArbDetector for ProbModelDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking prob model");
                }
            }
        }
        Ok(all_opps)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::{OrderbookLevel, OrderbookSnapshot, ProbEstimate, VwapEstimate};

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

    struct MockEstimator {
        probability: f64,
        ci_width: f64,
    }

    impl ProbabilityEstimator for MockEstimator {
        fn estimate(&self, _market: &MarketState) -> Result<ProbEstimate> {
            Ok(ProbEstimate {
                probabilities: vec![self.probability, 1.0 - self.probability],
                confidence_interval: vec![
                    (self.probability - self.ci_width / 2.0, self.probability + self.ci_width / 2.0),
                    (1.0 - self.probability - self.ci_width / 2.0, 1.0 - self.probability + self.ci_width / 2.0),
                ],
                method: "mock".into(),
            })
        }

        fn update(&mut self, _market: &MarketState, _new_data: &MarketState) {}
    }

    fn make_market(yes_price: Decimal) -> MarketState {
        MarketState {
            condition_id: "test".to_string(),
            question: "Test?".to_string(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["yes".into(), "no".into()],
            outcome_prices: vec![yes_price, dec!(1.00) - yes_price],
            orderbooks: vec![
                OrderbookSnapshot {
                    token_id: "yes".into(),
                    bids: vec![OrderbookLevel { price: yes_price - dec!(0.01), size: dec!(500) }],
                    asks: vec![OrderbookLevel { price: yes_price, size: dec!(500) }],
                    timestamp: Utc::now(),
                },
                OrderbookSnapshot {
                    token_id: "no".into(),
                    bids: vec![OrderbookLevel { price: dec!(1.00) - yes_price - dec!(0.01), size: dec!(500) }],
                    asks: vec![OrderbookLevel { price: dec!(1.00) - yes_price, size: dec!(500) }],
                    timestamp: Utc::now(),
                },
            ],
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
    async fn test_detects_underpriced_market() {
        let estimator = Arc::new(MockEstimator {
            probability: 0.65,  // model says 65%
            ci_width: 0.10,     // tight CI -> high confidence (90%)
        });

        let detector = ProbModelDetector::new(
            ProbModelConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            estimator,
        );

        // Market price 0.50, model says 0.65 -> 150 bps deviation > 100 bps threshold
        // Both YES (underpriced) and NO (overpriced) are detected
        let market = make_market(dec!(0.50));
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();

        assert_eq!(opps.len(), 2);
        assert_eq!(opps[0].strategy_type, StrategyType::ProbabilityModel);
        assert_eq!(opps[0].legs[0].side, Side::Buy); // YES underpriced
        assert_eq!(opps[1].legs[0].side, Side::Sell); // NO overpriced
    }

    #[tokio::test]
    async fn test_skips_small_deviation() {
        let estimator = Arc::new(MockEstimator {
            probability: 0.505,  // barely different from 0.50
            ci_width: 0.10,
        });

        let detector = ProbModelDetector::new(
            ProbModelConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            estimator,
        );

        let market = make_market(dec!(0.50));
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();
        assert!(opps.is_empty()); // 5 bps < 100 bps threshold
    }

    #[tokio::test]
    async fn test_skips_low_confidence() {
        let estimator = Arc::new(MockEstimator {
            probability: 0.70,
            ci_width: 0.50, // wide CI -> low confidence (50%)
        });

        let detector = ProbModelDetector::new(
            ProbModelConfig::default(),
            StrategyConfig::default(),
            Arc::new(MockSlippage),
            estimator,
        );

        let market = make_market(dec!(0.50));
        let opps = detector.scan(&[Arc::new(market)]).await.unwrap();
        assert!(opps.is_empty()); // confidence 0.50 < 0.70 threshold
    }
}
