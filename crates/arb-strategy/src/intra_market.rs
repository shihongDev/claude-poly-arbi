use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, TradeLeg,
    config::{IntraMarketConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::debug;
use uuid::Uuid;

/// Detects intra-market arbitrage in binary (YES/NO) markets.
///
/// A binary market's YES and NO tokens must resolve to $1.00 combined.
/// If you can buy both for less than $1.00, or sell both for more, that's riskless profit.
///
/// We use VWAP (not top-of-book) to ensure the edge is real at our fill size.
pub struct IntraMarketDetector {
    config: IntraMarketConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

impl IntraMarketDetector {
    pub fn new(
        config: IntraMarketConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
        }
    }

    /// Check a single binary market for intra-market arb.
    fn check_market(&self, market: &MarketState) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        // Must be a binary market with exactly 2 tokens and 2 orderbooks
        if market.token_ids.len() != 2 || market.orderbooks.len() != 2 {
            return Ok(opps);
        }

        // Skip neg_risk markets — those are multi-outcome, handled by MultiOutcomeDetector
        if market.neg_risk {
            return Ok(opps);
        }

        let yes_book = &market.orderbooks[0];
        let no_book = &market.orderbooks[1];

        // Skip if either book is empty
        if yes_book.asks.is_empty() || no_book.asks.is_empty() {
            return Ok(opps);
        }

        // ─── Buy-both opportunity ───
        // If best_ask(YES) + best_ask(NO) < 1.00, we can buy both for less than $1.00
        let yes_best_ask = yes_book.asks[0].price;
        let no_best_ask = no_book.asks[0].price;
        let ask_sum = yes_best_ask + no_best_ask;

        if ask_sum < dec!(1.00) - self.config.min_deviation {
            let gross_edge = dec!(1.00) - ask_sum;

            // Find the maximum size we can fill on both sides
            let yes_max = yes_book.asks.iter().map(|l| l.size).sum::<Decimal>();
            let no_max = no_book.asks.iter().map(|l| l.size).sum::<Decimal>();
            let max_size = yes_max.min(no_max);

            if max_size > Decimal::ZERO {
                // Try VWAP at a reasonable size (smaller of max available or $1000)
                let target_size = max_size.min(Decimal::from(1000));

                let yes_vwap = self.slippage_estimator.estimate_vwap(yes_book, Side::Buy, target_size);
                let no_vwap = self.slippage_estimator.estimate_vwap(no_book, Side::Buy, target_size);

                if let (Ok(yv), Ok(nv)) = (yes_vwap, no_vwap) {
                    let vwap_sum = yv.vwap + nv.vwap;
                    let net_edge = dec!(1.00) - vwap_sum;

                    // Fee estimate: 2% on notional
                    let fee_estimate = target_size * vwap_sum * dec!(0.02);
                    let net_edge_after_fees = net_edge * target_size - fee_estimate;

                    let net_edge_per_unit = if target_size > Decimal::ZERO {
                        net_edge_after_fees / target_size
                    } else {
                        Decimal::ZERO
                    };

                    let edge_bps = net_edge_per_unit * Decimal::from(10_000);

                    if edge_bps >= Decimal::from(self.strategy_config.min_edge_bps) {
                        debug!(
                            market = %market.condition_id,
                            gross_edge = %gross_edge,
                            net_edge = %net_edge_per_unit,
                            edge_bps = %edge_bps,
                            size = %target_size,
                            "Intra-market buy-both opportunity detected"
                        );

                        opps.push(Opportunity {
                            id: Uuid::new_v4(),
                            arb_type: ArbType::IntraMarket,
                            markets: vec![market.condition_id.clone()],
                            legs: vec![
                                TradeLeg {
                                    token_id: market.token_ids[0].clone(),
                                    side: Side::Buy,
                                    target_price: yes_best_ask,
                                    target_size,
                                    vwap_estimate: yv.vwap,
                                },
                                TradeLeg {
                                    token_id: market.token_ids[1].clone(),
                                    side: Side::Buy,
                                    target_price: no_best_ask,
                                    target_size,
                                    vwap_estimate: nv.vwap,
                                },
                            ],
                            gross_edge,
                            net_edge: net_edge_per_unit,
                            estimated_vwap: vec![yv.vwap, nv.vwap],
                            confidence: 1.0, // structural arb = certainty
                            size_available: target_size,
                            detected_at: Utc::now(),
                        });
                    }
                }
            }
        }

        // ─── Sell-both opportunity ───
        // If best_bid(YES) + best_bid(NO) > 1.00, we can sell both for more than $1.00
        if !yes_book.bids.is_empty() && !no_book.bids.is_empty() {
            let yes_best_bid = yes_book.bids[0].price;
            let no_best_bid = no_book.bids[0].price;
            let bid_sum = yes_best_bid + no_best_bid;

            if bid_sum > dec!(1.00) + self.config.min_deviation {
                let gross_edge = bid_sum - dec!(1.00);

                let yes_max = yes_book.bids.iter().map(|l| l.size).sum::<Decimal>();
                let no_max = no_book.bids.iter().map(|l| l.size).sum::<Decimal>();
                let max_size = yes_max.min(no_max);

                if max_size > Decimal::ZERO {
                    let target_size = max_size.min(Decimal::from(1000));

                    let yes_vwap = self.slippage_estimator.estimate_vwap(yes_book, Side::Sell, target_size);
                    let no_vwap = self.slippage_estimator.estimate_vwap(no_book, Side::Sell, target_size);

                    if let (Ok(yv), Ok(nv)) = (yes_vwap, no_vwap) {
                        let vwap_sum = yv.vwap + nv.vwap;
                        let net_edge = vwap_sum - dec!(1.00);

                        let fee_estimate = target_size * vwap_sum * dec!(0.02);
                        let net_edge_after_fees = net_edge * target_size - fee_estimate;
                        let net_edge_per_unit = if target_size > Decimal::ZERO {
                            net_edge_after_fees / target_size
                        } else {
                            Decimal::ZERO
                        };

                        let edge_bps = net_edge_per_unit * Decimal::from(10_000);

                        if edge_bps >= Decimal::from(self.strategy_config.min_edge_bps) {
                            debug!(
                                market = %market.condition_id,
                                gross_edge = %gross_edge,
                                net_edge = %net_edge_per_unit,
                                edge_bps = %edge_bps,
                                "Intra-market sell-both opportunity detected"
                            );

                            opps.push(Opportunity {
                                id: Uuid::new_v4(),
                                arb_type: ArbType::IntraMarket,
                                markets: vec![market.condition_id.clone()],
                                legs: vec![
                                    TradeLeg {
                                        token_id: market.token_ids[0].clone(),
                                        side: Side::Sell,
                                        target_price: yes_best_bid,
                                        target_size,
                                        vwap_estimate: yv.vwap,
                                    },
                                    TradeLeg {
                                        token_id: market.token_ids[1].clone(),
                                        side: Side::Sell,
                                        target_price: no_best_bid,
                                        target_size,
                                        vwap_estimate: nv.vwap,
                                    },
                                ],
                                gross_edge,
                                net_edge: net_edge_per_unit,
                                estimated_vwap: vec![yv.vwap, nv.vwap],
                                confidence: 1.0,
                                size_available: target_size,
                                detected_at: Utc::now(),
                            });
                        }
                    }
                }
            }
        }

        Ok(opps)
    }
}

#[async_trait]
impl ArbDetector for IntraMarketDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::IntraMarket
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let mut all_opps = Vec::new();
        for market in markets {
            match self.check_market(market) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(market = %market.condition_id, error = %e, "Error checking market");
                }
            }
        }
        Ok(all_opps)
    }
}
