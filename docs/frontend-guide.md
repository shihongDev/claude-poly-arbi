# Frontend Guide

Next.js 16 App Router dashboard for real-time monitoring, trading controls, and market exploration.

## Tech Stack

| Technology | Version | Purpose |
|------------|---------|---------|
| Next.js | 16 | App Router framework |
| React | 19 | UI library |
| TailwindCSS | v4 | Styling |
| Radix UI | 1.4 | Headless accessible primitives |
| Zustand | 5 | State management |
| TradingView Lightweight Charts | 5 | Equity curve / P&L charts |
| Apache ECharts | 6 | Gauges, depth charts, histograms, treemaps |
| Lucide React | 0.575 | Icons |
| Sonner | 2 | Toast notifications |

> **No shadcn/ui** — all UI components are custom-styled Radix wrappers in `src/components/ui/`.

## Pages

### `/` — Markets Browser

The primary view. Displays all markets with filtering, sorting, and three view modes (table, cards, treemap).

- Summary stats: total markets, orderbook coverage, average spread, tight-spread count
- Table with columns: watchlist star, question (with arb badges), price (stacked bar for multi-outcome), 24h change, spread (color-coded), depth bar, volume, health badge, expiry progress
- `SpreadHistogram` and `SpreadVolumeScatter` analytics charts
- `HotMarketsCarousel` and `WatchlistSection`
- `AdvancedFilterPanel` side sheet
- "Group Events" toggle for `EventGroupTable` view
- Clicking a row navigates to `/markets/[condition_id]`

### `/dashboard` — Portfolio

Consolidated portfolio view combining positions and performance.

- 56px hero P&L number with daily P&L subtitle
- 6 KPI MetricCards: Daily P&L, Total Exposure, Unrealized P&L, Open Positions, Brier Score, Total Trades
- Equity curve (TradingView LW) + Risk gauges (ECharts) for Exposure and Drawdown
- Positions table + exposure donut chart
- Performance section: Brier Score gauge, Execution Quality trend, P&L by Strategy bar chart
- Simulation Engine Status panel

### `/opportunities` — Playground

Sandbox for testing arbitrage detection with three tabs:

1. **Detect** — Run sandbox detection with config overrides. DataTable of detected opportunities with expandable `OpportunityRow` details.
2. **Backtest** — Replay history under different parameters. Shows original vs filtered PnL comparison, daily breakdown chart.
3. **Live** — Real-time opportunities feed from store. Filterable by arb type and min edge.

### `/markets/[id]` — Market Detail

Deep dive into a single market:

- Market question, description, badges (neg-risk, end date, polymarket link)
- 7-column stats grid: Best Bid, Best Ask, Spread, Last Trade, Volume, Liquidity, 24h Change
- Outcome probability bars with bid/ask tooltips and probability sum deviation warning
- `OrderForm` for paper trades
- `OrderbookDepth` (ECharts area chart) + `OrderbookLadder` (price ladder), tabbed for multi-token
- Related opportunities and your positions sections

### `/controls` — Trading Controls

- **Kill Switch Card** — red/green border based on state, reason input, activate/deactivate buttons
- **Daemon Status** — mode, uptime, market count, WebSocket status
- **Config Editor** — 5 card sections (General, Strategy, Risk, Slippage, Alerts), `GET/PUT /api/config`

### `/history` — Trade History

- Filter by mode (All/Paper/Live)
- KPI row: Total Trades, Total Realized Edge, Average Slippage
- DataTable with clickable rows opening leg detail dialog
- Slippage Analysis scatter plot (ECharts): Expected VWAP vs Actual Fill

### `/simulation` — Simulation

- Market selector dropdown, path count input
- Monte Carlo vs Particle Filter comparison table
- Probability comparison chart (ECharts horizontal bars with CI ranges)
- Divergence warning when model differs from market by >5pp

### `/simulation/stress-test` — Stress Testing

4 scenarios (Liquidity Shock, Correlation Spike, Flash Crash, Kill Switch Delay) with parameter sliders. Shows before/after VaR comparison.

---

## State Management

Single Zustand store: `useDashboardStore` (`src/store/index.ts`).

### State Fields

| Field | Type | Default | Updated By |
|-------|------|---------|------------|
| `wsStatus` | `"connected" \| "connecting" \| "disconnected"` | `"disconnected"` | WebSocket hook |
| `status` | `StatusResponse \| null` | `null` | REST + WS |
| `opportunities` | `Opportunity[]` | `[]` | REST + WS (capped at 200) |
| `positions` | `Position[]` | `[]` | REST + WS |
| `metrics` | `MetricsSnapshot \| null` | `null` | REST + WS |
| `markets` | `MarketState[]` | `[]` | REST + WS (merged by condition_id) |
| `history` | `ExecutionReport[]` | `[]` | REST + WS (capped at 500) |
| `killSwitchActive` | `boolean` | `false` | REST + WS |
| `*Loading` | `boolean` | `true` | Cleared on first data load |

