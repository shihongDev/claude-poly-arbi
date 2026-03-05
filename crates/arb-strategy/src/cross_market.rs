use std::sync::Arc;

use arb_core::{
    ArbType, CorrelationRelationship, MarketCorrelation, MarketState, Opportunity, Side, TradeLeg,
    config::{CrossMarketConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use arb_data::correlation::CorrelationGraph;
use arb_data::market_cache::MarketCache;
use arb_simulation::copula::TCopula;
use async_trait::async_trait;
use chrono::Utc;
use nalgebra::DMatrix;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
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

    /// Estimate confidence adjustment using t-copula tail dependence.
    ///
    /// Uses the price correlation between two markets (estimated from their
    /// current prices as a rough proxy) to compute the t-copula tail dependence
    /// coefficient. High λ means the pair's relationship is robust in extremes.
    fn copula_confidence_adjustment(price_a: f64, price_b: f64) -> f64 {
        // Estimate correlation from price proximity (both are probabilities in [0,1]).
        // If both markets are near 0.5, they're more uncertain -> lower correlation.
        // If both are extreme (near 0 or 1), the constraint is more binding.
        let extremity_a = (price_a - 0.5).abs() * 2.0; // 0..1
        let extremity_b = (price_b - 0.5).abs() * 2.0;
        let rho = 0.3 + 0.5 * (extremity_a * extremity_b).sqrt(); // 0.3..0.8 range

        let corr = DMatrix::from_row_slice(2, 2, &[1.0, rho, rho, 1.0]);
        let df = 5.0; // moderate tail heaviness

        match TCopula::new(corr, df) {
            Ok(copula) => {
                let lambda = copula.tail_dependence(0, 1);
                // Map λ to confidence: λ > 0.3 -> boost, λ < 0.1 -> penalize
                // Base confidence = 0.85, adjusted range: [0.60, 0.95]
                let base = 0.85;
                if lambda > 0.3 {
                    (base + (lambda - 0.3) * 0.3).min(0.95)
                } else if lambda < 0.1 {
                    (base - (0.1 - lambda) * 2.5).max(0.60)
                } else {
                    base
                }
            }
            Err(_) => 0.85, // fallback to static confidence
        }
    }

    fn check_pair(
        &self,
        pair: &MarketCorrelation,
        markets: &[Arc<MarketState>],
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
        // Ensure token_ids and orderbooks are aligned before accessing index 0.
        // A partial orderbook fetch could leave orderbooks shorter than token_ids,
        // causing the wrong token to be paired with the wrong book.
        if market_a.token_ids.len() != market_a.orderbooks.len()
            || market_b.token_ids.len() != market_b.orderbooks.len()
        {
            return Ok(());
        }

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
            // Adjust confidence using t-copula tail dependence when enabled
            let confidence = if self.config.use_copula_correlations {
                use rust_decimal::prelude::ToPrimitive;
                let pa = Self::yes_price(market_a).and_then(|p| p.to_f64()).unwrap_or(0.5);
                let pb = Self::yes_price(market_b).and_then(|p| p.to_f64()).unwrap_or(0.5);
                Self::copula_confidence_adjustment(pa, pb)
            } else {
                0.85 // static fallback
            };

            debug!(
                market_a = %market_a.condition_id,
                market_b = %market_b.condition_id,
                gross_edge = %gross_edge,
                net_edge = %net_edge_per_unit,
                edge_bps = %edge_bps,
                confidence = %confidence,
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
                confidence,
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

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
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
