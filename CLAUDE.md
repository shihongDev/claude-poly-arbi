# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

Polymarket arbitrage system built in Rust. Extends an existing Polymarket CLI (`polymarket-cli-main/polymarket-cli-main/`) with an institutional-grade arbitrage detection and execution engine.

The CLI is a complete, working tool for browsing markets, placing orders, and managing positions on Polymarket via their CLOB and on-chain APIs. The arb system (8-crate workspace: arb-core, arb-data, arb-strategy, arb-simulation, arb-execution, arb-risk, arb-monitor, arb-daemon) is planned but not yet scaffolded.

## Build & Development Commands

All commands run from `polymarket-cli-main/polymarket-cli-main/`:

```bash
cargo build                    # dev build
cargo build --release          # release build (thin LTO, stripped)
cargo fmt --check              # format check (CI enforced)
cargo clippy -- -D warnings    # lint, all warnings are errors (CI enforced)
cargo test                     # all tests (unit + integration)
cargo test --lib               # unit tests only
cargo test --test cli_integration  # integration tests only
cargo install --path .         # install binary locally
```

Rust edition 2024, MSRV 1.88.0.

## Architecture

### CLI Structure (polymarket-cli)

Single-crate project (not yet a workspace). Three-layer pattern per command group:

1. **`src/commands/<group>.rs`** — `clap` Args/Subcommand enums + `execute()` async fn that calls the SDK
2. **`src/output/<group>.rs`** — `print_*` functions for Table and Json output formats
3. **`src/main.rs`** — `run()` dispatches to each `execute()` based on parsed `Commands` enum

Adding a new command group: create both `commands/<group>.rs` and `output/<group>.rs`, register in their respective `mod.rs`, add variant to `Commands` enum in `main.rs`.

### Key Modules

- `src/auth.rs` — Wallet resolution (`resolve_signer()`), authenticated CLOB client, RPC provider (Polygon via `https://polygon.drpc.org`)
- `src/config.rs` — Config file at `~/.config/polymarket/config.json` (private_key, chain_id, signature_type)
- `src/shell.rs` — Interactive REPL via rustyline, re-parses each line as full CLI invocation

### Auth Priority

1. `--private-key` CLI flag
2. `POLYMARKET_PRIVATE_KEY` env var
3. Config file (`~/.config/polymarket/config.json`)

### SDK Clients

- `polymarket_client_sdk::clob::Client` — Order books, pricing, trading, balances (most important for arb)
- `polymarket_client_sdk::gamma::Client` — Market metadata, events, tags, series
- `polymarket_client_sdk::data::Client` — On-chain positions, trades, leaderboards
- `polymarket_client_sdk::bridge::Client` — Cross-chain deposits
- `polymarket_client_sdk::ctf` — Conditional token framework (split/merge/redeem via Alloy)

### Key Dependencies

- `alloy` — Ethereum/EVM interaction (Polygon mainnet)
- `clap` (derive) — CLI parsing
- `tokio` (multi-thread) — Async runtime
- `anyhow` — Error handling
- `rust_decimal` — Price arithmetic
- `tabled` — Table output rendering

## Output Format Convention

Every command supports `--output table` (default) and `--output json`. Table errors go to stderr; JSON errors go to stdout as `{"error": "..."}`. Non-zero exit code either way.

## Planned Arb Workspace

8 crates planned (see `architecture.md` and project memory for details):
- **arb-core**: Shared types, traits
- **arb-data**: Market data fetching/normalization
- **arb-strategy**: Arbitrage detection (intra-market, cross-market, multi-outcome)
- **arb-simulation**: Monte Carlo + importance sampling + particle filter + copula + ABM
- **arb-execution**: Order placement engine
- **arb-risk**: Risk management, position sizing
- **arb-monitor**: Dashboards, alerting
- **arb-daemon**: Continuous polling daemon

Key design constraint: Edge must be VWAP-based, not theoretical mid-price. Paper trading mode first, live trading behind `--live` flag.

## Workflow Orchestration

### 1. Plan Node Default
- Enter plan mode for ANY non-trivial task (3+ steps or architectural decisions)
- If something goes sideways, STOP and re-plan immediately — don't keep pushing
- Use plan mode for verification steps, not just building
- Write detailed specs upfront to reduce ambiguity

### 2. Subagent Strategy
- Use subagents liberally to keep main context window clean
- Offload research, exploration, and parallel analysis to subagents
- For complex problems, throw more compute at it via subagents
- One tack per subagent for focused execution

### 3. Self-Improvement Loop
- After ANY correction from the user: update `tasks/lessons.md` with the pattern
- Write rules for yourself that prevent the same mistake
- Ruthlessly iterate on these lessons until mistake rate drops
- Review lessons at session start for relevant project

### 4. Verification Before Done
- Never mark a task complete without proving it works
- Diff behavior between main and your changes when relevant
- Ask yourself: "Would a staff engineer approve this?"
- Run tests, check logs, demonstrate correctness

### 5. Demand Elegance (Balanced)
- For non-trivial changes: pause and ask "is there a more elegant way?"
- If a fix feels hacky: "Knowing everything I know now, implement the elegant solution"
- Skip this for simple, obvious fixes — don't over-engineer
- Challenge your own work before presenting it

### 6. Autonomous Bug Fixing
- When given a bug report: just fix it. Don't ask for hand-holding
- Point at logs, errors, failing tests — then resolve them
- Zero context switching required from the user
- Go fix failing CI tests without being told how

## Task Management

- **Plan First**: Write plan to `tasks/todo.md` with checkable items
- **Verify Plan**: Check in before starting implementation
- **Track Progress**: Mark items complete as you go
- **Explain Changes**: High-level summary at each step
- **Document Results**: Add review section to `tasks/todo.md`
- **Capture Lessons**: Update `tasks/lessons.md` after corrections

## Core Principles

- **Simplicity First**: Make every change as simple as possible. Impact minimal code.
- **No Laziness**: Find root causes. No temporary fixes. Senior developer standards.
- **Minimal Impact**: Changes should only touch what's necessary. Avoid introducing bugs.
