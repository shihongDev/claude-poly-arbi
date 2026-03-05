# Polymarket Arbitrage System

Institutional-grade arbitrage detection and execution engine for [Polymarket](https://polymarket.com), built in Rust with a Next.js monitoring dashboard.

## What It Does

The system continuously scans Polymarket's prediction markets for three types of pricing inefficiencies:

- **Intra-Market**: YES + NO prices that don't sum to $1.00 (e.g. YES@0.48 + NO@0.48 = $0.96, buy both for guaranteed $0.04 profit)
- **Cross-Market**: Correlated markets with inconsistent pricing (e.g. "Event by March" priced higher than "Event by June")
- **Multi-Outcome**: Events with multiple outcomes whose probabilities don't sum to 100%

All edge calculations use **VWAP** (Volume-Weighted Average Price) — not theoretical mid-prices — accounting for real orderbook depth, slippage, and fees.

## Architecture

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

9-crate Rust workspace + Next.js 16 frontend. See [architecture.md](architecture.md) for the full breakdown.

## Quick Start

### Prerequisites

- **Rust** 1.88.0+ (edition 2024)
- **Node.js** 20+ with **pnpm**
- A Polymarket wallet private key (for live trading — paper mode works without one)

### 1. Clone and build

```bash
git clone <repo-url> && cd claude-poly-arbi
cargo build --release
```

### 2. Configure (optional)

Create `~/.config/polymarket/arb-config.toml`:

```toml
[general]
trading_mode = "paper"
starting_equity = "10000"

[strategy]
min_edge_bps = 5

[risk]
max_total_exposure = "5000"
daily_loss_limit = "200"
```

All fields are optional — sensible defaults are used. See [configuration.md](configuration.md) for the full reference.

### 3. Add your private key (optional, for live trading)

```bash
echo "0xYOUR_PRIVATE_KEY" > secrets/key.txt
```

### 4. Start everything

```bash
bash scripts/dev.sh
```

This launches the API server on `:8080` and the frontend on `:3000`.

Or start them separately:

```bash
# Terminal 1: API server
cargo run -p arb-api

# Terminal 2: Frontend
cd frontend && pnpm install && pnpm dev
```

### 5. Open the dashboard

Navigate to [http://localhost:3000](http://localhost:3000). Markets load within ~15 seconds.

## Development Commands

### Rust

```bash
cargo build                    # dev build
cargo build --release          # optimized (thin LTO, stripped)
cargo fmt --check              # format check
cargo clippy -- -D warnings    # lint (all warnings = errors)
cargo test --workspace         # 229 tests
cargo run -p arb-api           # API server on :8080
cargo run -p arb-daemon -- scan --comprehensive  # one-shot market scan
```

### Frontend

```bash
cd frontend
pnpm install                   # install deps
pnpm dev                       # dev server on :3000
pnpm build                     # production build
npx tsc --noEmit               # type check
pnpm lint                      # ESLint
```

> **WSL2 note**: Use `NODE_OPTIONS="--max-old-space-size=4096" pnpm build` to avoid OOM during production builds.

## Documentation

| Document | Contents |
|----------|----------|
| [architecture.md](architecture.md) | Crate structure, dependency graph, design patterns |
| [api-reference.md](api-reference.md) | All 17 REST endpoints + WebSocket protocol |
| [frontend-guide.md](frontend-guide.md) | Pages, components, store, design system |
| [configuration.md](configuration.md) | TOML config, env vars, authentication |

## Key Design Decisions

- **VWAP-first edge calculation**: Never uses mid-price. All detectors walk the orderbook to compute real fill prices.
- **Paper mode by default**: Live trading requires explicit `--live` flag and a valid private key.
- **File-based kill switch**: `~/.config/polymarket/KILL_SWITCH` — can be triggered via API, CLI, or simply `touch KILL_SWITCH` in a shell.
- **Generation-based change detection**: Markets track update generations so detectors only scan changed orderbooks each cycle.
- **Fee model**: 2% on notional traded, applied uniformly across all strategies.

## Project Structure

```
claude-poly-arbi/
├── crates/
│   ├── arb-core/          # Shared types, traits, config, errors
│   ├── arb-data/          # Market cache, orderbook processing, polling
│   ├── arb-strategy/      # Arbitrage detectors, edge calculation
│   ├── arb-simulation/    # Monte Carlo, particle filter, copula
│   ├── arb-execution/     # Paper + live trade executors, auth
│   ├── arb-risk/          # Risk limits, kill switch, VaR, metrics
│   ├── arb-monitor/       # Alerts, model health, structured logging
│   ├── arb-daemon/        # CLI binary (scan, run, status, kill, etc.)
│   └── arb-api/           # Axum REST + WS server
├── frontend/              # Next.js 16 dashboard
├── scripts/               # dev.sh launch script
├── secrets/               # Private key (git-ignored)
├── docs/                  # This documentation
└── polymarket-cli-main/   # Original Polymarket CLI (reference)
```

## License

MIT