### WebSocket Event Dispatch (`handleWsEvent`)

| Event Type | Store Effect |
|------------|-------------|
| `opportunities_batch` | Prepend new, cap at 200 |
| `trade_executed` | Prepend to history, cap at 500 |
| `position_update` | Replace positions array |
| `metrics_update` | Replace metrics |
| `kill_switch_change` | Update kill switch state + reason |
| `markets_loaded` | Merge into existing by condition_id (upsert) |
| `market_update` | Update single market in-place or append |

---

## Data Flow

### Initialization (`DataInitializer` in `providers.tsx`)

On app mount, fires 6 parallel REST calls to populate the store:
- `GET /api/status` → `setStatus`
- `GET /api/opportunities` → `setOpportunities`
- `GET /api/positions` → `setPositions`
- `GET /api/metrics` → `setMetrics`
- `GET /api/markets` → `setMarkets`
- `GET /api/history` → `setHistory`

A second retry at t=20s catches markets that take ~15s to load from the Polymarket API.

### WebSocket (`useWebSocket` hook)

- Connects to `ws://localhost:8080/ws` (configurable via `NEXT_PUBLIC_WS_URL`)
- **Event batching**: Events buffer for 200ms before flushing to store (reduces re-renders)
- **Reconnection**: Exponential backoff from 1s to 30s max
- Status tracked via `wsStatus` in store

### API Client (`src/lib/api.ts`)

- **In-flight deduplication**: Concurrent GETs to the same URL share one Promise
- **TTL response cache**: Avoids re-fetching on tab switches / page transitions
  - `/api/config`: 30s
  - `/api/markets`, `/api/opportunities`, `/api/positions`, `/api/metrics`, `/api/status`: 5s
  - `/api/history`: 10s
- **10s timeout** on all requests via AbortController
- POST/PUT/DELETE bypass both cache and dedup
- **No silent fallbacks**: All functions propagate errors to callers. Components handle failures with their own loading/error UI (skeletons, error cards, alerts).

---

## Key Components

### Layout & Navigation

| Component | Description |
|-----------|-------------|
| `Sidebar` | Fixed 240px left sidebar. 7 nav items. Mobile: hamburger + overlay. |
| `ConnectionStatus` | Dot indicator: green (connected), amber (connecting), red (disconnected) |
| `KillSwitchBanner` | Full-width red banner when kill switch is active, with resume button |

### Data Display

| Component | Props | Description |
|-----------|-------|-------------|
| `MetricCard` | `title, value, delta?, deltaType?` | KPI card with 11px uppercase label, 4xl value, delta arrow |
| `DataTable<T>` | `columns, data, pageSize?, onRowClick?` | Generic sortable/paginated table (3-state sort cycle) |
| `OpportunityRow` | `opportunity` | Expandable table row showing arb details + legs |

### Charts

| Component | Library | Description |
|-----------|---------|-------------|
| `PnlChart` | TradingView LW | Area series equity curve, dynamically colored by P&L sign |
| `RiskGauge` | ECharts | Arc gauge with sage/amber/red thresholds |
| `OrderbookDepth` | ECharts | Cumulative depth area chart, bids green / asks red |
| `OrderbookLadder` | Custom | Traditional price ladder with proportional fill bars |
| `SimulationStatusPanel` | ECharts | Polls simulation status, shows divergence + model health |

### Market-Specific

| Component | Description |
|-----------|-------------|
| `MarketHealthBadge` | Composite health badge (good/warning/danger) |
| `LiquidityDepthBar` | Mini horizontal bar showing bid/ask depth |
| `OrderImbalanceArrow` | Up/down arrow for bid vs ask volume |
| `ExpiryProgressBar` | Time-to-expiry progress bar |
| `ProbSumBadge` | Warning badge when probabilities don't sum to 100% |
| `ArbOpportunityBadge` | Green badge with opportunity count |
| `HotMarketsCarousel` | Horizontal carousel of highest-volume markets |
| `SpreadHistogram` | ECharts histogram of spread distribution |
| `ProbabilityStackedBar` | Inline stacked bar for multi-outcome probabilities |
| `OrderForm` | Paper trade form with outcome selector + price/size inputs |

---

## Design System

### Colors

