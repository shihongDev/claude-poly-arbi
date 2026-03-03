# Strategy Playground Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Transform `/opportunities` into a Strategy Playground with backend sandbox detection, execution history backtesting, and parameterized simulation.

**Architecture:** Two new Axum POST endpoints (`/api/sandbox/detect`, `/api/sandbox/backtest`) clone config, merge overrides, and run detection/backtest server-side. Existing `/api/simulate/{id}` extended to accept optional params. Frontend is a two-panel page: config sidebar + tabbed results (Detect/Backtest/Simulate).

**Tech Stack:** Rust (Axum 0.8, serde), TypeScript (Next.js 16, React 19, ECharts, Zustand for live config bootstrap only)

---

### Task 1: Add SandboxConfigOverrides type to arb-core

**Files:**
- Modify: `crates/arb-core/src/types.rs` (append after line 209)

**Step 1: Add the SandboxConfigOverrides struct**

Append at end of `crates/arb-core/src/types.rs`:

```rust
/// Flat override struct for sandbox/playground requests.
/// All fields are optional — `None` means "use current live config value".
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SandboxConfigOverrides {
    // Strategy
    pub min_edge_bps: Option<u64>,
    pub intra_market_enabled: Option<bool>,
    pub cross_market_enabled: Option<bool>,
    pub multi_outcome_enabled: Option<bool>,
    pub intra_min_deviation: Option<Decimal>,
    pub cross_min_implied_edge: Option<Decimal>,
    pub multi_min_deviation: Option<Decimal>,
    // Slippage
    pub max_slippage_bps: Option<u64>,
    pub vwap_depth_levels: Option<usize>,
    // Risk
    pub max_position_per_market: Option<Decimal>,
    pub max_total_exposure: Option<Decimal>,
    pub daily_loss_limit: Option<Decimal>,
}
```

**Step 2: Add a merge helper on ArbConfig**

Append to `crates/arb-core/src/config.rs` inside `impl ArbConfig` block (after `is_live()` on line 298):

```rust
    /// Clone this config and apply sandbox overrides on top.
    pub fn with_overrides(&self, ov: &crate::types::SandboxConfigOverrides) -> Self {
        let mut c = self.clone();
        if let Some(v) = ov.min_edge_bps { c.strategy.min_edge_bps = v; }
        if let Some(v) = ov.intra_market_enabled { c.strategy.intra_market_enabled = v; }
        if let Some(v) = ov.cross_market_enabled { c.strategy.cross_market_enabled = v; }
        if let Some(v) = ov.multi_outcome_enabled { c.strategy.multi_outcome_enabled = v; }
        if let Some(v) = ov.intra_min_deviation { c.strategy.intra_market.min_deviation = v; }
        if let Some(v) = ov.cross_min_implied_edge { c.strategy.cross_market.min_implied_edge = v; }
        if let Some(v) = ov.multi_min_deviation { c.strategy.multi_outcome.min_deviation = v; }
        if let Some(v) = ov.max_slippage_bps { c.slippage.max_slippage_bps = v; }
        if let Some(v) = ov.vwap_depth_levels { c.slippage.vwap_depth_levels = v; }
        if let Some(v) = ov.max_position_per_market { c.risk.max_position_per_market = v; }
        if let Some(v) = ov.max_total_exposure { c.risk.max_total_exposure = v; }
        if let Some(v) = ov.daily_loss_limit { c.risk.daily_loss_limit = v; }
        c
    }
```

**Step 3: Verify it compiles**

Run: `cargo build -p arb-core`
Expected: success, zero warnings

**Step 4: Commit**

```bash
git add crates/arb-core/src/types.rs crates/arb-core/src/config.rs
git commit -m "feat(arb-core): add SandboxConfigOverrides type and merge helper"
```

---

### Task 2: Add sandbox detect endpoint

**Files:**
- Create: `crates/arb-api/src/routes/sandbox.rs`
- Modify: `crates/arb-api/src/routes/mod.rs` (add `pub mod sandbox;`)
- Modify: `crates/arb-api/src/main.rs` (register routes)

**Step 1: Create the sandbox routes module**

Create `crates/arb-api/src/routes/sandbox.rs`:

