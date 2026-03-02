use arb_core::{
    ArbType, CorrelationRelationship, MarketCorrelation, MarketState, Opportunity, Side, TradeLeg,
    config::{CrossMarketConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use arb_data::correlation::CorrelationGraph;
use arb_data::market_cache::MarketCache;
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::sync::Arc;
use tracing::debug;
use uuid::Uuid;

/// Detects cross-market arbitrage from user-defined logical relationships.
///
/// When two markets have a known constraint (e.g., P(A) <= P(B) because A implies B),
/// any price violation of that constraint represents a trading opportunity.
pub struct CrossMarketDetector {
    config: CrossMarketConfig,
    strategy_config: StrategyConfig,
    correlation_graph: Arc<CorrelationGraph>,
    _cache: Arc<MarketCache>,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

impl CrossMarketDetector {
    pub fn new(
        config: CrossMarketConfig,
        strategy_config: StrategyConfig,
        correlation_graph: Arc<CorrelationGraph>,
        cache: Arc<MarketCache>,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            correlation_graph,
            _cache: cache,
            slippage_estimator,
        }
    }

    /// Get the YES token's mid-price for a market.
    fn yes_price(market: &MarketState) -> Option<Decimal> {
        market.outcome_prices.first().copied()
    }

    /// Get the YES token's orderbook (index 0).
    fn yes_book(market: &MarketState) -> Option<&arb_core::OrderbookSnapshot> {
        market.orderbooks.first()
    }

    fn check_pair(
        &self,
        pair: &MarketCorrelation,
        markets: &[MarketState],
    ) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        let market_a = markets
            .iter()
            .find(|m| m.condition_id == pair.condition_id_a);
        let market_b = markets
            .iter()
            .find(|m| m.condition_id == pair.condition_id_b);

        let (market_a, market_b) = match (market_a, market_b) {
            (Some(a), Some(b)) => (a, b),
            _ => return Ok(opps), // one or both markets not in current scan
        };

        let price_a = match Self::yes_price(market_a) {
            Some(p) => p,
            None => return Ok(opps),
        };
        let price_b = match Self::yes_price(market_b) {
            Some(p) => p,
            None => return Ok(opps),
        };

        match &pair.relationship {
            // P(A) <= P(B): if A implies B, A's price must not exceed B's
            CorrelationRelationship::ImpliedBy => {
                if price_a > price_b + self.config.min_implied_edge {
                    let edge = price_a - price_b;
                    self.build_cross_opp(
                        market_a,
                        market_b,
                        Side::Sell, // sell overpriced A
                        Side::Buy,  // buy underpriced B
                        edge,
                        &mut opps,
                    )?;
                }
            }

            // P(A) + P(B) <= 1.0: mutually exclusive
            CorrelationRelationship::MutuallyExclusive => {
                if price_a + price_b > dec!(1.00) + self.config.min_implied_edge {
                    let edge = price_a + price_b - dec!(1.00);
                    self.build_cross_opp(
                        market_a,
                        market_b,
                        Side::Sell, // sell both
                        Side::Sell,
                        edge,
                        &mut opps,
                    )?;
                }
            }

            // P(A) + P(B) >= 1.0: at least one must be true
            CorrelationRelationship::Exhaustive => {
                if price_a + price_b < dec!(1.00) - self.config.min_implied_edge {
                    let edge = dec!(1.00) - price_a - price_b;
                    self.build_cross_opp(
                        market_a,
                        market_b,
                        Side::Buy, // buy both
                        Side::Buy,
                        edge,
                        &mut opps,
                    )?;
                }
            }

            CorrelationRelationship::Custom { .. } => {
                // Custom constraints require simulation engine — skip for now
                debug!(
                    pair_a = %pair.condition_id_a,
                    pair_b = %pair.condition_id_b,
                    "Skipping custom correlation constraint (requires simulation)"
                );
            }
        }

        Ok(opps)
    }

    fn build_cross_opp(
        &self,
        market_a: &MarketState,
        market_b: &MarketState,
        side_a: Side,
        side_b: Side,
        gross_edge: Decimal,
        opps: &mut Vec<Opportunity>,
    ) -> Result<()> {
        let book_a = match Self::yes_book(market_a) {
            Some(b) => b,
            None => return Ok(()),
        };
        let book_b = match Self::yes_book(market_b) {
            Some(b) => b,
            None => return Ok(()),
        };

        let target_size = Decimal::from(500); // conservative default

        let vwap_a = match self.slippage_estimator.estimate_vwap(book_a, side_a, target_size) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };
        let vwap_b = match self.slippage_estimator.estimate_vwap(book_b, side_b, target_size) {
            Ok(v) => v,
            Err(_) => return Ok(()),
        };

        let fee_estimate = target_size * (vwap_a.vwap + vwap_b.vwap) * dec!(0.02);
        let net_edge = gross_edge * target_size - fee_estimate;
        let net_edge_per_unit = net_edge / target_size;
        let edge_bps = net_edge_per_unit * Decimal::from(10_000);

        if edge_bps >= Decimal::from(self.strategy_config.min_edge_bps) {
            debug!(
                market_a = %market_a.condition_id,
                market_b = %market_b.condition_id,
                gross_edge = %gross_edge,
                net_edge = %net_edge_per_unit,
                edge_bps = %edge_bps,
                "Cross-market opportunity detected"
            );

            opps.push(Opportunity {
                id: Uuid::new_v4(),
                arb_type: ArbType::CrossMarket,
                markets: vec![
                    market_a.condition_id.clone(),
                    market_b.condition_id.clone(),
                ],
                legs: vec![
                    TradeLeg {
                        token_id: market_a.token_ids[0].clone(),
                        side: side_a,
                        target_price: Self::yes_price(market_a).unwrap_or_default(),
                        target_size,
                        vwap_estimate: vwap_a.vwap,
                    },
                    TradeLeg {
                        token_id: market_b.token_ids[0].clone(),
                        side: side_b,
                        target_price: Self::yes_price(market_b).unwrap_or_default(),
                        target_size,
                        vwap_estimate: vwap_b.vwap,
                    },
                ],
                gross_edge,
                net_edge: net_edge_per_unit,
                estimated_vwap: vec![vwap_a.vwap, vwap_b.vwap],
                confidence: 0.85, // cross-market arbs have lower confidence (depends on correlation correctness)
                size_available: target_size,
                detected_at: Utc::now(),
            });
        }

        Ok(())
    }
}

#[async_trait]
impl ArbDetector for CrossMarketDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::CrossMarket
    }

    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();

        for pair in self.correlation_graph.pairs() {
            match self.check_pair(pair, markets) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(
                        pair_a = %pair.condition_id_a,
                        pair_b = %pair.condition_id_b,
                        error = %e,
                        "Error checking cross-market pair"
                    );
                }
            }
        }

        Ok(all_opps)
    }
}
