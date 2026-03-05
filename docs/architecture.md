# System Architecture

## Philosophy

**Edge is a measurable pricing error, not an opinion.** Every trade must have positive expected value after spread, slippage, liquidity impact, and fees. VWAP-based edge calculation, not theoretical mid-price.

---

## Workspace Layout

```
claude-poly-arbi/
  Cargo.toml                          # workspace root
  polymarket-cli-main/                # existing CLI (workspace member, reference)
  crates/
    arb-core/                         # Core types, traits, config, errors
    arb-data/                         # Market polling, orderbook, VWAP, market cache
    arb-strategy/                     # Arb detectors (intra/cross/multi) + edge calc
    arb-simulation/                   # Monte Carlo, particle filter, copula, ABM
    arb-execution/                    # Trade executor (paper + live) + auth
    arb-risk/                         # Position limits, kill switch, PnL tracking
    arb-monitor/                      # Structured logging, alerts, Brier score
    arb-daemon/                       # CLI binary (arb) + subcommands
    arb-api/                          # HTTP server (arb-api) + engine loop
  frontend/                           # Next.js 16 dashboard
  scripts/                            # dev.sh launch script
  secrets/                            # Private key (git-ignored)
  docs/                               # Documentation
```

## Dependency Graph

```
arb-core ─────────────────────────────────────────────────────
   │                                                          │
   ├─── arb-data ─────────────────────────────────┐           │
   │       │                                      │           │
   ├─── arb-simulation                            │           │
   │       │                                      │           │
   ├─── arb-strategy ─── (depends on data + sim)  │           │
   │                                              │           │
   ├─── arb-execution ── (depends on data)        │           │
   │                                              │           │
   ├─── arb-risk                                  │           │
   │                                              │           │
   ├─── arb-monitor                               │           │
   │                                              │           │
   ├─── arb-daemon ──── (depends on all above)    │           │
   │                                              │           │
   └─── arb-api ─────── (depends on all above) ───┘           │
                                                              │
         polymarket-client-sdk ────────────────────────────────┘
```

Clean DAG — no circular dependencies. `arb-core` is the universal foundation. `arb-data` and `arb-simulation` are independent of each other.

## System Flow

```
┌──────────────────────────────────────────────────────────┐
│                    Next.js Frontend (:3000)               │
│  Markets · Portfolio · Playground · Simulation · Controls │
└─────────────────────────┬────────────────────────────────┘
                          │ REST + WebSocket
┌─────────────────────────┴────────────────────────────────┐
│                   arb-api (Axum :8080)                    │
│          17 REST endpoints · WS broadcast · Engine loop   │
├───────────┬──────────┬───────────┬───────────┬───────────┤
│ arb-data  │arb-strat │ arb-sim   │ arb-exec  │ arb-risk  │
│ Market    │ Detector │ Monte     │ Paper +   │ Position  │
│ Cache +   │ engines  │ Carlo +   │ Live      │ sizing +  │
│ Poller    │ + Edge   │ Particle  │ executor  │ Kill      │
│ + VWAP    │ calc     │ filter    │           │ switch    │
├───────────┴──────────┴───────────┴───────────┴───────────┤
│                      arb-core                             │
│            Types · Traits · Config · Errors               │
└──────────────────────────────────────────────────────────┘
```

Pipeline per tick (5-second interval):
1. **Kill switch check** — atomic read, skip cycle if active
2. **VWAP cache clear** — invalidate memoized slippage estimates
3. **Poll** markets due for refresh (tiered by volume: hot=5s, warm=15s, cold=60s)
4. **Change detection** — only scan markets whose orderbooks changed since last cycle
5. **Scan** all enabled detectors (intra-market, cross-market, multi-outcome)
6. **Refine** edge with VWAP walk on real orderbook depth
7. **Enrich** with probability estimates (if enabled, via ensemble estimator)
8. **Filter** by minimum net edge threshold (configurable at runtime)
9. **Risk check** each opportunity (position limits, exposure, daily PnL)
10. **Execute** approved opportunities (paper or live)
11. **Broadcast** results via WebSocket to connected frontends
12. **Persist** positions and history (every 60s + graceful shutdown)

