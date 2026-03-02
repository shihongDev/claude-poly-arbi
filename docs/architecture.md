# Polymarket Arbitrage System — Architecture

## Philosophy

**Edge is a measurable pricing error, not an opinion.** Every trade must have positive expected value after spread, slippage, liquidity impact, and fees. VWAP-based edge calculation, not theoretical mid-price.

---

## Workspace Layout

```
claude-poly-arbi/
  Cargo.toml                          # workspace root
  polymarket-cli-main/                # existing CLI (workspace member, untouched)
  crates/
    arb-core/                         # Core types, traits, config, errors
    arb-data/                         # Market polling, orderbook, VWAP, market cache
    arb-strategy/                     # Arb detectors (intra/cross/multi) + edge calc
    arb-simulation/                   # Monte Carlo, particle filter, copula, ABM
    arb-execution/                    # Trade executor (paper + live) + slippage protection
    arb-risk/                         # Position limits, kill switch, PnL tracking
    arb-monitor/                      # Structured logging, alerts, Brier score, metrics
    arb-daemon/                       # Main daemon binary + CLI subcommands
```

## Dependency Graph

```
arb-daemon → arb-strategy, arb-execution, arb-risk, arb-monitor, arb-data, arb-simulation
arb-strategy → arb-core, arb-data, arb-simulation
arb-execution → arb-core, arb-data
arb-risk → arb-core
arb-monitor → arb-core
arb-data → arb-core, polymarket-client-sdk
arb-simulation → arb-core
arb-core → (no internal deps, only external crates)
```

## System Flow

```
┌─────────────────────────────────────────────────────────────┐
│                      arb-daemon (event loop)                │
│                                                             │
│  ┌──────────┐   ┌───────────┐   ┌──────────┐   ┌────────┐ │
│  │ L1: Data │──>│ L2: Strat │──>│ L3: Risk │──>│Execute │ │
│  │  Poller  │   │ Detectors │   │  Check   │   │Paper/  │ │
│  │ +Cache   │   │ +Edge Calc│   │ +Limits  │   │Live    │ │
│  └──────────┘   └───────────┘   └──────────┘   └────────┘ │
│       │              │                              │       │
│       v              v                              v       │
│  ┌──────────┐   ┌───────────┐                ┌────────────┐│
│  │ Particle │   │Simulation │                │  Monitor   ││
│  │ Filter   │   │ Engine    │                │  +Logging  ││
│  └──────────┘   └───────────┘                └────────────┘│
└─────────────────────────────────────────────────────────────┘
```

Pipeline per tick:
1. **Poll** markets due for refresh (tiered by volume: hot/warm/cold)
2. **Update** market cache + particle filter probabilities
3. **Scan** all enabled detectors (intra-market, cross-market, multi-outcome)
4. **Refine** edge with VWAP walk on real orderbook depth
5. **Filter** by minimum net edge threshold
6. **Risk check** each opportunity (position limits, exposure, daily PnL)
7. **Execute** approved opportunities (paper or live)
8. **Log** everything in structured JSON

---

## Three Arbitrage Types

### 1. Intra-Market (YES + NO != $1.00)

Binary markets have YES and NO tokens that must sum to $1.00 at resolution. If `best_ask(YES) + best_ask(NO) < $1.00`, you can buy both and lock in riskless profit. If `best_bid(YES) + best_bid(NO) > $1.00`, you can sell both.

**Edge formula:**
```
gross_edge = 1.00 - (YES_vwap_ask + NO_vwap_ask)     [buying both]
gross_edge = (YES_vwap_bid + NO_vwap_bid) - 1.00      [selling both]
net_edge = gross_edge - fees
```

### 2. Cross-Market (Correlated mispricing)

User-defined logical relationships between markets. Examples:
- `P(A) <= P(B)` — if A implies B, A's price must not exceed B's
- `P(A) + P(B) <= 1.0` — mutually exclusive events
- `P(A) + P(B) >= 1.0` — exhaustive events

When prices violate these constraints, trade the spread.

### 3. Multi-Outcome (Sum != 100%)

Events with 3+ outcomes (neg_risk markets) where all outcome probabilities must sum to 100%. If the sum deviates, sell overpriced outcomes and buy underpriced ones.

---

## VWAP-Based Edge (Not Top-of-Book)

Critical distinction: we never use top-of-book price for edge calculation. Instead, we walk the orderbook to compute Volume-Weighted Average Price for the actual fill size we need:

```
VWAP = Σ(price_i × size_i) / Σ(size_i)   for levels consumed up to target fill
slippage_bps = |VWAP - best_price| / best_price × 10000
```

This ensures we only trade when there's *real* edge at *realistic* execution prices.

---

## Execution Modes

- **Paper**: Simulated fills at estimated VWAP. Same interface as live. Logs all trades for analysis.
- **Live**: Real orders through the CLOB API. Requires `--live` flag. Supports limit (post-only preferred) and market orders. Handles partial fills, timeouts, and cancellation.

---

## Risk Management

| Control | Description |
|---------|-------------|
| Per-market position limit | Max USDC exposure per condition_id |
| Total exposure cap | Sum of all position values |
| Daily loss limit | Running PnL — auto-pause if breached |
| Max open orders | Concurrent order count limit |
| Kill switch | File-based flag, checked every tick. Cancels all orders. Manual reset required. |

---

## Simulation Engine

For cross-market arbs where edge depends on probability estimation (not just structural constraints), we run a full quant simulation stack:

- **Monte Carlo** with variance reduction (antithetic variates, control variates, stratified sampling)
- **Importance Sampling** for tail-risk contracts (P < 1%)
- **Particle Filter** for real-time Bayesian probability updating from market price observations
- **Copula Models** (Student-t, Clayton) for joint probability of correlated markets
- **Agent-Based Model** for market microstructure simulation (informed/noise/market-maker agents)

---

## Configuration

TOML config at `~/.config/polymarket/arb-config.toml` with sections for polling, strategy, slippage, risk, simulation, and alerts. All fields have sensible defaults — zero configuration needed for paper trading.

---

## CLI Commands

```
polymarket arb scan              # One-shot scan + print opportunities
polymarket arb run               # Start daemon (paper mode)
polymarket arb run --live        # Start daemon (live mode)
polymarket arb status            # Positions, PnL, exposure, kill switch state
polymarket arb kill              # Activate kill switch
polymarket arb resume            # Deactivate kill switch
polymarket arb history           # Recent trade log
polymarket arb config            # Validate and display config
polymarket arb simulate <cid>    # Run simulation on a market
```
