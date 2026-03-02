# Frontend Dashboard Suite Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Build a professional trading dashboard (Next.js + Axum) for the Polymarket arbitrage system with real-time WebSocket streaming, monitoring, market exploration, and trading controls.

**Architecture:** Axum REST+WebSocket backend (`arb-api` crate) serves data from the existing arb engine. Next.js frontend (`frontend/`) connects via REST for snapshots and WebSocket for live streaming. Zustand manages client state. TradingView Lightweight Charts + Apache ECharts for visualizations.

**Tech Stack:** Rust/Axum, Next.js 15 (App Router), TypeScript, TailwindCSS, shadcn/ui, Zustand, TradingView Lightweight Charts, Apache ECharts

---

## Task Dependency Graph

```
Task 1 (arb-api crate)  ──┐
                           ├── Task 5 (integration test) ── Task 8+ (pages)
Task 2 (Next.js scaffold) ─┤
Task 3 (TypeScript types)  ─┤
Task 4 (shared components) ─┘
Task 6 (WebSocket hook)   ── Task 8+ (pages)
Task 7 (Zustand store)    ── Task 8+ (pages)
Tasks 8-15 (pages)         ── all parallel, independent
```

**Parallelization strategy:**
- Tasks 1-4 can run fully in parallel (no dependencies)
- Tasks 5-7 depend on 1-4
- Tasks 8-15 (pages) can all run in parallel after 5-7

---

### Task 1: Create arb-api Crate (Axum Backend)

**Files:**
- Create: `crates/arb-api/Cargo.toml`
- Create: `crates/arb-api/src/main.rs`
- Create: `crates/arb-api/src/state.rs`
- Create: `crates/arb-api/src/routes/mod.rs`
- Create: `crates/arb-api/src/routes/status.rs`
- Create: `crates/arb-api/src/routes/opportunities.rs`
- Create: `crates/arb-api/src/routes/positions.rs`
- Create: `crates/arb-api/src/routes/metrics.rs`
- Create: `crates/arb-api/src/routes/markets.rs`
- Create: `crates/arb-api/src/routes/history.rs`
- Create: `crates/arb-api/src/routes/config.rs`
- Create: `crates/arb-api/src/routes/control.rs`
- Create: `crates/arb-api/src/routes/simulate.rs`
- Create: `crates/arb-api/src/ws.rs`
- Modify: `Cargo.toml` (workspace root — add arb-api to members + workspace.dependencies)

**Step 1: Create Cargo.toml for arb-api**

`crates/arb-api/Cargo.toml`:
```toml
[package]
name = "arb-api"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
description = "Polymarket arb REST + WebSocket API server"

[[bin]]
name = "arb-api"
path = "src/main.rs"

[dependencies]
arb-core = { workspace = true }
arb-data = { workspace = true }
arb-strategy = { workspace = true }
arb-simulation = { workspace = true }
arb-execution = { workspace = true }
arb-risk = { workspace = true }
arb-monitor = { workspace = true }
polymarket-client-sdk = { workspace = true }
tokio = { workspace = true, features = ["rt-multi-thread", "macros", "time", "sync", "signal"] }
async-trait = { workspace = true }
rust_decimal = { workspace = true }
rust_decimal_macros = { workspace = true }
chrono = { workspace = true }
uuid = { workspace = true }
tracing = { workspace = true }
tracing-subscriber = { workspace = true }
serde = { workspace = true }
serde_json = { workspace = true }
anyhow = { workspace = true }
toml = { workspace = true }
axum = { version = "0.8", features = ["ws", "json"] }
axum-extra = { version = "0.10", features = ["typed-header"] }
tower = { version = "0.5", features = ["util"] }
tower-http = { version = "0.6", features = ["cors", "trace"] }
futures-util = "0.3"
```

**Step 2: Add to workspace Cargo.toml**

Add `"crates/arb-api"` to `[workspace] members` array. Add `arb-api = { path = "crates/arb-api" }` to `[workspace.dependencies]`.

**Step 3: Create AppState**

`crates/arb-api/src/state.rs`:

