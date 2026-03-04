# Long-Term Improvement Plan — Phase 1-7 Implementation

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Take the Polymarket arb system from paper-only to live trading with real-time data, production safety, and deployed monitoring.

**Architecture:** Wire the existing `LiveTradeExecutor` stub to real SDK calls, replace 5-second REST polling with WebSocket order book mirroring, harden risk management for real capital, and deploy to a VPS with alerting. The system already has solid strategy detection, simulation, and risk management — the gap is execution, data freshness, and operational readiness.

**Tech Stack:** Rust (tokio, axum, tungstenite), Polymarket CLOB SDK, SQLite, systemd, Discord/Telegram webhooks.

---

## Task 1: Store LocalSigner in LiveTradeExecutor

**Files:**
- Modify: `crates/arb-execution/src/auth.rs` (lines 67-82, 86-92)
- Modify: `crates/arb-execution/src/executor.rs` (lines 24-28, 45-52)
- Test: existing tests in `crates/arb-execution/src/auth.rs`

**Step 1: Modify `auth.rs` to return both client and signer**

Change `create_authenticated_client` to return a tuple:

```rust
// crates/arb-execution/src/auth.rs — replace lines 67-82
pub async fn create_authenticated_client(
    private_key: &str,
) -> Result<(clob::Client<Authenticated<Normal>>, LocalSigner<SigningKey>)> {
    let signer = LocalSigner::from_str(private_key)
        .map_err(|e| ArbError::Config(format!("Invalid private key: {e}")))?
        .with_chain_id(Some(POLYGON));

    let address = signer.address();
    info!(address = %address, "Authenticating with Polymarket CLOB");

    let client = clob::Client::default()
        .authentication_builder(&signer)
        .signature_type(SignatureType::Eoa)
        .authenticate()
        .await
        .map_err(|e| ArbError::Execution(format!("CLOB authentication failed: {e}")))?;

    info!("Authenticated successfully");
    Ok((client, signer))
}
```

And update `authenticate_from_key_file`:
```rust
// crates/arb-execution/src/auth.rs — replace lines 86-92
pub async fn authenticate_from_key_file(
    key_path: Option<&Path>,
) -> Result<(clob::Client<Authenticated<Normal>>, LocalSigner<SigningKey>)> {
    let path = resolve_key_path(key_path);
    let key = read_private_key(&path)?;
    create_authenticated_client(&key).await
}
```

**Step 2: Add signer field to `LiveTradeExecutor`**

```rust
// crates/arb-execution/src/executor.rs — replace lines 24-28
pub struct LiveTradeExecutor {
    clob_client: clob::Client<Authenticated<Normal>>,
    signer: LocalSigner<SigningKey>,
    prefer_post_only: bool,
    order_timeout_secs: u64,
}
```

Update `new()` and `from_key_file()`:
```rust
// replace new() constructor
pub fn new(
    clob_client: clob::Client<Authenticated<Normal>>,
    signer: LocalSigner<SigningKey>,
    prefer_post_only: bool,
    order_timeout_secs: u64,
) -> Self {
    Self { clob_client, signer, prefer_post_only, order_timeout_secs }
}

// replace from_key_file()
pub async fn from_key_file(
    key_path: Option<&std::path::Path>,
    prefer_post_only: bool,
    order_timeout_secs: u64,
) -> Result<Self> {
    let (client, signer) = auth::authenticate_from_key_file(key_path).await?;
    Ok(Self::new(client, signer, prefer_post_only, order_timeout_secs))
}
```

**Step 3: Add required import for `LocalSigner` type**

Add `use alloy::signers::local::LocalSigner;` and `use alloy::signers::k256::ecdsa::SigningKey;` to `executor.rs` imports.

**Step 4: Run tests**