---

## Crate Details

### arb-core

Foundation crate with zero internal dependencies.

**Config (`ArbConfig`):**
```rust
ArbConfig {
    general:    GeneralConfig,    // trading_mode, log_level, starting_equity
    polling:    PollingConfig,    // hot/warm/cold intervals and volume thresholds
    strategy:   StrategyConfig,   // min_edge_bps, detector enables, sub-configs
    slippage:   SlippageConfig,   // max_slippage_bps, vwap_depth_levels
    risk:       RiskConfig,       // position limits, exposure caps, daily loss limit
    simulation: SimulationConfig, // MC paths, particle count, variance reduction
    alerts:     AlertsConfig,     // drawdown thresholds, calibration interval
}
```

**Core traits:**

| Trait | Methods | Implementors |
|-------|---------|-------------|
| `MarketDataSource` | `fetch_markets()`, `fetch_orderbook()`, `fetch_orderbooks()` | `SdkMarketDataSource` |
| `ArbDetector` | `arb_type()`, `scan(&[Arc<MarketState>])` | `IntraMarketDetector`, `CrossMarketDetector`, `MultiOutcomeDetector` |
| `SlippageEstimator` | `estimate_vwap()`, `split_order()` | `OrderbookProcessor`, `CachedSlippageEstimator` |
| `TradeExecutor` | `execute_opportunity()`, `cancel_all()`, `mode()` | `PaperTradeExecutor`, `LiveTradeExecutor` |
| `RiskManager` | `check_opportunity()`, `record_execution()`, `is_kill_switch_active()` | `RiskLimits` |
| `ProbabilityEstimator` | `estimate()`, `update()` | `EnsembleEstimator` |

**Key types:**

| Type | Description |
|------|-------------|
| `MarketState` | Complete market snapshot: metadata + orderbooks + pricing |
| `Opportunity` | Detected arb: type, legs, edge, confidence, size |
| `ExecutionReport` | Trade result: legs, fills, slippage, fees |
| `RiskDecision` | `Approve { max_size }` / `Reject { reason }` / `ReduceSize { new_size }` |
| `ArbType` | `IntraMarket` / `CrossMarket` / `MultiOutcome` |
| `VwapEstimate` | VWAP result: price, size consumed, levels walked, slippage bps |

**Error type (`ArbError`):** `MarketData`, `Orderbook`, `InsufficientLiquidity`, `SlippageTooHigh`, `RiskLimit`, `KillSwitch`, `Execution`, `Config`, `Simulation`, `Io`, `Json`, `TomlParse`, `Sdk`.

---

### arb-data

Market data acquisition and caching. Depends on `arb-core` + `polymarket-client-sdk`.

- **`MarketCache`**: `DashMap<String, Arc<MarketState>>` with atomic `u64` generation counter. Updates bump the generation; consumers call `changed_since(gen)` to get only changed markets.

- **`SdkMarketDataSource`**: Wraps `gamma::Client` (metadata) + `Arc<clob::Client>` (orderbooks). Fetches markets paginated (100/page). Fetches orderbooks concurrently with semaphore + timeout.

- **`OrderbookProcessor`**: Implements `SlippageEstimator`. Walks bid/ask levels for VWAP calculation up to `vwap_depth_levels` (default 10).

- **`CachedSlippageEstimator<S>`**: Wraps any `SlippageEstimator` with per-cycle memoization cache. Cleared at the start of each engine cycle.

- **`MarketPoller`**: Tiered polling based on 24h volume:
  - Hot (>$100K): every 5 seconds
  - Warm ($10K-$100K): every 15 seconds
  - Cold (<$10K): every 60 seconds

- **`CorrelationGraph`**: User-defined market relationships loaded from TOML. Relationship types: `implied_by`, `mutually_exclusive`, `exhaustive`, `custom`.

---

### arb-strategy

