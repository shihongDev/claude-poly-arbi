# Polymarket Arbitrage System — Implementation Plan

## Phase Dependency Graph

```
Phase 1 ──┬── Phase 2 ──┐
           ├── Phase 3 ──┼── Phase 4 ──── Phase 6
           └── Phase 5 ──┘
```

- **Phase 1** must complete first (everything depends on it)
- **Phases 2, 3, 5** can run in parallel (independent, depend only on Phase 1)
- **Phase 4** depends on Phases 1, 2, 3
- **Phase 6** depends on everything

---

## Phase 1: Foundation (arb-core + arb-data)

### arb-core — Core Types, Traits, Config, Errors

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations, re-exports |
| `src/types.rs` | `OrderbookLevel`, `OrderbookSnapshot`, `MarketState`, `ArbType`, `Side`, `TradingMode`, `Opportunity`, `TradeLeg`, `ExecutionReport`, `LegReport`, `FillStatus`, `VwapEstimate`, `OrderChunk`, `ProbEstimate`, `RiskDecision`, `Position`, `MarketCorrelation`, `CorrelationRelationship` |
| `src/traits.rs` | `MarketDataSource`, `ArbDetector`, `SlippageEstimator`, `TradeExecutor`, `RiskManager`, `ProbabilityEstimator` |
| `src/config.rs` | `ArbConfig` with all sub-configs, TOML parsing, defaults, load/save |
| `src/error.rs` | `ArbError` enum (thiserror), `Result<T>` alias |

**Dependencies:** rust_decimal, serde, chrono, uuid, thiserror, async-trait, toml, dirs, tracing

**Key design decisions:**
- All prices use `rust_decimal::Decimal` (not f64) for precision
- Traits use `async_trait` for async methods in trait objects
- Config has defaults for every field — zero config for paper mode
- `Opportunity.net_edge_bps()` helper for filtering
- `Opportunity.with_max_size()` for risk-adjusted sizing

### arb-data — Market Data Ingestion

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations |
| `src/poller.rs` | `MarketPoller` — wraps `polymarket_client_sdk::gamma::Client` and `clob::Client`. Tiered polling (hot/warm/cold by 24hr volume). Fetches markets, prices, orderbooks. Implements `MarketDataSource` trait. |
| `src/orderbook.rs` | `OrderbookProcessor` — converts SDK `OrderBookSummaryResponse` to `OrderbookSnapshot`. VWAP calculation by walking bid/ask levels. Implements `SlippageEstimator` trait. |
| `src/market_cache.rs` | `MarketCache` — thread-safe `DashMap<String, MarketState>`. Updated by poller, read by detectors. Methods: `update()`, `get()`, `all_markets()`, `markets_due_for_refresh()`. |
| `src/correlation.rs` | `CorrelationGraph` — loads market relationships from TOML. Stores `Vec<MarketCorrelation>`. Methods: `load()`, `pairs_for_market()`. |

**Dependencies:** arb-core, polymarket-client-sdk, tokio, dashmap, async-trait