Run: `cargo test -p arb-execution`
Expected: All existing tests pass (auth tests don't construct LiveTradeExecutor)

**Step 5: Commit**

```bash
git add crates/arb-execution/src/auth.rs crates/arb-execution/src/executor.rs
git commit -m "refactor: return LocalSigner from auth, store in LiveTradeExecutor"
```

---

## Task 2: Wire Real Order Placement in execute_leg()

**Files:**
- Modify: `crates/arb-execution/src/executor.rs` (lines 55-95)

**Step 1: Replace the stub with real SDK calls**

Replace the stub in `execute_leg()` (lines 76-94) with:

```rust
// crates/arb-execution/src/executor.rs — replace the stub block
let token_id: U256 = leg.token_id.parse()
    .map_err(|e| ArbError::Execution(format!("Invalid token_id: {e}")))?;

let order = self.clob_client
    .limit_order()
    .token_id(token_id)
    .side(sdk_side)
    .price(leg.vwap_estimate)
    .size(leg.target_size)
    .order_type(order_type)
    .build()
    .await
    .map_err(|e| ArbError::Execution(format!("Order build failed: {e}")))?;

let signed = self.clob_client
    .sign(&self.signer, order)
    .await
    .map_err(|e| ArbError::Execution(format!("Order signing failed: {e}")))?;

let result = self.clob_client
    .post_order(signed)
    .await
    .map_err(|e| ArbError::Execution(format!("Order post failed: {e}")))?;

info!(
    token_id = %leg.token_id,
    side = ?leg.side,
    price = %leg.vwap_estimate,
    size = %leg.target_size,
    "Order placed successfully"
);

// Map SDK response to LegReport
// The SDK's PostOrderResponse contains order_id and status
Ok(LegReport {
    order_id: result.order_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string()),
    token_id: leg.token_id.clone(),
    condition_id: String::new(),
    side: leg.side,
    expected_vwap: leg.vwap_estimate,
    actual_fill_price: leg.vwap_estimate, // FOK fills at limit or better
    filled_size: leg.target_size,
    status: FillStatus::FullyFilled,
})
```

Also remove the `_` prefix from `_side` → `sdk_side` and `_order_type` → `order_type` variables earlier in the function (lines 56-65).

**Step 2: Add U256 import**

Add `use alloy::primitives::U256;` to executor.rs imports.

**Step 3: Verify compilation**

Run: `cargo build -p arb-execution`
Expected: Compiles. May need to adjust based on exact SDK response type — check `PostOrderResponse` fields.

**Step 4: Commit**

```bash
git add crates/arb-execution/src/executor.rs
git commit -m "feat: wire live order placement via SDK in LiveTradeExecutor"
```

---

## Task 3: Wire Live/Paper Mode Toggle in Engine

**Files:**
- Modify: `crates/arb-api/src/engine_task.rs` (line 125)
- Modify: `crates/arb-api/Cargo.toml` (add arb-execution dependency if missing)

**Step 1: Add arb-execution dependency to arb-api**

Check if `arb-execution` is already in `crates/arb-api/Cargo.toml`. If not:
```toml
arb-execution = { path = "../arb-execution" }
```

**Step 2: Replace hardcoded PaperTradeExecutor with mode-based branching**

Replace line 125 in `engine_task.rs`:

```rust
// crates/arb-api/src/engine_task.rs — replace line 125
let executor: Box<dyn arb_core::TradeExecutor> = {
    let config = state.config.read().await;
    if config.is_live() {
        info!("Starting engine in LIVE trading mode");
        let key_path = config.general.key_file.as_ref().map(std::path::Path::new);
        match arb_execution::LiveTradeExecutor::from_key_file(
            key_path,
            config.slippage.prefer_post_only.unwrap_or(false),
            config.risk.order_timeout_secs.unwrap_or(30),
        ).await {
            Ok(live) => Box::new(live),
            Err(e) => {
                error!("Failed to initialize live executor: {e}. Falling back to paper.");
                Box::new(arb_execution::PaperTradeExecutor::default_pessimism())
            }
        }
    } else {
        info!("Starting engine in PAPER trading mode");
        Box::new(arb_execution::PaperTradeExecutor::default_pessimism())
    }
};
```

**Step 3: Add config fields if missing**

In `crates/arb-core/src/config.rs`, ensure `GeneralConfig` has:
```rust
pub key_file: Option<String>,
```

And `RiskConfig` has:
```rust
pub order_timeout_secs: Option<u64>,
```

And `SlippageConfig` has:
```rust
pub prefer_post_only: Option<bool>,
```

**Step 4: Run tests**

Run: `cargo test --workspace`
Expected: All tests pass

**Step 5: Commit**

```bash
git add crates/arb-api/src/engine_task.rs crates/arb-api/Cargo.toml crates/arb-core/src/config.rs
git commit -m "feat: wire live/paper mode toggle in engine loop"
```

---

## Task 4: EOA Pre-Flight Checks

**Files:**
- Create: `crates/arb-execution/src/preflight.rs`
- Modify: `crates/arb-execution/src/lib.rs`
- Modify: `crates/arb-api/src/engine_task.rs`

**Step 1: Write pre-flight check module**

```rust
// crates/arb-execution/src/preflight.rs
use alloy::primitives::{Address, U256};
use alloy::providers::{Provider, ProviderBuilder};
use anyhow::Result;
use tracing::{info, warn, error};

const POLYGON_RPC: &str = "https://polygon.drpc.org";
const MIN_POL_WEI: u128 = 100_000_000_000_000_000; // 0.1 POL
const USDC_E_ADDRESS: &str = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174";

pub struct PreflightResult {
    pub pol_balance_sufficient: bool,
    pub usdc_balance: U256,
    pub warnings: Vec<String>,
}

pub async fn run_preflight_checks(address: Address) -> Result<PreflightResult> {
    let provider = ProviderBuilder::new().on_http(POLYGON_RPC.parse()?);
    let mut warnings = Vec::new();

    // Check POL balance for gas
    let pol_balance = provider.get_balance(address).await?;
    let pol_sufficient = pol_balance >= U256::from(MIN_POL_WEI);
    if !pol_sufficient {
        let msg = format!("POL balance too low for gas: {} wei", pol_balance);
        error!("{}", msg);
        warnings.push(msg);
    } else {
        info!(balance = %pol_balance, "POL balance sufficient for gas");
    }

    // Check USDC.e balance
    // This is a simplified check — a full check would call the ERC-20 balanceOf
    let usdc_balance = U256::ZERO; // TODO: ERC-20 call
    info!(usdc = %usdc_balance, "USDC.e balance check (placeholder)");

    Ok(PreflightResult {
        pol_balance_sufficient: pol_sufficient,
        usdc_balance,
        warnings,
    })
}
```

**Step 2: Register module**

Add `pub mod preflight;` to `crates/arb-execution/src/lib.rs`.

**Step 3: Call pre-flight in engine startup**

In `engine_task.rs`, after creating the live executor, call:
```rust
if config.is_live() {
    let address = executor_live.address(); // add a method to expose signer address
    let preflight = arb_execution::preflight::run_preflight_checks(address).await;
    match preflight {
        Ok(result) if !result.pol_balance_sufficient => {
            error!("Pre-flight failed: insufficient POL for gas. Falling back to paper.");
            // fall back to paper
        }
        Ok(result) => {
            for warning in &result.warnings {
                warn!("{}", warning);
            }
        }
        Err(e) => warn!("Pre-flight check failed: {e}. Proceeding with caution."),
    }
}
```

**Step 4: Verify compilation**

Run: `cargo build -p arb-execution -p arb-api`

**Step 5: Commit**

```bash
git add crates/arb-execution/src/preflight.rs crates/arb-execution/src/lib.rs crates/arb-api/src/engine_task.rs
git commit -m "feat: add EOA pre-flight checks for live trading"
```

---

## Task 5: Token Bucket Rate Limiter

**Files:**
- Create: `crates/arb-execution/src/rate_limiter.rs`
- Modify: `crates/arb-execution/src/lib.rs`
- Modify: `crates/arb-execution/src/executor.rs`

**Step 1: Write failing test**

```rust
// At bottom of rate_limiter.rs
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_rate_limiter_allows_within_budget() {
        let limiter = RateLimiter::new(10, Duration::from_secs(1));
        for _ in 0..10 {
            assert!(limiter.try_acquire().await);
        }
    }

    #[tokio::test]
    async fn test_rate_limiter_blocks_over_budget() {
        let limiter = RateLimiter::new(2, Duration::from_secs(10));
        assert!(limiter.try_acquire().await);
        assert!(limiter.try_acquire().await);
        assert!(!limiter.try_acquire().await); // exhausted
    }
}
```

**Step 2: Implement rate limiter**

```rust
// crates/arb-execution/src/rate_limiter.rs
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Token bucket rate limiter
pub struct RateLimiter {
    inner: Arc<Mutex<TokenBucket>>,
}

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(max_tokens: u32, window: Duration) -> Self {
        let max = max_tokens as f64;
        Self {
            inner: Arc::new(Mutex::new(TokenBucket {
                tokens: max,
                max_tokens: max,
                refill_rate: max / window.as_secs_f64(),
                last_refill: Instant::now(),
            })),
        }
    }

    pub async fn try_acquire(&self) -> bool {
        let mut bucket = self.inner.lock().await;
        bucket.refill();
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Wait until a token is available, then acquire it
    pub async fn acquire(&self) {
        loop {
            if self.try_acquire().await {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    pub async fn tokens_remaining(&self) -> u32 {
        let mut bucket = self.inner.lock().await;
        bucket.refill();
        bucket.tokens as u32
    }
}

impl TokenBucket {
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }
}

impl Clone for RateLimiter {
    fn clone(&self) -> Self {
        Self { inner: Arc::clone(&self.inner) }
    }
}

/// Pre-configured rate limiters for Polymarket API endpoints
pub struct ApiRateLimiters {
    pub orders: RateLimiter,     // 500/10s burst
    pub cancels: RateLimiter,    // 500/10s burst
    pub data: RateLimiter,       // 1000/10s
    pub gamma: RateLimiter,      // 300/10s
}

impl ApiRateLimiters {
    pub fn new() -> Self {
        Self {
            orders: RateLimiter::new(500, Duration::from_secs(10)),
            cancels: RateLimiter::new(500, Duration::from_secs(10)),
            data: RateLimiter::new(1000, Duration::from_secs(10)),
            gamma: RateLimiter::new(300, Duration::from_secs(10)),
        }
    }
}

impl Default for ApiRateLimiters {
    fn default() -> Self { Self::new() }
}
```

**Step 3: Register module and run tests**

Add `pub mod rate_limiter;` to `lib.rs`.

Run: `cargo test -p arb-execution rate_limiter`
Expected: Both tests pass

**Step 4: Wire rate limiter into LiveTradeExecutor**

Add `rate_limiters: ApiRateLimiters` field to `LiveTradeExecutor`. Call `self.rate_limiters.orders.acquire().await` before every `post_order` call in `execute_leg()`.

**Step 5: Commit**

```bash
git add crates/arb-execution/src/rate_limiter.rs crates/arb-execution/src/lib.rs crates/arb-execution/src/executor.rs
git commit -m "feat: add token bucket rate limiter for Polymarket API"
```

---

## Task 6: Execution Timeout Enforcement

**Files:**
- Modify: `crates/arb-execution/src/executor.rs`

**Step 1: Wrap order placement with timeout**

In `execute_leg()`, wrap the order placement block with `tokio::time::timeout`:

```rust
use tokio::time::timeout;
use std::time::Duration;

// Wrap the build+sign+post sequence:
let timeout_dur = Duration::from_secs(self.order_timeout_secs);
let result = timeout(timeout_dur, async {
    let order = self.clob_client.limit_order()/* ... */.build().await?;
    let signed = self.clob_client.sign(&self.signer, order).await?;
    self.clob_client.post_order(signed).await
}).await;

match result {
    Ok(Ok(response)) => { /* map to LegReport */ }
    Ok(Err(e)) => {
        warn!(token_id = %leg.token_id, error = %e, "Order placement failed");
        return Ok(LegReport { status: FillStatus::Rejected, /* ... */ });
    }
    Err(_) => {
        warn!(token_id = %leg.token_id, timeout_secs = self.order_timeout_secs, "Order timed out");
        return Ok(LegReport { status: FillStatus::Cancelled, /* ... */ });
    }
}
```

**Step 2: Verify compilation**

Run: `cargo build -p arb-execution`

**Step 3: Commit**

```bash
git add crates/arb-execution/src/executor.rs
git commit -m "feat: enforce execution timeout on live orders"
```

---

## Task 7: WebSocket Dependencies

**Files:**
- Modify: `crates/arb-data/Cargo.toml`
- Modify: root `Cargo.toml` (workspace dependencies)

**Step 1: Add WebSocket dependencies to workspace**

In root `Cargo.toml` under `[workspace.dependencies]`, add:
```toml
tokio-tungstenite = { version = "0.26", features = ["native-tls"] }
futures-util = "0.3"
url = "2"
```

**Step 2: Add to arb-data**

In `crates/arb-data/Cargo.toml` under `[dependencies]`:
```toml
tokio-tungstenite = { workspace = true }
futures-util = { workspace = true }
url = { workspace = true }
```

**Step 3: Verify workspace compiles**

Run: `cargo build -p arb-data`

**Step 4: Commit**

```bash
git add Cargo.toml Cargo.lock crates/arb-data/Cargo.toml
git commit -m "deps: add tokio-tungstenite for WebSocket market data"
```

---

## Task 8: Local Order Book Data Structure

**Files:**
- Create: `crates/arb-data/src/local_book.rs`
- Modify: `crates/arb-data/src/lib.rs`

**Step 1: Write tests first**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_empty_book_best_prices() {
        let book = LocalOrderBook::new("token1".into());
        assert!(book.best_bid().is_none());
        assert!(book.best_ask().is_none());
    }

    #[test]
    fn test_add_bid_ask_levels() {
        let mut book = LocalOrderBook::new("token1".into());
        book.update_bid(dec!(0.45), dec!(100.0));
        book.update_bid(dec!(0.44), dec!(200.0));
        book.update_ask(dec!(0.55), dec!(150.0));
        book.update_ask(dec!(0.56), dec!(50.0));

        assert_eq!(book.best_bid(), Some((dec!(0.45), dec!(100.0))));
        assert_eq!(book.best_ask(), Some((dec!(0.55), dec!(150.0))));
    }

    #[test]
    fn test_remove_level_on_zero_size() {
        let mut book = LocalOrderBook::new("token1".into());
        book.update_bid(dec!(0.45), dec!(100.0));
        book.update_bid(dec!(0.45), dec!(0.0)); // should remove
        assert!(book.best_bid().is_none());
    }

    #[test]
    fn test_vwap_buy() {
        let mut book = LocalOrderBook::new("token1".into());
        book.update_ask(dec!(0.50), dec!(100.0));
        book.update_ask(dec!(0.51), dec!(100.0));

        // Buy 150 shares: 100 @ 0.50 + 50 @ 0.51 = 75.50/150 = 0.5033..
        let vwap = book.calculate_vwap(Side::Buy, dec!(150.0));
        assert!(vwap.is_some());
        let v = vwap.unwrap();
        assert!(v > dec!(0.50) && v < dec!(0.51));
    }

    #[test]
    fn test_insufficient_liquidity() {
        let mut book = LocalOrderBook::new("token1".into());
        book.update_ask(dec!(0.50), dec!(10.0));
        let vwap = book.calculate_vwap(Side::Buy, dec!(100.0));
        assert!(vwap.is_none());
    }
}
```

**Step 2: Implement LocalOrderBook**

```rust
// crates/arb-data/src/local_book.rs
use std::collections::BTreeMap;
use std::time::Instant;
use dashmap::DashMap;
use rust_decimal::Decimal;
use std::cmp::Reverse;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