```rust
use std::sync::Arc;
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, SlippageEstimator};
use arb_core::types::{Opportunity, SandboxConfigOverrides};
use arb_data::correlation::CorrelationGraph;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::vwap_cache::CachedSlippageEstimator;
use arb_strategy::cross_market::CrossMarketDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::debug;

use crate::state::AppState;

// ── Detect ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DetectRequest {
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
}

pub async fn detect(
    State(state): State<AppState>,
    Json(req): Json<DetectRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    // Merge overrides onto live config
    let base_config = state.config.read().unwrap().clone();
    let config = base_config.with_overrides(&req.config_overrides);

    // Build slippage estimator with sandbox config
    let estimator = Arc::new(CachedSlippageEstimator::new(
        OrderbookProcessor::new(config.slippage.clone()),
    ));
    let slippage: Arc<dyn SlippageEstimator> = estimator;

    // Build detectors from sandbox config
    let mut detectors: Vec<Box<dyn ArbDetector>> = Vec::new();

    if config.strategy.intra_market_enabled {
        detectors.push(Box::new(IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.multi_outcome_enabled {
        detectors.push(Box::new(MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.cross_market_enabled {
        let graph = if let Some(ref file) = config.strategy.cross_market.correlation_file {
            let path = ArbConfig::config_dir().join(file);
            if path.exists() {
                CorrelationGraph::load(&path).unwrap_or_else(|_| CorrelationGraph::empty())
            } else {
                CorrelationGraph::empty()
            }
        } else {
            CorrelationGraph::empty()
        };
        detectors.push(Box::new(CrossMarketDetector::new(
            config.strategy.cross_market.clone(),
            config.strategy.clone(),
            Arc::new(graph),
            state.market_cache.clone(),
            slippage.clone(),
        )));
    }

    let edge_calculator = EdgeCalculator::default_with_estimator(slippage);

    // Run detection against current market cache
    let markets = state.market_cache.active_markets();
    let markets_scanned = markets.len();
    let mut opportunities: Vec<Opportunity> = Vec::new();

    for detector in &detectors {
        if let Ok(opps) = detector.scan(&markets).await {
            opportunities.extend(opps);
        }
    }

    // Refine with VWAP
    for opp in &mut opportunities {
        let _ = edge_calculator.refine_with_vwap(opp, &state.market_cache);
    }

    // Filter by min_edge_bps and sort
    let min_edge = Decimal::from(config.strategy.min_edge_bps);
    opportunities.retain(|o| o.net_edge_bps() >= min_edge);
    opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

    let elapsed = start.elapsed().as_millis();

    let result = serde_json::json!({
        "opportunities": opportunities,
        "detection_time_ms": elapsed,
        "markets_scanned": markets_scanned,
        "config_used": {
            "min_edge_bps": config.strategy.min_edge_bps,
            "intra_market_enabled": config.strategy.intra_market_enabled,
            "cross_market_enabled": config.strategy.cross_market_enabled,
            "multi_outcome_enabled": config.strategy.multi_outcome_enabled,
        },
    });

    (StatusCode::OK, Json(result)).into_response()
}

// ── Backtest ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BacktestRequest {
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
}

pub async fn backtest(
    State(state): State<AppState>,
    Json(req): Json<BacktestRequest>,
) -> impl IntoResponse {
    let base_config = state.config.read().unwrap().clone();
    let config = base_config.with_overrides(&req.config_overrides);

    let history = state.execution_history.read().unwrap().clone();
    let total_original = history.len();
    let min_edge_bps = Decimal::from(config.strategy.min_edge_bps);

    let mut trades = Vec::new();
    let mut cumulative_exposure = Decimal::ZERO;
    let mut daily_pnl_tracker: std::collections::BTreeMap<String, (Decimal, usize)> =
        std::collections::BTreeMap::new();
    let mut aggregate_pnl = Decimal::ZERO;
    let mut aggregate_pnl_original = Decimal::ZERO;

    for report in &history {
        let net_pnl = report.realized_edge - report.total_fees;
        aggregate_pnl_original += net_pnl;

        // Check if this trade would pass the sandbox config
        let edge_bps = if report.realized_edge != Decimal::ZERO {
            // Approximate: use realized_edge as a proxy for net_edge per unit
            report.realized_edge * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        let trade_size: Decimal = report
            .legs
            .iter()
            .map(|l| l.filled_size)
            .sum();

        // Check risk limits
        let would_exceed_exposure =
            cumulative_exposure + trade_size > config.risk.max_total_exposure;
        let below_min_edge = edge_bps.abs() < min_edge_bps;

        let (included, rejection_reason) = if below_min_edge {
            (false, Some(format!(
                "edge {edge_bps} below min_edge_bps ({})",
                config.strategy.min_edge_bps
            )))
        } else if would_exceed_exposure {
            (false, Some(format!(
                "would exceed max_total_exposure ({})",
                config.risk.max_total_exposure
            )))
        } else {
            (true, None)
        };

        if included {
            aggregate_pnl += net_pnl;
            cumulative_exposure += trade_size;
        }

        let date = report.timestamp.format("%Y-%m-%d").to_string();
        let entry = daily_pnl_tracker.entry(date).or_insert((Decimal::ZERO, 0));
        if included {
            entry.0 += net_pnl;
            entry.1 += 1;
        }

        trades.push(serde_json::json!({
            "opportunity_id": report.opportunity_id.to_string(),
            "realized_edge": report.realized_edge.to_string(),
            "total_fees": report.total_fees.to_string(),
            "net_pnl": net_pnl.to_string(),
            "timestamp": report.timestamp.to_rfc3339(),
            "included": included,
            "rejection_reason": rejection_reason,
        }));
    }

    let total_filtered = trades.iter().filter(|t| t["included"] == true).count();

    let daily_breakdown: Vec<_> = daily_pnl_tracker
        .into_iter()
        .map(|(date, (pnl, count))| {
            serde_json::json!({
                "date": date,
                "pnl": pnl.to_string(),
                "trade_count": count,
            })
        })
        .collect();

    let result = serde_json::json!({
        "total_trades_original": total_original,
        "total_trades_filtered": total_filtered,
        "trades_rejected": total_original - total_filtered,
        "aggregate_pnl": aggregate_pnl.to_string(),
        "aggregate_pnl_original": aggregate_pnl_original.to_string(),
        "daily_breakdown": daily_breakdown,
        "trades": trades,
    });

    (StatusCode::OK, Json(result)).into_response()
}
```