Arbitrage detection engines. Depends on `arb-core` + `arb-data` + `arb-simulation`.

**Detectors:**

| Detector | `ArbType` | What it detects |
|----------|-----------|-----------------|
| `IntraMarketDetector` | `IntraMarket` | YES + NO prices != $1.00 in binary markets. Checks `ask_sum < 1.00` (buy-both) and `bid_sum > 1.00` (sell-both). |
| `CrossMarketDetector` | `CrossMarket` | Correlated market mispricings from user-defined `CorrelationGraph`. Supports `ImpliedBy`, `MutuallyExclusive`, `Exhaustive` relationships. Optional t-copula tail dependence. |
| `MultiOutcomeDetector` | `MultiOutcome` | Multi-outcome events where sum of YES prices deviates from 100%. Groups neg-risk markets by `event_id`. |
| `DeadlineMonotonicityDetector` | — | Deadline inversions in event series (standalone, not an `ArbDetector` impl). |

**Edge calculation (`EdgeCalculator`):**
- Default 2% fee rate on notional traded
- `refine_with_vwap()`: replaces theoretical edge with actual VWAP-based edge
- `confidence_adjusted_edge()`: scales edge by ensemble estimator confidence

---

### arb-simulation

Probability estimation and simulation. Depends on `arb-core` only.

| Module | Description |
|--------|-------------|
| `monte_carlo` | GBM: `S_T = S_0 * exp((mu - sigma^2/2)T + sigma*sqrt(T)*Z)`. Returns probability, SE, CI. |
| `particle_filter` | Bayesian state-space filter with sequential importance resampling. Per-market state tracking. |
| `estimator` | `EnsembleEstimator`: combines MC + PF via inverse-variance weighting. Lazily initializes per-market filters. |
| `importance_sampling` | Variance reduction for rare events. |
| `variance_reduction` | Antithetic variates and control variates. |
| `copula` | t-copula with configurable degrees of freedom for tail dependence. |
| `agent_model` | ABM with informed/noise traders, Kyle lambda market impact. |
| `jump_diffusion` | Merton jump-diffusion for price paths. |
| `convergence` | Convergence diagnostics. |

---

### arb-execution

Trade execution. Depends on `arb-core` + `arb-data` + `polymarket-client-sdk`.

| Executor | Mode | Description |
|----------|------|-------------|
| `PaperTradeExecutor` | Paper | Simulated fills with 10% pessimism (fills worse than VWAP). 2% fee on notional. Tracks virtual positions. |
| `LiveTradeExecutor` | Live | Authenticated CLOB client. Auth wired. Order placement is a stub pending SDK finalization. `cancel_all_orders()` is live. |

**Auth chain:** key file → `LocalSigner` → Polygon chain 137 → EIP-712 → `Client<Authenticated<Normal>>`

---

### arb-risk

Risk management. Depends on `arb-core` only.

| Component | Description |
|-----------|-------------|
| `RiskLimits` | Implements `RiskManager`. Checks kill switch, daily loss, orders, exposure, per-market limits. Can `ReduceSize` before rejecting. |
| `KillSwitch` | File-based at `~/.config/polymarket/KILL_SWITCH`. Triggerable via API, CLI, or `touch`. |
| `PositionTracker` | Tracks positions with avg entry prices. JSON-serializable. |
| `PerformanceMetrics` | Brier score, drawdown, execution quality, PnL by strategy. |
| `kelly_criterion()` | Quarter-Kelly sizing (0.25 multiplier). |
| `var` | Value at Risk (parametric + historical). |

---

### arb-monitor

Observability. Depends on `arb-core` only.

| Component | Description |
|-----------|-------------|
| `AlertManager` | Drawdown alerts, Brier score calibration checks. |
| `ModelHealth` | Rolling Brier scores (30m/24h). Confidence: <0.20=full, 0.20-0.35=linear, >0.45=halt. Drift at 0.35. |
| `logger` | Structured logging (JSON/compact), optional file output. |

---

### arb-daemon

CLI binary (`arb`). Depends on all library crates.