pub struct LocalOrderBook {
    pub token_id: String,
    bids: BTreeMap<Reverse<Decimal>, Decimal>, // highest first
    asks: BTreeMap<Decimal, Decimal>,           // lowest first
    pub last_updated: Instant,
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

    pub fn update_bid(&mut self, price: Decimal, size: Decimal) {
        if size.is_zero() {
            self.bids.remove(&Reverse(price));
        } else {
            self.bids.insert(Reverse(price), size);
        }
        self.last_updated = Instant::now();
    }

    pub fn update_ask(&mut self, price: Decimal, size: Decimal) {
        if size.is_zero() {
            self.asks.remove(&price);
        } else {
            self.asks.insert(price, size);
        }
        self.last_updated = Instant::now();
    }

    pub fn best_bid(&self) -> Option<(Decimal, Decimal)> {
        self.bids.iter().next().map(|(Reverse(p), s)| (*p, *s))
    }

    pub fn best_ask(&self) -> Option<(Decimal, Decimal)> {
        self.asks.iter().next().map(|(p, s)| (*p, *s))
    }

    pub fn spread(&self) -> Option<Decimal> {
        match (self.best_bid(), self.best_ask()) {
            (Some((bid, _)), Some((ask, _))) => Some(ask - bid),
            _ => None,
        }
    }