The AppState holds shared references to the arb engine components:
- `market_cache: Arc<MarketCache>` — thread-safe DashMap cache
- `risk_limits: Arc<Mutex<RiskLimits>>` — risk manager (mutable)
- `config: Arc<RwLock<ArbConfig>>` — config (read-heavy, write-rare)
- `ws_tx: broadcast::Sender<String>` — WebSocket broadcast channel
- `opportunities: Arc<RwLock<Vec<Opportunity>>>` — latest detected opportunities
- `execution_history: Arc<RwLock<Vec<ExecutionReport>>>` — trade history

AppState must implement `Clone` (Axum requirement) and be constructed in main.

**Step 4: Create route handlers**

Each route file follows this pattern:
```rust
use axum::{extract::State, Json};
use crate::state::AppState;

pub async fn handler(State(state): State<AppState>) -> Json<serde_json::Value> {
    // read from state, return JSON
}
```

Routes to implement:
- `GET /api/status` → `routes::status::get_status` — returns daemon mode, kill switch state, uptime, market count
- `GET /api/opportunities` → `routes::opportunities::list` — returns current opportunities vec
- `GET /api/positions` → `routes::positions::list` — returns all positions from risk_limits.positions()
- `GET /api/metrics` → `routes::metrics::get_metrics` — returns brier_score, drawdown_pct, execution_quality, pnl_by_type, total_pnl, trade_count
- `GET /api/markets` → `routes::markets::list` — returns all cached MarketState
- `GET /api/markets/:id` → `routes::markets::get_one` — returns single market by condition_id
- `GET /api/history?limit=N` → `routes::history::list` — returns last N execution reports
- `GET /api/config` → `routes::config::get_config` — returns current ArbConfig as JSON
- `PUT /api/config` → `routes::config::update_config` — update specific config sections
- `POST /api/kill` → `routes::control::kill` — activate kill switch with reason from body
- `POST /api/resume` → `routes::control::resume` — deactivate kill switch
- `POST /api/simulate/:condition_id` → `routes::simulate::run_simulation` — run MC + PF

**Step 5: Create WebSocket handler**

`crates/arb-api/src/ws.rs`:

- Upgrade HTTP to WebSocket connection via `axum::extract::ws::WebSocketUpgrade`
- Subscribe to `broadcast::Receiver` from AppState
- Forward each event as JSON text frame to the client
- Handle client disconnection gracefully
- Support multiple concurrent clients

**Step 6: Create main.rs**

Wire everything together:
```rust
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 1. Init tracing
    // 2. Load ArbConfig
    // 3. Create shared state (AppState)
    // 4. Build Axum router with CORS + routes
    // 5. Spawn background task that runs the arb engine loop
    //    and broadcasts events via ws_tx
    // 6. axum::serve on 0.0.0.0:8080
}
```

Router setup:
```rust
let app = Router::new()
    .route("/api/status", get(routes::status::get_status))
    .route("/api/opportunities", get(routes::opportunities::list))
    .route("/api/positions", get(routes::positions::list))
    .route("/api/metrics", get(routes::metrics::get_metrics))
    .route("/api/markets", get(routes::markets::list))
    .route("/api/markets/{id}", get(routes::markets::get_one))
    .route("/api/history", get(routes::history::list))
    .route("/api/config", get(routes::config::get_config).put(routes::config::update_config))
    .route("/api/kill", post(routes::control::kill))
    .route("/api/resume", post(routes::control::resume))
    .route("/api/simulate/{condition_id}", post(routes::simulate::run_simulation))
    .route("/ws", get(ws::ws_handler))
    .layer(CorsLayer::permissive())
    .layer(TraceLayer::new_for_http())
    .with_state(state);
```

**Step 7: Verify compilation**

Run: `cargo check -p arb-api`
Expected: Compiles with zero errors

**Step 8: Commit**

```bash
git add crates/arb-api/ Cargo.toml Cargo.lock
git commit -m "feat: add arb-api crate with Axum REST + WebSocket server"
```

---

### Task 2: Scaffold Next.js Frontend

**Files:**
- Create: `frontend/` directory (via create-next-app)
- Create: `frontend/package.json`
- Create: `frontend/tsconfig.json`
- Create: `frontend/tailwind.config.ts`
- Create: `frontend/src/app/layout.tsx`
- Create: `frontend/src/app/page.tsx`
- Create: `frontend/src/app/globals.css`
- Create: `frontend/src/lib/api.ts`

**Step 1: Create Next.js project**

```bash
cd /mnt/c/Users/shiho/Desktop/projects/claude-poly-arbi
npx create-next-app@latest frontend --typescript --tailwind --eslint --app --src-dir --import-alias "@/*" --no-git --use-pnpm
```

