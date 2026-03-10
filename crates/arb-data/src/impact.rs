//! Market impact estimation combining Kyle's lambda model with orderbook depth walk.
//!
//! Estimates the price impact of an order by combining:
//! - **Kyle's lambda**: permanent impact proportional to order size / daily volume
//! - **Orderbook depth walk**: temporary impact from consuming visible liquidity (VWAP-based)

use arb_core::{OrderbookSnapshot, Side};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;

/// Estimates market impact for prospective orders.
pub struct MarketImpactEstimator {
    kyle_lambda: f64,
}

/// Result of a market impact estimation.
#[derive(Debug, Clone)]
pub struct ImpactEstimate {
    /// Combined impact in basis points (max of Kyle and depth-based).
    pub impact_bps: Decimal,
    /// VWAP-weighted effective fill price walking through the book.
    pub effective_price: Decimal,
    /// Fraction of visible book depth consumed (0.0 to 1.0+).
    pub depth_consumed_pct: f64,
}

impl MarketImpactEstimator {
    pub fn new(kyle_lambda: f64) -> Self {
        Self { kyle_lambda }
    }

    pub fn default_config() -> Self {
        Self::new(0.5)
    }

    /// Estimate market impact for an order.
    ///
    /// Combines Kyle's lambda model (permanent impact) with orderbook depth walk
    /// (temporary impact). Returns the max of both as the conservative estimate.
    pub fn estimate(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        size: Decimal,
        avg_daily_volume: Option<Decimal>,
    ) -> ImpactEstimate {
        // Kyle impact: impact_bps = kyle_lambda * order_size / avg_daily_volume * 10000
        let kyle_impact_bps = if let Some(adv) = avg_daily_volume {
            if adv > Decimal::ZERO {
                let ratio = size.to_f64().unwrap_or(0.0) / adv.to_f64().unwrap_or(1.0);
                Decimal::from_f64_retain(self.kyle_lambda * ratio * 10000.0)
                    .unwrap_or(Decimal::ZERO)
            } else {
                Decimal::ZERO
            }
        } else {
            Decimal::ZERO
        };

        // Orderbook depth walk: consume levels on the relevant side
        let levels = match side {
            Side::Buy => &book.asks,
            Side::Sell => &book.bids,
        };

        let total_depth: Decimal = levels.iter().map(|l| l.size).sum();
        let depth_consumed_pct = if total_depth > Decimal::ZERO {
            size.to_f64().unwrap_or(0.0) / total_depth.to_f64().unwrap_or(1.0)
        } else {
            1.0
        };

        // Walk the book to calculate effective price (VWAP through the levels)
        let mut remaining = size;
        let mut total_cost = Decimal::ZERO;
        for level in levels {
            if remaining <= Decimal::ZERO {
                break;
            }
            let fill = remaining.min(level.size);
            total_cost += fill * level.price;
            remaining -= fill;
        }

        let filled = size - remaining;
        let effective_price = if filled > Decimal::ZERO {
            total_cost / filled
        } else if !levels.is_empty() {
            levels[0].price // fallback to best level
        } else {
            Decimal::ZERO
        };

        // Depth-based impact: how far the effective price moved from best
        let best_price = levels.first().map(|l| l.price).unwrap_or(Decimal::ZERO);
        let depth_impact_bps = if best_price > Decimal::ZERO {
            (effective_price - best_price).abs() / best_price
                * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        // Combined impact: max of kyle and depth-based (conservative)
        let impact_bps = kyle_impact_bps.max(depth_impact_bps);

        ImpactEstimate {
            impact_bps,
            effective_price,
            depth_consumed_pct,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::OrderbookLevel;
    use chrono::Utc;
    use rust_decimal_macros::dec;

    fn make_book(bids: Vec<OrderbookLevel>, asks: Vec<OrderbookLevel>) -> OrderbookSnapshot {
        OrderbookSnapshot {
            token_id: "tok_test".to_string(),
            bids,
            asks,
            timestamp: Utc::now(),
        }
    }

    #[test]
    fn test_empty_book() {
        let estimator = MarketImpactEstimator::default_config();
        let book = make_book(vec![], vec![]);

        let est = estimator.estimate(&book, Side::Buy, dec!(100), None);
        assert_eq!(est.effective_price, Decimal::ZERO);
        assert_eq!(est.impact_bps, Decimal::ZERO);
        assert_eq!(est.depth_consumed_pct, 1.0);
    }

    #[test]
    fn test_single_level_full_fill() {
        let estimator = MarketImpactEstimator::default_config();
        let book = make_book(
            vec![OrderbookLevel { price: dec!(0.58), size: dec!(500) }],
            vec![OrderbookLevel { price: dec!(0.62), size: dec!(500) }],
        );

        // Buy 200 from a single ask level at 0.62 — no price impact
        let est = estimator.estimate(&book, Side::Buy, dec!(200), None);
        assert_eq!(est.effective_price, dec!(0.62));
        assert_eq!(est.impact_bps, Decimal::ZERO); // no movement from best
        assert!(est.depth_consumed_pct > 0.39 && est.depth_consumed_pct < 0.41);
    }

    #[test]
    fn test_multi_level_walk() {
        let estimator = MarketImpactEstimator::default_config();
        let book = make_book(
            vec![],
            vec![
                OrderbookLevel { price: dec!(0.62), size: dec!(100) },
                OrderbookLevel { price: dec!(0.64), size: dec!(100) },
                OrderbookLevel { price: dec!(0.68), size: dec!(100) },
            ],
        );

        // Buy 200: fill 100@0.62 + 100@0.64 => VWAP = (62+64)/200 = 0.63
        let est = estimator.estimate(&book, Side::Buy, dec!(200), None);

        // effective_price = (100*0.62 + 100*0.64) / 200 = 126/200 = 0.63
        assert_eq!(est.effective_price, dec!(0.63));

        // depth impact = |0.63 - 0.62| / 0.62 * 10000 ~ 161.29 bps
        let expected_impact = (dec!(0.63) - dec!(0.62)) / dec!(0.62) * dec!(10000);
        assert_eq!(est.impact_bps, expected_impact);

        // depth consumed = 200 / 300 ~ 0.667
        assert!(est.depth_consumed_pct > 0.66 && est.depth_consumed_pct < 0.67);
    }

    #[test]
    fn test_kyle_impact_dominates() {
        let estimator = MarketImpactEstimator::new(1.0);
        let book = make_book(
            vec![],
            vec![
                // Very deep book — depth impact negligible
                OrderbookLevel { price: dec!(0.50), size: dec!(1_000_000) },
            ],
        );

        // Small order relative to book, but large relative to daily volume
        // kyle_impact_bps = 1.0 * (1000 / 2000) * 10000 = 5000 bps
        let est = estimator.estimate(
            &book,
            Side::Buy,
            dec!(1000),
            Some(dec!(2000)),
        );

        // Depth impact is ~0 bps (single deep level), so Kyle should dominate
        assert!(est.impact_bps >= dec!(4999));
        assert_eq!(est.effective_price, dec!(0.50));
    }

    #[test]
    fn test_sell_side_uses_bids() {
        let estimator = MarketImpactEstimator::default_config();
        let book = make_book(
            vec![
                OrderbookLevel { price: dec!(0.58), size: dec!(100) },
                OrderbookLevel { price: dec!(0.56), size: dec!(100) },
            ],
            vec![
                OrderbookLevel { price: dec!(0.62), size: dec!(500) },
            ],
        );

        // Sell 150: fill 100@0.58 + 50@0.56
        let est = estimator.estimate(&book, Side::Sell, dec!(150), None);

        // effective = (100*0.58 + 50*0.56) / 150 = (58 + 28) / 150 = 86/150
        let expected_price = (dec!(100) * dec!(0.58) + dec!(50) * dec!(0.56)) / dec!(150);
        assert_eq!(est.effective_price, expected_price);

        // Impact from best bid (0.58)
        let expected_impact = (dec!(0.58) - expected_price).abs() / dec!(0.58) * dec!(10000);
        assert_eq!(est.impact_bps, expected_impact);
    }
}