    pub fn is_stale(&self, max_age: std::time::Duration) -> bool {
        self.last_updated.elapsed() > max_age
    }

    /// Calculate VWAP for a given side and size
    pub fn calculate_vwap(&self, side: Side, target_size: Decimal) -> Option<Decimal> {
        let mut remaining = target_size;
        let mut total_cost = Decimal::ZERO;

        let levels: Vec<(Decimal, Decimal)> = match side {
            Side::Buy => self.asks.iter().map(|(p, s)| (*p, *s)).collect(),
            Side::Sell => self.bids.iter().map(|(Reverse(p), s)| (*p, *s)).collect(),
        };

        for (price, size) in levels {
            let fill = remaining.min(size);
            total_cost += fill * price;
            remaining -= fill;
            if remaining.is_zero() {
                break;
            }
        }

        if remaining.is_zero() {
            Some(total_cost / target_size)
        } else {
            None // insufficient liquidity
        }
    }

    /// Apply a full snapshot (replaces all levels)
    pub fn apply_snapshot(&mut self, bids: Vec<(Decimal, Decimal)>, asks: Vec<(Decimal, Decimal)>) {
        self.bids.clear();
        self.asks.clear();
        for (price, size) in bids {
            if !size.is_zero() {
                self.bids.insert(Reverse(price), size);
            }
        }
        for (price, size) in asks {
            if !size.is_zero() {
                self.asks.insert(price, size);
            }
        }
        self.last_updated = Instant::now();
    }
}

/// Thread-safe store of all local order books
pub struct OrderBookStore {
    books: DashMap<String, Arc<RwLock<LocalOrderBook>>>,
}

impl OrderBookStore {
    pub fn new() -> Self {
        Self { books: DashMap::new() }
    }

    pub fn get_or_create(&self, token_id: &str) -> Arc<RwLock<LocalOrderBook>> {
        self.books
            .entry(token_id.to_string())
            .or_insert_with(|| Arc::new(RwLock::new(LocalOrderBook::new(token_id.to_string()))))
            .clone()
    }

    pub fn get(&self, token_id: &str) -> Option<Arc<RwLock<LocalOrderBook>>> {
        self.books.get(token_id).map(|r| r.clone())
    }

    pub fn token_count(&self) -> usize {
        self.books.len()
    }
}

impl Default for OrderBookStore {
    fn default() -> Self { Self::new() }
}
```

**Step 3: Register module, run tests**

Add `pub mod local_book;` to `crates/arb-data/src/lib.rs`.

Run: `cargo test -p arb-data local_book`
Expected: All 5 tests pass

**Step 4: Commit**

```bash
git add crates/arb-data/src/local_book.rs crates/arb-data/src/lib.rs
git commit -m "feat: add local order book data structure with VWAP"
```

---

## Task 9: WebSocket Market Data Client

**Files:**
- Create: `crates/arb-data/src/ws_feed.rs`
- Modify: `crates/arb-data/src/lib.rs`

**Step 1: Implement WebSocket client**

This is a large module. Key structure:

```rust
// crates/arb-data/src/ws_feed.rs
use crate::local_book::{OrderBookStore, Side};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tracing::{debug, error, info, warn};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);
const HEALTH_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Serialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    assets_ids: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    market: Option<String>,
    // Book snapshot fields
    bids: Option<Vec<WsLevel>>,
    asks: Option<Vec<WsLevel>>,
    // Price change fields
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
}

