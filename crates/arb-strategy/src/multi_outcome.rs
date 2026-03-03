use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, TradeLeg,
    config::{MultiOutcomeConfig, StrategyConfig},
    error::Result,
    traits::{ArbDetector, SlippageEstimator},
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::debug;
use uuid::Uuid;

/// Detects multi-outcome arbitrage in neg_risk markets.
///
/// Events with 3+ outcomes must have probabilities summing to 100%.
/// If the sum deviates (e.g., all outcome asks sum to 0.95), buy all
/// outcomes. If all bids sum to 1.05, sell all outcomes.
pub struct MultiOutcomeDetector {
    config: MultiOutcomeConfig,
    strategy_config: StrategyConfig,
    slippage_estimator: Arc<dyn SlippageEstimator>,
}

impl MultiOutcomeDetector {
    pub fn new(
        config: MultiOutcomeConfig,
        strategy_config: StrategyConfig,
        slippage_estimator: Arc<dyn SlippageEstimator>,
    ) -> Self {
        Self {
            config,
            strategy_config,
            slippage_estimator,
        }
    }

    /// Group markets by shared event (neg_risk markets sharing the same question prefix).
    ///
    /// In Polymarket, multi-outcome events have multiple markets with `neg_risk = true`.
    /// Each market represents one outcome. We group them by shared condition characteristics.
    fn group_by_event<'a>(&self, markets: &'a [Arc<MarketState>]) -> Vec<Vec<&'a MarketState>> {
        // For now, we check each individual neg_risk market that has >2 outcomes
        // In reality, multi-outcome events span multiple markets that share an event_id.
        // Since we don't have event_id in MarketState yet, we treat each neg_risk market
        // with multiple outcomes as its own group.
        let mut groups: Vec<Vec<&'a MarketState>> = Vec::new();

        for market in markets {
            if market.neg_risk && market.outcomes.len() > 2 && market.active {
                groups.push(vec![market.as_ref()]);
            }
        }

        groups
    }

    fn check_event_group(&self, outcomes: &[&MarketState]) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();

        for market in outcomes {
            if market.outcome_prices.len() < 2 || market.orderbooks.len() < 2 {
                continue;
            }

            // Ensure token_ids and orderbooks are aligned (same length).
            // If a partial fetch returned fewer orderbooks than tokens, the
            // index-based access below would pair the wrong token with the
            // wrong orderbook, producing incorrect VWAP estimates.
            if market.token_ids.len() != market.orderbooks.len() {
                debug!(
                    market = %market.condition_id,
                    tokens = market.token_ids.len(),
                    books = market.orderbooks.len(),
                    "Skipping: token_ids/orderbooks length mismatch"
                );
                continue;
            }

            let n = market.outcome_prices.len();

            // ─── Sum of all ask prices (buying all outcomes) ───
            let mut ask_prices = Vec::with_capacity(n);
            let mut has_all_asks = true;
            for book in &market.orderbooks {
                if book.asks.is_empty() {
                    has_all_asks = false;
                    break;
                }
                ask_prices.push(book.asks[0].price);
            }

            if has_all_asks && ask_prices.len() == n {
                let ask_sum: Decimal = ask_prices.iter().sum();

                if ask_sum < dec!(1.00) - self.config.min_deviation {
                    let gross_edge = dec!(1.00) - ask_sum;

                    // Find the minimum available size across all outcomes
                    let min_ask_size = market
                        .orderbooks
                        .iter()
                        .map(|b| b.asks.iter().map(|l| l.size).sum::<Decimal>())
                        .min()
                        .unwrap_or(Decimal::ZERO);

                    let target_size = min_ask_size.min(Decimal::from(500));

                    if target_size > Decimal::ZERO {
                        // Try VWAP for each leg
                        let mut legs = Vec::with_capacity(n);
                        let mut vwaps = Vec::with_capacity(n);
                        let mut all_ok = true;

                        for (i, book) in market.orderbooks.iter().enumerate() {
                            match self.slippage_estimator.estimate_vwap(book, Side::Buy, target_size) {
                                Ok(v) => {
                                    vwaps.push(v.vwap);
                                    legs.push(TradeLeg {
                                        token_id: market.token_ids[i].clone(),
                                        side: Side::Buy,
                                        target_price: ask_prices[i],
                                        target_size,
                                        vwap_estimate: v.vwap,
                                    });
                                }
                                Err(_) => {
                                    all_ok = false;
                                    break;
                                }
                            }
                        }

                        if all_ok {
                            let vwap_sum: Decimal = vwaps.iter().sum();
                            let net_edge = dec!(1.00) - vwap_sum;
                            let fee_estimate = target_size * vwap_sum * dec!(0.02);
                            let net_edge_after_fees = net_edge * target_size - fee_estimate;
                            let net_edge_per_unit = net_edge_after_fees / target_size;
                            let edge_bps = net_edge_per_unit * Decimal::from(10_000);

                            if edge_bps >= Decimal::from(self.strategy_config.min_edge_bps) {
                                debug!(
                                    market = %market.condition_id,
                                    n_outcomes = n,
                                    gross_edge = %gross_edge,
                                    net_edge = %net_edge_per_unit,
                                    edge_bps = %edge_bps,
                                    "Multi-outcome buy-all opportunity"
                                );

                                opps.push(Opportunity {
                                    id: Uuid::new_v4(),
                                    arb_type: ArbType::MultiOutcome,
                                    markets: vec![market.condition_id.clone()],
                                    legs,
                                    gross_edge,
                                    net_edge: net_edge_per_unit,
                                    estimated_vwap: vwaps,
                                    confidence: 1.0,
                                    size_available: target_size,
                                    detected_at: Utc::now(),
                                });
                            }
                        }
                    }
                }
            }

            // ─── Sum of all bid prices (selling all outcomes) ───
            let mut bid_prices = Vec::with_capacity(n);
            let mut has_all_bids = true;
            for book in &market.orderbooks {
                if book.bids.is_empty() {
                    has_all_bids = false;
                    break;
                }
                bid_prices.push(book.bids[0].price);
            }

            if has_all_bids && bid_prices.len() == n {
                let bid_sum: Decimal = bid_prices.iter().sum();

                if bid_sum > dec!(1.00) + self.config.min_deviation {
                    let gross_edge = bid_sum - dec!(1.00);
                    let min_bid_size = market
                        .orderbooks
                        .iter()
                        .map(|b| b.bids.iter().map(|l| l.size).sum::<Decimal>())
                        .min()
                        .unwrap_or(Decimal::ZERO);

                    let target_size = min_bid_size.min(Decimal::from(500));

                    if target_size > Decimal::ZERO {
                        let mut legs = Vec::with_capacity(n);
                        let mut vwaps = Vec::with_capacity(n);
                        let mut all_ok = true;

                        for (i, book) in market.orderbooks.iter().enumerate() {
                            match self.slippage_estimator.estimate_vwap(book, Side::Sell, target_size) {
                                Ok(v) => {
                                    vwaps.push(v.vwap);
                                    legs.push(TradeLeg {
                                        token_id: market.token_ids[i].clone(),
                                        side: Side::Sell,
                                        target_price: bid_prices[i],
                                        target_size,
                                        vwap_estimate: v.vwap,
                                    });
                                }
                                Err(_) => {
                                    all_ok = false;
                                    break;
                                }
                            }
                        }

                        if all_ok {
                            let vwap_sum: Decimal = vwaps.iter().sum();
                            let net_edge = vwap_sum - dec!(1.00);
                            let fee_estimate = target_size * vwap_sum * dec!(0.02);
                            let net_edge_after_fees = net_edge * target_size - fee_estimate;
                            let net_edge_per_unit = net_edge_after_fees / target_size;
                            let edge_bps = net_edge_per_unit * Decimal::from(10_000);

                            if edge_bps >= Decimal::from(self.strategy_config.min_edge_bps) {
                                debug!(
                                    market = %market.condition_id,
                                    n_outcomes = n,
                                    gross_edge = %gross_edge,
                                    net_edge = %net_edge_per_unit,
                                    "Multi-outcome sell-all opportunity"
                                );

                                opps.push(Opportunity {
                                    id: Uuid::new_v4(),
                                    arb_type: ArbType::MultiOutcome,
                                    markets: vec![market.condition_id.clone()],
                                    legs,
                                    gross_edge,
                                    net_edge: net_edge_per_unit,
                                    estimated_vwap: vwaps,
                                    confidence: 1.0,
                                    size_available: target_size,
                                    detected_at: Utc::now(),
                                });
                            }
                        }
                    }
                }
            }
        }

        Ok(opps)
    }
}

#[async_trait]
impl ArbDetector for MultiOutcomeDetector {
    fn arb_type(&self) -> ArbType {
        ArbType::MultiOutcome
    }

    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>> {
        let groups = self.group_by_event(markets);
        let mut all_opps = Vec::new();

        for group in &groups {
            match self.check_event_group(group) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(error = %e, "Error checking multi-outcome group");
                }
            }
        }

        Ok(all_opps)
    }
}
