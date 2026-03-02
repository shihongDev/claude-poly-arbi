# Frontend Dashboard Suite — Design Document

**Date:** 2026-03-01
**Status:** Approved
**Scope:** Full monitoring + trading control + market exploration dashboard

## Summary

A professional-grade web dashboard for the Polymarket arbitrage system. Built with Next.js + React (frontend) and Axum (Rust API backend). Real-time data via WebSocket. Charts via TradingView Lightweight Charts + Apache ECharts.

## Tech Stack

| Layer | Technology | Purpose |
|-------|-----------|---------|
| Frontend | Next.js 15 (App Router) | SSR/routing/pages |
| UI | TailwindCSS + shadcn/ui | Component library |
| Charts (financial) | TradingView Lightweight Charts | P&L time series, price charts |
| Charts (analytics) | Apache ECharts | Gauges, heatmaps, distributions, orderbook depth |
| State | Zustand | Client-side state management |
| API client | fetch + custom hooks | REST + WebSocket |
| Backend API | Axum (new `arb-api` crate) | REST + WebSocket server |
| Serialization | serde_json / OpenAPI | Type-safe JSON contracts |

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│  Next.js Frontend (localhost:3000)                          │
│  ┌──────────┐ ┌──────────┐ ┌──────────┐ ┌──────────┐      │
│  │Dashboard │ │Opps Feed │ │Positions │ │Controls  │ ...   │
│  └────┬─────┘ └────┬─────┘ └────┬─────┘ └────┬─────┘      │
│       │             │            │             │             │
│  ┌────▼─────────────▼────────────▼─────────────▼──────┐     │
│  │  Zustand Store (client state)                      │     │
│  │  + useWebSocket hook (auto-reconnect)              │     │
│  └────┬───────────────────────────────────────────────┘     │
│       │ REST (fetch)          │ WS (ws://)                  │
└───────┼───────────────────────┼─────────────────────────────┘
        │                       │
┌───────▼───────────────────────▼─────────────────────────────┐
│  arb-api (Axum, localhost:8080)                             │
│  ┌──────────┐  ┌──────────┐  ┌──────────┐                  │
│  │REST routes│  │WebSocket │  │AppState  │                  │
│  │/api/*     │  │broadcast │  │(shared)  │                  │
│  └──────────┘  └──────────┘  └────┬─────┘                  │
│                                    │                         │
│  ┌─────────────────────────────────▼──────────────────────┐ │
│  │  ArbEngine (from arb-daemon)                           │ │
│  │  MarketCache | RiskLimits | Detectors | Executor       │ │
│  └────────────────────────────────────────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## New Rust Crate: arb-api

Added to workspace as `crates/arb-api/`. Depends on arb-core, arb-daemon, arb-risk, arb-execution, arb-data, arb-monitor, arb-simulation.

### REST Endpoints

| Method | Path | Description |
|--------|------|-------------|
| GET | /api/status | Daemon status, mode, kill switch, uptime |
| GET | /api/opportunities | Current + recent opportunities |
| GET | /api/positions | All positions with P&L |
| GET | /api/metrics | Performance snapshot (Brier, drawdown, execution quality) |
| GET | /api/markets | All cached markets |
| GET | /api/markets/:id | Single market with orderbook |
| GET | /api/history | Trade history (paginated) |
| GET | /api/config | Current ArbConfig |
| PUT | /api/config | Update config sections |
| POST | /api/kill | Activate kill switch |
| POST | /api/resume | Deactivate kill switch |
| POST | /api/simulate/:condition_id | Run MC/PF simulation |

### WebSocket Events (server → client)

```typescript
type WsEvent =
  | { type: "opportunity_detected"; data: Opportunity }
  | { type: "trade_executed"; data: ExecutionReport }
  | { type: "position_update"; data: Position[] }
  | { type: "metrics_update"; data: MetricsSnapshot }
  | { type: "kill_switch_change"; data: { active: boolean; reason?: string } }
  | { type: "market_update"; data: MarketState }
  | { type: "alert"; data: { level: "warn" | "critical"; message: string } }
```

### AppState (shared across handlers)

```rust
pub struct AppState {
    pub engine: Arc<Mutex<ArbEngine>>,
    pub market_cache: Arc<MarketCache>,
    pub risk_limits: Arc<Mutex<RiskLimits>>,
    pub config: Arc<RwLock<ArbConfig>>,
    pub ws_tx: broadcast::Sender<WsEvent>,
}
```

## Frontend Pages

### 1. Dashboard (`/`)
- **KPI Row**: Total P&L, Daily P&L, Open Positions, Active Opportunities, Brier Score
- **P&L Chart**: Lightweight Charts area chart, equity curve over time
- **Recent Opportunities**: Last 10 detected, with edge_bps, arb_type, status
- **Position Summary**: Top positions by exposure
- **Risk Gauges**: Exposure %, drawdown %, daily loss vs limit (ECharts gauge)

### 2. Opportunities (`/opportunities`)
- **Live Feed**: Auto-updating table of opportunities (WebSocket)
- **Columns**: Time, Type, Markets, Gross Edge, Net Edge (bps), Confidence, Size, Status
- **Filters**: By arb_type, min edge, min confidence
- **Detail Drawer**: Click to expand → legs, VWAP estimates, orderbook snapshot

### 3. Positions (`/positions`)
- **Positions Table**: Token, Market Question, Size, Entry Price, Current Price, Unrealized P&L
- **Exposure Breakdown**: Pie chart by market (ECharts)
- **P&L Attribution**: Stacked bar chart by arb type (ECharts)

### 4. Performance (`/performance`)
- **Brier Score Trend**: Line chart over time (ECharts)
- **Calibration Plot**: Predicted vs actual probability scatter (ECharts)
- **Execution Quality**: Average fill quality metric over time
- **Drawdown Chart**: Underwater chart from peak equity (Lightweight Charts)
- **P&L by Strategy**: Grouped bar chart (intra, cross, multi)

### 5. Markets (`/markets`)
- **Market Browser**: Searchable/filterable table of all markets
- **Orderbook Depth**: Bid/ask depth visualization (ECharts)
- **Price History**: Lightweight Charts candlestick/line
- **Market Detail**: Volume, liquidity, outcome prices, active status

### 6. Controls (`/controls`)
- **Kill Switch**: Big red button + status indicator + reason display
- **Daemon Status**: Running/stopped, mode (paper/live), uptime
- **Config Editor**: Form-based editor for ArbConfig sections
- **Polling Tiers**: Visualize hot/warm/cold market distribution

### 7. History (`/history`)
- **Trade Log**: Paginated table of ExecutionReports
- **Columns**: Time, Opportunity ID, Mode, Legs, Realized Edge, Slippage, Fees
- **Slippage Analysis**: Expected vs actual VWAP scatter plot (ECharts)
- **Detail View**: Full leg-by-leg breakdown

### 8. Simulation (`/simulation`)
- **Market Selector**: Pick a market to simulate
- **Simulation Controls**: Path count, methods (MC, variance-reduced, particle filter)
- **Results**: Probability distributions, confidence intervals, ESS
- **Comparison**: Side-by-side simulation method results

## Shared Components

| Component | Description |
|-----------|-------------|
| `<KillSwitchBanner>` | Red banner across all pages when kill switch active |
| `<ConnectionStatus>` | WebSocket connection state indicator (green/yellow/red) |
| `<MetricCard>` | KPI card: value, label, delta arrow, optional sparkline |
| `<OrderbookDepth>` | ECharts bid/ask depth visualization |
| `<PnLChart>` | Lightweight Charts equity curve |
| `<DataTable>` | shadcn/ui table with sorting, filtering, pagination |
| `<RiskGauge>` | ECharts gauge for exposure/drawdown |
| `<OpportunityRow>` | Expandable row with leg details |
| `<Sidebar>` | Navigation sidebar with page links + status badges |

## Data Flow

1. **ArbEngine** runs its 100ms tick loop inside `arb-api`
2. On each event (opportunity, execution, position change), engine pushes to `broadcast::Sender`
3. WebSocket handler subscribes to broadcast and forwards to connected clients
4. REST endpoints query `AppState` directly for point-in-time snapshots
5. Frontend Zustand store receives WS events and merges into client state
6. React components subscribe to store slices and re-render

## Build & Development

```bash
# Backend (from repo root)
cargo run -p arb-api             # Start API server on :8080

# Frontend (from frontend/)
pnpm install                     # Install deps
pnpm dev                         # Start Next.js on :3000

# Both together (convenience)
# Add a Procfile or justfile recipe
```

## MVP Scope (Monitor-First)

Phase 1 delivers:
- arb-api crate with core REST + WebSocket
- Dashboard page with KPIs, P&L chart, risk gauges
- Opportunities live feed
- Positions table
- Kill switch banner + controls
- Connection status indicator

Phase 2 adds: Performance, Markets, History, Simulation pages.
