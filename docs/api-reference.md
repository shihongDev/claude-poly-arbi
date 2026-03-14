# API Reference

The `arb-api` crate runs an Axum HTTP server on `0.0.0.0:8080` providing REST endpoints and a WebSocket broadcast channel.

**CORS**: Permissive (all origins, methods, headers).
**Content-Type**: All responses are `application/json`.
**Error format**: `{"error": "description"}` with appropriate HTTP status code.

---

## Table of Contents

- [Status](#get-apistatus)
- [Opportunities](#get-apiopportunities)
- [Positions](#get-apipositions)
- [Metrics](#get-apimetrics)
- [Markets](#get-apimarkets)
- [Market Detail](#get-apimarketsid)
- [History](#get-apihistory)
- [Config](#get-apiconfig)
- [Update Config](#put-apiconfig)
- [Place Order](#post-apiorder)
- [Kill Switch](#post-apikill)
- [Resume](#post-apiresume)
- [Simulate](#post-apisimulatecondition_id)
- [Sandbox Detect](#post-apisandboxdetect)
- [Sandbox Backtest](#post-apisandboxbacktest)
- [Stress Test](#post-apistress-test)
- [Simulation Status](#get-apisimulationstatus)
- [WebSocket](#websocket)
- [Engine Loop](#engine-loop)

---

## GET /api/status

Daemon liveness summary.

**Response:**
```json
{
  "mode": "Paper",
  "kill_switch_active": false,
  "kill_switch_reason": null,
  "market_count": 1234,
  "uptime_secs": 3600
}
```

| Field | Type | Description |
|-------|------|-------------|
| `mode` | `"Paper" \| "Live"` | Current trading mode |
| `kill_switch_active` | `boolean` | Whether engine is halted |
| `market_count` | `integer` | Markets in cache |
| `uptime_secs` | `integer` | Seconds since server start |

---

## GET /api/opportunities

Detected arbitrage opportunities (up to 200, newest first).

**Headers:** `Cache-Control: max-age=5`

**Response:** Array of `Opportunity`:
```json
[
  {
    "id": "uuid-v4",
    "arb_type": "IntraMarket",
    "markets": ["condition_id_1"],
    "legs": [
      {
        "token_id": "string",
        "side": "Buy",
        "target_price": "0.48",
        "target_size": "100",
        "vwap_estimate": "0.482"
      }
    ],
    "gross_edge": "0.02",
    "net_edge": "0.018",
    "estimated_vwap": ["0.482"],
    "confidence": 0.75,
    "size_available": "100",
    "detected_at": "2026-03-04T12:00:00Z"
  }
]
```

| Field | Type | Description |
|-------|------|-------------|
| `arb_type` | `"IntraMarket" \| "CrossMarket" \| "MultiOutcome"` | Strategy type |
| `legs` | `TradeLeg[]` | Individual trade legs |
| `gross_edge` | `string (Decimal)` | Edge before fees |
| `net_edge` | `string (Decimal)` | Edge after fees and slippage |
| `confidence` | `number (f64)` | Ensemble estimator confidence (0-1) |

---

## GET /api/positions

All tracked positions.

**Response:** Array of `Position`:
```json
[
  {
    "token_id": "string",
    "condition_id": "string",
    "size": "100",
    "avg_entry_price": "0.50",
    "current_price": "0.55",
    "unrealized_pnl": "5.00"
  }
]
```

---

## GET /api/metrics

Pre-computed risk and performance metrics. Cached by the engine loop — no lock acquisition on this call.

**Headers:** `Cache-Control: max-age=5`

**Response:**
```json
{
  "brier_score": 0.15,
  "drawdown_pct": 2.3,
  "execution_quality": "0.98",
  "total_pnl": "142.50",
  "daily_pnl": "12.00",
  "trade_count": 47,
  "current_exposure": "3400.00",
  "peak_equity": "10142.50",
  "current_equity": "10142.50",
  "pnl_by_type": {
    "IntraMarket": "100.00",
    "CrossMarket": "30.00",
    "MultiOutcome": "12.50"
  }
}
```

---

## GET /api/markets

Active markets with optional filtering.

**Query Parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `limit` | `integer` | 500 | Max markets (max 2000) |
| `with_orderbooks` | `boolean` | false | Only return markets with orderbook data |

**Headers:** `Cache-Control: max-age=5`

**Response:** Array of `MarketState`:
```json
[
  {
    "condition_id": "0xabc...",
    "question": "Will X happen?",
    "outcomes": ["Yes", "No"],
    "token_ids": ["tok_yes", "tok_no"],
    "outcome_prices": ["0.65", "0.35"],
    "orderbooks": [
      {
        "token_id": "tok_yes",
        "bids": [{"price": "0.64", "size": "500"}],
        "asks": [{"price": "0.66", "size": "300"}],
        "timestamp": "2026-03-04T12:00:00Z"
      }
    ],
    "volume_24hr": "50000",
    "liquidity": "10000",
    "active": true,
    "neg_risk": false,
    "best_bid": "0.64",
    "best_ask": "0.66",
    "spread": "0.02",
    "last_trade_price": "0.65",
    "description": "...",
    "end_date_iso": "2026-12-31",
    "slug": "will-x-happen",
    "one_day_price_change": "0.01",
    "event_id": "evt_123",
    "last_updated_gen": 42
  }
]
```

---

## GET /api/markets/{id}

Single market by condition ID.

**Path Parameter:** `id` — condition_id string

**Response (200):** Single `MarketState` object

**Response (404):**
```json
{"error": "market not found"}
```

---

## GET /api/history

Execution history (newest first).

**Query Parameters:**

| Param | Type | Default | Description |
|-------|------|---------|-------------|
| `limit` | `integer` | 50 | Max records to return |

**Response:** Array of `ExecutionReport`:
```json
[
  {
    "opportunity_id": "uuid-v4",
    "legs": [
      {
        "order_id": "uuid-v4",
        "token_id": "string",
        "condition_id": "string",
        "side": "Buy",
        "expected_vwap": "0.48",
        "actual_fill_price": "0.482",
        "filled_size": "100",
        "status": "FullyFilled"
      }
    ],
    "realized_edge": "0.018",
    "slippage": "0.002",
    "total_fees": "0.001",
    "timestamp": "2026-03-04T12:00:00Z",
    "mode": "Paper"
  }
]
```

| `status` values | Description |
|-----------------|-------------|
| `FullyFilled` | Order completely filled |
| `PartiallyFilled` | Partial fill |
| `Rejected` | Order rejected |
| `Cancelled` | Order cancelled |

---

## GET /api/config

Returns the full `ArbConfig` as JSON.

**Response:** See [configuration.md](configuration.md) for the complete structure.

---

## PUT /api/config

Update and persist the configuration. Validated before applying; takes effect on the next engine cycle.

**Request Body:** Full `ArbConfig` JSON (same shape as GET response).

**Response (200):**
```json
{"status": "updated"}
```

**Response (400):**
```json
{"errors": ["risk.max_position_per_market must not exceed max_total_exposure"]}
```

---

## POST /api/order

Place a manual paper trade order.

**Request Body:**
```json
{
  "token_id": "string",
  "condition_id": "string",
  "side": "Buy",
  "price": "0.55",
  "size": "100"
}
```

**Validation:**
- `price` must be in (0, 1]
- `size` must be > 0

**Response (200):** `ExecutionReport` (same shape as history entries). Also broadcasts `trade_executed` and `position_update` WebSocket events.

**Response (400):**
```json
{"error": "price must be in (0, 1]"}
```

---

## POST /api/kill

Activate the kill switch, halting the engine loop.

**Request Body:**
```json
{"reason": "Manual override"}
```

**Response (200):**
```json
{"status": "kill switch activated", "reason": "Manual override"}
```

Broadcasts a `kill_switch_change` WebSocket event.

---

## POST /api/resume

Deactivate the kill switch, resuming the engine loop.

**Request Body:** None required.

**Response (200):**
```json
{"status": "kill switch deactivated"}
```

Broadcasts a `kill_switch_change` WebSocket event.

---

## POST /api/simulate/{condition_id}

Run Monte Carlo + Particle Filter probability estimation for a specific market.

**Path Parameter:** `condition_id`

**Request Body** (all fields optional):
```json
{
  "num_paths": 10000,
  "volatility": 0.3,
  "drift": 0.0,
  "time_horizon": 1.0,
  "strike": 0.5,
  "particle_count": 500,
  "process_noise": 0.03,
  "observation_noise": 0.02
}
```

**Response (200):**
```json
{
  "condition_id": "0xabc...",
  "initial_price": 0.65,
  "monte_carlo": {
    "probability": 0.67,
    "standard_error": 0.005,
    "confidence_interval": [0.66, 0.68],
    "n_paths": 10000
  },
  "particle_filter": {
    "probability": [0.66],
    "confidence_interval": [[0.63, 0.69]],
    "method": "ParticleFilter"
  }
}
```

**Response (404):**
```json
{"error": "market not found"}
```

---

## POST /api/sandbox/detect

Run arbitrage detection with config overrides against the live market cache, without modifying the live config.

**Request Body:**
```json
{
  "config_overrides": {
    "min_edge_bps": 10,
    "intra_market_enabled": true,
    "cross_market_enabled": false,
    "multi_outcome_enabled": true,
    "intra_min_deviation": "0.005",
    "multi_min_deviation": "0.005",
    "max_slippage_bps": 50,
    "vwap_depth_levels": 5,
    "max_position_per_market": "500",
    "max_total_exposure": "2000",
    "daily_loss_limit": "100"
  }
}
```

All `config_overrides` fields are optional. An empty `{}` body uses the live config.

**Response (200):**
```json
{
  "opportunities": [],
  "detection_time_ms": 142,
  "markets_scanned": 1234,
  "config_used": {
    "min_edge_bps": 10,
    "intra_market_enabled": true,
    "cross_market_enabled": false,
    "multi_outcome_enabled": true,
    "intra_min_deviation": "0.005",
    "multi_min_deviation": "0.005"
  },
  "diagnostics": {
    "binary_markets": 800,
    "neg_risk_markets": 200,
    "markets_with_orderbooks": 600,
    "closest_binary_ask_sum": "1.02",
    "closest_binary_bid_sum": "0.98",
    "pre_filter_count": 3,
    "post_filter_count": 1
  }
}
```

---

## POST /api/sandbox/backtest

Replay execution history under different config parameters.

**Request Body:** Same shape as `/api/sandbox/detect`.

**Response (200):**
```json
{
  "total_trades_original": 100,
  "total_trades_filtered": 72,
  "trades_rejected": 28,
  "aggregate_pnl": "142.50",
  "aggregate_pnl_original": "98.00",
  "daily_breakdown": [
    {"date": "2026-03-01", "pnl": "40.00", "trade_count": 12}
  ],
  "trades": [
    {
      "opportunity_id": "uuid",
      "realized_edge": "0.02",
      "total_fees": "0.001",
      "net_pnl": "0.019",
      "timestamp": "2026-03-01T10:00:00Z",
      "included": true,
      "rejection_reason": null
    }
  ]
}
```

---

## POST /api/stress-test

Run a named stress scenario against current positions.

**Request Body:**
```json
{"scenario": "liquidity_shock"}
```

**Valid scenarios:**

| Scenario | Description |
|----------|-------------|
| `liquidity_shock` | 50% orderbook depth reduction |
| `correlation_spike` | Correlation increase to 0.85 |
| `flash_crash` | 15% adverse move across all positions |
| `kill_switch_delay` | 30-second activation delay |

**Response (200):**
```json
{
  "scenario": "liquidity_shock",
  "portfolio_impact": "$-250.00",
  "max_loss": "$-600.00",
  "positions_at_risk": 5,
  "var_before": "$-12.00",
  "var_after": "$-2400.00",
  "details": "Simulated 50% depth reduction across all active orderbooks"
}
```

**Error (500):**
```json
{"error": "stress test failed: insufficient position data"}
```

---

## GET /api/simulation/status

Current simulation engine status including probability estimates, convergence diagnostics, model health, and VaR summary.

**Response (200):**
```json
{
  "estimates": [
    {
      "condition_id": "0xabc...",
      "market_price": 0.65,
      "model_estimate": 0.65,
      "divergence": 0.0,
      "confidence_interval": [0.60, 0.70],
      "method": "Ensemble"
    }
  ],
  "convergence": {
    "paths_used": 10000,
    "target_paths": 10000,
    "standard_error": 0.005,
    "converged": true,
    "gelman_rubin": 1.01
  },
  "model_health": {
    "brier_score_30m": 0.18,
    "brier_score_24h": 0.18,
    "confidence_level": 1.0,
    "drift_detected": false,
    "status": "healthy"
  },
  "var_summary": {
    "var_95": "$-200.00",
    "var_99": "$-350.00",
    "cvar_95": "$-250.00",
    "method": "Parametric"
  }
}
```

**Error (500):**
```json
{"error": "simulation engine not initialized"}
```

---

## WebSocket

**Endpoint:** `GET /ws` (upgraded to WebSocket)

Broadcast channel with capacity 256 messages. All events use the envelope format:

```json
{
  "type": "<event_type>",
  "data": <payload>
}
```

### Event Types

| Type | Trigger | Data Shape |
|------|---------|------------|
| `markets_loaded` | Startup (phase 1 + phase 2) | `MarketState[]` (up to 500) |
| `market_update` | Orderbook refresh during polling | Single `MarketState` |
| `opportunities_batch` | End of engine cycle with opportunities | `Opportunity[]` |
| `trade_executed` | Paper trade execution | Single `ExecutionReport` |
| `position_update` | After every trade execution | `Position[]` (all positions) |
| `metrics_update` | Every engine cycle | Metrics object |
| `kill_switch_change` | Kill/resume API calls | `{"active": bool, "reason": "..."}` |

### Connection Notes

- Client messages are accepted but discarded (server-push only).
- Lagging subscribers are dropped to prevent broadcast buffer backpressure.
- Disconnected clients are explicitly cleaned up.

---

## Engine Loop

The engine runs as a background Tokio task with a 5-second tick interval.

### Startup (one-time)

1. **Phase 1**: Fetch all active market metadata via Polymarket Gamma API. Broadcast first 500 as `markets_loaded`.
2. **Phase 2**: Fetch orderbooks for up to 400 tokens concurrently. Broadcast markets with orderbooks as second `markets_loaded`.

### Main Loop (every 5 seconds)

1. **Kill switch check** — atomic read, skip cycle if active
2. **VWAP cache clear** — invalidate memoized slippage estimates
3. **Tiered polling** — refresh markets based on volume tier (hot=5s, warm=15s, cold=60s). Broadcast `market_update` per refresh.
4. **Change detection** — only scan markets whose orderbooks changed since last cycle
5. **Arb detection** — run enabled detectors (intra-market, multi-outcome, cross-market)
6. **VWAP edge refinement** — replace theoretical edge with actual VWAP-based edge
7. **Probability enrichment** — (if enabled) run ensemble estimator
8. **Price history recording** — append to SQLite
9. **Filtering** — retain opportunities with `net_edge_bps >= min_edge_bps`
10. **Auto-execution** — paper trade approved opportunities
11. **Broadcast** — `opportunities_batch`, `metrics_update`, `position_update`
12. **Maintenance** (every ~8 min) — clean stale poller entries, purge old price history

### Persistence

- Positions: `~/.config/polymarket/positions.json` (auto-saved every 60s + on shutdown)
- History: `~/.config/polymarket/history.json` (auto-saved every 60s + on shutdown)
- Price history: `~/.config/polymarket/price_history.db` (SQLite)
