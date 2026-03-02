use std::collections::HashMap;
use std::time::Instant;

use arb_core::{
    MarketState, OrderbookLevel, OrderbookSnapshot,
    config::PollingConfig,
    error::{ArbError, Result},
    traits::MarketDataSource,
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use tracing::{debug, info, warn};

/// Polling tier based on 24hr volume.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PollingTier {
    Hot,
    Warm,
    Cold,
}

/// Wraps the Polymarket SDK clients to fetch market data with tiered polling.
pub struct MarketPoller {
    last_poll: HashMap<String, Instant>,
    config: PollingConfig,
}

impl MarketPoller {
    pub fn new(config: PollingConfig) -> Self {
        Self {
            last_poll: HashMap::new(),
            config,
        }
    }

    pub fn polling_tier(&self, market: &MarketState) -> PollingTier {
        let vol = market
            .volume_24hr
            .unwrap_or(Decimal::ZERO)
            .to_string()
            .parse::<u64>()
            .unwrap_or(0);

        if vol >= self.config.hot_volume_threshold {
            PollingTier::Hot
        } else if vol >= self.config.warm_volume_threshold {
            PollingTier::Warm
        } else {
            PollingTier::Cold
        }
    }

    pub fn is_due(&self, condition_id: &str, tier: PollingTier) -> bool {
        let interval = match tier {
            PollingTier::Hot => self.config.hot_interval_secs,
            PollingTier::Warm => self.config.warm_interval_secs,
            PollingTier::Cold => self.config.cold_interval_secs,
        };

        match self.last_poll.get(condition_id) {
            Some(last) => last.elapsed().as_secs() >= interval,
            None => true,
        }
    }

    pub fn record_poll(&mut self, condition_id: &str) {
        self.last_poll
            .insert(condition_id.to_string(), Instant::now());
    }

    pub fn filter_due(&self, markets: &[MarketState]) -> Vec<MarketState> {
        markets
            .iter()
            .filter(|m| {
                let tier = self.polling_tier(m);
                self.is_due(&m.condition_id, tier)
            })
            .cloned()
            .collect()
    }
}

/// Markets classified by type for downstream arb strategy selection.
#[derive(Debug, Clone, Default)]
pub struct ClassifiedMarkets {
    /// Standard binary markets (exactly 2 tokens, NOT neg_risk).
    /// These are candidates for intra-market YES+NO arbitrage.
    pub binary: Vec<MarketState>,
    /// Neg-risk markets (any token count, neg_risk=true).
    /// These are part of multi-outcome events and candidates for multi-outcome arbitrage.
    pub neg_risk: Vec<MarketState>,
    /// All unique token IDs across every classified market, for bulk orderbook fetching.
    pub all_token_ids: Vec<String>,
}

/// Classify a slice of markets into binary vs neg_risk buckets.
///
/// A market is classified as:
/// - **binary**: exactly 2 token IDs and `neg_risk == false`
/// - **neg_risk**: `neg_risk == true` (any number of tokens)
///
/// Markets that don't match either category (e.g., non-neg-risk with 3+ tokens)
/// are silently dropped. All token IDs from classified markets are collected
/// into `all_token_ids` (deduplicated, deterministic order).
pub fn classify_markets(markets: &[MarketState]) -> ClassifiedMarkets {
    let mut binary = Vec::new();
    let mut neg_risk = Vec::new();
    let mut seen_tokens = HashMap::new();
    let mut all_token_ids = Vec::new();

    for market in markets {
        let classified = if market.neg_risk {
            neg_risk.push(market.clone());
            true
        } else if market.token_ids.len() == 2 {
            binary.push(market.clone());
            true
        } else {
            false
        };

        if classified {
            for tid in &market.token_ids {
                if let std::collections::hash_map::Entry::Vacant(e) = seen_tokens.entry(tid.clone())
                {
                    e.insert(());
                    all_token_ids.push(tid.clone());
                }
            }
        }
    }

    ClassifiedMarkets {
        binary,
        neg_risk,
        all_token_ids,
    }
}

/// Live market data source using the Polymarket SDK.
pub struct SdkMarketDataSource {
    gamma_client: polymarket_client_sdk::gamma::Client,
}

impl SdkMarketDataSource {
    pub fn new() -> Self {
        Self {
            gamma_client: polymarket_client_sdk::gamma::Client::default(),
        }
    }