**Step 2: Register the module and routes**

Add to `crates/arb-api/src/routes/mod.rs`:
```rust
pub mod sandbox;
```

Add to `crates/arb-api/src/main.rs` router (after the simulate route, before `.route("/ws", ...)`):
```rust
        .route("/api/sandbox/detect", post(routes::sandbox::detect))
        .route("/api/sandbox/backtest", post(routes::sandbox::backtest))
```

**Step 3: Verify it compiles**

Run: `cargo build -p arb-api`
Expected: success

**Step 4: Commit**

```bash
git add crates/arb-api/src/routes/sandbox.rs crates/arb-api/src/routes/mod.rs crates/arb-api/src/main.rs
git commit -m "feat(arb-api): add sandbox detect and backtest endpoints"
```

---

### Task 3: Parameterize the simulate endpoint

**Files:**
- Modify: `crates/arb-api/src/routes/simulate.rs`

**Step 1: Add request body with optional params**

Replace the entire `crates/arb-api/src/routes/simulate.rs`:

```rust
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};
use arb_simulation::monte_carlo::{MonteCarloParams, run_monte_carlo};
use arb_simulation::particle_filter::ParticleFilter;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize, Default)]
pub struct SimulateRequest {
    pub num_paths: Option<usize>,
    pub volatility: Option<f64>,
    pub drift: Option<f64>,
    pub time_horizon: Option<f64>,
    pub strike: Option<f64>,
    pub particle_count: Option<usize>,
    pub process_noise: Option<f64>,
    pub observation_noise: Option<f64>,
}

pub async fn run_simulation(
    Path(condition_id): Path<String>,
    State(state): State<AppState>,
    body: Option<Json<SimulateRequest>>,
) -> impl IntoResponse {
    let req = body.map(|b| b.0).unwrap_or_default();

    let market = match state.market_cache.get(&condition_id) {
        Some(m) => m,
        None => {
            let json = serde_json::json!({"error": "market not found"});
            return (StatusCode::NOT_FOUND, Json(json)).into_response();
        }
    };

    let initial_price = market
        .outcome_prices
        .first()
        .map(|p| p.to_string().parse::<f64>().unwrap_or(0.5))
        .unwrap_or(0.5);

    let config = state.config.read().unwrap();
    let n_paths = req.num_paths.unwrap_or(config.simulation.monte_carlo_paths);
    let n_particles = req.particle_count.unwrap_or(config.simulation.particle_count);
    drop(config);

    let mc_params = MonteCarloParams {
        initial_price,
        drift: req.drift.unwrap_or(0.0),
        volatility: req.volatility.unwrap_or(0.3),
        time_horizon: req.time_horizon.unwrap_or(1.0),
        strike: req.strike.unwrap_or(0.5),
        n_paths,
    };
    let mc_result = run_monte_carlo(&mc_params);

    let process_noise = req.process_noise.unwrap_or(0.03);
    let observation_noise = req.observation_noise.unwrap_or(0.02);
    let mut pf = ParticleFilter::new(n_particles, initial_price, process_noise, observation_noise);
    pf.update(initial_price);
    let pf_estimate = pf.estimate();

    let result = serde_json::json!({
        "condition_id": condition_id,
        "initial_price": initial_price,
        "monte_carlo": {
            "probability": mc_result.probability,
            "standard_error": mc_result.standard_error,
            "confidence_interval": mc_result.confidence_interval,
            "n_paths": mc_result.n_paths,
        },
        "particle_filter": {
            "probability": pf_estimate.probabilities,
            "confidence_interval": pf_estimate.confidence_interval,
            "method": pf_estimate.method,
        },
    });

    (StatusCode::OK, Json(result)).into_response()
}
```