**Step 2: Install dependencies**

```bash
cd frontend
pnpm add zustand lightweight-charts echarts echarts-for-react
pnpm add -D @types/node
```

**Step 3: Install shadcn/ui**

```bash
cd frontend
pnpm dlx shadcn@latest init
# When prompted: style=new-york, baseColor=zinc, css variables=yes
pnpm dlx shadcn@latest add button card table badge tabs separator input label select switch dialog sheet toast dropdown-menu scroll-area
```

**Step 4: Create API client utility**

`frontend/src/lib/api.ts`:
```typescript
const API_BASE = process.env.NEXT_PUBLIC_API_URL || "http://localhost:8080";

export async function fetchApi<T>(path: string, options?: RequestInit): Promise<T> {
  const res = await fetch(`${API_BASE}${path}`, {
    headers: { "Content-Type": "application/json" },
    ...options,
  });
  if (!res.ok) throw new Error(`API error: ${res.status} ${res.statusText}`);
  return res.json();
}
```

**Step 5: Set up base layout**

`frontend/src/app/layout.tsx` — Dark theme, sidebar navigation with links to all pages, responsive.

`frontend/src/app/globals.css` — Import tailwind, set dark background defaults.

**Step 6: Verify build**

```bash
cd frontend && pnpm build
```

Expected: Builds successfully

**Step 7: Commit**

```bash
git add frontend/
git commit -m "feat: scaffold Next.js frontend with shadcn/ui, charts, Zustand"
```

---

### Task 3: Create Shared TypeScript Types

**Files:**
- Create: `frontend/src/lib/types.ts`

Mirror all arb-core Rust types as TypeScript interfaces. These must match the JSON serialization of the Rust structs exactly.

```typescript
// Core types matching arb-core serde output
export interface OrderbookLevel {
  price: string; // Decimal serializes as string
  size: string;
}

export interface OrderbookSnapshot {
  token_id: string;
  bids: OrderbookLevel[];
  asks: OrderbookLevel[];
  timestamp: string; // ISO 8601
}

export interface MarketState {
  condition_id: string;
  question: string;
  outcomes: string[];
  token_ids: string[];
  outcome_prices: string[]; // Decimal as string
  orderbooks: OrderbookSnapshot[];
  volume_24hr: string | null;
  liquidity: string | null;
  active: boolean;
  neg_risk: boolean;
}

export type ArbType = "IntraMarket" | "CrossMarket" | "MultiOutcome";
export type Side = "Buy" | "Sell";
export type TradingMode = "Paper" | "Live";
export type FillStatus = "FullyFilled" | "PartiallyFilled" | "Rejected" | "Cancelled";

export interface TradeLeg {
  token_id: string;
  side: Side;
  target_price: string;
  target_size: string;
  vwap_estimate: string;
}

export interface Opportunity {
  id: string; // UUID
  arb_type: ArbType;
  markets: string[];
  legs: TradeLeg[];
  gross_edge: string;
  net_edge: string;
  estimated_vwap: string[];
  confidence: number;
  size_available: string;
  detected_at: string;
}

export interface LegReport {
  order_id: string;
  token_id: string;
  side: Side;
  expected_vwap: string;
  actual_fill_price: string;
  filled_size: string;
  status: FillStatus;
}

export interface ExecutionReport {
  opportunity_id: string;
  legs: LegReport[];
  realized_edge: string;
  slippage: string;
  total_fees: string;
  timestamp: string;
  mode: TradingMode;
}

export interface Position {
  token_id: string;
  condition_id: string;
  size: string;
  avg_entry_price: string;
  current_price: string;
  unrealized_pnl: string;
}

export type RiskDecision =
  | { Approve: { max_size: string } }
  | { ReduceSize: { new_size: string; reason: string } }
  | { Reject: { reason: string } };

export interface MetricsSnapshot {
  brier_score: number;
  drawdown_pct: number;
  execution_quality: string;
  total_pnl: string;
  daily_pnl: string;
  trade_count: number;
  pnl_by_type: Record<string, string>;
  current_exposure: string;
  peak_equity: string;
  current_equity: string;
}

export interface StatusResponse {
  mode: TradingMode;
  kill_switch_active: boolean;
  kill_switch_reason: string | null;
  market_count: number;
  uptime_secs: number;
}

export interface WsEvent {
  type: "opportunity_detected" | "trade_executed" | "position_update" | "metrics_update" | "kill_switch_change" | "market_update" | "alert";
  data: unknown;
}
```

