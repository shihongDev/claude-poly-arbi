use arb_core::{ArbType, MarketState, Opportunity, Side, TradeLeg};
use chrono::Utc;
use rust_decimal::Decimal;
use uuid::Uuid;

/// Detects pricing inversions in cumulative "by deadline" event series.
///
/// Polymarket hosts event series like "Starmer out by March?", "Starmer out by June?",
/// "Starmer out by Dec?" — these are cumulative, so later deadlines must always be
/// priced >= earlier ones. An inversion (later deadline priced cheaper than an
/// earlier one) is a potential arbitrage signal.
///
/// Markets must be pre-sorted by deadline (earliest first) before calling
/// [`check_event_group`].
pub struct DeadlineMonotonicityDetector;

impl DeadlineMonotonicityDetector {
    pub fn new() -> Self {
        Self
    }

    /// Scan a group of markets from the same event for deadline inversions.
    ///
    /// Markets should be pre-sorted by deadline (earliest first).
    /// Returns opportunities for each consecutive pair where the later deadline's
    /// YES price is strictly less than the earlier deadline's YES price.
    pub fn check_event_group(&self, markets: &[MarketState]) -> Vec<Opportunity> {
        if markets.len() < 2 {
            return Vec::new();
        }

        let mut opps = Vec::new();

        for pair in markets.windows(2) {
            let earlier = &pair[0];
            let later = &pair[1];

            let earlier_yes = match earlier.outcome_prices.first().copied() {
                Some(p) => p,
                None => continue,
            };
            let later_yes = match later.outcome_prices.first().copied() {
                Some(p) => p,
                None => continue,
            };

            // Skip if earlier price is zero — no meaningful inversion possible
            if earlier_yes == Decimal::ZERO {
                continue;
            }

            // Inversion: later deadline priced cheaper than earlier
            if later_yes < earlier_yes {
                let gross_edge = earlier_yes - later_yes;
                // Use a conservative default size — EdgeCalculator will refine
                // with actual orderbook depth when available
                let default_size = Decimal::from(100);

                opps.push(Opportunity {
                    id: Uuid::new_v4(),
                    arb_type: ArbType::CrossMarket,
                    markets: vec![
                        earlier.condition_id.clone(),
                        later.condition_id.clone(),
                    ],
                    legs: vec![
                        TradeLeg {
                            token_id: later.token_ids.first().cloned().unwrap_or_default(),
                            side: Side::Buy,
                            target_price: later_yes,
                            target_size: default_size,
                            vwap_estimate: later_yes,
                        },
                        TradeLeg {
                            token_id: earlier.token_ids.first().cloned().unwrap_or_default(),
                            side: Side::Sell,
                            target_price: earlier_yes,
                            target_size: default_size,
                            vwap_estimate: earlier_yes,
                        },
                    ],
                    gross_edge,
                    net_edge: Decimal::ZERO, // refined later by EdgeCalculator
                    estimated_vwap: vec![later_yes, earlier_yes],
                    confidence: 0.5,
                    size_available: default_size,
                    detected_at: Utc::now(),
                });
            }
        }

        opps
    }
}

impl Default for DeadlineMonotonicityDetector {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::MarketState;
    use rust_decimal_macros::dec;

    /// Helper to build a minimal MarketState with given YES price.
    fn make_market(condition_id: &str, yes_price: Decimal) -> MarketState {
        MarketState {
            condition_id: condition_id.to_string(),
            question: format!("Event by {}?", condition_id),
            outcomes: vec!["Yes".to_string(), "No".to_string()],
            token_ids: vec![
                format!("{}_yes", condition_id),
                format!("{}_no", condition_id),
            ],
            outcome_prices: vec![yes_price, Decimal::ONE - yes_price],
            orderbooks: vec![],
            volume_24hr: None,
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
            last_updated_gen: 0,
        }
    }

    #[test]
    fn test_no_inversion() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("march", dec!(0.20)),
            make_market("june", dec!(0.40)),
            make_market("december", dec!(0.65)),
        ];

        let opps = detector.check_event_group(&markets);
        assert!(opps.is_empty(), "Monotonically increasing prices should produce no opportunities");
    }

    #[test]
    fn test_inversion_detected() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("march", dec!(0.30)),
            make_market("june", dec!(0.25)), // inversion: cheaper than March
            make_market("december", dec!(0.65)),
        ];

        let opps = detector.check_event_group(&markets);
        assert_eq!(opps.len(), 1, "Should detect exactly 1 inversion (March > June)");

        let opp = &opps[0];
        assert_eq!(opp.arb_type, ArbType::CrossMarket);
        assert_eq!(opp.gross_edge, dec!(0.05));
        assert_eq!(opp.markets, vec!["march", "june"]);

        // First leg: buy the underpriced later deadline
        assert_eq!(opp.legs[0].side, Side::Buy);
        assert_eq!(opp.legs[0].token_id, "june_yes");
        assert_eq!(opp.legs[0].target_price, dec!(0.25));

        // Second leg: sell the overpriced earlier deadline
        assert_eq!(opp.legs[1].side, Side::Sell);
        assert_eq!(opp.legs[1].token_id, "march_yes");
        assert_eq!(opp.legs[1].target_price, dec!(0.30));
    }

    #[test]
    fn test_multiple_inversions() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("march", dec!(0.40)),
            make_market("june", dec!(0.35)),      // inversion #1: cheaper than March
            make_market("september", dec!(0.50)),
            make_market("december", dec!(0.45)),   // inversion #2: cheaper than September
        ];

        let opps = detector.check_event_group(&markets);
        assert_eq!(opps.len(), 2, "Should detect 2 inversions");

        // First inversion: March(0.40) > June(0.35)
        assert_eq!(opps[0].markets, vec!["march", "june"]);
        assert_eq!(opps[0].gross_edge, dec!(0.05));

        // Second inversion: September(0.50) > December(0.45)
        assert_eq!(opps[1].markets, vec!["september", "december"]);
        assert_eq!(opps[1].gross_edge, dec!(0.05));
    }

    #[test]
    fn test_single_market() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![make_market("march", dec!(0.30))];

        let opps = detector.check_event_group(&markets);
        assert!(opps.is_empty(), "Single market should produce no opportunities");
    }

    #[test]
    fn test_zero_price_skipped() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("march", dec!(0.00)),     // zero price
            make_market("june", dec!(0.25)),
        ];

        let opps = detector.check_event_group(&markets);
        assert!(opps.is_empty(), "Zero earlier price should be skipped (no inversion reported)");
    }
}
