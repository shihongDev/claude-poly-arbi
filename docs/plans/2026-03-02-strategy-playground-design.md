# Strategy Playground Design

**Date:** 2026-03-02
**Status:** Approved

## Goal

Transform the `/opportunities` page into an interactive Strategy Playground that combines parameter tuning, live detection with full backend fidelity, execution history backtesting, and per-opportunity simulation — all in a sandboxed environment isolated from the live engine.

## Approach: Backend Sandbox (Approach B)

Full-fidelity detection runs server-side against the current MarketCache using config overrides. Execution history is replayed under new parameters to show how many past trades would have been taken and the resulting P&L delta.

## Backend API Endpoints

### `POST /api/sandbox/detect`

Runs the full detection pipeline against the current MarketCache with overridden config parameters.

**Request:**
```json
{
  "config_overrides": {
    "min_edge_bps": 30,
    "intra_market_enabled": true,
    "cross_market_enabled": true,
    "multi_outcome_enabled": false,
    "intra_min_deviation": "0.003",
    "cross_min_implied_edge": "0.015",
    "multi_min_deviation": "0.01",
    "max_slippage_bps": 80,
    "vwap_depth_levels": 15
  }
}
```

**Response:**
```json
{
  "opportunities": [ ...Opportunity[] ],
  "detection_time_ms": 42,
  "markets_scanned": 156,
  "config_used": { ...merged config }
}
```

Implementation: Clone current ArbConfig, merge overrides, construct detectors, run against MarketCache, return results.

### `POST /api/sandbox/backtest`

Re-scores stored execution history under new thresholds.

**Request:**
```json
{
  "config_overrides": {
    "min_edge_bps": 30,
    "max_total_exposure": 8000,
    "daily_loss_limit": 500
  }
}
```

**Response:**
```json
{
  "total_trades_original": 47,
  "total_trades_filtered": 32,
  "trades_rejected": 15,
  "aggregate_pnl": "1234.56",
  "aggregate_pnl_original": "987.65",
  "daily_breakdown": [
    { "date": "2026-02-28", "pnl": "45.20", "trade_count": 3 }
  ],
  "trades": [
    {
      "opportunity_id": "...",
      "realized_edge": "12.50",
      "total_fees": "3.20",
      "net_pnl": "9.30",
      "timestamp": "...",
      "included": true,
      "rejection_reason": null
    }
  ]
}
```

### Parameterized Simulation

Expand existing `POST /api/simulate/{condition_id}` to accept optional params:

```json
{
  "num_paths": 10000,
  "volatility": 0.4,
  "drift": 0.0,
  "time_horizon": 0.5,
  "particle_count": 1000,
  "process_noise": 0.05,
  "observation_noise": 0.03
}
```

All fields optional; server uses config defaults for any omitted field.

## Frontend Page Layout

Two-panel layout: config sidebar (left, ~280px) + tabbed results (right).

```
┌──────────────┬───────────────────────────────────────────────┐
│ SANDBOX      │  [Detect] [Backtest] [Simulate]    (tabs)     │
│ CONFIG       │                                               │
│              │  Detect: Opportunity table + stats banner      │
│ Strategy:    │  Backtest: KPIs + daily P&L chart + trade log  │
│ Risk:        │  Simulate: Market picker + params + MC/PF      │
│ Slippage:    │                                               │
│              │                                               │
│ [Detect]     │                                               │
│ [Backtest]   │                                               │
│ ───────────  │                                               │
│ [Apply Live] │                                               │
└──────────────┴───────────────────────────────────────────────┘
```

### Detect Tab
- Stats banner: "{N} opportunities found in {T}ms — {M} markets scanned"
- DataTable with same columns as old Opportunities page (time, type, markets, net_edge, confidence, size, legs)
- Row click opens detail sheet
- "Simulate" action on each row switches to Simulate tab with that opportunity's market pre-filled

### Backtest Tab
- 4 KPI cards: Original Trades, Filtered Trades, Sandbox P&L, P&L Delta
- Daily P&L line chart (ECharts) with original vs sandbox overlay
- Trade-by-trade table with included/rejected badge and rejection reason

### Simulate Tab
- Market selector (from detected opportunities or manual)
- Parameter inputs: volatility, drift, time_horizon, num_paths, particle_count, process/observation noise
- Run button → MC + PF comparison table and probability chart (reuse existing simulation page components)

### Apply to Live
- Bottom of config sidebar
- Confirmation dialog: "This will update the live engine configuration. Changes take effect immediately."
- Calls PUT /api/config with merged sandbox config
- Toast notification on success

## State Management

All playground state is **local React state** (not Zustand). This ensures complete isolation from the live engine.

```typescript
sandboxConfig: SandboxConfig        // initialized from GET /api/config
detectResult: DetectResult | null    // from POST /api/sandbox/detect
backtestResult: BacktestResult       // from POST /api/sandbox/backtest
simResult: SimulationResult | null   // from POST /api/simulate/{id}
activeTab: "detect" | "backtest" | "simulate"
```

## Files Changed

### Backend (Rust)
| File | Change |
|------|--------|
| `crates/arb-core/src/types.rs` | Add SandboxConfigOverrides, DetectResponse, BacktestResponse, BacktestTrade types |
| `crates/arb-api/src/routes/mod.rs` | Add sandbox module |
| `crates/arb-api/src/routes/sandbox.rs` | New — detect + backtest handlers |
| `crates/arb-api/src/routes/simulate.rs` | Accept optional params in request body |
| `crates/arb-api/src/main.rs` | Register /api/sandbox/* routes |

### Frontend (TypeScript)
| File | Change |
|------|--------|
| `frontend/src/app/opportunities/page.tsx` | Complete rewrite → Strategy Playground |
| `frontend/src/lib/types.ts` | Add sandbox-related types |
| `frontend/src/lib/api.ts` | Add sandboxDetect(), sandboxBacktest(), update simulate() |
| `frontend/src/components/sidebar.tsx` | Rename "Opportunities" → "Playground" |

## Simulation Engines

MC + Particle Filter only. Parameters exposed to user rather than hardcoded. Future expansion to ABM/copulas possible but not in scope.

## Isolation Model

Pure sandbox. Parameter changes affect only the playground preview. The live engine continues running with production config. "Apply to Live" button pushes changes with confirmation.