**Commit:**
```bash
git add frontend/src/lib/types.ts
git commit -m "feat: add TypeScript type definitions mirroring arb-core Rust types"
```

---

### Task 4: Build Shared UI Components

**Files:**
- Create: `frontend/src/components/kill-switch-banner.tsx`
- Create: `frontend/src/components/connection-status.tsx`
- Create: `frontend/src/components/metric-card.tsx`
- Create: `frontend/src/components/sidebar.tsx`
- Create: `frontend/src/components/orderbook-depth.tsx`
- Create: `frontend/src/components/pnl-chart.tsx`
- Create: `frontend/src/components/risk-gauge.tsx`
- Create: `frontend/src/components/data-table.tsx`
- Create: `frontend/src/components/opportunity-row.tsx`

**KillSwitchBanner**: Full-width red banner fixed to top when `kill_switch_active === true`. Shows reason and a "Resume" button that POSTs to `/api/resume`.

**ConnectionStatus**: Small dot indicator (green=connected, yellow=reconnecting, red=disconnected) for WebSocket state.

**MetricCard**: Card with large value, label, delta (up/down arrow + percentage change), optional ECharts sparkline. Props: `{ title, value, delta?, deltaType?: "positive"|"negative", sparkData?: number[] }`.

**Sidebar**: Left navigation with links to all pages: Dashboard, Opportunities, Positions, Performance, Markets, Controls, History, Simulation. Active page highlighted. Collapsible on mobile. Shows ConnectionStatus and mode badge (Paper/Live) at bottom.

**OrderbookDepth**: ECharts component rendering bid/ask depth chart. Props: `{ bids: OrderbookLevel[], asks: OrderbookLevel[] }`. Green for bids, red for asks, filled area chart.

**PnLChart**: TradingView Lightweight Charts area/line chart for equity curve. Props: `{ data: { time: string, value: number }[] }`. Dark theme matching the dashboard.

**RiskGauge**: ECharts gauge component for exposure/drawdown percentage. Props: `{ value: number, max: number, label: string, warningThreshold?: number, criticalThreshold?: number }`. Green→yellow→red coloring.

**DataTable**: Wrapper around shadcn/ui Table with sorting, filtering, and pagination. Generic over column definitions. Uses `@tanstack/react-table` if needed.

**OpportunityRow**: Table row component for an Opportunity. Shows arb_type badge, market names, edge in bps, confidence bar, size. Expandable to show legs detail.

**Commit:**
```bash
git add frontend/src/components/
git commit -m "feat: add shared dashboard UI components"
```

---

### Task 5: Integration Wiring — Engine Background Task + WS Broadcasting

**Files:**
- Modify: `crates/arb-api/src/main.rs`
- Modify: `crates/arb-api/src/state.rs`

**Purpose:** Wire the ArbEngine to run as a background tokio task inside arb-api. On each engine event (opportunity detected, trade executed, position change), serialize and broadcast via `ws_tx`.

The engine currently calls `self.monitor.log_opportunity(opp)` etc. We need to also push these events to the broadcast channel. Approach: After each significant action in the engine loop, clone relevant data and send via channel.

Since the engine owns mutable references to its internals, the cleanest approach is:
1. Run a modified engine loop inside a `tokio::spawn` task
2. The spawned task has access to `AppState` via `Arc`
3. After detecting opportunities: `ws_tx.send(serde_json::to_string(&WsEvent::OpportunityDetected(opps)))`
4. After executing: `ws_tx.send(serde_json::to_string(&WsEvent::TradeExecuted(report)))`
5. Periodically (every 1s): broadcast metrics + positions snapshot

**Verify:** `cargo check -p arb-api` compiles. Run `cargo run -p arb-api` and verify it starts without panic (will fail to connect to Polymarket without credentials, but should start the HTTP server).

**Commit:**
```bash
git add crates/arb-api/
git commit -m "feat: wire arb engine as background task with WS event broadcasting"
```

---

### Task 6: Create useWebSocket Hook

**Files:**
- Create: `frontend/src/hooks/use-websocket.ts`