**Step 2: Verify backend compiles and all tests pass**

Run: `cargo build -p arb-api && cargo test --workspace`
Expected: success

**Step 3: Commit**

```bash
git add crates/arb-api/src/routes/simulate.rs
git commit -m "feat(arb-api): accept optional simulation parameters in request body"
```

---

### Task 4: Add frontend types and API functions

**Files:**
- Modify: `frontend/src/lib/types.ts` (append new types)
- Modify: `frontend/src/lib/api.ts` (add API functions)

**Step 1: Add sandbox types to `frontend/src/lib/types.ts`**

Append at end of file:

```typescript
// ── Sandbox / Playground types ──────────────────────────────

export interface SandboxConfigOverrides {
  min_edge_bps?: number;
  intra_market_enabled?: boolean;
  cross_market_enabled?: boolean;
  multi_outcome_enabled?: boolean;
  intra_min_deviation?: string;
  cross_min_implied_edge?: string;
  multi_min_deviation?: string;
  max_slippage_bps?: number;
  vwap_depth_levels?: number;
  max_position_per_market?: string;
  max_total_exposure?: string;
  daily_loss_limit?: string;
}

export interface DetectResponse {
  opportunities: Opportunity[];
  detection_time_ms: number;
  markets_scanned: number;
  config_used: {
    min_edge_bps: number;
    intra_market_enabled: boolean;
    cross_market_enabled: boolean;
    multi_outcome_enabled: boolean;
  };
}

export interface BacktestTrade {
  opportunity_id: string;
  realized_edge: string;
  total_fees: string;
  net_pnl: string;
  timestamp: string;
  included: boolean;
  rejection_reason: string | null;
}

export interface BacktestDailyBreakdown {
  date: string;
  pnl: string;
  trade_count: number;
}

export interface BacktestResponse {
  total_trades_original: number;
  total_trades_filtered: number;
  trades_rejected: number;
  aggregate_pnl: string;
  aggregate_pnl_original: string;
  daily_breakdown: BacktestDailyBreakdown[];
  trades: BacktestTrade[];
}

export interface SimulateParams {
  num_paths?: number;
  volatility?: number;
  drift?: number;
  time_horizon?: number;
  strike?: number;
  particle_count?: number;
  process_noise?: number;
  observation_noise?: number;
}
```

**Step 2: Add API functions to `frontend/src/lib/api.ts`**

Append at end of file:

```typescript
export function sandboxDetect(
  overrides: import("./types").SandboxConfigOverrides
): Promise<import("./types").DetectResponse> {
  return fetchApi("/api/sandbox/detect", {
    method: "POST",
    body: JSON.stringify({ config_overrides: overrides }),
  });
}

export function sandboxBacktest(
  overrides: import("./types").SandboxConfigOverrides
): Promise<import("./types").BacktestResponse> {
  return fetchApi("/api/sandbox/backtest", {
    method: "POST",
    body: JSON.stringify({ config_overrides: overrides }),
  });
}

export function runSimulation(
  conditionId: string,
  params: import("./types").SimulateParams
): Promise<unknown> {
  return fetchApi(`/api/simulate/${conditionId}`, {
    method: "POST",
    body: JSON.stringify(params),
  });
}
```