    /// Convert an SDK Market into our internal MarketState.
    fn convert_market(
        market: &polymarket_client_sdk::gamma::types::response::Market,
    ) -> Option<MarketState> {
        // condition_id is Option<B256> — convert to hex string
        let condition_id = market.condition_id.map(|b| format!("{b:#x}"))?;

        // clob_token_ids is Option<Vec<U256>> — convert each to string
        let token_ids: Vec<String> = market
            .clob_token_ids
            .as_ref()
            .map(|ids| ids.iter().map(|id| id.to_string()).collect())
            .unwrap_or_default();

        if token_ids.is_empty() {
            return None;
        }

        // outcome_prices is Option<Vec<Decimal>> — already parsed
        let outcome_prices: Vec<Decimal> = market
            .outcome_prices
            .clone()
            .unwrap_or_default();

        // outcomes is Option<Vec<String>> — already parsed
        let outcomes: Vec<String> = market.outcomes.clone().unwrap_or_default();

        Some(MarketState {
            condition_id,
            question: market.question.clone().unwrap_or_default(),
            outcomes,
            token_ids,
            outcome_prices,
            orderbooks: vec![],
            volume_24hr: market.volume_24hr,
            liquidity: market.liquidity_num,
            active: market.active.unwrap_or(false),
            neg_risk: market.neg_risk.unwrap_or(false),
        })
    }

    /// Paginate the Gamma API to fetch ALL active markets with token IDs.
    ///
    /// Pages through results 100 at a time (the Gamma API max) using `limit`
    /// and `offset`. Stops when a page returns fewer results than the limit.
    /// Filters server-side for non-closed markets via `closed(false)`, and
    /// client-side for `active == true` and non-empty token IDs (via `convert_market`).
    pub async fn fetch_all_active_markets(&self) -> Result<Vec<MarketState>> {
        use polymarket_client_sdk::gamma::types::request::MarketsRequest;

        const PAGE_SIZE: i32 = 100;
        let mut all_markets = Vec::new();
        let mut offset: i32 = 0;

        loop {
            let request = MarketsRequest::builder()
                .limit(PAGE_SIZE)
                .offset(offset)
                .closed(false)
                .build();

            let sdk_markets = self
                .gamma_client
                .markets(&request)
                .await
                .map_err(|e| {
                    ArbError::MarketData(format!(
                        "Failed to fetch markets (offset={offset}): {e}"
                    ))
                })?;

            let page_count = sdk_markets.len();

            let converted: Vec<MarketState> = sdk_markets
                .iter()
                .filter_map(Self::convert_market)
                .filter(|m| m.active)
                .collect();

            debug!(
                "Page offset={offset}: {page_count} raw, {} after filter",
                converted.len()
            );

            all_markets.extend(converted);

            // If we got fewer than PAGE_SIZE results, we've reached the last page.
            if (page_count as i32) < PAGE_SIZE {
                break;
            }

            offset += PAGE_SIZE;
        }

        info!(
            "Fetched {} active markets across {} pages",
            all_markets.len(),
            (offset / PAGE_SIZE) + 1
        );

        Ok(all_markets)
    }
}

impl Default for SdkMarketDataSource {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl MarketDataSource for SdkMarketDataSource {
    async fn fetch_markets(&self) -> Result<Vec<MarketState>> {
        use polymarket_client_sdk::gamma::types::request::MarketsRequest;

        let request = MarketsRequest::builder().build();

        let sdk_markets = self
            .gamma_client
            .markets(&request)
            .await
            .map_err(|e| ArbError::MarketData(format!("Failed to fetch markets: {e}")))?;

        let markets: Vec<MarketState> = sdk_markets
            .iter()
            .filter_map(Self::convert_market)
            .filter(|m| m.active)
            .collect();

        debug!("Fetched {} active markets from SDK", markets.len());
        Ok(markets)
    }

    async fn fetch_orderbook(&self, token_id: &str) -> Result<OrderbookSnapshot> {
        use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;
        use polymarket_client_sdk::types::U256;

        let client = polymarket_client_sdk::clob::Client::default();

        let token_u256: U256 = token_id
            .parse()
            .map_err(|e| ArbError::Orderbook(format!("Invalid token ID '{token_id}': {e}")))?;

        let request = OrderBookSummaryRequest::builder()
            .token_id(token_u256)
            .build();

        let response = client
            .order_book(&request)
            .await
            .map_err(|e| {
                ArbError::Orderbook(format!(
                    "Failed to fetch orderbook for {token_id}: {e}"
                ))
            })?;

        // OrderSummary already has price: Decimal, size: Decimal
        let mut bids: Vec<OrderbookLevel> = response
            .bids
            .iter()
            .map(|s| OrderbookLevel {
                price: s.price,
                size: s.size,
            })
            .collect();

        let mut asks: Vec<OrderbookLevel> = response
            .asks
            .iter()
            .map(|s| OrderbookLevel {
                price: s.price,
                size: s.size,
            })
            .collect();

        // Ensure correct sort order
        bids.sort_by(|a, b| b.price.cmp(&a.price));
        asks.sort_by(|a, b| a.price.cmp(&b.price));

        Ok(OrderbookSnapshot {
            token_id: token_id.to_string(),
            bids,
            asks,
            timestamp: Utc::now(),
        })
    }