**Integration with existing CLI:**
- Reuses `polymarket_client_sdk::gamma::Client` for market data (same as CLI's `Commands::Markets`)
- Reuses `polymarket_client_sdk::clob::Client` for orderbooks (same as CLI's `Commands::Clob`)
- Does NOT modify existing CLI code — just uses the same SDK
- Auth is handled in arb-daemon when creating clients, following same pattern as CLI's `auth.rs`

**VWAP algorithm (core of the system):**
```rust
fn estimate_vwap(book, side, target_size) -> VwapEstimate:
    levels = book.asks if side == Buy, else book.bids
    remaining = target_size
    total_cost = 0
    levels_consumed = 0
    for level in levels:
        fill = min(remaining, level.size)
        total_cost += fill * level.price
        remaining -= fill
        levels_consumed += 1
        if remaining <= 0: break
    if remaining > 0: return InsufficientLiquidity error
    vwap = total_cost / target_size
    best_price = levels[0].price
    slippage_bps = |vwap - best_price| / best_price * 10000
    return VwapEstimate { vwap, total_size: target_size, levels_consumed, max_available, slippage_bps }
```

**Tiered polling logic:**
```
volume_24hr >= hot_threshold  → poll every hot_interval_secs (5s)
volume_24hr >= warm_threshold → poll every warm_interval_secs (15s)
otherwise                     → poll every cold_interval_secs (60s)
```

**Verification:** Unit tests for VWAP calculation, config parsing, type serialization, orderbook conversion.

---

## Phase 2: Intra-Market Arb + Slippage + Paper Trading

### arb-strategy (partial) — Intra-Market Detection + Edge Calculation

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations |
| `src/intra_market.rs` | `IntraMarketDetector` — implements `ArbDetector`. For each binary market, checks `YES_price + NO_price` vs $1.00. Walks orderbook for VWAP. Emits opportunities exceeding min_edge. |
| `src/edge.rs` | `EdgeCalculator` — central EV computation. `refine_with_vwap()` takes raw opportunity + orderbook cache, recomputes net edge with actual VWAP walk. Handles fee calculation (2% on Polymarket). |

**Intra-market detection logic:**
```
for each binary market:
    yes_ask = best_ask(YES token)
    no_ask = best_ask(NO token)
    if yes_ask + no_ask < 1.00 - min_deviation:  # buy both
        vwap_yes = walk_asks(YES, target_size)
        vwap_no = walk_asks(NO, target_size)
        gross_edge = 1.00 - (vwap_yes + vwap_no)
        net_edge = gross_edge - fees
        if net_edge_bps >= min_edge_bps: emit Opportunity

    yes_bid = best_bid(YES token)
    no_bid = best_bid(NO token)
    if yes_bid + no_bid > 1.00 + min_deviation:  # sell both
        vwap_yes = walk_bids(YES, target_size)
        vwap_no = walk_bids(NO, target_size)
        gross_edge = (vwap_yes + vwap_no) - 1.00
        net_edge = gross_edge - fees
        if net_edge_bps >= min_edge_bps: emit Opportunity
```

### arb-execution (partial) — Slippage Estimator + Paper Trading

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations |
| `src/slippage.rs` | `VwapSlippageEstimator` — implements `SlippageEstimator`. VWAP walk algorithm + order splitting logic (split if size > threshold, each chunk stays within slippage limit). |
| `src/paper_trade.rs` | `PaperTradeExecutor` — implements `TradeExecutor`. Simulated fills at VWAP (with configurable pessimism factor). Tracks virtual positions and PnL. Logs trades. |

**Verification:** `arb scan` command outputs real opportunities with calculated net edge from live market data.

---

## Phase 3: Risk Management + Monitoring

### arb-risk — Risk Management

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations |
| `src/limits.rs` | `RiskLimits` — implements `RiskManager`. Per-market position limits, total exposure cap, daily loss limit, max open orders. |
| `src/position_tracker.rs` | `PositionTracker` — `HashMap<String, Position>` by token_id. Updated on each ExecutionReport. Persists to state file on shutdown. |
| `src/kill_switch.rs` | `KillSwitch` — file-based flag at `~/.config/polymarket/KILL_SWITCH`. Checked every tick. Activate/deactivate methods. Cancels all orders on activation. |
| `src/metrics.rs` | `PerformanceMetrics` — Brier score tracking, PnL attribution by arb type, drawdown tracking (peak-to-trough), execution quality ratio (`realized_edge / expected_edge`). |

### arb-monitor — Logging + Alerts

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations |
| `src/logger.rs` | `init_logging()` — structured JSON logging via `tracing` + `tracing-subscriber`. Configurable level and output (stdout + file). |
| `src/alerts.rs` | `AlertManager` — drawdown warning/critical thresholds, kill switch notifications, calibration drift (Brier score degradation). Output to log + optional file. |

**Verification:** Unit tests for risk limits, kill switch toggle, Brier score formula, position tracking math.

---

## Phase 4: Daemon + Multi-Outcome + Cross-Market

### arb-daemon — Main Binary

**Files:**
| File | Contents |
|------|----------|
| `src/main.rs` | CLI entry point with clap. Subcommands: scan, run, status, kill, resume, history, config, simulate. |
| `src/engine.rs` | Main event loop (the pipeline described in architecture). Poll → Detect → Risk → Execute → Log. |
| `src/commands/scan.rs` | One-shot scan: fetch all markets, run detectors, print opportunities table. |
| `src/commands/run.rs` | Start daemon loop. Paper by default, `--live` flag for live. |
| `src/commands/status.rs` | Print current positions, PnL, exposure, kill switch state. |
| `src/commands/kill.rs` | Activate kill switch. |
| `src/commands/resume.rs` | Deactivate kill switch. |
| `src/commands/history.rs` | Print recent trade log from state file. |
| `src/commands/config.rs` | Validate and display current config. |
| `src/commands/simulate.rs` | Run simulation engine on a specific market by condition_id. |

### arb-strategy (completion) — Multi-Outcome + Cross-Market

**Files:**
| File | Contents |
|------|----------|
| `src/multi_outcome.rs` | `MultiOutcomeDetector` — groups neg_risk markets by event. Sums outcome prices. If deviation > threshold, constructs multi-leg opportunity. |
| `src/cross_market.rs` | `CrossMarketDetector` — loads correlation graph. For each pair, checks if prices violate the defined constraint. Trades the spread. |

**Multi-outcome detection logic:**
```
for each event with multiple outcomes (neg_risk):
    prices = [market.outcome_prices for all outcomes in event]
    total = sum(prices)
    if |total - 1.00| > min_deviation:
        if total > 1.00:  # overpriced — sell overpriced outcomes
            identify outcomes where price > fair_share
            construct sell legs with VWAP
        if total < 1.00:  # underpriced — buy underpriced outcomes
            identify outcomes where price < fair_share
            construct buy legs with VWAP
        net_edge = |total - 1.00| - total_fees
        emit multi-leg Opportunity
```

**Cross-market detection logic:**
```
for each (market_A, market_B, relationship) in correlation_graph:
    match relationship:
        ImpliedBy:  # P(A) <= P(B)
            if price_A > price_B + min_edge:
                sell A, buy B → edge = price_A - price_B - fees
        MutuallyExclusive:  # P(A) + P(B) <= 1.0
            if price_A + price_B > 1.0 + min_edge:
                sell A, sell B → edge = (price_A + price_B - 1.0) - fees
        Exhaustive:  # P(A) + P(B) >= 1.0
            if price_A + price_B < 1.0 - min_edge:
                buy A, buy B → edge = (1.0 - price_A - price_B) - fees
```

**Verification:** Daemon runs continuously, detects real opportunities across all three arb types, paper-trades automatically, logs everything.

---

## Phase 5: Simulation Engine (arb-simulation)

**Files:**
| File | Contents |
|------|----------|
| `src/lib.rs` | Module declarations, `SimulationEngine` facade |
| `src/monte_carlo.rs` | Binary contract pricing via GBM. `S_T = S_0 × exp((μ - σ²/2)T + σ√T × Z)`, `payoff = 1{S_T > K}`. Configurable paths, drift, volatility, time horizon. |
| `src/variance_reduction.rs` | Builder pattern for stacking: antithetic variates (pair Z/-Z), control variates (use BS digital price as control), stratified sampling (partition [0,1] into J strata). |
| `src/importance_sampling.rs` | Tail-risk contracts (P < 1%). Tilt sampling distribution toward rare event. Likelihood ratio correction. Monitor effective sample size. |
| `src/particle_filter.rs` | Sequential Monte Carlo for real-time probability updating. State: logit(p_true) random walk. Observation: market price. Systematic resampling when ESS < N/2. Output: weighted mean probability + credible interval. |
| `src/copula.rs` | Joint probability models for correlated markets. Student-t copula (Cholesky + Student-t CDF, captures tail dependence). Clayton copula (lower tail dependence). |
| `src/agent_model.rs` | Market microstructure simulation. Informed agents (Kyle model), noise agents (exponential size), market makers (spread tightening on volume). For backtesting and convergence analysis. |

**Key formulas:**

Monte Carlo with antithetic variates:
```
for i in 0..N/2:
    Z = sample_normal()
    p1 = payoff(S_0 * exp(drift + vol*Z))
    p2 = payoff(S_0 * exp(drift + vol*(-Z)))
    estimate += (p1 + p2) / 2
p_hat = estimate / (N/2)
SE = sqrt(var(paired_estimates) / (N/2))
```

Particle filter update step:
```
for each particle i:
    x_i(t) = x_i(t-1) + N(0, process_vol)         # propagate
    p_i = sigmoid(x_i(t))                           # convert to probability
    w_i *= likelihood(observed_price | p_i)          # reweight
normalize weights
if ESS < N/2: systematic_resample()
output = weighted_mean(sigmoid(particles), weights)
```

Importance sampling tilt:
```
θ* = argmax P(rare_event | tilted_θ)    # optimal tilt
for i in 0..N:
    X_i ~ f_θ*(x)                       # sample under tilted distribution
    w_i = f(X_i) / f_θ*(X_i)           # likelihood ratio
    estimate += w_i * 1{X_i in rare_event}
p_hat = estimate / N
```

**Verification:** Monte Carlo convergence tests (SE decreases as 1/√N), particle filter tracks known price paths, copula tail dependence matches theoretical values.

---

## Phase 6: Live Execution + Integration

### arb-execution (completion) — Live Trade Executor

**Files:**
| File | Contents |
|------|----------|
| `src/executor.rs` | `LiveTradeExecutor` — implements `TradeExecutor`. Real execution through `polymarket_client_sdk::clob::Client`. Limit orders (post_only preferred), market orders for urgent fills. Partial fill handling (resubmit/cancel). Timeout-based cancellation for stale orders. |

**Integration tasks:**
- Wire simulation engine probabilities into cross-market edge calculation
- End-to-end pipeline verification: daemon detects → risk checks → paper executes → logs
- Live mode behind `--live` flag with extra confirmation
- Execution quality tracking: `realized_edge / expected_edge` across trades

**Verification:**
- Full integration test with mock data through entire pipeline
- Paper trading against live Polymarket data
- Compare `realized_edge` vs `expected_edge` (execution quality ratio)
- Brier score tracking for probability calibration

---

## External Dependencies

| Purpose | Crate | Version | Why this one |
|---------|-------|---------|--------------|
| Linear algebra | `nalgebra` | 0.33 | Pure Rust Cholesky decomposition (no LAPACK) |
| Statistics | `statrs` | 0.18 | Normal, Student-t, distribution functions |
| RNG | `rand` + `rand_distr` | 0.9 / 0.5 | Monte Carlo sampling |
| Decimal math | `rust_decimal` | 1 | Already in CLI, no floating point errors |
| Async runtime | `tokio` | 1 | Already in CLI |
| Serialization | `serde` + `toml` + `serde_json` | 1 / 0.8 / 1 | Config + state + structured logs |
| CLI | `clap` | 4 | Already in CLI |
| Logging | `tracing` + subscriber + appender | 0.1 / 0.3 / 0.2 | Structured JSON logging |
| UUID | `uuid` | 1 | Opportunity IDs |
| Time | `chrono` | 0.4 | Already in CLI |
| Errors | `thiserror` | 2 | Ergonomic error enums |
| Concurrent map | `dashmap` | 6 | Thread-safe market cache |
| Async traits | `async-trait` | 0.1 | Trait objects with async methods |

---

## Files in Existing CLI to Understand (not modify)

| File | What we reuse |
|------|---------------|
| `polymarket-cli-main/.../src/auth.rs` | Auth patterns: `resolve_signer()`, `create_clob_client()`, `create_provider()` |
| `polymarket-cli-main/.../src/config.rs` | Config file pattern: `~/.config/polymarket/config.json` |
| `polymarket-cli-main/.../src/commands/clob.rs` | All CLOB API patterns: orderbook, prices, order placement |
| `polymarket-cli-main/.../src/main.rs` | CLI entry point structure (add `Arb` subcommand) |
| `polymarket-cli-main/.../Cargo.toml` | Becomes workspace member |

---

## Testing Strategy

### Unit Tests (per crate)
- **arb-core**: Config parsing, type serialization roundtrip, Opportunity helpers
- **arb-data**: VWAP calculation (known orderbook → expected VWAP), orderbook conversion, cache thread safety
- **arb-strategy**: Edge computation (known inputs → expected edge), detector logic with mock markets
- **arb-simulation**: MC convergence (SE ~ 1/√N), particle filter tracking, copula tail dependence
- **arb-risk**: Position limits (approve/reject), kill switch toggle, Brier score formula, drawdown tracking
- **arb-monitor**: Log output format, alert thresholds

### Integration Tests
- Full pipeline with mock market data: detect → risk check → paper execute → log
- Multiple opportunities sorted by edge, best-first execution

### Validation (manual)
- Paper trading against live Polymarket data
- Verify detected opportunities match manual calculation
- Execution quality ratio tracking over 100+ paper trades
