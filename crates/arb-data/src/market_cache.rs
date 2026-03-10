use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use arb_core::MarketState;
use dashmap::DashMap;

/// Thread-safe market cache backed by DashMap.
///
/// Updated by the poller, read concurrently by arb detectors.
/// Keyed by `condition_id`. Values are `Arc<MarketState>` to avoid
/// expensive deep clones on every read — readers get a cheap ref-count bump.
///
/// Tracks a monotonically increasing `generation` counter. Each update stamps
/// the market with the current generation, enabling change detection:
/// detectors only need to scan markets where `last_updated_gen > last_scan_gen`.
pub struct MarketCache {
    markets: DashMap<String, Arc<MarketState>>,
    generation: AtomicU64,
}

impl MarketCache {
    pub fn new() -> Self {
        Self {
            markets: DashMap::new(),
            generation: AtomicU64::new(0),
        }
    }

    /// Current generation counter.
    pub fn generation(&self) -> u64 {
        self.generation.load(Ordering::Relaxed)
    }

    /// Insert or update multiple markets, stamping each with the current generation.
    pub fn update(&self, markets: &[MarketState]) {
        let next_gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        for market in markets {
            let mut stamped = market.clone();
            stamped.last_updated_gen = next_gen;
            self.markets
                .insert(stamped.condition_id.clone(), Arc::new(stamped));
        }
    }

    /// Insert or update a single market, stamping with the current generation.
    pub fn update_one(&self, mut market: MarketState) {
        let next_gen = self.generation.fetch_add(1, Ordering::Relaxed) + 1;
        market.last_updated_gen = next_gen;
        self.markets
            .insert(market.condition_id.clone(), Arc::new(market));
    }

    /// Get a market by condition_id (cheap Arc clone).
    pub fn get(&self, condition_id: &str) -> Option<Arc<MarketState>> {
        self.markets
            .get(condition_id)
            .map(|r| Arc::clone(r.value()))
    }

    /// Return all cached markets (cheap Arc clones).
    pub fn all_markets(&self) -> Vec<Arc<MarketState>> {
        self.markets.iter().map(|r| Arc::clone(r.value())).collect()
    }

    /// Return only active markets (cheap Arc clones).
    pub fn active_markets(&self) -> Vec<Arc<MarketState>> {
        self.markets
            .iter()
            .filter(|r| r.value().active)
            .map(|r| Arc::clone(r.value()))
            .collect()
    }

    /// Return only markets updated since `since_gen` (for change detection).
    pub fn changed_since(&self, since_gen: u64) -> Vec<Arc<MarketState>> {
        self.markets
            .iter()
            .filter(|r| r.value().last_updated_gen > since_gen)
            .map(|r| Arc::clone(r.value()))
            .collect()
    }

    pub fn len(&self) -> usize {
        self.markets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markets.is_empty()
    }

    /// Remove a market by condition_id.
    pub fn remove(&self, condition_id: &str) -> Option<Arc<MarketState>> {
        self.markets.remove(condition_id).map(|(_, v)| v)
    }

    /// Clear all cached markets.
    pub fn clear(&self) {
        self.markets.clear();
    }
}

impl Default for MarketCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    fn make_market(cid: &str, active: bool) -> MarketState {
        MarketState {
            condition_id: cid.to_string(),
            question: format!("Question {cid}"),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec![format!("{cid}_yes"), format!("{cid}_no")],
            outcome_prices: vec![Decimal::new(5, 1), Decimal::new(5, 1)],
            orderbooks: vec![],
            volume_24hr: Some(Decimal::from(50_000)),
            liquidity: Some(Decimal::from(10_000)),
            active,
            neg_risk: false,
            best_bid: None,
            best_ask: None,
            spread: None,
            last_trade_price: None,
            description: None,
            end_date_iso: None,
            slug: None,
            one_day_price_change: None,
            event_id: None,
            last_updated_gen: 0,
        }
    }

    #[test]
    fn test_cache_crud() {
        let cache = MarketCache::new();
        assert!(cache.is_empty());

        let m1 = make_market("abc", true);
        let m2 = make_market("def", true);
        cache.update(&[m1.clone(), m2.clone()]);
        assert_eq!(cache.len(), 2);

        let fetched = cache.get("abc").unwrap();
        assert_eq!(fetched.condition_id, "abc");

        assert!(cache.get("nonexistent").is_none());

        cache.remove("abc");
        assert_eq!(cache.len(), 1);
        assert!(cache.get("abc").is_none());
    }

    #[test]
    fn test_active_filter() {
        let cache = MarketCache::new();
        cache.update(&[
            make_market("a", true),
            make_market("b", false),
            make_market("c", true),
        ]);
        let active = cache.active_markets();
        assert_eq!(active.len(), 2);
    }

    #[test]
    fn test_generation_tracking() {
        let cache = MarketCache::new();
        assert_eq!(cache.generation(), 0);

        cache.update(&[make_market("a", true)]);
        let gen1 = cache.generation();
        assert!(gen1 > 0);

        let fetched = cache.get("a").unwrap();
        assert_eq!(fetched.last_updated_gen, gen1);

        cache.update_one(make_market("b", true));
        let gen2 = cache.generation();
        assert!(gen2 > gen1);

        // Only "b" should appear in changed_since(gen1)
        let changed = cache.changed_since(gen1);
        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].condition_id, "b");
    }
}
