# Configuration Reference

## Config File

**Path:** `~/.config/polymarket/arb-config.toml`

Loaded on startup via `ArbConfig::load()`. If the file doesn't exist, defaults are used. The config is validated before applying — invalid configs are rejected with descriptive errors.

The config can be viewed and modified at runtime via the REST API (`GET/PUT /api/config`) or through the frontend Controls page.

---

## Complete Config Reference

```toml
[general]
trading_mode = "paper"         # "paper" | "live"
log_level = "info"             # "error" | "warn" | "info" | "debug" | "trace"
log_format = "json"            # "json" | "compact"
# log_file = "/var/log/arb.log"  # optional file output
# state_file = "~/.config/polymarket/arb-state.json"  # optional state persistence
starting_equity = "10000"      # initial equity for drawdown calculations

[polling]
hot_interval_secs = 5          # refresh interval for high-volume markets
warm_interval_secs = 15        # refresh interval for medium-volume markets
cold_interval_secs = 60        # refresh interval for low-volume markets
hot_volume_threshold = 100000  # USD 24h volume threshold for "hot" tier
warm_volume_threshold = 10000  # USD 24h volume threshold for "warm" tier

[strategy]
min_edge_bps = 5               # minimum net edge in basis points to trade
intra_market_enabled = true    # detect YES+NO != $1.00
cross_market_enabled = true    # detect correlated market mispricings
multi_outcome_enabled = true   # detect multi-outcome probability sum != 100%

[strategy.intra_market]
min_deviation = "0.001"        # minimum YES+NO deviation from 1.00

[strategy.cross_market]
# correlation_file = "correlations.toml"  # path to correlation definitions
min_implied_edge = "0.02"      # minimum implied edge for cross-market arbs
use_copula_correlations = false # use t-copula tail dependence for confidence

[strategy.multi_outcome]
min_deviation = "0.003"        # minimum deviation from 100% probability sum

[slippage]
max_slippage_bps = 100         # maximum acceptable slippage in basis points
order_split_threshold = 500    # USD size above which orders are split
prefer_post_only = true        # use post-only orders to avoid taker fees
vwap_depth_levels = 10         # orderbook levels consumed in VWAP calculation

[risk]
max_position_per_market = "1000"  # USD cap per market
max_total_exposure = "5000"       # USD total portfolio exposure cap
daily_loss_limit = "200"          # USD daily loss before kill switch
max_open_orders = 20              # maximum simultaneous open orders

[simulation]
monte_carlo_paths = 10000            # number of Monte Carlo paths
importance_sampling_enabled = false  # enable importance sampling
particle_count = 500                 # particle filter count
variance_reduction = ["antithetic"]  # variance reduction techniques
probability_estimation_enabled = false  # run ensemble estimator per opportunity

[alerts]
drawdown_warning_pct = 5.0     # % drawdown for warning alert
drawdown_critical_pct = 10.0   # % drawdown for critical alert
calibration_check_interval_mins = 60  # Brier score check interval
```

### Field Notes

- `Decimal` fields are serialized as strings in TOML (e.g. `"0.001"`, `"1000"`)
- `u64`, `usize`, and `f64` fields are plain numbers
- The `correlation_file` path is relative to `~/.config/polymarket/`
- `variance_reduction` accepts: `"antithetic"`, `"control_variate"`

### Validation Rules

- All risk `Decimal` values must be positive
- `max_position_per_market` must not exceed `max_total_exposure`
- All polling intervals must be > 0
- `vwap_depth_levels` must be > 0

---

## Environment Variables

### Rust Backend

| Variable | Used In | Purpose | Default |
|----------|---------|---------|---------|
| `POLYMARKET_KEY_FILE` | `arb-execution` | Path to private key file | `secrets/key.txt` |
| `RUST_LOG` | `arb-api`, `arb-monitor` | Tracing filter override | `arb_api=debug,tower_http=debug` |

### Frontend

| Variable | Used In | Purpose | Default |
|----------|---------|---------|---------|
| `NEXT_PUBLIC_API_URL` | `api.ts` | REST API base URL | `http://localhost:8080` |
| `NEXT_PUBLIC_WS_URL` | `api.ts` | WebSocket base URL | `ws://localhost:8080` |

Set frontend variables in `frontend/.env.local`:

```
NEXT_PUBLIC_API_URL=http://localhost:8080
NEXT_PUBLIC_WS_URL=ws://localhost:8080
```

