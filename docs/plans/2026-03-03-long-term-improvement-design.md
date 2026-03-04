# Long-Term Improvement Plan: Polymarket Arbitrage System

**Date:** 2026-03-03
**Goal:** Full-stack incremental improvement roadmap, optimized for revenue generation
**Capital Scale:** <$5K initial, EOA wallet
**Execution Order:** P1 → P2 → P6 → P7 → P3 → P4 → P5 → P9 → P8

---

## Context

The `trading_system_design_investigation.txt` describes an institutional-grade HFT arbitrage system covering WebSocket order book mirroring, MEV auction protocols, colocated infrastructure, and cross-exchange arbitrage. The current system has:

- 9 Rust crates, 228+ tests, all compiling
- 4 arbitrage strategies (intra-market, cross-market, multi-outcome, deadline monotonicity)
- Full simulation suite (MC, jump diffusion, particle filter, copula, ABM, importance sampling)
- Risk management (VaR, stress testing, Kelly criterion, kill switch, position tracking)
- Paper trading with auto-execution
- Frontend dashboard (9 pages, Zustand + WebSocket)

**Critical gaps:** Live executor is a stub. Data ingestion is REST-only (5s polling). No rate limiting. No deployment infrastructure.

---

## Phase 1: Critical Path to Live Trading (P0, ~3-4 days)

### 1.1 Wire Live Executor
- Store `LocalSigner` in `LiveTradeExecutor` alongside authenticated CLOB client
- Replace stub in `execute_leg()` with: `limit_order().build() → sign(&signer, order) → post_order(signed)`
- Wire order type selection (remove `_` prefix, pass to builder)
- Parse `OrderResponse` into `FillStatus::FullyFilled` / `PartiallyFilled` / `Rejected`

### 1.2 Mode Toggle
- Add `trading_mode: "paper" | "live"` to `ArbConfig`
- Live mode requires `key_file` in config; refuse to start without it
- Default to paper; require explicit `--live` flag

### 1.3 EOA Gas Management
- Pre-flight: verify POL balance sufficient for ~100 transactions
- Pre-flight: verify USDC.e balance matches expected bankroll
- Pre-flight: verify ERC-20 approvals to Exchange, NegRisk, NegRiskAdapter contracts
- Refuse live mode if approvals missing

### 1.4 Rate Limiter
- Token bucket rate limiter in `arb-execution`
- Per-endpoint configs: Orders (500/10s burst, 3000/10min sustained), Cancels (500/10s), Data (1000/10s), Gamma (300-500/10s)
- Pre-check before every SDK call; delay (not drop) when budget exhausted
- Track request counts in metrics

### 1.5 Execution Timeout & Leg Failure
- Enforce `order_timeout_secs` (currently stored but ignored)
- FOK rejection: mark leg failed, skip remaining legs
- GTC: cancel-on-timeout if order doesn't fill within threshold

---

## Phase 2: Real-Time Data Infrastructure (P0, ~7-8 days)

### 2.1 WebSocket Market Data Client
- New module `arb-data/src/ws_feed.rs`
- Connect to `wss://ws-subscriptions-clob.polymarket.com/ws/market`
- Handle: `book` (snapshot), `price_change` (delta), `last_trade_price`
- Dynamic subscribe/unsubscribe as markets move between tiers
- Sequence validation: detect gaps, invalidate book, request fresh snapshot

### 2.2 Local Order Book
- New module `arb-data/src/local_book.rs`
- `BTreeMap<Decimal, Decimal>` per side per token (cache-friendly, O(log n))
- Thread-safe: `DashMap<String, Arc<RwLock<LocalOrderBook>>>`
- VWAP-on-demand by walking local book levels
- Staleness tracking: mark stale if no updates for >30s, trigger REST fallback

### 2.3 Hybrid Polling + WebSocket
- Hot tier (~50-100 markets): WebSocket-driven real-time
- Warm/Cold tier: Continue REST polling at tiered intervals
- Auto-promotion/demotion by 24h volume thresholds
- Fallback to REST on WebSocket disconnect

### 2.4 Event-Driven Strategy Triggering
- WebSocket book updates push via `tokio::mpsc` channel
- Strategy detectors wake only on relevant book changes (sub-second response)
- Extend `MarketCache` generation counter to per-token-ID granularity

### 2.5 WebSocket Reconnection Hardening
- Exponential backoff: 1s → 2s → 4s → ... → 30s cap
- Fresh snapshot on reconnect before resuming deltas
- Ping/pong health monitoring (force reconnect if silent >60s)
- Metrics: reconnection count, downtime, message processing latency

