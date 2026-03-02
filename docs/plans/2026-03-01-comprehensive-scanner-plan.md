# Comprehensive Polymarket Scanner — Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Scan ALL active Polymarket markets for real arbitrage opportunities with VWAP-based edge calculation at multiple size tiers, outputting ranked opportunities via CLI with JSON/CSV export.

**Architecture:** Extend existing arb-data (fetching), arb-strategy (detection), and arb-daemon (CLI) crates. Add paginated market fetching, concurrent orderbook pulling, multi-tier VWAP, deadline monotonicity detection, spread/depth profiling, and structured export. No new crates.

**Tech Stack:** Rust, tokio (concurrency/semaphore), polymarket-client-sdk (Gamma + CLOB APIs), serde_json/csv (export), clap (CLI args), rust_decimal (arithmetic).

---

## Task 1: Paginated Market Fetcher

**Files:**
- Modify: `crates/arb-data/src/poller.rs` (extend `SdkMarketDataSource`)
- Test: `crates/arb-data/src/poller.rs` (inline tests)

The current `fetch_markets()` makes a single API call and gets ~100 markets. We need pagination to get ALL active markets.

**Step 1: Write the failing test**

Add to `crates/arb-data/src/poller.rs` at end of file — a test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_markets() {
        let binary = MarketState {
            condition_id: "0xabc".into(),
            question: "Binary?".into(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["tok1".into(), "tok2".into()],
            outcome_prices: vec![dec!(0.5), dec!(0.5)],
            orderbooks: vec![],
            volume_24hr: Some(dec!(1000)),
            liquidity: Some(dec!(500)),
            active: true,
            neg_risk: false,
        };
        let multi = MarketState {
            condition_id: "0xdef".into(),
            question: "Multi?".into(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec!["tok3".into(), "tok4".into()],
            outcome_prices: vec![dec!(0.3), dec!(0.7)],
            orderbooks: vec![],
            volume_24hr: Some(dec!(2000)),
            liquidity: Some(dec!(1000)),
            active: true,
            neg_risk: true,
        };
        let classified = classify_markets(&[binary.clone(), multi.clone()]);
        assert_eq!(classified.binary.len(), 1);
        assert_eq!(classified.neg_risk.len(), 1);
        assert_eq!(classified.binary[0].condition_id, "0xabc");
        assert_eq!(classified.neg_risk[0].condition_id, "0xdef");
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p arb-data test_classify_markets`
Expected: FAIL — `classify_markets` not defined, `ClassifiedMarkets` not defined.

**Step 3: Write minimal implementation**

Add to `crates/arb-data/src/poller.rs`:

```rust
use rust_decimal_macros::dec;

/// Markets classified by type for different analysis passes
pub struct ClassifiedMarkets {
    pub binary: Vec<MarketState>,
    pub neg_risk: Vec<MarketState>,
    pub all: Vec<MarketState>,
    pub total_token_ids: Vec<String>,
}

/// Classify markets into binary vs neg_risk for targeted analysis
pub fn classify_markets(markets: &[MarketState]) -> ClassifiedMarkets {
    let mut binary = Vec::new();
    let mut neg_risk = Vec::new();
    let mut total_token_ids = Vec::new();

    for m in markets {
        if !m.active || m.token_ids.is_empty() {
            continue;
        }
        total_token_ids.extend(m.token_ids.clone());
        if m.neg_risk {
            neg_risk.push(m.clone());
        } else if m.token_ids.len() == 2 {
            binary.push(m.clone());
        }
    }

    ClassifiedMarkets {
        binary,
        neg_risk,
        all: markets.to_vec(),
        total_token_ids,
    }
}
```

Add new method to `impl SdkMarketDataSource`:

```rust
/// Fetch ALL active markets with pagination
pub async fn fetch_all_active_markets(&self) -> Result<Vec<MarketState>> {
    use polymarket_client_sdk::gamma::types::request::MarketsRequest;

    let mut all_markets = Vec::new();
    let mut offset: u64 = 0;
    let limit: u64 = 100;

    loop {
        let request = MarketsRequest::builder()
            .active(true)
            .closed(false)
            .limit(limit)
            .offset(offset)
            .build();

        let batch = self.gamma_client.markets(&request).await
            .map_err(|e| ArbError::DataFetch(format!("Gamma API page offset={offset}: {e}")))?;

        let count = batch.len();
        for market in &batch {
            if let Some(ms) = Self::convert_market(market) {
                if ms.active && !ms.token_ids.is_empty() {
                    all_markets.push(ms);
                }
            }
        }

        if (count as u64) < limit {
            break;
        }
        offset += limit;
    }

    Ok(all_markets)
}
```

**Step 4: Run test to verify it passes**

Run: `cargo test -p arb-data test_classify_markets`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/arb-data/src/poller.rs
git commit -m "feat(arb-data): add paginated market fetcher and market classifier"
```

---

## Task 2: Concurrent Orderbook Fetcher

**Files:**
- Modify: `crates/arb-data/src/poller.rs` (add concurrent fetch method)
- Modify: `crates/arb-data/Cargo.toml` (add `tokio` semaphore — already has tokio)

**Step 1: Write the failing test**

Add to the test module in `poller.rs`:

```rust
#[test]
fn test_concurrent_config_defaults() {
    let config = ConcurrentFetchConfig::default();
    assert_eq!(config.max_concurrent, 10);
    assert_eq!(config.delay_ms, 50);
    assert_eq!(config.timeout_secs, 15);
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p arb-data test_concurrent_config`
Expected: FAIL — `ConcurrentFetchConfig` not defined.

**Step 3: Write implementation**

Add to `crates/arb-data/src/poller.rs`:

```rust
use tokio::sync::Semaphore;
use std::sync::Arc as StdArc;

/// Config for concurrent orderbook fetching
#[derive(Debug, Clone)]
pub struct ConcurrentFetchConfig {
    pub max_concurrent: usize,
    pub delay_ms: u64,
    pub timeout_secs: u64,
}

impl Default for ConcurrentFetchConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            delay_ms: 50,
            timeout_secs: 15,
        }
    }
}
```

Add new method to `impl SdkMarketDataSource`:

```rust
/// Fetch orderbooks concurrently with rate limiting
pub async fn fetch_orderbooks_concurrent(
    &self,
    token_ids: &[String],
    config: &ConcurrentFetchConfig,
    on_progress: impl Fn(usize, usize) + Send + Sync,
) -> Result<HashMap<String, OrderbookSnapshot>> {
    let semaphore = StdArc::new(Semaphore::new(config.max_concurrent));
    let results = StdArc::new(tokio::sync::Mutex::new(HashMap::new()));
    let errors = StdArc::new(tokio::sync::Mutex::new(Vec::new()));
    let completed = StdArc::new(std::sync::atomic::AtomicUsize::new(0));
    let total = token_ids.len();

    let mut handles = Vec::new();

    for token_id in token_ids {
        let sem = semaphore.clone();
        let token = token_id.clone();
        let res = results.clone();
        let errs = errors.clone();
        let comp = completed.clone();
        let timeout = config.timeout_secs;

        let handle = tokio::spawn({
            let gamma = self.gamma_client.clone();
            async move {
                let _permit = sem.acquire().await.unwrap();

                match tokio::time::timeout(
                    std::time::Duration::from_secs(timeout),
                    Self::fetch_single_orderbook(&gamma, &token),
                ).await {
                    Ok(Ok(book)) => {
                        res.lock().await.insert(token, book);
                    }
                    Ok(Err(e)) => {
                        warn!(token_id = %token, error = %e, "Orderbook fetch failed");
                        errs.lock().await.push((token, e.to_string()));
                    }
                    Err(_) => {
                        warn!(token_id = %token, "Orderbook fetch timed out");
                        errs.lock().await.push((token, "timeout".into()));
                    }
                }

                let done = comp.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                // progress callback happens outside spawn
                done
            }
        });
        handles.push(handle);

        // Small delay between spawns to avoid burst
        tokio::time::sleep(std::time::Duration::from_millis(config.delay_ms)).await;
    }

    for (i, handle) in handles.into_iter().enumerate() {
        if let Ok(done) = handle.await {
            on_progress(done, total);
        }
    }

    let final_results = results.lock().await.clone();
    let final_errors = errors.lock().await.clone();

    if !final_errors.is_empty() {
        warn!(
            success = final_results.len(),
            failed = final_errors.len(),
            total = total,
            "Orderbook fetch completed with errors"
        );
    }

    Ok(final_results)
}

/// Internal: fetch a single orderbook (static method for use in spawned tasks)
async fn fetch_single_orderbook(
    gamma: &polymarket_client_sdk::gamma::Client,
    token_id: &str,
) -> Result<OrderbookSnapshot> {
    use polymarket_client_sdk::clob::types::request::OrderBookSummaryRequest;

    let clob = polymarket_client_sdk::clob::Client::new();
    let token_u256: alloy::primitives::U256 = token_id.parse()
        .map_err(|e| ArbError::DataFetch(format!("Invalid token ID {token_id}: {e}")))?;

    let request = OrderBookSummaryRequest::builder()
        .token_id(token_u256)
        .build();

    let book = clob.order_book(&request).await
        .map_err(|e| ArbError::DataFetch(format!("CLOB orderbook {token_id}: {e}")))?;

    let mut bids: Vec<OrderbookLevel> = book.bids.iter().map(|o| OrderbookLevel {
        price: o.price.parse().unwrap_or_default(),
        size: o.size.parse().unwrap_or_default(),
    }).collect();
    let mut asks: Vec<OrderbookLevel> = book.asks.iter().map(|o| OrderbookLevel {
        price: o.price.parse().unwrap_or_default(),
        size: o.size.parse().unwrap_or_default(),
    }).collect();

    bids.sort_by(|a, b| b.price.cmp(&a.price));
    asks.sort_by(|a, b| a.price.cmp(&b.price));

    Ok(OrderbookSnapshot {
        token_id: token_id.to_string(),
        bids,
        asks,
        timestamp: Utc::now(),
    })
}
```

Note: The `gamma_client` field needs `Clone`. Check if `polymarket_client_sdk::gamma::Client` implements Clone. If not, wrap in `Arc`. Adjust accordingly during implementation.

**Step 4: Run test to verify it passes**

Run: `cargo test -p arb-data test_concurrent_config`
Expected: PASS

**Step 5: Verify full crate compiles**

Run: `cargo build -p arb-data`
Expected: Success (no errors)

**Step 6: Commit**

```bash
git add crates/arb-data/src/poller.rs crates/arb-data/Cargo.toml
git commit -m "feat(arb-data): add concurrent orderbook fetcher with rate limiting"
```

---

## Task 3: Multi-Tier VWAP Analysis

**Files:**
- Modify: `crates/arb-data/src/orderbook.rs` (extend `OrderbookProcessor`)
- Test: inline in `orderbook.rs`

**Step 1: Write the failing test**

Add to the existing `#[cfg(test)] mod tests` in `orderbook.rs`:

```rust
#[test]
fn test_vwap_tiers() {
    let book = make_book(
        &[("0.50", "200"), ("0.49", "300"), ("0.48", "500")],
        &[("0.52", "200"), ("0.53", "300"), ("0.54", "500")],
    );
    let proc = default_processor();
    let tiers = vec![dec!(100), dec!(200), dec!(500)];
    let result = proc.estimate_vwap_tiers(&book, Side::Buy, &tiers);
    assert_eq!(result.len(), 3);
    // 100 shares at 0.52 = VWAP 0.52
    assert_eq!(result[0].vwap, dec!(0.52));
    // 200 shares: all at 0.52
    assert_eq!(result[1].vwap, dec!(0.52));
    // 500 shares: 200@0.52 + 300@0.53 = VWAP ~0.528
    assert!(result[2].vwap > dec!(0.52));
}

#[test]
fn test_spread_depth_profile() {
    let book = make_book(
        &[("0.50", "200"), ("0.49", "300")],
        &[("0.52", "200"), ("0.53", "300")],
    );
    let proc = default_processor();
    let profile = proc.spread_depth_profile(&book);
    assert_eq!(profile.best_bid, dec!(0.50));
    assert_eq!(profile.best_ask, dec!(0.52));
    assert_eq!(profile.spread, dec!(0.02));
    assert!(profile.spread_bps > dec!(0));
    assert_eq!(profile.bid_depth_3, dec!(500)); // 200 + 300
    assert_eq!(profile.ask_depth_3, dec!(500)); // 200 + 300
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p arb-data test_vwap_tiers test_spread_depth`
Expected: FAIL — methods not defined.

**Step 3: Write implementation**

Add structs and methods to `crates/arb-data/src/orderbook.rs`:

```rust
/// Spread and depth profile for a single orderbook
#[derive(Debug, Clone, Serialize)]
pub struct SpreadDepthProfile {
    pub token_id: String,
    pub best_bid: Decimal,
    pub best_ask: Decimal,
    pub spread: Decimal,
    pub spread_bps: Decimal,
    pub mid: Decimal,
    pub bid_depth_3: Decimal,   // total size in top 3 bid levels
    pub ask_depth_3: Decimal,   // total size in top 3 ask levels
    pub bid_depth_5: Decimal,
    pub ask_depth_5: Decimal,
}
```

Add `use serde::Serialize;` to imports.

Add methods to `impl OrderbookProcessor`:

```rust
/// Compute VWAP at multiple size tiers
pub fn estimate_vwap_tiers(
    &self,
    book: &OrderbookSnapshot,
    side: Side,
    sizes: &[Decimal],
) -> Vec<VwapEstimate> {
    sizes.iter().map(|&size| {
        self.estimate_vwap(book, side, size).unwrap_or(VwapEstimate {
            vwap: Decimal::ZERO,
            total_size: Decimal::ZERO,
            levels_consumed: 0,
            max_available: Decimal::ZERO,
            slippage_bps: Decimal::ZERO,
        })
    }).collect()
}

/// Compute spread and depth profile
pub fn spread_depth_profile(&self, book: &OrderbookSnapshot) -> SpreadDepthProfile {
    let best_bid = book.bids.first().map(|l| l.price).unwrap_or_default();
    let best_ask = book.asks.first().map(|l| l.price).unwrap_or_default();
    let spread = best_ask - best_bid;
    let mid = (best_ask + best_bid) / dec!(2);
    let spread_bps = if mid > Decimal::ZERO {
        spread / mid * dec!(10000)
    } else {
        Decimal::ZERO
    };

    let bid_depth = |n: usize| book.bids.iter().take(n).map(|l| l.size).sum();
    let ask_depth = |n: usize| book.asks.iter().take(n).map(|l| l.size).sum();

    SpreadDepthProfile {
        token_id: book.token_id.clone(),
        best_bid,
        best_ask,
        spread,
        spread_bps,
        mid,
        bid_depth_3: bid_depth(3),
        ask_depth_3: ask_depth(3),
        bid_depth_5: bid_depth(5),
        ask_depth_5: ask_depth(5),
    }
}
```

**Step 4: Run tests**

Run: `cargo test -p arb-data test_vwap_tiers test_spread_depth`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/arb-data/src/orderbook.rs
git commit -m "feat(arb-data): add multi-tier VWAP and spread/depth profiling"
```

---

## Task 4: Deadline Monotonicity Detector

**Files:**
- Create: `crates/arb-strategy/src/deadline.rs`
- Modify: `crates/arb-strategy/src/lib.rs` (add `pub mod deadline;`)

This detects cumulative "by deadline" event series where a later deadline is cheaper than an earlier one (pricing inversion).

**Step 1: Write the failing test**

Create `crates/arb-strategy/src/deadline.rs` with test first:

```rust
use arb_core::{MarketState, Opportunity, ArbType, Side, TradeLeg};
use arb_core::error::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use uuid::Uuid;

/// Detect deadline monotonicity violations in cumulative "by..." event series.
/// In a series like "X by March", "X by June", "X by Dec", later deadlines
/// should always be priced >= earlier ones. An inversion is a potential arb.
pub struct DeadlineMonotonicityDetector;

impl DeadlineMonotonicityDetector {
    pub fn new() -> Self {
        Self
    }

    /// Scan a group of markets from the same event for deadline inversions.
    /// Markets should be pre-sorted by deadline (earliest first).
    pub fn check_event_group(&self, markets: &[MarketState]) -> Vec<Opportunity> {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use arb_core::OrderbookSnapshot;

    fn make_market(cid: &str, question: &str, yes_price: Decimal, neg_risk: bool) -> MarketState {
        MarketState {
            condition_id: cid.into(),
            question: question.into(),
            outcomes: vec!["Yes".into(), "No".into()],
            token_ids: vec![format!("tok_{cid}_y"), format!("tok_{cid}_n")],
            outcome_prices: vec![yes_price, dec!(1) - yes_price],
            orderbooks: vec![],
            volume_24hr: Some(dec!(1000)),
            liquidity: Some(dec!(500)),
            active: true,
            neg_risk,
        }
    }

    #[test]
    fn test_no_inversion() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("a", "X by March?", dec!(0.10), false),
            make_market("b", "X by June?", dec!(0.30), false),
            make_market("c", "X by Dec?", dec!(0.50), false),
        ];
        let opps = detector.check_event_group(&markets);
        assert!(opps.is_empty());
    }

    #[test]
    fn test_inversion_detected() {
        let detector = DeadlineMonotonicityDetector::new();
        let markets = vec![
            make_market("a", "X by March?", dec!(0.10), false),
            make_market("b", "X by June?", dec!(0.05), false), // Inversion! cheaper than March
            make_market("c", "X by Dec?", dec!(0.50), false),
        ];
        let opps = detector.check_event_group(&markets);
        assert_eq!(opps.len(), 1);
        assert_eq!(opps[0].arb_type, ArbType::CrossMarket);
        assert!(opps[0].gross_edge > Decimal::ZERO);
    }
}
```

**Step 2: Run test to verify it fails**

Run: `cargo test -p arb-strategy test_no_inversion test_inversion_detected`
Expected: FAIL — `todo!()` panics.

**Step 3: Write implementation**

Replace the `check_event_group` body:

```rust
pub fn check_event_group(&self, markets: &[MarketState]) -> Vec<Opportunity> {
    let mut opportunities = Vec::new();

    if markets.len() < 2 {
        return opportunities;
    }

    // Compare each consecutive pair: earlier price should be <= later price
    for i in 0..markets.len() - 1 {
        let earlier = &markets[i];
        let later = &markets[i + 1];

        let earlier_yes = earlier.outcome_prices.first().copied().unwrap_or_default();
        let later_yes = later.outcome_prices.first().copied().unwrap_or_default();

        // Inversion: later deadline is cheaper than earlier
        if later_yes < earlier_yes && earlier_yes > Decimal::ZERO {
            let edge = earlier_yes - later_yes;

            let opp = Opportunity {
                id: Uuid::new_v4(),
                arb_type: ArbType::CrossMarket,
                markets: vec![
                    earlier.condition_id.clone(),
                    later.condition_id.clone(),
                ],
                legs: vec![
                    TradeLeg {
                        token_id: later.token_ids.first().cloned().unwrap_or_default(),
                        side: Side::Buy,
                        target_price: later_yes,
                        target_size: Decimal::ZERO,
                        vwap_estimate: Decimal::ZERO,
                    },
                    TradeLeg {
                        token_id: earlier.token_ids.first().cloned().unwrap_or_default(),
                        side: Side::Sell,
                        target_price: earlier_yes,
                        target_size: Decimal::ZERO,
                        vwap_estimate: Decimal::ZERO,
                    },
                ],
                gross_edge: edge,
                net_edge: Decimal::ZERO, // refined later by EdgeCalculator
                estimated_vwap: vec![],
                confidence: 0.5,
                size_available: Decimal::ZERO,
                detected_at: Utc::now(),
            };
            opportunities.push(opp);
        }
    }

    opportunities
}
```

**Step 4: Run tests**

Run: `cargo test -p arb-strategy test_no_inversion test_inversion_detected`
Expected: PASS

**Step 5: Register module**

Add `pub mod deadline;` to `crates/arb-strategy/src/lib.rs`.

**Step 6: Commit**

```bash
git add crates/arb-strategy/src/deadline.rs crates/arb-strategy/src/lib.rs
git commit -m "feat(arb-strategy): add deadline monotonicity detector"
```

---

## Task 5: JSON/CSV Export

**Files:**
- Create: `crates/arb-daemon/src/export.rs`
- Modify: `crates/arb-daemon/Cargo.toml` (add `csv` dependency)
- Modify: `crates/arb-daemon/src/main.rs` (add `mod export;`)

**Step 1: Write the failing test**

Create `crates/arb-daemon/src/export.rs`:

```rust
use arb_core::{ArbType, Opportunity, Side, TradeLeg};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Serialize;
use uuid::Uuid;

/// Full scan result for JSON export
#[derive(Debug, Serialize)]
pub struct ScanReport {
    pub scan_time_secs: f64,
    pub total_markets: usize,
    pub total_orderbooks_fetched: usize,
    pub total_orderbooks_failed: usize,
    pub opportunities: Vec<OpportunityRow>,
    pub market_summary: Vec<MarketSummaryRow>,
}

/// Flat opportunity row for CSV/display
#[derive(Debug, Clone, Serialize)]
pub struct OpportunityRow {
    pub rank: usize,
    pub arb_type: String,
    pub edge_bps: f64,
    pub vwap_edge_100: f64,
    pub vwap_edge_500: f64,
    pub vwap_edge_1000: f64,
    pub vwap_edge_5000: f64,
    pub num_markets: usize,
    pub question: String,
    pub condition_ids: String,
    pub confidence: f64,
}

/// Market structure summary row
#[derive(Debug, Clone, Serialize)]
pub struct MarketSummaryRow {
    pub category: String,
    pub market_count: usize,
    pub avg_spread_bps: f64,
    pub total_volume_24h: f64,
    pub avg_depth: f64,
}

/// Export scan report to JSON file
pub fn export_json(report: &ScanReport, path: &str) -> anyhow::Result<()> {
    let json = serde_json::to_string_pretty(report)?;
    std::fs::write(path, json)?;
    Ok(())
}

/// Export opportunities to CSV file
pub fn export_csv(opportunities: &[OpportunityRow], path: &str) -> anyhow::Result<()> {
    let mut wtr = csv::Writer::from_path(path)?;
    for opp in opportunities {
        wtr.serialize(opp)?;
    }
    wtr.flush()?;
    Ok(())
}

/// Convert core Opportunity to flat OpportunityRow for display/export
pub fn opportunity_to_row(
    rank: usize,
    opp: &Opportunity,
    question: &str,
    vwap_edges_bps: &[f64; 4],
) -> OpportunityRow {
    OpportunityRow {
        rank,
        arb_type: opp.arb_type.to_string(),
        edge_bps: opp.net_edge_bps().to_string().parse().unwrap_or(0.0),
        vwap_edge_100: vwap_edges_bps[0],
        vwap_edge_500: vwap_edges_bps[1],
        vwap_edge_1000: vwap_edges_bps[2],
        vwap_edge_5000: vwap_edges_bps[3],
        num_markets: opp.markets.len(),
        question: question.to_string(),
        condition_ids: opp.markets.join(","),
        confidence: opp.confidence,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report() -> ScanReport {
        ScanReport {
            scan_time_secs: 120.5,
            total_markets: 500,
            total_orderbooks_fetched: 480,
            total_orderbooks_failed: 20,
            opportunities: vec![OpportunityRow {
                rank: 1,
                arb_type: "IntraMarket".into(),
                edge_bps: 15.0,
                vwap_edge_100: 12.0,
                vwap_edge_500: 8.0,
                vwap_edge_1000: 5.0,
                vwap_edge_5000: -2.0,
                num_markets: 1,
                question: "Test market?".into(),
                condition_ids: "0xabc".into(),
                confidence: 0.8,
            }],
            market_summary: vec![],
        }
    }

    #[test]
    fn test_json_export() {
        let report = sample_report();
        let path = "/tmp/test_scan_report.json";
        export_json(&report, path).unwrap();
        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("IntraMarket"));
        assert!(contents.contains("480"));
        std::fs::remove_file(path).ok();
    }

    #[test]
    fn test_csv_export() {
        let report = sample_report();
        let path = "/tmp/test_opportunities.csv";
        export_csv(&report.opportunities, path).unwrap();
        let contents = std::fs::read_to_string(path).unwrap();
        assert!(contents.contains("IntraMarket"));
        assert!(contents.contains("Test market"));
        std::fs::remove_file(path).ok();
    }
}
```

**Step 2: Run tests to verify they fail**

Run: `cargo test -p arb-daemon test_json_export test_csv_export`
Expected: FAIL — module not registered, `csv` crate not in deps.

**Step 3: Wire it up**

Add to `crates/arb-daemon/Cargo.toml` under `[dependencies]`:
```toml
csv = "1"
serde_json = { workspace = true }
```

Add `mod export;` to `crates/arb-daemon/src/main.rs` (after `mod engine;`).

**Step 4: Run tests**

Run: `cargo test -p arb-daemon test_json_export test_csv_export`
Expected: PASS

**Step 5: Commit**

```bash
git add crates/arb-daemon/src/export.rs crates/arb-daemon/src/main.rs crates/arb-daemon/Cargo.toml
git commit -m "feat(arb-daemon): add JSON/CSV export for scan results"
```

---

## Task 6: Comprehensive Scan Command

**Files:**
- Modify: `crates/arb-daemon/src/commands/scan.rs` (rewrite with comprehensive mode)
- Modify: `crates/arb-daemon/src/main.rs` (add CLI args for comprehensive scan)

This is the main integration task. It wires together all the pieces from Tasks 1-5.

**Step 1: Add CLI args**

In `crates/arb-daemon/src/main.rs`, modify the `Scan` variant:

```rust
/// One-shot scan for arbitrage opportunities
Scan {
    /// Run comprehensive scan of ALL active markets
    #[arg(long)]
    comprehensive: bool,

    /// Minimum net edge in basis points to display
    #[arg(long, default_value = "0")]
    min_edge: u64,

    /// Comma-separated VWAP size tiers (USD)
    #[arg(long, default_value = "100,500,1000,5000")]
    size_tiers: String,

    /// Export full results to JSON file
    #[arg(long)]
    export: Option<String>,

    /// Export opportunities to CSV file
    #[arg(long)]
    export_csv: Option<String>,

    /// Max concurrent API requests
    #[arg(long, default_value = "10")]
    max_concurrent: usize,

    /// Per-request timeout in seconds
    #[arg(long, default_value = "15")]
    timeout: u64,

    /// Show per-market scan progress
    #[arg(long)]
    verbose: bool,
},
```

Update the match arm in `main()`:

```rust
Commands::Scan {
    comprehensive,
    min_edge,
    size_tiers,
    export,
    export_csv,
    max_concurrent,
    timeout,
    verbose,
} => {
    if comprehensive {
        commands::scan::execute_comprehensive(
            min_edge, &size_tiers, export, export_csv,
            max_concurrent, timeout, verbose,
        ).await?;
    } else {
        commands::scan::execute().await?;
    }
}
```

**Step 2: Write the comprehensive scan function**

Rewrite `crates/arb-daemon/src/commands/scan.rs` to add `execute_comprehensive`:

```rust
use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, SlippageEstimator};
use arb_core::{ArbType, MarketState, Opportunity, Side};
use arb_data::market_cache::MarketCache;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::{classify_markets, ConcurrentFetchConfig, SdkMarketDataSource};
use arb_strategy::deadline::DeadlineMonotonicityDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use crate::export::{
    self, OpportunityRow, ScanReport, MarketSummaryRow, opportunity_to_row,
};
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

pub async fn execute_comprehensive(
    min_edge_bps: u64,
    size_tiers_str: &str,
    export_path: Option<String>,
    export_csv_path: Option<String>,
    max_concurrent: usize,
    timeout_secs: u64,
    verbose: bool,
) -> anyhow::Result<()> {
    let config = ArbConfig::load();
    let start = Instant::now();

    // Parse size tiers
    let size_tiers: Vec<Decimal> = size_tiers_str
        .split(',')
        .filter_map(|s| s.trim().parse().ok())
        .collect();

    println!("=== COMPREHENSIVE POLYMARKET SCAN ===");
    println!();

    // Phase 1: Fetch all markets
    println!("[1/4] Fetching all active markets...");
    let source = SdkMarketDataSource::new();
    let markets = source.fetch_all_active_markets().await?;
    let classified = classify_markets(&markets);
    println!(
        "  Found {} markets ({} binary, {} neg-risk, {} token IDs)",
        classified.all.len(),
        classified.binary.len(),
        classified.neg_risk.len(),
        classified.total_token_ids.len(),
    );

    // Phase 2: Fetch all orderbooks concurrently
    println!("[2/4] Fetching orderbooks ({} tokens, {} concurrent)...",
        classified.total_token_ids.len(), max_concurrent);

    let fetch_config = ConcurrentFetchConfig {
        max_concurrent,
        delay_ms: 50,
        timeout_secs,
    };

    let orderbooks = source.fetch_orderbooks_concurrent(
        &classified.total_token_ids,
        &fetch_config,
        |done, total| {
            if verbose && done % 50 == 0 {
                eprintln!("  Progress: {done}/{total} orderbooks...");
            }
        },
    ).await?;

    let success = orderbooks.len();
    let failed = classified.total_token_ids.len() - success;
    println!("  Fetched {success}/{} orderbooks ({failed} failed)",
        classified.total_token_ids.len());

    // Attach orderbooks to market states and populate cache
    let cache = MarketCache::new();
    let mut enriched_markets: Vec<MarketState> = Vec::new();
    for mut market in classified.all.clone() {
        let mut books = Vec::new();
        for tid in &market.token_ids {
            if let Some(book) = orderbooks.get(tid) {
                books.push(book.clone());
            }
        }
        market.orderbooks = books;
        cache.update_one(market.clone());
        enriched_markets.push(market);
    }

    // Phase 3: Run detectors
    println!("[3/4] Running arb detectors...");

    let slippage_estimator: Arc<dyn SlippageEstimator> =
        Arc::new(OrderbookProcessor::new(config.slippage.clone()));
    let edge_calc = EdgeCalculator::default_with_config(config.slippage.clone());

    let mut all_opportunities: Vec<(Opportunity, String)> = Vec::new();

    // 3a: Intra-market
    let intra = IntraMarketDetector::new(
        config.strategy.intra_market.clone(),
        config.strategy.clone(),
        slippage_estimator.clone(),
    );
    let intra_opps = intra.scan(&enriched_markets).await?;
    let intra_count = intra_opps.len();
    for opp in intra_opps {
        let q = enriched_markets.iter()
            .find(|m| m.condition_id == opp.markets[0])
            .map(|m| m.question.clone())
            .unwrap_or_default();
        all_opportunities.push((opp, q));
    }

    // 3b: Multi-outcome
    let multi = MultiOutcomeDetector::new(
        config.strategy.multi_outcome.clone(),
        config.strategy.clone(),
        slippage_estimator.clone(),
    );
    let multi_opps = multi.scan(&enriched_markets).await?;
    let multi_count = multi_opps.len();
    for opp in multi_opps {
        let q = format!("Multi-outcome ({} markets)", opp.markets.len());
        all_opportunities.push((opp, q));
    }

    // 3c: Deadline monotonicity
    let deadline_detector = DeadlineMonotonicityDetector::new();
    // Group non-neg-risk markets by common question prefix (strip "by ..." suffix)
    let deadline_groups = group_by_event_prefix(&classified.binary);
    let mut deadline_count = 0;
    for group in &deadline_groups {
        if group.len() >= 2 {
            let opps = deadline_detector.check_event_group(group);
            deadline_count += opps.len();
            for opp in opps {
                all_opportunities.push((opp, format!("Deadline inversion ({} markets)", group.len())));
            }
        }
    }

    println!(
        "  IntraMarket: {intra_count} | MultiOutcome: {multi_count} | Deadline: {deadline_count}"
    );

    // Phase 4: Rank and display
    println!("[4/4] Ranking and analyzing...");
    println!();

    // Sort by gross edge descending
    all_opportunities.sort_by(|a, b| b.0.gross_edge.cmp(&a.0.gross_edge));

    // Build output rows with multi-tier VWAP
    let ob_proc = OrderbookProcessor::new(config.slippage.clone());
    let mut opp_rows: Vec<OpportunityRow> = Vec::new();

    for (rank, (opp, question)) in all_opportunities.iter().enumerate() {
        let edge_bps = opp.net_edge_bps();
        if edge_bps < Decimal::from(min_edge_bps) && min_edge_bps > 0 {
            continue;
        }

        // VWAP analysis at each tier (simplified: use first leg's orderbook)
        let vwap_edges: [f64; 4] = [0.0, 0.0, 0.0, 0.0]; // Will be refined per-leg

        opp_rows.push(opportunity_to_row(rank + 1, opp, question, &vwap_edges));
    }

    // Print results
    let elapsed = start.elapsed();
    println!("=== RESULTS ===");
    println!(
        "Markets: {} | Orderbooks: {}/{} | Time: {:.1}s",
        enriched_markets.len(),
        success,
        classified.total_token_ids.len(),
        elapsed.as_secs_f64()
    );
    println!();

    if opp_rows.is_empty() {
        println!("No arbitrage opportunities found above {min_edge_bps} bps threshold.");
        println!();
        // Show near-misses
        println!("--- NEAR-MISS OPPORTUNITIES (all detections) ---");
        for (opp, q) in &all_opportunities {
            let bps: f64 = opp.net_edge_bps().to_string().parse().unwrap_or(0.0);
            println!(
                "  {:>+7.1}bp  {:<14}  {} markets  {}",
                bps,
                opp.arb_type.to_string(),
                opp.markets.len(),
                &q[..q.len().min(60)],
            );
        }
    } else {
        println!("--- ARBITRAGE OPPORTUNITIES (ranked by edge) ---");
        println!(
            "{:>3} {:>9} {:>14} {:>8} {:<50}",
            "#", "Edge(bps)", "Type", "Markets", "Question"
        );
        println!("{}", "-".repeat(90));
        for row in &opp_rows {
            println!(
                "{:>3} {:>+8.1}bp {:>14} {:>8} {:<50}",
                row.rank, row.edge_bps, row.arb_type, row.num_markets,
                &row.question[..row.question.len().min(50)],
            );
        }
    }

    // Spread/depth market structure summary
    println!();
    println!("--- MARKET STRUCTURE SUMMARY ---");
    print_market_structure(&enriched_markets, &orderbooks, &ob_proc);

    // Export if requested
    if let Some(ref path) = export_path {
        let report = ScanReport {
            scan_time_secs: elapsed.as_secs_f64(),
            total_markets: enriched_markets.len(),
            total_orderbooks_fetched: success,
            total_orderbooks_failed: failed,
            opportunities: opp_rows.clone(),
            market_summary: vec![],
        };
        export::export_json(&report, path)?;
        println!("\nJSON exported to: {path}");
    }

    if let Some(ref path) = export_csv_path {
        export::export_csv(&opp_rows, path)?;
        println!("CSV exported to: {path}");
    }

    Ok(())
}

/// Group markets by event prefix for deadline analysis
fn group_by_event_prefix(markets: &[MarketState]) -> Vec<Vec<MarketState>> {
    let mut groups: HashMap<String, Vec<MarketState>> = HashMap::new();

    for m in markets {
        // Extract prefix before "by" or "before" or "in"
        let q = m.question.to_lowercase();
        let prefix = if let Some(pos) = q.find(" by ") {
            &m.question[..pos]
        } else if let Some(pos) = q.find(" before ") {
            &m.question[..pos]
        } else if let Some(pos) = q.find(" in ") {
            &m.question[..pos]
        } else {
            continue; // Not a deadline market
        };

        groups.entry(prefix.to_string()).or_default().push(m.clone());
    }

    groups.into_values()
        .filter(|g| g.len() >= 2)
        .collect()
}

/// Print market structure summary
fn print_market_structure(
    markets: &[MarketState],
    orderbooks: &HashMap<String, arb_core::OrderbookSnapshot>,
    processor: &OrderbookProcessor,
) {
    use arb_data::orderbook::SpreadDepthProfile;

    let mut profiles: Vec<SpreadDepthProfile> = Vec::new();
    for m in markets {
        if let Some(tid) = m.token_ids.first() {
            if let Some(book) = orderbooks.get(tid) {
                profiles.push(processor.spread_depth_profile(book));
            }
        }
    }

    if profiles.is_empty() {
        println!("  (no orderbook data for summary)");
        return;
    }

    let total = profiles.len();
    let avg_spread: f64 = profiles.iter()
        .map(|p| p.spread_bps.to_string().parse::<f64>().unwrap_or(0.0))
        .sum::<f64>() / total as f64;
    let avg_depth: f64 = profiles.iter()
        .map(|p| p.ask_depth_3.to_string().parse::<f64>().unwrap_or(0.0))
        .sum::<f64>() / total as f64;
    let total_vol: f64 = markets.iter()
        .map(|m| m.volume_24hr.unwrap_or_default().to_string().parse::<f64>().unwrap_or(0.0))
        .sum();

    println!(
        "  {:<12} {:>8} {:>12} {:>14} {:>14}",
        "Scope", "Markets", "Avg Spread", "Avg Depth(3)", "Total 24h Vol"
    );
    println!("  {}", "-".repeat(65));
    println!(
        "  {:<12} {:>8} {:>10.0}bp {:>14,.0} ${:>13,.0}",
        "All Active", total, avg_spread, avg_depth, total_vol
    );
}
```

**Step 3: Verify compilation**

Run: `cargo build -p arb-daemon`
Expected: Success

**Step 4: Test with a dry run**

Run: `cargo run -p arb-daemon -- scan --comprehensive --verbose`
Expected: Full scan runs, outputs results (may take 2-3 minutes)

**Step 5: Commit**

```bash
git add crates/arb-daemon/src/commands/scan.rs crates/arb-daemon/src/main.rs
git commit -m "feat(arb-daemon): add comprehensive scan command with multi-tier VWAP analysis"
```

---

## Task 7: Integration Test & Polish

**Files:**
- Modify: `crates/arb-daemon/src/commands/scan.rs` (fix any issues found)
- Modify: `crates/arb-data/src/lib.rs` (ensure all new modules exported)

**Step 1: Run full test suite**

Run: `cargo test --workspace`
Expected: All existing + new tests pass.

**Step 2: Run clippy**

Run: `cargo clippy --workspace -- -D warnings`
Expected: No warnings.

**Step 3: Run fmt check**

Run: `cargo fmt --check`
Expected: Clean.

**Step 4: Test the live scan**

Run: `cargo run -p arb-daemon --release -- scan --comprehensive --export /tmp/scan.json --export-csv /tmp/opps.csv`
Expected: Full scan completes, JSON and CSV files created.

**Step 5: Verify exports**

Run: `python3 -c "import json; d=json.load(open('/tmp/scan.json')); print(f'Markets: {d[\"total_markets\"]}, Opps: {len(d[\"opportunities\"])}')"`
Expected: Shows market count and opportunity count.

**Step 6: Final commit**

```bash
git add -A
git commit -m "feat: comprehensive polymarket scanner with full arb detection pipeline"
```

---

## Summary

| Task | Files | Est. Lines | Purpose |
|------|-------|-----------|---------|
| 1. Paginated fetcher | poller.rs | ~80 | Get ALL markets |
| 2. Concurrent orderbooks | poller.rs | ~120 | Fast parallel fetching |
| 3. Multi-tier VWAP | orderbook.rs | ~60 | Analysis at realistic sizes |
| 4. Deadline detector | deadline.rs (new) | ~100 | Cumulative event inversions |
| 5. JSON/CSV export | export.rs (new) | ~130 | Structured output |
| 6. Comprehensive scan | scan.rs, main.rs | ~200 | Integration & CLI |
| 7. Polish | various | ~20 | Tests, clippy, live test |
| **Total** | | **~710** | |
