use arb_core::MarketState;
use dashmap::DashMap;

/// Thread-safe market cache backed by DashMap.
///
/// Updated by the poller, read concurrently by arb detectors.
/// Keyed by `condition_id`.
pub struct MarketCache {
    markets: DashMap<String, MarketState>,
}

impl MarketCache {
    pub fn new() -> Self {
        Self {
            markets: DashMap::new(),
        }
    }

    /// Insert or update multiple markets.
    pub fn update(&self, markets: &[MarketState]) {
        for market in markets {
            self.markets
                .insert(market.condition_id.clone(), market.clone());
        }
    }

    /// Insert or update a single market.
    pub fn update_one(&self, market: MarketState) {
        self.markets.insert(market.condition_id.clone(), market);
    }

    /// Get a market by condition_id.
    pub fn get(&self, condition_id: &str) -> Option<MarketState> {
        self.markets.get(condition_id).map(|r| r.clone())
    }

    /// Return all cached markets.
    pub fn all_markets(&self) -> Vec<MarketState> {
        self.markets.iter().map(|r| r.value().clone()).collect()
    }

    /// Return only active markets.
    pub fn active_markets(&self) -> Vec<MarketState> {
        self.markets
            .iter()
            .filter(|r| r.value().active)
            .map(|r| r.value().clone())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.markets.len()
    }

    pub fn is_empty(&self) -> bool {
        self.markets.is_empty()
    }

    /// Remove a market by condition_id.
    pub fn remove(&self, condition_id: &str) -> Option<MarketState> {
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
        cache.update(&[make_market("a", true), make_market("b", false), make_market("c", true)]);
        let active = cache.active_markets();
        assert_eq!(active.len(), 2);
    }
}
