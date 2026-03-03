use std::collections::HashMap;
use std::sync::Mutex;

use arb_core::{
    OrderChunk, OrderbookSnapshot, Side, VwapEstimate,
    error::Result,
    traits::SlippageEstimator,
};
use rust_decimal::Decimal;

type CacheKey = (String, Side, Decimal);

/// Caching wrapper around any `SlippageEstimator`.
///
/// Memoizes `estimate_vwap` results keyed by `(token_id, side, size)`.
/// Intended to be created fresh each engine cycle so stale results
/// are automatically discarded when the orderbook changes.
///
/// Uses `Mutex<HashMap>` for interior mutability. Since the engine loop
/// is single-threaded, the mutex is never contended — it's essentially
/// a zero-cost atomic flag check.
pub struct CachedSlippageEstimator<S> {
    inner: S,
    cache: Mutex<HashMap<CacheKey, VwapEstimate>>,
}

impl<S: SlippageEstimator> CachedSlippageEstimator<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Number of cached entries (useful for diagnostics).
    pub fn cache_len(&self) -> usize {
        self.cache.lock().unwrap().len()
    }

    /// Clear all cached VWAP results. Call at the start of each engine cycle
    /// when orderbooks may have changed.
    pub fn clear_cache(&self) {
        self.cache.lock().unwrap().clear();
    }
}

impl<S: SlippageEstimator> SlippageEstimator for CachedSlippageEstimator<S> {
    fn estimate_vwap(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        size: Decimal,
    ) -> Result<VwapEstimate> {
        let key = (book.token_id.clone(), side, size);

        // Check cache first
        if let Some(cached) = self.cache.lock().unwrap().get(&key) {
            return Ok(cached.clone());
        }

        // Cache miss — compute and store
        let result = self.inner.estimate_vwap(book, side, size)?;
        self.cache
            .lock()
            .unwrap()
            .insert(key, result.clone());
        Ok(result)
    }

    fn split_order(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        total_size: Decimal,
        max_slippage_bps: Decimal,
    ) -> Result<Vec<OrderChunk>> {
        // split_order is called rarely — no caching needed
        self.inner.split_order(book, side, total_size, max_slippage_bps)
    }
}