#[derive(Debug, Deserialize)]
struct WsLevel {
    price: String,
    size: String,
}

/// Notification sent when a book changes
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub token_id: String,
}

pub struct WsFeedClient {
    book_store: Arc<OrderBookStore>,
    update_tx: mpsc::Sender<BookUpdate>,
}

impl WsFeedClient {
    pub fn new(
        book_store: Arc<OrderBookStore>,
        update_tx: mpsc::Sender<BookUpdate>,
    ) -> Self {
        Self { book_store, update_tx }
    }

    /// Spawn the WebSocket connection loop as a background task.
    /// Returns a channel sender for dynamically subscribing to token IDs.
    pub fn spawn(
        self,
        initial_tokens: Vec<String>,
    ) -> mpsc::Sender<Vec<String>> {
        let (sub_tx, mut sub_rx) = mpsc::channel::<Vec<String>>(32);
        let book_store = self.book_store;
        let update_tx = self.update_tx;

        tokio::spawn(async move {
            let mut subscribed_tokens = initial_tokens;
            let mut retry_count = 0u32;

            loop {
                match Self::connect_and_run(
                    &book_store,
                    &update_tx,
                    &mut sub_rx,
                    &mut subscribed_tokens,
                ).await {
                    Ok(()) => {
                        info!("WebSocket connection closed cleanly");
                        retry_count = 0;
                    }
                    Err(e) => {
                        warn!(error = %e, retry = retry_count, "WebSocket connection error");
                    }
                }

                // Exponential backoff
                let delay = RECONNECT_BASE_DELAY * 2u32.pow(retry_count.min(4));
                let delay = delay.min(RECONNECT_MAX_DELAY);
                info!(delay_ms = delay.as_millis(), "Reconnecting WebSocket...");
                tokio::time::sleep(delay).await;
                retry_count += 1;
            }
        });

        sub_tx
    }

    async fn connect_and_run(
        book_store: &OrderBookStore,
        update_tx: &mpsc::Sender<BookUpdate>,
        sub_rx: &mut mpsc::Receiver<Vec<String>>,
        subscribed_tokens: &mut Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(WS_URL).await?;
        let (mut write, mut read) = ws_stream.split();
        info!(tokens = subscribed_tokens.len(), "WebSocket connected");

        // Subscribe to initial tokens
        if !subscribed_tokens.is_empty() {
            let msg = SubscribeMessage {
                msg_type: "subscribe".into(),
                assets_ids: subscribed_tokens.clone(),
            };
            write.send(tokio_tungstenite::tungstenite::Message::Text(
                serde_json::to_string(&msg)?.into()
            )).await?;
        }

        loop {
            tokio::select! {
                // Handle incoming WebSocket messages
                msg = read.next() => {
                    match msg {
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Text(text))) => {
                            if let Err(e) = Self::handle_message(
                                &text, book_store, update_tx
                            ).await {
                                debug!(error = %e, "Failed to handle WS message");
                            }
                        }
                        Some(Ok(tokio_tungstenite::tungstenite::Message::Ping(data))) => {
                            let _ = write.send(
                                tokio_tungstenite::tungstenite::Message::Pong(data)
                            ).await;
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => return Ok(()), // stream ended
                        _ => {}
                    }
                }
                // Handle subscription changes
                new_tokens = sub_rx.recv() => {
                    if let Some(tokens) = new_tokens {
                        let msg = SubscribeMessage {
                            msg_type: "subscribe".into(),
                            assets_ids: tokens.clone(),
                        };
                        write.send(tokio_tungstenite::tungstenite::Message::Text(
                            serde_json::to_string(&msg)?.into()
                        )).await?;
                        subscribed_tokens.extend(tokens);
                    }
                }
            }
        }
    }

    async fn handle_message(
        text: &str,
        book_store: &OrderBookStore,
        update_tx: &mpsc::Sender<BookUpdate>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg: WsMessage = serde_json::from_str(text)?;

        match msg.msg_type.as_deref() {
            Some("book") => {
                if let Some(token_id) = &msg.market {
                    let book_lock = book_store.get_or_create(token_id);
                    let mut book = book_lock.write().await;

                    let bids: Vec<(Decimal, Decimal)> = msg.bids.unwrap_or_default()
                        .iter()
                        .filter_map(|l| Some((l.price.parse().ok()?, l.size.parse().ok()?)))
                        .collect();
                    let asks: Vec<(Decimal, Decimal)> = msg.asks.unwrap_or_default()
                        .iter()
                        .filter_map(|l| Some((l.price.parse().ok()?, l.size.parse().ok()?)))
                        .collect();

                    book.apply_snapshot(bids, asks);
                    let _ = update_tx.send(BookUpdate { token_id: token_id.clone() }).await;
                }
            }
            Some("price_change") => {
                if let (Some(token_id), Some(price_str), Some(size_str), Some(side_str)) =
                    (&msg.market, &msg.price, &msg.size, &msg.side) {
                    if let (Ok(price), Ok(size)) = (price_str.parse::<Decimal>(), size_str.parse::<Decimal>()) {
                        let book_lock = book_store.get_or_create(token_id);
                        let mut book = book_lock.write().await;

                        match side_str.as_str() {
                            "BUY" | "buy" => book.update_bid(price, size),
                            "SELL" | "sell" => book.update_ask(price, size),
                            _ => {}
                        }
                        let _ = update_tx.send(BookUpdate { token_id: token_id.clone() }).await;
                    }
                }
            }
            _ => { /* ignore unknown message types */ }
        }
        Ok(())
    }
}
```

**Step 2: Register module**

Add `pub mod ws_feed;` to `crates/arb-data/src/lib.rs`.

**Step 3: Verify compilation**

Run: `cargo build -p arb-data`

**Step 4: Commit**

```bash
git add crates/arb-data/src/ws_feed.rs crates/arb-data/src/lib.rs
git commit -m "feat: add WebSocket market data client with reconnection"
```

---

## Task 10: Wire WebSocket into Engine Loop

**Files:**
- Modify: `crates/arb-api/src/engine_task.rs`

**Step 1: Integrate WebSocket feed alongside existing REST polling**

In the engine setup phase (before the main loop):

```rust
// After Phase 2 (orderbook fetch), spawn WebSocket for hot-tier tokens
let book_store = Arc::new(arb_data::local_book::OrderBookStore::new());
let (update_tx, mut update_rx) = tokio::sync::mpsc::channel::<arb_data::ws_feed::BookUpdate>(1000);