---

## Authentication

### Private Key Setup

Place your Polymarket wallet private key in `secrets/key.txt`:

```bash
echo "0xYOUR_64_HEX_CHAR_PRIVATE_KEY" > secrets/key.txt
```

**Accepted formats:**
- With prefix: `0x` + 64 hex characters
- Without prefix: 64 hex characters

The `secrets/` directory is git-ignored.

### Key Resolution Priority

1. Explicit path argument (CLI `--key-file` flag)
2. `POLYMARKET_KEY_FILE` environment variable
3. `secrets/key.txt` (relative to working directory)

### Authentication Flow

```
Private key file
    → LocalSigner (alloy)
    → Set chain: Polygon mainnet (chain ID 137)
    → EIP-712 signature exchange with Polymarket CLOB API
    → Client<Authenticated<Normal>>
```

> **Paper mode** works without a private key. Only live trading requires authentication.

---

## Kill Switch

File-based at `~/.config/polymarket/KILL_SWITCH`.

### Activation Methods

| Method | Command |
|--------|---------|
| REST API | `POST /api/kill {"reason": "..."}` |
| CLI | `cargo run -p arb-daemon -- kill` |
| Frontend | Controls page → Kill Switch card |
| Shell | `touch ~/.config/polymarket/KILL_SWITCH` |

### What Happens

When active, the engine loop skips all detection and execution. It only checks the kill switch and sleeps for 5 seconds. All positions are preserved (not liquidated).

### Deactivation

| Method | Command |
|--------|---------|
| REST API | `POST /api/resume` |
| CLI | `cargo run -p arb-daemon -- resume` |
| Frontend | Controls page → Resume Trading button |
| Shell | `rm ~/.config/polymarket/KILL_SWITCH` |

---

## Correlation File Format

For cross-market arbitrage, define market relationships in a TOML file:

```toml
[[pairs]]
condition_id_a = "0xabc..."
condition_id_b = "0xdef..."
relationship = "implied_by"

[[pairs]]
condition_id_a = "0x123..."
condition_id_b = "0x456..."
relationship = "mutually_exclusive"

[[pairs]]
condition_id_a = "0x789..."
condition_id_b = "0xabc..."
relationship = "custom"
constraint = "sum_leq"
bound = "1.0"
```

**Relationship types:**

| Type | Meaning | Example |
|------|---------|---------|
| `implied_by` | If A is true, B must be true | "Trump wins Iowa" → "Trump wins election" |
| `mutually_exclusive` | Both can't be true | "Democrat wins" vs "Republican wins" |
| `exhaustive` | At least one must be true | Covers all possible outcomes |
| `custom` | User-defined constraint + bound | Custom probability constraints |

Set the file path in config:

```toml
[strategy.cross_market]
correlation_file = "correlations.toml"
```

---

## Persisted State Files

| File | Contents | Auto-save |
|------|----------|-----------|
| `~/.config/polymarket/arb-config.toml` | Configuration | On `PUT /api/config` |
| `~/.config/polymarket/positions.json` | Active positions | Every 60s + shutdown |
| `~/.config/polymarket/history.json` | Execution history | Every 60s + shutdown |
| `~/.config/polymarket/price_history.db` | Price history (SQLite) | Every engine cycle |
| `~/.config/polymarket/KILL_SWITCH` | Kill switch state | On activation |

---

## Build Configuration

### Rust (`Cargo.toml`)

```toml
[workspace.package]
edition = "2024"
rust-version = "1.88.0"

[profile.release]
lto = "thin"        # link-time optimization
codegen-units = 1   # single codegen unit for better optimization
strip = true        # strip debug symbols
```

### Frontend (`package.json`)

```json
{
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "eslint"
  }
}
```

> **WSL2**: Use `NODE_OPTIONS="--max-old-space-size=4096" pnpm build` for production builds.

---

## Launch Script

`scripts/dev.sh` starts both the API server and frontend:

```bash
bash scripts/dev.sh
```

What it does:
1. Kills stale processes on ports 8080 and 3000
2. Removes `frontend/.next/dev/lock` if stale
3. Checks prerequisites (`cargo`, `pnpm`/`npm`)
4. Installs frontend deps if `node_modules/` is absent
5. Starts `cargo run -p arb-api` (background, port 8080)
6. Starts `pnpm dev` (background, port 3000)
7. Traps EXIT/INT/TERM to kill both processes and clean up