| Token | Value | Usage |
|-------|-------|-------|
| Background | `#F8F7F4` | Warm cream page background |
| Surface | `#FFFFFF` | Cards, panels |
| Text | `#1A1A19` | Primary text |
| Sage Green | `#2D6A4F` | Positive values, active states, CTAs |
| Sage Light | `#DAE9E0` | Active nav, positive badges |
| Brick Red | `#B44C3F` | Negative values, kill switch, errors |
| Brick Light | `#F5E0DD` | Error backgrounds |
| Muted | `#F0EEEA` | Secondary backgrounds |
| Muted Text | `#6B6B6B` | Secondary text, labels |
| Border | `#E6E4DF` | Borders, dividers |
| Amber | `#D97706` | Warning state |

### Typography

- **Space Grotesk** (`--font-space-grotesk`): Body text, headings, labels
- **JetBrains Mono** (`--font-jetbrains-mono`): All numeric data, IDs, prices, timestamps

| Pattern | Style |
|---------|-------|
| Section headers | `text-[11px] font-medium uppercase tracking-wider text-[#9B9B9B]` |
| Hero P&L | `text-[56px] font-bold` Space Grotesk |
| KPI values | `text-4xl font-bold` |
| Body | `text-sm` |

### Border Radius

- Cards/panels: 18px (`rounded-2xl`)
- Buttons/inputs: 10px (`rounded-[10px]`)
- Badges: pill (`rounded-full`)
- Base: 10px (`--radius: 0.625rem`)

### Spread Color Convention

| Range | Color | Label |
|-------|-------|-------|
| < 30 bps | Sage `#2D6A4F` | Good |
| 30-100 bps | Amber `#D97706` | Warning |
| > 100 bps | Brick `#B44C3F` | Danger |

### Chart Styling (ECharts)

- Background: transparent
- Grid lines: `#F0EEEA`
- Axis text: `#6B6B6B`, 10-11px, JetBrains Mono
- Tooltips: white background, `#E6E4DF` border, monospace font
- Positive: `#2D6A4F`, Negative: `#B44C3F`, Reference: `#9B9B9B` dashed

---

## TypeScript Types (`src/lib/types.ts`)

### Core Types

```typescript
interface MarketState {
  condition_id: string; question: string; outcomes: string[];
  token_ids: string[]; outcome_prices: string[];
  orderbooks: OrderbookSnapshot[];
  volume_24hr: string | null; liquidity: string | null;
  active: boolean; neg_risk: boolean;
  best_bid: string | null; best_ask: string | null;
  spread: string | null; last_trade_price: string | null;
  description: string | null; end_date_iso: string | null;
  slug: string | null; one_day_price_change: string | null;
  event_id?: string;
}

interface Opportunity {
  id: string; arb_type: ArbType; markets: string[];
  legs: TradeLeg[]; gross_edge: string; net_edge: string;
  estimated_vwap: string[]; confidence: number;
  size_available: string; detected_at: string;
}

interface ExecutionReport {
  opportunity_id: string; legs: LegReport[];
  realized_edge: string; slippage: string;
  total_fees: string; timestamp: string; mode: TradingMode;
}

interface Position {
  token_id: string; condition_id: string; size: string;
  avg_entry_price: string; current_price: string;
  unrealized_pnl: string;
}

interface MetricsSnapshot {
  brier_score: number; drawdown_pct: number;
  execution_quality: string; total_pnl: string;
  daily_pnl: string; trade_count: number;
  pnl_by_type: Record<string, string>;
  current_exposure: string; peak_equity: string;
  current_equity: string;
}
```

> **Note**: All numeric fields (prices, P&L, edges, sizes) are **strings** on the wire — decimal-formatted. Pages parse them with `parseFloat()`.

### Utility Functions (`src/lib/utils.ts`)

| Function | Description |
|----------|-------------|
| `formatDecimal(value, decimals)` | Fixed decimal with em-dash fallback |
| `formatBps(value)` | `"123 bps"` from decimal edge string |
| `formatUsd(value)` | `"$1,234.56"` via Intl.NumberFormat |
| `formatPnl(value)` | `"+$1,234.56"` or `"-$1,234.56"` |
| `formatPercent(value)` | `"12.34%"` |
| `formatCents(price)` | `"65¢"` from `"0.65"` |
| `formatUsdCompact(value)` | `"$1.2M"`, `"$500K"` |
| `timeAgo(isoString)` | `"5s ago"`, `"3m ago"` |
| `spreadSeverity(bps)` | `"good" \| "warning" \| "danger"` |
| `probSumDeviation(prices)` | Deviation from 1.0 in percentage points |