// Get hot-tier token IDs for WebSocket subscription
let hot_tokens: Vec<String> = classified.all_token_ids.iter()
    .take(200) // cap initial WS subscriptions
    .cloned()
    .collect();

let ws_client = arb_data::ws_feed::WsFeedClient::new(
    Arc::clone(&book_store),
    update_tx,
);
let ws_sub_tx = ws_client.spawn(hot_tokens);
info!(count = classified.all_token_ids.len(), "WebSocket feed spawned");
```

In the main loop, add a branch to process WebSocket updates:

```rust
// Inside the main loop, add a non-blocking drain of WS updates
let mut ws_updates = Vec::new();
while let Ok(update) = update_rx.try_recv() {
    ws_updates.push(update);
}
if !ws_updates.is_empty() {
    debug!(count = ws_updates.len(), "Processed WebSocket book updates");
    // These updates already applied to book_store;
    // mark affected markets as changed for detector scan
}
```

**Step 2: Verify compilation**

Run: `cargo build -p arb-api`

**Step 3: Commit**

```bash
git add crates/arb-api/src/engine_task.rs
git commit -m "feat: wire WebSocket feed into engine loop alongside REST polling"
```

---

## Task 11: Circuit Breakers

**Files:**
- Create: `crates/arb-risk/src/circuit_breaker.rs`
- Modify: `crates/arb-risk/src/lib.rs`

**Step 1: Write tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn test_no_trigger_under_threshold() {
        let config = CircuitBreakerConfig::default();
        let mut cb = CircuitBreaker::new(config);
        cb.record_pnl(dec!(-10.0));
        assert!(cb.check().is_none());
    }

    #[test]
    fn test_daily_loss_trigger() {
        let config = CircuitBreakerConfig { daily_loss_limit: dec!(100.0), ..Default::default() };
        let mut cb = CircuitBreaker::new(config);
        cb.record_pnl(dec!(-150.0));
        let trigger = cb.check();
        assert!(trigger.is_some());
        assert!(trigger.unwrap().contains("daily loss"));
    }

    #[test]
    fn test_error_rate_trigger() {
        let config = CircuitBreakerConfig { max_error_rate: 0.5, ..Default::default() };
        let mut cb = CircuitBreaker::new(config);
        for _ in 0..10 { cb.record_api_error(); }
        for _ in 0..5 { cb.record_api_success(); }
        let trigger = cb.check();
        assert!(trigger.is_some());
    }
}
```

**Step 2: Implement circuit breaker**

```rust
// crates/arb-risk/src/circuit_breaker.rs
use rust_decimal::Decimal;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct CircuitBreakerConfig {
    pub daily_loss_limit: Decimal,
    pub max_error_rate: f64,         // e.g., 0.5 = 50%
    pub error_window: Duration,      // window for error rate calculation
    pub max_latency_ms: u64,         // e.g., 500
    pub latency_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            daily_loss_limit: Decimal::from(1000),
            max_error_rate: 0.5,
            error_window: Duration::from_secs(60),
            max_latency_ms: 500,
            latency_window: Duration::from_secs(60),
        }
    }
}

pub struct CircuitBreaker {
    config: CircuitBreakerConfig,
    daily_pnl: Decimal,
    api_results: VecDeque<(Instant, bool)>,  // (timestamp, success)
    latencies: VecDeque<(Instant, u64)>,     // (timestamp, ms)
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            daily_pnl: Decimal::ZERO,
            api_results: VecDeque::new(),
            latencies: VecDeque::new(),
        }
    }

    pub fn record_pnl(&mut self, pnl: Decimal) {
        self.daily_pnl = pnl;
    }

    pub fn record_api_success(&mut self) {
        self.api_results.push_back((Instant::now(), true));
        self.prune_api_results();
    }

    pub fn record_api_error(&mut self) {
        self.api_results.push_back((Instant::now(), false));
        self.prune_api_results();
    }

    pub fn record_latency(&mut self, ms: u64) {
        self.latencies.push_back((Instant::now(), ms));
        self.prune_latencies();
    }

    /// Returns Some(reason) if a circuit breaker should trip
    pub fn check(&self) -> Option<String> {
        // Check daily loss
        if self.daily_pnl < -self.config.daily_loss_limit {
            return Some(format!(
                "Circuit breaker: daily loss {} exceeds limit {}",
                self.daily_pnl, self.config.daily_loss_limit
            ));
        }

        // Check API error rate
        if self.api_results.len() >= 10 {
            let errors = self.api_results.iter().filter(|(_, ok)| !ok).count();
            let rate = errors as f64 / self.api_results.len() as f64;
            if rate > self.config.max_error_rate {
                return Some(format!(
                    "Circuit breaker: API error rate {:.0}% exceeds {:.0}% threshold",
                    rate * 100.0, self.config.max_error_rate * 100.0
                ));
            }
        }

        // Check sustained high latency
        if self.latencies.len() >= 5 {
            let high = self.latencies.iter().filter(|(_, ms)| *ms > self.config.max_latency_ms).count();
            if high == self.latencies.len() {
                return Some(format!(
                    "Circuit breaker: all recent latencies exceed {}ms",
                    self.config.max_latency_ms
                ));
            }
        }

        None
    }

    pub fn reset_daily(&mut self) {
        self.daily_pnl = Decimal::ZERO;
    }

    fn prune_api_results(&mut self) {
        let cutoff = Instant::now() - self.config.error_window;
        while self.api_results.front().is_some_and(|(t, _)| *t < cutoff) {
            self.api_results.pop_front();
        }
    }

    fn prune_latencies(&mut self) {
        let cutoff = Instant::now() - self.config.latency_window;
        while self.latencies.front().is_some_and(|(t, _)| *t < cutoff) {
            self.latencies.pop_front();
        }
    }
}
```