    async fn fetch_orderbooks(&self, token_ids: &[String]) -> Result<Vec<OrderbookSnapshot>> {
        let mut books = Vec::with_capacity(token_ids.len());
        for token_id in token_ids {
            match self.fetch_orderbook(token_id).await {
                Ok(book) => books.push(book),
                Err(e) => {
                    warn!("Failed to fetch orderbook for {token_id}: {e}");
                }
            }
        }
        Ok(books)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    fn make_market(
        condition_id: &str,
        token_ids: Vec<&str>,
        neg_risk: bool,
        active: bool,
    ) -> MarketState {
        MarketState {
            condition_id: condition_id.to_string(),
            question: format!("Market {condition_id}"),
            outcomes: token_ids.iter().map(|_| "Yes".to_string()).collect(),
            token_ids: token_ids.into_iter().map(String::from).collect(),
            outcome_prices: vec![dec!(0.5); 2],
            orderbooks: vec![],
            volume_24hr: Some(dec!(1000)),
            liquidity: Some(dec!(5000)),
            active,
            neg_risk,
        }
    }

    #[test]
    fn classify_empty_input() {
        let result = classify_markets(&[]);
        assert!(result.binary.is_empty());
        assert!(result.neg_risk.is_empty());
        assert!(result.all_token_ids.is_empty());
    }

    #[test]
    fn classify_binary_markets() {
        let markets = vec![
            make_market("cond_a", vec!["tok_a1", "tok_a2"], false, true),
            make_market("cond_b", vec!["tok_b1", "tok_b2"], false, true),
        ];

        let result = classify_markets(&markets);
        assert_eq!(result.binary.len(), 2);
        assert!(result.neg_risk.is_empty());
        assert_eq!(result.all_token_ids.len(), 4);
    }

    #[test]
    fn classify_neg_risk_markets() {
        let markets = vec![
            make_market("cond_c", vec!["tok_c1", "tok_c2", "tok_c3"], true, true),
            make_market("cond_d", vec!["tok_d1", "tok_d2"], true, true),
        ];

        let result = classify_markets(&markets);
        assert!(result.binary.is_empty());
        assert_eq!(result.neg_risk.len(), 2);
        assert_eq!(result.all_token_ids.len(), 5);
    }

    #[test]
    fn classify_mixed_markets() {
        let markets = vec![
            // Binary: 2 tokens, not neg_risk
            make_market("cond_bin", vec!["tok_1", "tok_2"], false, true),
            // Neg-risk: neg_risk=true
            make_market("cond_neg", vec!["tok_3", "tok_4", "tok_5"], true, true),
            // Dropped: 3 tokens but not neg_risk
            make_market("cond_drop", vec!["tok_6", "tok_7", "tok_8"], false, true),
            // Neg-risk with 2 tokens (neg_risk takes precedence)
            make_market("cond_neg2", vec!["tok_9", "tok_10"], true, true),
        ];

        let result = classify_markets(&markets);
        assert_eq!(result.binary.len(), 1);
        assert_eq!(result.binary[0].condition_id, "cond_bin");
        assert_eq!(result.neg_risk.len(), 2);
        assert_eq!(result.neg_risk[0].condition_id, "cond_neg");
        assert_eq!(result.neg_risk[1].condition_id, "cond_neg2");
        // 2 (binary) + 3 (neg1) + 2 (neg2) = 7; dropped market tokens excluded
        assert_eq!(result.all_token_ids.len(), 7);
    }

    #[test]
    fn classify_deduplicates_token_ids() {
        // Same token IDs appearing in multiple markets
        let markets = vec![
            make_market("cond_a", vec!["shared_tok", "tok_a2"], false, true),
            make_market("cond_b", vec!["shared_tok", "tok_b2"], false, true),
        ];

        let result = classify_markets(&markets);
        assert_eq!(result.binary.len(), 2);
        // "shared_tok" appears in both but should only be listed once
        assert_eq!(result.all_token_ids.len(), 3);
        assert_eq!(
            result
                .all_token_ids
                .iter()
                .filter(|t| t.as_str() == "shared_tok")
                .count(),
            1
        );
    }

    #[test]
    fn classify_preserves_insertion_order() {
        let markets = vec![
            make_market("cond_a", vec!["tok_z", "tok_y"], false, true),
            make_market("cond_b", vec!["tok_x", "tok_w"], false, true),
        ];

        let result = classify_markets(&markets);
        assert_eq!(
            result.all_token_ids,
            vec!["tok_z", "tok_y", "tok_x", "tok_w"]
        );
    }
}