**Step 3: Verify types compile**

Run: `cd frontend && npx tsc --noEmit`
Expected: zero errors

**Step 4: Commit**

```bash
git add frontend/src/lib/types.ts frontend/src/lib/api.ts
git commit -m "feat(frontend): add sandbox types and API client functions"
```

---

### Task 5: Build the Strategy Playground page — Config Sidebar

**Files:**
- Modify: `frontend/src/app/opportunities/page.tsx` (complete rewrite)

This is a large page. We build it in three parts: config sidebar first, then tabs one by one.

**Step 1: Write the config sidebar and page shell**

Replace entire `frontend/src/app/opportunities/page.tsx` with the page shell containing:
- Local state for `sandboxConfig` (initialized from `GET /api/config`)
- Config sidebar with three collapsible sections: Strategy, Risk, Slippage
- Input fields for each parameter
- Detect and Backtest action buttons
- Apply to Live button with confirmation dialog
- Tab bar (Detect / Backtest / Simulate) with empty tab content areas
- All config state uses React `useState` (not Zustand) for sandbox isolation

The config sidebar should include these inputs:
- **Strategy section:** `min_edge_bps` (number), `intra_market_enabled` (switch), `cross_market_enabled` (switch), `multi_outcome_enabled` (switch), `intra_min_deviation` (number step=0.001), `cross_min_implied_edge` (number step=0.005), `multi_min_deviation` (number step=0.005)
- **Risk section:** `max_position_per_market` (number), `max_total_exposure` (number), `daily_loss_limit` (number)
- **Slippage section:** `max_slippage_bps` (number), `vwap_depth_levels` (number)

Use existing UI components: `Switch` from `@/components/ui/switch`, `Input` from `@/components/ui/input`, `Button` from `@/components/ui/button`, `Badge` from `@/components/ui/badge`.

**Step 2: Verify it compiles (type check only — tabs are empty)**

Run: `cd frontend && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add frontend/src/app/opportunities/page.tsx
git commit -m "feat(frontend): add playground config sidebar and page shell"
```

---

### Task 6: Build the Detect tab

**Files:**
- Modify: `frontend/src/app/opportunities/page.tsx` (add detect tab content)

**Step 1: Implement the Detect tab**

Add to the detect tab area:
- Stats banner: "{N} opportunities found in {T}ms — {M} markets scanned"
- Reuse `DataTable` with opportunity columns from the old page (type badge, markets, net edge in bps, confidence bar, size available, legs count)
- Row click opens the same detail sheet (opportunity ID, legs, VWAPs, markets)
- "Simulate" button on each row that switches to Simulate tab and sets `simTarget`
- Loading spinner while detect is running
- Empty state when no detection has been run yet

Column definitions reused from old opportunities page: `time`, `type` (badge), `markets` (truncated), `net_edge` (bps, colored), `confidence` (progress bar), `size_available` (USD), `legs` (count).

The detect handler calls `sandboxDetect(overrides)` from `api.ts`.

**Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add frontend/src/app/opportunities/page.tsx
git commit -m "feat(frontend): add detect tab with opportunity table and detail sheet"
```

---

### Task 7: Build the Backtest tab

**Files:**
- Modify: `frontend/src/app/opportunities/page.tsx` (add backtest tab content)

**Step 1: Implement the Backtest tab**

Add to the backtest tab area:
- 4 KPI cards using `MetricCard`: Original Trades, Filtered Trades, Sandbox P&L, P&L Delta (sandbox - original, colored)
- Daily P&L overlay chart (ECharts line chart): `daily_breakdown` mapped to date × pnl
- Trade-by-trade table using `DataTable` with columns:
  - `opportunity_id` (truncated, mono)
  - `realized_edge` (USD, mono)
  - `total_fees` (USD, mono)
  - `net_pnl` (USD, colored green/red, mono)
  - `included` (badge: green "Included" or red "Rejected")
  - `rejection_reason` (text, gray)
- Loading spinner while backtest runs
- Empty state when no backtest has been run yet

The backtest handler calls `sandboxBacktest(overrides)` from `api.ts`.

Import `ReactECharts` dynamically with `dynamic(() => import("echarts-for-react"), { ssr: false })`.

**Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add frontend/src/app/opportunities/page.tsx
git commit -m "feat(frontend): add backtest tab with KPIs, daily P&L chart, and trade log"
```

