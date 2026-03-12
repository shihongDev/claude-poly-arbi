use std::sync::Arc;

use arb_core::{
    Opportunity, TradeLeg, Side,
    config::FeeConfig,
    error::Result,
    traits::SlippageEstimator,
};
use arb_data::market_cache::MarketCache;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;

/// Central EV computation engine.
///
/// Handles fee calculation and VWAP refinement for all arb types.
/// Supports Polymarket's maker/taker fee model: makers (GTC/post-only)
/// pay 0%, takers (FOK/market orders) pay ~2%.
pub struct EdgeCalculator {
    fee_rate: Decimal,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

impl EdgeCalculator {
    pub fn new(fee_rate: Decimal, slippage_estimator: Arc<dyn SlippageEstimator>) -> Self {
        Self {
            fee_rate,
            slippage_estimator,
        }
    }

    /// Construct from `FeeConfig` and `prefer_post_only` flag.
    ///
    /// When `prefer_post_only` is true, uses the maker fee rate (typically 0%).
    /// When false, uses the taker fee rate (typically 2%).
    pub fn from_config(
        fee_config: &FeeConfig,
        prefer_post_only: bool,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self::new(
            fee_config.effective_rate(prefer_post_only),
            slippage_estimator,
        )
    }

    /// Default with Polymarket's 2% taker fee rate (backward-compatible).
    pub fn default_with_estimator(slippage_estimator: Arc<dyn SlippageEstimator>) -> Self {
        Self::new(dec!(0.02), slippage_estimator)
    }

    /// Refine an opportunity's edge using real VWAP from the market cache.
    ///
    /// Re-walks the orderbook for each leg and updates net_edge and estimated_vwap.
    pub fn refine_with_vwap(&self, opp: &mut Opportunity, cache: &MarketCache) -> Result<()> {
        let mut total_vwap_cost = Decimal::ZERO;
        let mut estimated_vwaps = Vec::with_capacity(opp.legs.len());

        // Pre-fetch all relevant markets from cache once (cheap Arc clones)
        let cached_markets: Vec<_> = opp
            .markets
            .iter()
            .filter_map(|cid| cache.get(cid))
            .collect();

        for leg in &mut opp.legs {
            // Find the orderbook for this token in the cached markets
            let market = cached_markets
                .iter()
                .find_map(|m| {
                    m.orderbooks
                        .iter()
                        .find(|ob| ob.token_id == leg.token_id)
                });

            if let Some(book) = market {
                let vwap_est = self
                    .slippage_estimator
                    .estimate_vwap(book, leg.side, leg.target_size)?;

                leg.vwap_estimate = vwap_est.vwap;
                estimated_vwaps.push(vwap_est.vwap);

                match leg.side {
                    Side::Buy => total_vwap_cost += vwap_est.vwap * leg.target_size,
                    Side::Sell => total_vwap_cost -= vwap_est.vwap * leg.target_size,
                }
            } else {
                estimated_vwaps.push(leg.target_price);
            }
        }

        opp.estimated_vwap = estimated_vwaps;
        let fees = self.calculate_fees(&opp.legs);
        opp.net_edge = opp.gross_edge - fees;

        Ok(())
    }

    /// Calculate total fees across all legs.
    pub fn calculate_fees(&self, legs: &[TradeLeg]) -> Decimal {
        legs.iter()
            .map(|leg| leg.target_size * leg.vwap_estimate * self.fee_rate)
            .sum()
    }

    /// For structural arbs (intra-market, multi-outcome), compute edge from
    /// the deviation of price sum from the theoretical target (1.00).
    pub fn structural_edge(&self, price_sum: Decimal, target: Decimal) -> Decimal {
        (price_sum - target).abs()
    }

    /// Compute probability-weighted expected edge.
    ///
    /// A quant desk doesn't just look at raw edge — it weights by the probability
    /// of realization. A 200bps edge with 90% confidence beats a 300bps edge at 40%.
    ///
    /// Formula: `expected_edge = net_edge × confidence − net_edge × (1 − confidence) × loss_factor`
    ///
    /// `loss_factor` accounts for adverse selection when the edge doesn't materialize
    /// (typically you lose more than the edge when wrong, due to fees + slippage).
    pub fn confidence_adjusted_edge(
        net_edge: Decimal,
        confidence: f64,
        loss_factor: f64,
    ) -> Decimal {
        let conf = Decimal::try_from(confidence).unwrap_or(dec!(0.5));
        let loss = Decimal::try_from(loss_factor).unwrap_or(dec!(1.5));
        net_edge * conf - net_edge * (Decimal::ONE - conf) * loss
    }