---

## Phase 3: Execution Optimization (P1, ~5 days)

### 3.1 Pre-Signed Order Templates
- Background task: pre-compute and sign orders at ~10 price levels per side per hot market
- Cache: `HashMap<(token_id, side, price_tick), SignedOrder>`
- Invalidate on significant price moves (>2%)
- Skip `build() → sign()` when matching template exists (~5-15ms saved per order)

### 3.2 Multi-Leg Atomic Execution
- Intra-market (2 legs): fire concurrently via `tokio::join!` (both FOK)
- Multi-outcome (3+ legs): use SDK's `post_orders(Vec<SignedOrder>)` batch endpoint
- Track per-leg fill status, compute realized vs. estimated edge

### 3.3 HTTP/2 Connection Pooling
- Verify `reqwest` uses HTTP/2 multiplexing or connection pooling
- Configure keep-alive to avoid TLS handshake overhead

### 3.4 Dynamic Fee Lookup
- Query Polymarket fee schedule at startup, cache result
- Replace hardcoded 2% with actual rates (0% maker, ~2% taker)
- Feed into `EdgeCalculator`

### 3.5 Partial Fill Handling
- Produce real `FillStatus::PartiallyFilled` from order responses
- For non-FOK: poll status, proportionally reduce subsequent legs
- Track unhedged exposure, alert on threshold breach

---

## Phase 4: Strategy Enhancement (P1, ~7-8 days)

### 4.1 Wire Kelly Criterion to Detectors
- After edge calculation, call Kelly with `(confidence, net_edge, bankroll)`
- Quarter-Kelly default; `RiskLimits` caps still apply on top
- Replaces fixed $500 cross-market / min-of-book intra-market sizing

### 4.2 Auto-Discovery of Correlated Markets
- Same-event grouping (already in MultiOutcomeDetector)
- Semantic similarity: TF-IDF / keyword matching on market `question` fields
- Price co-movement: rolling 24h correlation from `PriceHistoryStore`, flag |ρ| > 0.7
- Output: dynamically populated `CorrelationGraph`

### 4.3 Latency Arbitrage from CEX Feeds
- New module `arb-strategy/src/latency_arb.rs`
- Binance WebSocket for BTC/ETH/SOL spot prices
- Map crypto-linked Polymarket markets via regex on question text
- Simplified Black-Scholes probability model with realized vol from CEX
- Signal when model diverges from Polymarket price by >5%
- Directional (not structural) — Kelly sizing critical

### 4.4 Wire Custom Correlation Relationships
- For `Custom { constraint, bound }` pairs in `CorrelationGraph`
- Use `EnsembleEstimator` to model joint probability distribution
- Emit opportunity with confidence from simulation credible interval

### 4.5 Deadline Detector Improvements
- Wire `EdgeCalculator::refine_with_vwap()` (currently `net_edge = ZERO`)
- Compute confidence from historical monotonicity patterns
- Add sorting guard for time-ordered validation

---

## Phase 5: Simulation & Analytics Wiring (P2, ~8 days)

### 5.1 Wire All Simulation Models into Engine
- `EnsembleEstimator` becomes configurable pipeline:
  - Base: GBM (default) or Jump Diffusion (crypto-linked markets)
  - Variance reduction: always-on antithetic variates; stratified for rare events
  - Importance sampling: auto-engage for deep OTM (price < 0.05 or > 0.95)
  - Copula: for correlated market pairs
- Market classification heuristic drives model selection

### 5.2 Wire Convergence Diagnostics to API
- Replace hardcoded `gelman_rubin: 1.01` with real values
- Store `ConvergenceDiagnostics` in `AppState`, serve via `/api/simulation/status`

### 5.3 Wire Real Estimator Data to Simulation Status
- Replace `model_estimate == market_price, divergence == 0.0` placeholders
- Read from `EnsembleEstimator` per-market cache

### 5.4 Expose ABM via API Route
- `POST /api/simulate/{condition_id}/abm`
- Configurable agent mix, return `SimulationTrace`

### 5.5 Historical Backtesting Framework
- New module: `backtest.rs`
- Replay `PriceHistoryStore` ticks, run detectors, simulate execution
- Output: trade count, win rate, P&L, max drawdown, Sharpe, Brier
- Route: `POST /api/backtest`