**Step 3: Register, test, commit**

Add `pub mod circuit_breaker;` to `crates/arb-risk/src/lib.rs`.

Run: `cargo test -p arb-risk circuit_breaker`

```bash
git add crates/arb-risk/src/circuit_breaker.rs crates/arb-risk/src/lib.rs
git commit -m "feat: add circuit breaker with daily loss, error rate, and latency triggers"
```

---

## Task 12: Position Write-Ahead Log

**Files:**
- Modify: `crates/arb-risk/src/position_tracker.rs`

**Step 1: Add WAL append on every fill**

Add a method to PositionTracker that appends fills to a WAL file:

```rust
/// Append a fill to the write-ahead log for crash recovery
fn append_wal(&self, leg: &LegReport, timestamp: DateTime<Utc>) {
    if let Some(wal_path) = &self.wal_path {
        let entry = serde_json::json!({
            "token_id": leg.token_id,
            "side": leg.side,
            "price": leg.actual_fill_price.to_string(),
            "size": leg.filled_size.to_string(),
            "ts": timestamp.to_rfc3339(),
        });
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true).append(true).open(wal_path)
        {
            use std::io::Write;
            let _ = writeln!(file, "{}", entry);
        }
    }
}
```

Call `self.append_wal(leg, Utc::now())` after every successful fill in `record_fill()`.

**Step 2: Add WAL replay on startup**

```rust
pub fn replay_wal(wal_path: &Path) -> Vec<FillRecord> {
    // Read each line, parse JSON, return fill records for position reconstruction
}
```

**Step 3: Commit**

```bash
git add crates/arb-risk/src/position_tracker.rs
git commit -m "feat: add write-ahead log for position persistence"
```

---

## Task 13: Discord/Telegram Webhook Alerting

**Files:**
- Create: `crates/arb-monitor/src/webhook.rs`
- Modify: `crates/arb-monitor/src/lib.rs`
- Modify: `crates/arb-core/src/config.rs`

**Step 1: Add webhook config**

In `AlertsConfig` in `crates/arb-core/src/config.rs`:
```rust
pub discord_webhook_url: Option<String>,
pub telegram_bot_token: Option<String>,
pub telegram_chat_id: Option<String>,
```

**Step 2: Implement webhook sender**

```rust
// crates/arb-monitor/src/webhook.rs
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AlertLevel {
    Critical,
    Warning,
    Info,
}

pub struct WebhookAlerter {
    discord_url: Option<String>,
    telegram_bot_token: Option<String>,
    telegram_chat_id: Option<String>,
    http_client: reqwest::Client,
    last_sent: Mutex<HashMap<String, Instant>>,  // category -> last send time
    cooldown: Duration,
}

impl WebhookAlerter {
    pub fn new(
        discord_url: Option<String>,
        telegram_bot_token: Option<String>,
        telegram_chat_id: Option<String>,
    ) -> Self {
        Self {
            discord_url,
            telegram_bot_token,
            telegram_chat_id,
            http_client: reqwest::Client::new(),
            last_sent: Mutex::new(HashMap::new()),
            cooldown: Duration::from_secs(60),
        }
    }

    pub async fn send(&self, level: AlertLevel, category: &str, message: &str) {
        // Rate limit: max 1 per category per minute
        {
            let mut last = self.last_sent.lock().unwrap();
            if let Some(t) = last.get(category) {
                if t.elapsed() < self.cooldown {
                    return;
                }
            }
            last.insert(category.to_string(), Instant::now());
        }

        let prefix = match level {
            AlertLevel::Critical => "[CRITICAL]",
            AlertLevel::Warning => "[WARNING]",
            AlertLevel::Info => "[INFO]",
        };
        let full_msg = format!("{} {}: {}", prefix, category, message);

        if let Some(url) = &self.discord_url {
            let payload = serde_json::json!({ "content": full_msg });
            let _ = self.http_client.post(url).json(&payload).send().await;
        }

        if let (Some(token), Some(chat_id)) = (&self.telegram_bot_token, &self.telegram_chat_id) {
            let url = format!("https://api.telegram.org/bot{}/sendMessage", token);
            let payload = serde_json::json!({
                "chat_id": chat_id,
                "text": full_msg,
            });
            let _ = self.http_client.post(&url).json(&payload).send().await;
        }
    }

    pub fn is_configured(&self) -> bool {
        self.discord_url.is_some()
            || (self.telegram_bot_token.is_some() && self.telegram_chat_id.is_some())
    }
}
```

**Step 3: Register, commit**

Add `pub mod webhook;` to `crates/arb-monitor/src/lib.rs`.

```bash
git add crates/arb-monitor/src/webhook.rs crates/arb-monitor/src/lib.rs crates/arb-core/src/config.rs
git commit -m "feat: add Discord/Telegram webhook alerting with rate limiting"
```

---

## Task 14: Health Check Endpoint

**Files:**
- Create: `crates/arb-api/src/routes/health.rs`
- Modify: `crates/arb-api/src/routes/mod.rs`
- Modify: `crates/arb-api/src/main.rs` (add route)

**Step 1: Implement health check handler**