```
arb scan [--comprehensive] [--min-edge <bps>] [--min-volume <usd>] [--verbose]
arb run [--live]
arb status
arb kill
arb resume
arb history [-l <n>]
arb config
arb simulate <condition_id>
```

---

### arb-api

HTTP server binary (`arb-api`). Depends on all library crates.

17 REST endpoints + WebSocket broadcast on `0.0.0.0:8080`. See [api-reference.md](api-reference.md).

**AppState** (shared via `Arc`):
```rust
AppState {
    market_cache:        Arc<MarketCache>,
    risk_limits:         Arc<Mutex<RiskLimits>>,
    kill_switch_active:  Arc<AtomicBool>,           // lock-free hot-path check
    config:              Arc<RwLock<ArbConfig>>,
    ws_tx:               broadcast::Sender<String>,  // capacity 256
    opportunities:       Arc<RwLock<Vec<Opportunity>>>,
    execution_history:   Arc<RwLock<Vec<ExecutionReport>>>,
    cached_metrics_json: Arc<RwLock<String>>,        // pre-serialized
    start_time:          Instant,
}
```

---

## Three Arbitrage Types

### 1. Intra-Market (YES + NO != $1.00)

Binary markets have YES and NO tokens that must sum to $1.00 at resolution. If `best_ask(YES) + best_ask(NO) < $1.00`, buy both for riskless profit. If `best_bid(YES) + best_bid(NO) > $1.00`, sell both.

```
gross_edge = 1.00 - (YES_vwap_ask + NO_vwap_ask)     [buying both]
gross_edge = (YES_vwap_bid + NO_vwap_bid) - 1.00      [selling both]
net_edge = gross_edge - fees
```

### 2. Cross-Market (Correlated mispricing)

User-defined logical relationships between markets:
- `P(A) <= P(B)` — if A implies B, A's price must not exceed B's
- `P(A) + P(B) <= 1.0` — mutually exclusive events
- `P(A) + P(B) >= 1.0` — exhaustive events

### 3. Multi-Outcome (Sum != 100%)

Events with 3+ outcomes where all probabilities must sum to 100%. Sell overpriced, buy underpriced.

---

## VWAP-Based Edge

We never use top-of-book price for edge calculation. Instead, we walk the orderbook to compute VWAP for the actual fill size:

```
VWAP = Σ(price_i × size_i) / Σ(size_i)   for levels consumed up to target fill
slippage_bps = |VWAP - best_price| / best_price × 10000
```

This ensures we only trade when there's *real* edge at *realistic* execution prices.

---

## Key External Dependencies

| Crate | Version | Used For |
|-------|---------|----------|
| `polymarket-client-sdk` | 0.4 | Polymarket API (gamma, clob, data, ctf) |
| `tokio` | 1 | Async runtime |
| `axum` | 0.8 | HTTP server |
| `dashmap` | 6 | Concurrent market cache |
| `rust_decimal` | 1 | Price arithmetic |
| `nalgebra` | 0.33 | Matrix operations (copula, ABM) |
| `statrs` | 0.18 | Statistical distributions |
| `rusqlite` | 0.32 | Price history storage |
| `alloy` | — | Ethereum/Polygon interaction (via SDK) |

---

## Design Patterns

- **VWAP-first**: All edge calculations walk the orderbook. Never uses mid-price.
- **Arc-wrapped market state**: `DashMap<String, Arc<MarketState>>` — cheap reads via reference counting, no deep cloning.
- **Generation-based change detection**: Atomic counter on `MarketCache` tracks mutations. Detectors only scan changed markets.
- **Tiered polling**: Markets classified by volume into hot/warm/cold tiers.
- **Pre-serialized responses**: Metrics JSON serialized once per cycle, served as `Arc<String>`.
- **Typestate authentication**: SDK's `Client<Authenticated<K>>` ensures order methods only on authenticated clients.
- **File-based kill switch**: Simple, atomic, debuggable. Triggerable by any process.
- **Paper mode by default**: Live trading behind explicit `--live` flag.
