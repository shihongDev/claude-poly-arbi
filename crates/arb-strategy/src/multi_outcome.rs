use std::collections::HashMap;
use std::sync::Arc;

use arb_core::{
    ArbType, MarketState, Opportunity, Side, StrategyType, TradeLeg,
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

    /// Group markets by shared event for multi-outcome arbitrage detection.
    ///
    /// Two grouping strategies are used:
    ///
    /// 1. **Event-based** (primary): active neg_risk markets with a known `event_id`
    ///    are grouped by that ID. Each group represents a multi-outcome event where
    ///    the YES prices across all markets should sum to 1.0.
    ///
    /// 2. **Single-market fallback**: active neg_risk markets without an `event_id`
    ///    but with >2 outcomes are treated as self-contained multi-outcome markets
    ///    (the original behaviour).
    fn group_by_event<'a>(&self, markets: &'a [Arc<MarketState>]) -> Vec<Vec<&'a MarketState>> {
        let mut event_groups: HashMap<&str, Vec<&'a MarketState>> = HashMap::new();
        let mut standalone_groups: Vec<Vec<&'a MarketState>> = Vec::new();

        for market in markets {
            if !market.active || !market.neg_risk {
                continue;
            }

            if let Some(ref eid) = market.event_id {
                event_groups
                    .entry(eid.as_str())
                    .or_default()
                    .push(market.as_ref());
            } else if market.outcomes.len() > 2 {
                // Fallback: single market with multiple outcomes but no event_id
                standalone_groups.push(vec![market.as_ref()]);
            }
        }

        let mut groups: Vec<Vec<&'a MarketState>> = event_groups.into_values().collect();
        groups.extend(standalone_groups);
        groups
    }

    /// Check a group of markets sharing the same event_id for cross-market
    /// multi-outcome arbitrage.
    ///
    /// Each market in the group represents one binary outcome of a shared event.
    /// The first outcome price (YES) of each market should sum to 1.0.
    /// If the sum of best asks < 1.0, buying all YES tokens is profitable.
    /// If the sum of best bids > 1.0, selling all YES tokens is profitable.
    fn check_cross_market_group(&self, group: &[&MarketState]) -> Result<Vec<Opportunity>> {
        let mut opps = Vec::new();
        let n = group.len();

        // Need at least one orderbook per market to read best bid/ask
        let all_have_books = group.iter().all(|m| !m.orderbooks.is_empty());
        if !all_have_books {
            return Ok(opps);
        }

        let market_ids: Vec<String> = group.iter().map(|m| m.condition_id.clone()).collect();

        // ─── Buy-all: sum of first-outcome ask prices ───
        let ask_prices: Option<Vec<Decimal>> = group
            .iter()
            .map(|m| {
                m.orderbooks
                    .first()
                    .and_then(|b| b.asks.first())
                    .map(|l| l.price)
            })
            .collect();

        if let Some(ref asks) = ask_prices {
            let ask_sum: Decimal = asks.iter().sum();

            if ask_sum < dec!(1.00) - self.config.min_deviation {
                let gross_edge = dec!(1.00) - ask_sum;

                // Minimum available size across all outcomes
                let min_ask_size = group
                    .iter()
                    .filter_map(|m| m.orderbooks.first())
                    .map(|b| b.asks.iter().map(|l| l.size).sum::<Decimal>())
                    .min()
                    .unwrap_or(Decimal::ZERO);

                let target_size = min_ask_size.min(Decimal::from(500));

                if target_size > Decimal::ZERO {
                    let mut legs = Vec::with_capacity(n);
                    let mut vwaps = Vec::with_capacity(n);
                    let mut all_ok = true;

                    for (i, market) in group.iter().enumerate() {
                        if let Some(book) = market.orderbooks.first() {
                            match self.slippage_estimator.estimate_vwap(
                                book,
                                Side::Buy,
                                target_size,
                            ) {
                                Ok(v) => {
                                    vwaps.push(v.vwap);
                                    legs.push(TradeLeg {
                                        token_id: market
                                            .token_ids
                                            .first()
                                            .cloned()
                                            .unwrap_or_default(),
                                        side: Side::Buy,
                                        target_price: asks[i],
                                        target_size,
                                        vwap_estimate: v.vwap,
                                    });
                                }
                                Err(_) => {
                                    all_ok = false;
                                    break;
                                }
                            }
                        } else {
                            all_ok = false;
                            break;
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
                                n_markets = n,
                                gross_edge = %gross_edge,
                                net_edge = %net_edge_per_unit,
                                edge_bps = %edge_bps,
                                "Cross-market multi-outcome buy-all opportunity"
                            );

                            opps.push(Opportunity {
                                id: Uuid::new_v4(),
                                arb_type: ArbType::MultiOutcome,
                                strategy_type: StrategyType::MultiOutcomeArb,
                                markets: market_ids.clone(),
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

        // ─── Sell-all: sum of first-outcome bid prices ───
        let bid_prices: Option<Vec<Decimal>> = group
            .iter()
            .map(|m| {
                m.orderbooks
                    .first()
                    .and_then(|b| b.bids.first())
                    .map(|l| l.price)
            })
            .collect();

        if let Some(ref bids) = bid_prices {
            let bid_sum: Decimal = bids.iter().sum();

            if bid_sum > dec!(1.00) + self.config.min_deviation {
                let gross_edge = bid_sum - dec!(1.00);

                let min_bid_size = group
                    .iter()
                    .filter_map(|m| m.orderbooks.first())
                    .map(|b| b.bids.iter().map(|l| l.size).sum::<Decimal>())
                    .min()
                    .unwrap_or(Decimal::ZERO);

                let target_size = min_bid_size.min(Decimal::from(500));

                if target_size > Decimal::ZERO {
                    let mut legs = Vec::with_capacity(n);
                    let mut vwaps = Vec::with_capacity(n);
                    let mut all_ok = true;

                    for (i, market) in group.iter().enumerate() {
                        if let Some(book) = market.orderbooks.first() {
                            match self.slippage_estimator.estimate_vwap(
                                book,
                                Side::Sell,
                                target_size,
                            ) {
                                Ok(v) => {
                                    vwaps.push(v.vwap);
                                    legs.push(TradeLeg {
                                        token_id: market
                                            .token_ids
                                            .first()
                                            .cloned()
                                            .unwrap_or_default(),
                                        side: Side::Sell,
                                        target_price: bids[i],
                                        target_size,
                                        vwap_estimate: v.vwap,
                                    });
                                }
                                Err(_) => {
                                    all_ok = false;
                                    break;
                                }
                            }
                        } else {
                            all_ok = false;
                            break;
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
                                n_markets = n,
                                gross_edge = %gross_edge,
                                net_edge = %net_edge_per_unit,
                                "Cross-market multi-outcome sell-all opportunity"
                            );

                            opps.push(Opportunity {
                                id: Uuid::new_v4(),
                                arb_type: ArbType::MultiOutcome,
                                strategy_type: StrategyType::MultiOutcomeArb,
                                markets: market_ids,
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

        Ok(opps)
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
                            match self.slippage_estimator.estimate_vwap(
                                book,
                                Side::Buy,
                                target_size,
                            ) {
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
                                    strategy_type: StrategyType::MultiOutcomeArb,
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
                            match self.slippage_estimator.estimate_vwap(
                                book,
                                Side::Sell,
                                target_size,
                            ) {
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
                                    strategy_type: StrategyType::MultiOutcomeArb,
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
            if group.len() >= 2 {
                // Cross-market event group: multiple markets sharing an event_id,
                // each representing one outcome of a multi-outcome event.
                match self.check_cross_market_group(group) {
                    Ok(opps) => all_opps.extend(opps),
                    Err(e) => {
                        debug!(error = %e, "Error checking cross-market multi-outcome group");
                    }
                }
            }

            // Always run the single-market check too.
            // This handles markets with >2 internal outcomes (e.g., a single market
            // with 3+ tokens) regardless of whether they were grouped by event_id.
            match self.check_event_group(group) {
                Ok(opps) => all_opps.extend(opps),
                Err(e) => {
                    debug!(error = %e, "Error checking single-market multi-outcome group");
                }
            }
        }

        Ok(all_opps)
    }
}