### 5.6 Calibration & Brier Score Tracking
- Detect market resolutions via polling
- Compute Brier score for model probability at trade time
- Rolling 30-day Brier on Performance page
- Alert if Brier > 0.30

---

## Phase 6: Risk Management Hardening (P1, ~4-5 days)

### 6.1 Wire Stress Test Parameters from Frontend
- Unprefix `_params`, parse values, pass to `StressScenario` constructors
- Frontend sliders already exist

### 6.2 Per-Token Volatility from Price History
- Rolling 7-day std of log returns from `PriceHistoryStore`
- Feed into VaR and stress tests
- Fallback to 10% default if <20 ticks

### 6.3 Rolling Window VaR
- 30-day rolling daily PnL window in `PerformanceMetrics`
- Daily VaR computation, trend on Performance page

### 6.4 Position Persistence Hardening
- Write-ahead log: append every fill immediately
- Reconstruct positions from WAL on startup
- Max data loss: 0 fills (vs current 60s)

### 6.5 Circuit Breakers
- Auto kill-switch triggers:
  - Daily loss exceeds threshold
  - API error rate >50% over 1-minute window
  - Execution latency >500ms sustained
  - Unhedged exposure exceeds limit for >30s
- `cancel_all()` + kill switch activation (two-layer defense)
- Configurable cooldown (default: manual resume only)

---

## Phase 7: Deployment & Monitoring (P1, ~3 days)

### 7.1 VPS Deployment
- $5-20/mo VPS (Hetzner/DO/Linode), 2 vCPU, 4GB RAM
- Region: us-east or eu-west
- Systemd service, auto-restart on crash (10s delay)

### 7.2 Alerting
- New module `arb-monitor/src/webhook.rs`
- Discord/Telegram webhook
- Categories: Critical (kill switch, 429), Warning (loss limit approach, high slippage), Info (trade, daily summary)
- Rate limited: max 1 per category per minute

### 7.3 Structured Logging
- JSON logs via `tracing-subscriber` json layer
- File rotation (daily, 7-day retention)
- Trade-specific fields: opportunity_id, condition_id, net_edge, fill_status

### 7.4 Health Check Endpoint
- `GET /api/health` — WebSocket status, last poll age, disk space, POL balance
- VPS monitoring integration

---

## Phase 8: Advanced Infrastructure (P3, 12+ days)

### 8.1 Dedicated Polygon Node
- **When:** Capital >$50K or demonstrable RPC latency cost
- Erigon + Bor, 16-core, 64GB RAM, 4TB NVMe
- ~$200-500/mo hosting

### 8.2 MEV / Polygon FastLane
- **When:** On-chain settlement timing affects fill quality
- Sealed-bid bundle submission, atomic execution or full revert
- Requires algorithmic bidding strategy

### 8.3 Cross-Exchange Arbitrage
- **When:** Capital split across Polymarket + Kalshi
- Dual monitoring, cross-platform routing
- Major leg risk (no atomic cross-chain execution)

### 8.4 External Signal Integration
- Sports odds APIs, weather APIs, election polling aggregators
- Modular `SignalSource` trait in `arb-data`

---

## Phase 9: Frontend Completeness (P3, ~3-4 days)

### 9.1 Live P&L from Real Positions
- Wire frontend P&L chart to real `PositionTracker` in live mode

### 9.2 Real-Time Order Book Visualization
- Wire `OrderbookDepth` component to local book data (Phase 2.2)
- Show VWAP line at target fill size

### 9.3 Strategy Sandbox with Real Simulation
- Wire sandbox to use `EnsembleEstimator` confidence values

### 9.4 Execution Quality Dashboard
- Estimated VWAP vs. realized fill price comparison
- Slippage trend tracking
- Systematic estimation error detection

---

## Recommended Execution Order

```
Phase 1 (Live Executor)     ─┐
                              ├─ Foundation: can trade + see data
Phase 2 (WebSocket Data)    ─┘
                              │
Phase 6 (Risk Hardening)    ─┐│
                              ├─ Protection: won't lose everything
Phase 7 (Deploy + Monitor)  ─┘│
                              │
Phase 3 (Execution Optim)   ─┐│
                              ├─ Performance: trade better + more
Phase 4 (Strategy Enhance)  ─┘│
                              │
Phase 5 (Simulation Wire)   ── Analytics: smarter confidence
                              │
Phase 9 (Frontend)          ── Polish: visibility into everything
                              │
Phase 8 (Advanced Infra)    ── Scale: when capital justifies
```

Total estimated effort: ~50-60 days of focused development across all phases.