Custom React hook that:
1. Connects to `ws://localhost:8080/ws` on mount
2. Auto-reconnects with exponential backoff (1s, 2s, 4s, 8s, max 30s)
3. Parses incoming JSON messages as `WsEvent`
4. Dispatches to Zustand store (Task 7)
5. Exposes connection state: `"connected" | "connecting" | "disconnected"`
6. Cleans up on unmount

```typescript
export function useWebSocket() {
  const [status, setStatus] = useState<"connected" | "connecting" | "disconnected">("disconnected");
  // ... WebSocket logic with reconnect
  return { status };
}
```

**Commit:**
```bash
git add frontend/src/hooks/
git commit -m "feat: add useWebSocket hook with auto-reconnect"
```

---

### Task 7: Create Zustand Store

**Files:**
- Create: `frontend/src/store/index.ts`

Central state store holding all dashboard data:

```typescript
interface DashboardStore {
  // Connection
  wsStatus: "connected" | "connecting" | "disconnected";
  setWsStatus: (status: string) => void;

  // Status
  status: StatusResponse | null;
  setStatus: (s: StatusResponse) => void;

  // Opportunities (ring buffer, keep last 200)
  opportunities: Opportunity[];
  addOpportunity: (o: Opportunity) => void;
  setOpportunities: (o: Opportunity[]) => void;

  // Positions
  positions: Position[];
  setPositions: (p: Position[]) => void;

  // Metrics
  metrics: MetricsSnapshot | null;
  setMetrics: (m: MetricsSnapshot) => void;

  // Markets
  markets: MarketState[];
  setMarkets: (m: MarketState[]) => void;

  // History
  history: ExecutionReport[];
  addExecution: (e: ExecutionReport) => void;
  setHistory: (h: ExecutionReport[]) => void;

  // Kill switch
  killSwitchActive: boolean;
  killSwitchReason: string | null;
  setKillSwitch: (active: boolean, reason?: string) => void;

  // WS event dispatcher
  handleWsEvent: (event: WsEvent) => void;
}
```

The `handleWsEvent` method routes each WS event type to the appropriate setter.

**Commit:**
```bash
git add frontend/src/store/
git commit -m "feat: add Zustand dashboard store with WS event dispatch"
```

---

### Task 8: Dashboard Page (`/`)

**Files:**
- Create: `frontend/src/app/page.tsx` (overwrite scaffold)
- Create: `frontend/src/app/providers.tsx`

The main dashboard overview. Layout:
- **Top**: KillSwitchBanner (conditional)
- **Row 1**: 5 MetricCards — Total P&L, Daily P&L, Open Positions, Active Opportunities, Brier Score
- **Row 2 left**: PnLChart (equity curve, 60% width)
- **Row 2 right**: RiskGauge x2 (exposure %, drawdown %, 40% width stacked)
- **Row 3**: Recent opportunities table (last 10) + top positions by exposure

Data: Fetches initial state via REST on mount, then updates via WebSocket.

Providers wrapper initializes useWebSocket + polls REST for initial data.

**Commit:**
```bash
git add frontend/src/app/
git commit -m "feat: build dashboard overview page with KPIs, P&L chart, risk gauges"
```

---

### Task 9: Opportunities Page (`/opportunities`)

**Files:**
- Create: `frontend/src/app/opportunities/page.tsx`

Live auto-updating table of arbitrage opportunities:
- Columns: Time, Type (badge), Markets, Gross Edge, Net Edge (bps), Confidence (bar), Size, Status
- Filters: arb_type dropdown, min edge slider, min confidence slider
- Click row → Sheet/drawer with leg details (token, side, target price, VWAP estimate)
- New opportunities animate in at top (green flash)
- Uses DataTable with shadcn Sheet for detail view

**Commit:**
```bash
git add frontend/src/app/opportunities/
git commit -m "feat: build live opportunities feed page"
```

---

### Task 10: Positions Page (`/positions`)

**Files:**
- Create: `frontend/src/app/positions/page.tsx`

- Positions table: Token, Market, Size, Entry, Current, Unrealized P&L (color-coded)
- Exposure pie chart (ECharts) — breakdown by market
- P&L attribution stacked bar (ECharts) — by arb type
- Total exposure KPI + unrealized P&L KPI at top

**Commit:**
```bash
git add frontend/src/app/positions/
git commit -m "feat: build positions page with exposure charts"
```

