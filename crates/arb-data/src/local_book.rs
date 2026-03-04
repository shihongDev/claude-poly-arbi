use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arb_core::Side;
use dashmap::DashMap;
use rust_decimal::Decimal;
use tokio::sync::RwLock;

/// In-memory order book for a single token, fed by WebSocket updates.
///
/// Bids are stored with `Reverse<Decimal>` keys so that iteration yields
/// highest-price-first order. Asks use plain `Decimal` keys for natural
/// lowest-price-first ordering. A size of zero signals level removal.
#[derive(Debug)]
pub struct LocalOrderBook {
    pub token_id: String,
    bids: BTreeMap<Reverse<Decimal>, Decimal>,
    asks: BTreeMap<Decimal, Decimal>,
    last_updated: Instant,
}

impl LocalOrderBook {
    pub fn new(token_id: String) -> Self {
        Self {
            token_id,
            bids: BTreeMap::new(),
            asks: BTreeMap::new(),
            last_updated: Instant::now(),
        }
    }

    /// Insert or update a bid level. Removes the level if `size` is zero.
    pub fn update_bid(&mut self, price: Decimal, size: Decimal) {
        if size.is_zero() {
            self.bids.remove(&Reverse(price));
        } else {
            self.bids.insert(Reverse(price), size);
        }
        self.last_updated = Instant::now();
    }

    /// Insert or update an ask level. Removes the level if `size` is zero.
    pub fn update_ask(&mut self, price: Decimal, size: Decimal) {
        if size.is_zero() {
            self.asks.remove(&price);
        } else {
            self.asks.insert(price, size);
        }
        self.last_updated = Instant::now();
    }

    /// Returns the best (highest) bid as `(price, size)`, or `None` if empty.
    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.first_key_value().map(|(Reverse(p), s)| (*p, *s))
    }

    /// Returns the best (lowest) ask as `(price, size)`, or `None` if empty.
    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.first_key_value().map(|(p, s)| (*p, *s))
    }

    /// Returns the bid-ask spread, or `None` if either side is empty.
    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask - bid),
            _ => None,
        }
    }

    /// Returns `true` if the book has not been updated within `max_age`.
    pub fn is_stale(&self, max_age: Duration) -> bool {
        self.last_updated.elapsed() > max_age
    }

    /// Compute VWAP by walking order book levels for the given side and target size.
    ///
    /// For `Side::Buy`, walks ask levels (ascending price).
    /// For `Side::Sell`, walks bid levels (descending price).
    ///
    /// Returns `None` if the book has insufficient liquidity to fill `target_size`.
    pub fn calculate_vwap(&self, side: Side, target_size: Decimal) -> Option<Decimal> {
        if target_size <= Decimal::ZERO {
            return None;
        }

        let mut remaining = target_size;
        let mut total_cost = Decimal::ZERO;

        match side {
            Side::Buy => {
                for (price, size) in &self.asks {
                    let fill = remaining.min(*size);
                    total_cost += fill * *price;
                    remaining -= fill;
                    if remaining.is_zero() {
                        break;
                    }
                }
            }
            Side::Sell => {
                for (Reverse(price), size) in &self.bids {
                    let fill = remaining.min(*size);
                    total_cost += fill * *price;
                    remaining -= fill;
                    if remaining.is_zero() {
                        break;
                    }
                }
            }
        }

        if remaining > Decimal::ZERO {
            return None;
        }

        Some(total_cost / target_size)
    }

    /// Replace all levels with the given snapshot data.
    pub fn apply_snapshot(
        &mut self,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
    ) {
        self.bids.clear();
        for (price, size) in bids {
            if !size.is_zero() {
                self.bids.insert(Reverse(price), size);
            }
        }
        self.asks.clear();
        for (price, size) in asks {
            if !size.is_zero() {
                self.asks.insert(price, size);
            }
        }
        self.last_updated = Instant::now();
    }
}

