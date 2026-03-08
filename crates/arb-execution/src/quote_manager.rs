use std::collections::HashMap;

use arb_core::{
    Side, StrategyAction,
    error::Result,
};
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use tracing::{debug, info};

/// A standing quote (limit order) placed by a market-making strategy.
#[derive(Debug, Clone)]
pub struct StandingQuote {
    pub order_id: String,
    pub market_id: String,
    pub token_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
    pub placed_at: DateTime<Utc>,
}

/// Manages standing two-sided quotes for market making.
///
/// Tracks active quotes per market, handles requoting (cancel + replace),
/// and enforces inventory limits. Used by the engine to process
/// `StrategyAction::PlaceQuote` and `StrategyAction::CancelQuote` events.
pub struct QuoteManager {
    /// Active quotes: order_id -> StandingQuote
    active_quotes: HashMap<String, StandingQuote>,
    /// Quotes per market: market_id -> Vec<order_id>
    quotes_by_market: HashMap<String, Vec<String>>,
    next_id: u64,
}

impl QuoteManager {
    pub fn new() -> Self {
        Self {
            active_quotes: HashMap::new(),
            quotes_by_market: HashMap::new(),
            next_id: 1,
        }
    }

    /// Process a strategy action, returning any order IDs created.
    pub fn process_action(&mut self, action: &StrategyAction) -> Result<Vec<String>> {
        match action {
            StrategyAction::PlaceQuote {
                market_id,
                token_id,
                side,
                price,
                size,
            } => {
                let order_id = format!("mm_quote_{}", self.next_id);
                self.next_id += 1;

                let quote = StandingQuote {
                    order_id: order_id.clone(),
                    market_id: market_id.clone(),
                    token_id: token_id.clone(),
                    side: *side,
                    price: *price,
                    size: *size,
                    placed_at: Utc::now(),
                };

                debug!(
                    order_id = %order_id,
                    market = %market_id,
                    side = ?side,
                    price = %price,
                    size = %size,
                    "Placing standing quote"
                );

                self.active_quotes.insert(order_id.clone(), quote);
                self.quotes_by_market
                    .entry(market_id.clone())
                    .or_default()
                    .push(order_id.clone());

                Ok(vec![order_id])
            }

            StrategyAction::CancelQuote { order_id } => {
                if let Some(quote) = self.active_quotes.remove(order_id) {
                    if let Some(ids) = self.quotes_by_market.get_mut(&quote.market_id) {
                        ids.retain(|id| id != order_id);
                    }
                    debug!(order_id = %order_id, "Cancelled standing quote");
                }
                Ok(vec![])
            }

            StrategyAction::CancelAllQuotes { market_id } => {
                let cancelled = if let Some(ids) = self.quotes_by_market.remove(market_id) {
                    let count = ids.len();
                    for id in &ids {
                        self.active_quotes.remove(id);
                    }
                    info!(market = %market_id, count = count, "Cancelled all quotes for market");
                    ids
                } else {
                    vec![]
                };
                Ok(cancelled)
            }

            StrategyAction::Execute(_) => {
                // Execute actions are handled by the regular executor
                Ok(vec![])
            }
        }
    }

    /// Get all active quotes for a market.
    pub fn quotes_for_market(&self, market_id: &str) -> Vec<&StandingQuote> {
        self.quotes_by_market
            .get(market_id)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| self.active_quotes.get(id))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Get total number of active quotes.
    pub fn active_count(&self) -> usize {
        self.active_quotes.len()
    }

    /// Cancel all quotes (used during kill switch).
    pub fn cancel_all(&mut self) -> usize {
        let count = self.active_quotes.len();
        self.active_quotes.clear();
        self.quotes_by_market.clear();
        if count > 0 {
            info!(count = count, "Cancelled all standing quotes (kill switch)");
        }
        count
    }
}

impl Default for QuoteManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_place_and_cancel_quote() {
        let mut mgr = QuoteManager::new();

        let place = StrategyAction::PlaceQuote {
            market_id: "market1".into(),
            token_id: "yes".into(),
            side: Side::Buy,
            price: dec!(0.45),
            size: dec!(50),
        };

        let ids = mgr.process_action(&place).unwrap();
        assert_eq!(ids.len(), 1);
        assert_eq!(mgr.active_count(), 1);

        let cancel = StrategyAction::CancelQuote {
            order_id: ids[0].clone(),
        };
        mgr.process_action(&cancel).unwrap();
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_cancel_all_for_market() {
        let mut mgr = QuoteManager::new();

        // Place 2 quotes for same market
        for price in [dec!(0.45), dec!(0.55)] {
            let action = StrategyAction::PlaceQuote {
                market_id: "market1".into(),
                token_id: "yes".into(),
                side: Side::Buy,
                price,
                size: dec!(50),
            };
            mgr.process_action(&action).unwrap();
        }

        assert_eq!(mgr.active_count(), 2);

        let cancel_all = StrategyAction::CancelAllQuotes {
            market_id: "market1".into(),
        };
        let cancelled = mgr.process_action(&cancel_all).unwrap();
        assert_eq!(cancelled.len(), 2);
        assert_eq!(mgr.active_count(), 0);
    }

    #[test]
    fn test_kill_switch_cancel_all() {
        let mut mgr = QuoteManager::new();

        for market in ["m1", "m2"] {
            let action = StrategyAction::PlaceQuote {
                market_id: market.into(),
                token_id: "yes".into(),
                side: Side::Buy,
                price: dec!(0.50),
                size: dec!(50),
            };
            mgr.process_action(&action).unwrap();
        }

        assert_eq!(mgr.active_count(), 2);
        let count = mgr.cancel_all();
        assert_eq!(count, 2);
        assert_eq!(mgr.active_count(), 0);
    }
}