---

### Task 11: Performance Page (`/performance`)

**Files:**
- Create: `frontend/src/app/performance/page.tsx`

Analytics dashboard:
- Brier score trend line (ECharts) — with 0.25 "random" baseline marked
- Calibration scatter plot — predicted probability vs actual outcome frequency
- Execution quality trend — average fill quality over time
- Drawdown chart — underwater from peak equity (Lightweight Charts, inverted area)
- P&L by strategy — grouped bar chart (intra, cross, multi-outcome)

**Commit:**
```bash
git add frontend/src/app/performance/
git commit -m "feat: build performance analytics page"
```

---

### Task 12: Markets Page (`/markets`)

**Files:**
- Create: `frontend/src/app/markets/page.tsx`
- Create: `frontend/src/app/markets/[id]/page.tsx`

Market explorer:
- Searchable/filterable table of all markets (question, prices, volume, liquidity, active)
- Click market → detail page with:
  - Orderbook depth chart (OrderbookDepth component)
  - Outcome prices with visual bars
  - Volume/liquidity stats
  - Token IDs for reference

**Commit:**
```bash
git add frontend/src/app/markets/
git commit -m "feat: build market explorer with orderbook depth charts"
```

---

### Task 13: Controls Page (`/controls`)

**Files:**
- Create: `frontend/src/app/controls/page.tsx`

Trading control panel:
- Kill switch section: Big red/green button. When active: red with reason + activated_at. Resume button.
- Daemon status: Mode badge (Paper/Live), uptime, market count
- Config editor: Form sections matching ArbConfig structure:
  - General (trading_mode, log_level)
  - Polling (hot/warm/cold intervals + thresholds)
  - Strategy (min_edge_bps, enabled strategies, per-strategy configs)
  - Slippage (max_slippage_bps, split threshold, post_only, depth levels)
  - Risk (max_position, max_exposure, daily_loss_limit, max_orders)
  - Alerts (drawdown thresholds, calibration interval)
- Save button PUTs to `/api/config`

**Commit:**
```bash
git add frontend/src/app/controls/
git commit -m "feat: build controls page with kill switch and config editor"
```

---

### Task 14: History Page (`/history`)

**Files:**
- Create: `frontend/src/app/history/page.tsx`

Trade history:
- Paginated table: Time, Opportunity ID, Mode, Leg Count, Realized Edge, Slippage, Fees
- Slippage analysis scatter plot (ECharts): expected VWAP vs actual fill price per leg
- Click row → detail dialog with full leg-by-leg breakdown
- Filter by mode (Paper/Live), date range

**Commit:**
```bash
git add frontend/src/app/history/
git commit -m "feat: build trade history page with slippage analysis"
```

---

### Task 15: Simulation Page (`/simulation`)

**Files:**
- Create: `frontend/src/app/simulation/page.tsx`

Monte Carlo simulation runner:
- Market selector (dropdown of cached markets by question)
- Config inputs: path count, methods to run (MC, variance-reduced, particle filter)
- Run button POSTs to `/api/simulate/:condition_id`
- Results display: probability estimates, confidence intervals, ESS
- Distribution histogram (ECharts) for simulation outcomes
- Comparison table of all methods side-by-side

**Commit:**
```bash
git add frontend/src/app/simulation/
git commit -m "feat: build simulation page with Monte Carlo runner"
```

---

### Task 16: Final Integration + Build Verification

**Files:**
- Modify: `frontend/src/app/layout.tsx` (ensure all page links work)
- Modify: `CLAUDE.md` (add frontend build commands)

**Steps:**
1. `cargo build -p arb-api` — verify Rust backend compiles
2. `cd frontend && pnpm build` — verify Next.js builds without errors
3. `cargo test --workspace` — verify existing 97 tests still pass
4. `cargo clippy -p arb-api -- -D warnings` — lint the new crate
5. Update CLAUDE.md with frontend dev commands:
   ```
   # Frontend commands (from frontend/)
   pnpm install                     # install deps
   pnpm dev                         # dev server on :3000
   pnpm build                       # production build
   pnpm lint                        # ESLint check

   # API server (from repo root)
   cargo run -p arb-api             # API server on :8080
   ```

**Commit:**
```bash
git add -A
git commit -m "feat: complete frontend dashboard suite — 8 pages, REST+WS API"
```