```rust
// crates/arb-api/src/routes/health.rs
use axum::{extract::State, Json};
use serde::Serialize;
use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub uptime_secs: u64,
    pub markets_loaded: usize,
    pub kill_switch_active: bool,
    pub last_cycle_secs_ago: Option<u64>,
    pub warnings: Vec<String>,
}

pub async fn health_check(State(state): State<AppState>) -> Json<HealthStatus> {
    let mut warnings = Vec::new();
    let kill = state.kill_switch_active.load(std::sync::atomic::Ordering::Relaxed);
    let markets = state.market_cache.len();
    let uptime = state.start_time.elapsed().as_secs();

    if kill { warnings.push("Kill switch is active".into()); }
    if markets == 0 { warnings.push("No markets loaded".into()); }

    Json(HealthStatus {
        healthy: !kill && markets > 0,
        uptime_secs: uptime,
        markets_loaded: markets,
        kill_switch_active: kill,
        last_cycle_secs_ago: None, // TODO: track in AppState
        warnings,
    })
}
```

**Step 2: Register route**

In router setup: `.route("/api/health", get(health::health_check))`

**Step 3: Commit**

```bash
git add crates/arb-api/src/routes/health.rs crates/arb-api/src/routes/mod.rs crates/arb-api/src/main.rs
git commit -m "feat: add /api/health endpoint for monitoring"
```

---

## Task 15: Structured JSON Logging

**Files:**
- Modify: `crates/arb-api/src/main.rs`

**Step 1: Replace default tracing subscriber with JSON output**

```rust
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

tracing_subscriber::registry()
    .with(EnvFilter::from_default_env().add_directive("arb=info".parse().unwrap()))
    .with(fmt::layer().json())
    .init();
```

**Step 2: Commit**

```bash
git add crates/arb-api/src/main.rs
git commit -m "feat: switch to structured JSON logging"
```

---

## Task 16: Systemd Service File

**Files:**
- Create: `deploy/arb-engine.service`

**Step 1: Write systemd unit**

```ini
[Unit]
Description=Polymarket Arbitrage Engine
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=arb
WorkingDirectory=/opt/arb
ExecStart=/opt/arb/arb-api
Restart=always
RestartSec=10
Environment=RUST_LOG=arb=info
Environment=POLYMARKET_KEY_FILE=/opt/arb/key.txt

# Security hardening
NoNewPrivileges=yes
ProtectSystem=strict
ProtectHome=yes
ReadWritePaths=/opt/arb

[Install]
WantedBy=multi-user.target
```

**Step 2: Commit**

```bash
git add deploy/arb-engine.service
git commit -m "ops: add systemd service file for VPS deployment"
```

---

## Task 17: Wire Stress Test Parameters from Frontend

**Files:**
- Modify: `crates/arb-api/src/routes/stress.rs`

**Step 1: Parse frontend params instead of using hardcoded values**

The `_params` field in the stress test handler currently has a leading underscore. Remove it and use the actual values from the frontend sliders to construct the `StressScenario` variants with the user-specified parameters.

**Step 2: Commit**

```bash
git add crates/arb-api/src/routes/stress.rs
git commit -m "feat: wire stress test parameters from frontend sliders"
```

---

## Task 18: Per-Token Volatility from Price History

**Files:**
- Modify: `crates/arb-data/src/price_history.rs`
- Modify: `crates/arb-risk/src/stress_test.rs`

**Step 1: Add volatility calculation to PriceHistoryStore**

```rust
/// Calculate realized volatility (std of log returns) for a token
pub fn realized_volatility(&self, condition_id: &str, days: u32) -> Option<f64> {
    let since = Utc::now() - chrono::Duration::days(days as i64);
    let ticks = self.get_history(condition_id, since, Utc::now()).ok()?;

    if ticks.len() < 20 { return None; }

    let prices: Vec<f64> = ticks.iter()
        .map(|t| t.price.to_f64().unwrap_or(0.0))
        .filter(|p| *p > 0.0)
        .collect();

    if prices.len() < 2 { return None; }

    let log_returns: Vec<f64> = prices.windows(2)
        .map(|w| (w[1] / w[0]).ln())
        .collect();

    let mean = log_returns.iter().sum::<f64>() / log_returns.len() as f64;
    let variance = log_returns.iter()
        .map(|r| (r - mean).powi(2))
        .sum::<f64>() / (log_returns.len() - 1) as f64;

    Some(variance.sqrt())
}
```

**Step 2: Use in stress test instead of hardcoded 10%**

Replace hardcoded `0.10` in `stress_test.rs` with a call to fetch realized vol, falling back to `0.10` if insufficient history.

**Step 3: Commit**

```bash
git add crates/arb-data/src/price_history.rs crates/arb-risk/src/stress_test.rs
git commit -m "feat: compute per-token realized volatility from price history"
```

---

## Summary

| Task | Phase | Description | Dependencies |
|------|-------|-------------|--------------|
| 1 | P1 | Store LocalSigner in executor | None |
| 2 | P1 | Wire real order placement | Task 1 |
| 3 | P1 | Live/paper mode toggle in engine | Task 2 |
| 4 | P1 | EOA pre-flight checks | Task 3 |
| 5 | P1 | Token bucket rate limiter | None |
| 6 | P1 | Execution timeout | Task 2 |
| 7 | P2 | WebSocket dependencies | None |
| 8 | P2 | Local order book data structure | None |
| 9 | P2 | WebSocket market data client | Tasks 7, 8 |
| 10 | P2 | Wire WebSocket into engine | Task 9 |
| 11 | P6 | Circuit breakers | None |
| 12 | P6 | Position write-ahead log | None |
| 13 | P7 | Discord/Telegram alerting | None |
| 14 | P7 | Health check endpoint | None |
| 15 | P7 | Structured JSON logging | None |
| 16 | P7 | Systemd service file | None |
| 17 | P6 | Wire stress test params | None |
| 18 | P6 | Per-token volatility | None |

**Parallelizable groups:**
- Tasks 1-6 (Phase 1) are mostly sequential
- Tasks 7-8 can run in parallel, then 9-10 sequential
- Tasks 11-12, 13-16, 17-18 are all independent and can run in parallel