---

### Task 8: Build the Simulate tab

**Files:**
- Modify: `frontend/src/app/opportunities/page.tsx` (add simulate tab content)

**Step 1: Implement the Simulate tab**

Add to the simulate tab area:
- Market info header (if `simTarget` is set from detect tab, show market details)
- Market selector dropdown (from Zustand store `markets` array) for manual selection
- Simulation parameter inputs: `volatility` (step=0.05), `drift` (step=0.01), `time_horizon` (step=0.1), `num_paths` (step=1000), `particle_count` (step=100), `process_noise` (step=0.01), `observation_noise` (step=0.01)
- "Run Simulation" button
- Results display: reuse the comparison table and probability chart from the simulation page
  - Method comparison table (Monte Carlo, Variance-Reduced, Particle Filter rows)
  - Horizontal bar chart with CI and market price reference line
  - Summary card with divergence detection
- Copy the `METHOD_META`, result normalization logic, and chart builder from `simulation/page.tsx`

The handler calls `runSimulation(conditionId, params)` from `api.ts`.

**Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add frontend/src/app/opportunities/page.tsx
git commit -m "feat(frontend): add simulate tab with parameterized MC and PF"
```

---

### Task 9: Update sidebar navigation

**Files:**
- Modify: `frontend/src/components/sidebar.tsx`

**Step 1: Rename Opportunities to Playground and add icon**

In `frontend/src/components/sidebar.tsx`, change the navItems entry:

From:
```typescript
  { href: "/opportunities", label: "Opportunities", icon: Target },
```

To:
```typescript
  { href: "/opportunities", label: "Playground", icon: Target },
```

The route stays at `/opportunities` (the file is still `app/opportunities/page.tsx`).

Also remove `/simulation` from navItems since the playground now includes simulation capability:
```typescript
  // Remove: { href: "/simulation", label: "Simulation", icon: FlaskConical },
```

And clean up unused imports (`FlaskConical`).

**Step 2: Verify it compiles**

Run: `cd frontend && npx tsc --noEmit`

**Step 3: Commit**

```bash
git add frontend/src/components/sidebar.tsx
git commit -m "feat(frontend): rename Opportunities to Playground in sidebar"
```

---

### Task 10: Full build verification

**Step 1: Backend full build + tests**

Run: `cargo build -p arb-api && cargo test --workspace`
Expected: all tests pass, zero warnings

**Step 2: Frontend type check**

Run: `cd frontend && npx tsc --noEmit`
Expected: zero errors

**Step 3: Frontend production build**

Run: `cd frontend && NODE_OPTIONS="--max-old-space-size=4096" pnpm build`
Expected: zero errors, routes include `/opportunities` (now the playground), no `/positions` or `/performance`

**Step 4: Verify sidebar shows 5 items**

Markets, Portfolio, Playground, Controls, History

**Step 5: Final commit (if any fixups needed)**

---

## File Change Summary

| File | Action | Task |
|------|--------|------|
| `crates/arb-core/src/types.rs` | Append `SandboxConfigOverrides` | 1 |
| `crates/arb-core/src/config.rs` | Add `with_overrides()` method | 1 |
| `crates/arb-api/src/routes/sandbox.rs` | **Create** — detect + backtest handlers | 2 |
| `crates/arb-api/src/routes/mod.rs` | Add `pub mod sandbox;` | 2 |
| `crates/arb-api/src/main.rs` | Register 2 new routes | 2 |
| `crates/arb-api/src/routes/simulate.rs` | Add optional request body params | 3 |
| `frontend/src/lib/types.ts` | Append sandbox types | 4 |
| `frontend/src/lib/api.ts` | Add 3 API functions | 4 |
| `frontend/src/app/opportunities/page.tsx` | **Complete rewrite** — Strategy Playground | 5-8 |
| `frontend/src/components/sidebar.tsx` | Rename nav item, remove Simulation | 9 |