/// Thread-safe store of `LocalOrderBook` instances keyed by token ID.
///
/// Uses `DashMap` for concurrent access and `tokio::sync::RwLock` per book
/// so that readers and writers can coexist with minimal contention.
#[derive(Debug, Default)]
pub struct OrderBookStore {
    books: DashMap<String, Arc<RwLock<LocalOrderBook>>>,
}

impl OrderBookStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the book for the given token, creating a new empty one if absent.
    pub fn get_or_create(&self, token_id: &str) -> Arc<RwLock<LocalOrderBook>> {
        self.books
            .entry(token_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(LocalOrderBook::new(token_id.to_string()))))
            .value()
            .clone()
    }

    /// Returns the book for the given token, or `None` if it does not exist.
    pub fn get(&self, token_id: &str) -> Option<Arc<RwLock<LocalOrderBook>>> {
        self.books.get(token_id).map(|entry| entry.value().clone())
    }

    /// Returns the number of tracked tokens.
    pub fn token_count(&self) -> usize {
        self.books.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_empty_book_best_prices() {
        let book = LocalOrderBook::new("token_empty".into());
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
        assert!(book.spread().is_none());
    }

    #[test]
    fn test_add_bid_ask_levels() {
        let mut book = LocalOrderBook::new("token1".into());

        // Add bids at different prices
        book.update_bid(dec!(0.50), dec!(100));
        book.update_bid(dec!(0.55), dec!(200));
        book.update_bid(dec!(0.52), dec!(150));

        // Add asks at different prices
        book.update_ask(dec!(0.60), dec!(100));
        book.update_ask(dec!(0.58), dec!(200));
        book.update_ask(dec!(0.62), dec!(150));

        // Best bid should be highest price
        let (bid_price, bid_size) = book.best_bid().unwrap();
        assert_eq!(bid_price, dec!(0.55));
        assert_eq!(bid_size, dec!(200));

        // Best ask should be lowest price
        let (ask_price, ask_size) = book.best_ask().unwrap();
        assert_eq!(ask_price, dec!(0.58));
        assert_eq!(ask_size, dec!(200));
    }

    #[test]
    fn test_remove_level_on_zero_size() {
        let mut book = LocalOrderBook::new("token2".into());

        // Add then remove a bid level
        book.update_bid(dec!(0.50), dec!(100));
        book.update_bid(dec!(0.55), dec!(200));
        assert_eq!(book.best_bid().unwrap().0, dec!(0.55));

        // Remove best bid by setting size to zero
        book.update_bid(dec!(0.55), dec!(0));
        assert_eq!(book.best_bid().unwrap().0, dec!(0.50));

        // Same for asks
        book.update_ask(dec!(0.60), dec!(100));
        book.update_ask(dec!(0.58), dec!(200));
        assert_eq!(book.best_ask().unwrap().0, dec!(0.58));

        book.update_ask(dec!(0.58), dec!(0));
        assert_eq!(book.best_ask().unwrap().0, dec!(0.60));
    }

    #[test]
    fn test_vwap_buy() {
        let mut book = LocalOrderBook::new("token_vwap".into());

        // Ask levels: 100 @ 0.50, 200 @ 0.52, 100 @ 0.55
        book.update_ask(dec!(0.50), dec!(100));
        book.update_ask(dec!(0.52), dec!(200));
        book.update_ask(dec!(0.55), dec!(100));

        // Buy 150: 100 @ 0.50 + 50 @ 0.52
        // VWAP = (100*0.50 + 50*0.52) / 150 = (50 + 26) / 150 = 76/150
        let vwap = book.calculate_vwap(Side::Buy, dec!(150)).unwrap();
        let expected = (dec!(100) * dec!(0.50) + dec!(50) * dec!(0.52)) / dec!(150);
        assert_eq!(vwap, expected);

        // VWAP should be between the two ask levels consumed
        assert!(vwap >= dec!(0.50));
        assert!(vwap <= dec!(0.52));
    }

    #[test]
    fn test_vwap_sell() {
        let mut book = LocalOrderBook::new("token_vwap_sell".into());

        // Bid levels: 100 @ 0.60, 200 @ 0.58, 100 @ 0.55
        book.update_bid(dec!(0.60), dec!(100));
        book.update_bid(dec!(0.58), dec!(200));
        book.update_bid(dec!(0.55), dec!(100));

        // Sell 150: 100 @ 0.60 + 50 @ 0.58
        // VWAP = (100*0.60 + 50*0.58) / 150 = (60 + 29) / 150 = 89/150
        let vwap = book.calculate_vwap(Side::Sell, dec!(150)).unwrap();
        let expected = (dec!(100) * dec!(0.60) + dec!(50) * dec!(0.58)) / dec!(150);
        assert_eq!(vwap, expected);

        // VWAP should be between the two bid levels consumed
        assert!(vwap >= dec!(0.58));
        assert!(vwap <= dec!(0.60));
    }

    #[test]
    fn test_insufficient_liquidity() {
        let mut book = LocalOrderBook::new("token_liq".into());

        // Only 50 available on ask side
        book.update_ask(dec!(0.50), dec!(50));

        // Try to buy 100 — not enough liquidity
        assert!(book.calculate_vwap(Side::Buy, dec!(100)).is_none());

        // Empty bid side — sell should also fail
        assert!(book.calculate_vwap(Side::Sell, dec!(10)).is_none());
    }

    #[test]
    fn test_apply_snapshot() {
        let mut book = LocalOrderBook::new("token_snap".into());

        // Start with some levels
        book.update_bid(dec!(0.50), dec!(100));
        book.update_ask(dec!(0.60), dec!(100));

        // Apply a snapshot that completely replaces everything
        book.apply_snapshot(
            vec![(dec!(0.45), dec!(200)), (dec!(0.44), dec!(150))],
            vec![(dec!(0.65), dec!(300)), (dec!(0.66), dec!(250))],
        );

        // Old levels should be gone
        let (bid_price, bid_size) = book.best_bid().unwrap();
        assert_eq!(bid_price, dec!(0.45));
        assert_eq!(bid_size, dec!(200));

        let (ask_price, ask_size) = book.best_ask().unwrap();
        assert_eq!(ask_price, dec!(0.65));
        assert_eq!(ask_size, dec!(300));
    }

    #[test]
    fn test_spread_calculation() {
        let mut book = LocalOrderBook::new("token_spread".into());

        book.update_bid(dec!(0.55), dec!(100));
        book.update_ask(dec!(0.57), dec!(100));

        let spread = book.spread().unwrap();
        assert_eq!(spread, dec!(0.02));

        // Negative spread (crossed book) is represented as-is
        book.update_bid(dec!(0.58), dec!(100));
        let crossed_spread = book.spread().unwrap();
        assert_eq!(crossed_spread, dec!(-0.01));
    }

    #[test]
    fn test_is_stale() {
        let book = LocalOrderBook::new("token_stale".into());
        // Just created — should not be stale with a generous max_age
        assert!(!book.is_stale(Duration::from_secs(60)));
        // But stale with a zero-duration max_age
        assert!(book.is_stale(Duration::ZERO));
    }

    #[tokio::test]
    async fn test_order_book_store() {
        let store = OrderBookStore::new();

        // Initially empty
        assert_eq!(store.token_count(), 0);
        assert!(store.get("token_a").is_none());

        // get_or_create should create a new book
        let book_a = store.get_or_create("token_a");
        assert_eq!(store.token_count(), 1);

        // get should now return the same book
        let book_a2 = store.get("token_a").unwrap();
        assert!(Arc::ptr_eq(&book_a, &book_a2));

        // Add another token
        let _book_b = store.get_or_create("token_b");
        assert_eq!(store.token_count(), 2);

        // Verify we can write and read through the lock
        {
            let mut guard = book_a.write().await;
            guard.update_bid(dec!(0.50), dec!(100));
        }
        {
            let guard = book_a.read().await;
            assert_eq!(guard.best_bid().unwrap().0, dec!(0.50));
        }
    }
}
