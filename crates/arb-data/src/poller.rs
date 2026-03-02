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
use tracing::{debug, warn};

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