    /// Confidence-adjusted edge in basis points.
    pub fn confidence_adjusted_edge_bps(
        net_edge: Decimal,
        confidence: f64,
        loss_factor: f64,
    ) -> Decimal {
        Self::confidence_adjusted_edge(net_edge, confidence, loss_factor)
            * Decimal::from(10_000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_data::orderbook::OrderbookProcessor;
    use rust_decimal_macros::dec;

    fn default_calc() -> EdgeCalculator {
        let estimator: Arc<dyn SlippageEstimator> =
            Arc::new(OrderbookProcessor::new(arb_core::config::SlippageConfig {
                max_slippage_bps: 100,
                order_split_threshold: 500,
                prefer_post_only: true,
                vwap_depth_levels: 10,
            }));
        EdgeCalculator::default_with_estimator(estimator)
    }

    #[test]
    fn test_fee_calculation() {
        let calc = default_calc();
        let legs = vec![
            TradeLeg {
                token_id: "a".into(),
                side: Side::Buy,
                target_price: dec!(0.50),
                target_size: dec!(100),
                vwap_estimate: dec!(0.50),
            },
            TradeLeg {
                token_id: "b".into(),
                side: Side::Buy,
                target_price: dec!(0.48),
                target_size: dec!(100),
                vwap_estimate: dec!(0.48),
            },
        ];
        // fees = 100 * 0.50 * 0.02 + 100 * 0.48 * 0.02 = 1.00 + 0.96 = 1.96
        let fees = calc.calculate_fees(&legs);
        assert_eq!(fees, dec!(1.96));
    }

    #[test]
    fn test_structural_edge() {
        let calc = default_calc();
        assert_eq!(calc.structural_edge(dec!(0.97), dec!(1.00)), dec!(0.03));
        assert_eq!(calc.structural_edge(dec!(1.03), dec!(1.00)), dec!(0.03));
    }

    #[test]
    fn test_confidence_adjusted_edge_high_confidence() {
        // 200bps edge, 90% confidence, 1.5x loss factor
        let adj = EdgeCalculator::confidence_adjusted_edge(dec!(0.02), 0.9, 1.5);
        // expected = 0.02 * 0.9 - 0.02 * 0.1 * 1.5 = 0.018 - 0.003 = 0.015
        assert!(adj > Decimal::ZERO, "High confidence should be profitable: {adj}");
        assert!((adj - dec!(0.015)).abs() < dec!(0.001));
    }

    #[test]
    fn test_confidence_adjusted_edge_low_confidence() {
        // 200bps edge, 30% confidence, 1.5x loss factor
        let adj = EdgeCalculator::confidence_adjusted_edge(dec!(0.02), 0.3, 1.5);
        // expected = 0.02 * 0.3 - 0.02 * 0.7 * 1.5 = 0.006 - 0.021 = -0.015
        assert!(adj < Decimal::ZERO, "Low confidence should be negative: {adj}");
    }

    #[test]
    fn test_confidence_adjusted_edge_breakeven() {
        // At what confidence does edge break even?
        // 0 = net_edge * c - net_edge * (1-c) * loss_factor
        // c = loss_factor / (1 + loss_factor) = 1.5 / 2.5 = 0.6
        let adj = EdgeCalculator::confidence_adjusted_edge(dec!(0.02), 0.6, 1.5);
        assert!(adj.abs() < dec!(0.001), "Should be near zero at breakeven: {adj}");
    }

    #[test]
    fn test_confidence_adjusted_edge_bps() {
        let bps = EdgeCalculator::confidence_adjusted_edge_bps(dec!(0.02), 0.9, 1.5);
        // 0.015 * 10000 = 150 bps
        assert!((bps - dec!(150)).abs() < dec!(10));
    }
}
