# Comprehensive Polymarket Scanner Design

## Goal

Scan ALL active Polymarket markets for real arbitrage opportunities. Fetch every market, pull every orderbook, run all detectors, compute VWAP edges at realistic sizes, and output actionable ranked opportunities via CLI.

## Data Fetching Pipeline

### Paginated Market Fetch

The Gamma API paginates at ~100 markets per request. We paginate until empty:

```
fetch_all_active_markets()
  → loop: offset += limit until response.len() < limit
  → filter: active=true, closed=false, has clobTokenIds
  → group by event slug → HashMap<String, Vec<Market>>
  → classify: Binary (2 tokens) vs MultiOutcome (3+) vs NegRisk
```

Expected volume: ~500-800 markets, ~1000-1600 token IDs.

### Concurrent Orderbook Fetch

```
fetch_all_orderbooks(token_ids: Vec<String>)
  → tokio semaphore (max 10 concurrent)
  → 50ms delay between batches for rate limiting
  → HashMap<TokenId, OrderbookSnapshot>
  → warn on failures, continue with partial data
```

Estimated time: ~2-3 minutes for full scan.

## Analysis Pipeline

Four analysis passes:

### 1. Intra-Market (binary YES/NO)

For each binary market: does buying YES at best ask + NO at best ask cost less than $1.00?
Compute VWAP cost at configurable size tiers (default: $100, $500, $1K, $5K).

### 2. Multi-Outcome (NegRisk events)

For each NegRisk event with 3+ outcomes:
- Buy-all arb: sum(best_asks) < 1.00?
- Sell-all arb: sum(best_bids) > 1.00?
- VWAP analysis at multiple size tiers across all legs.

### 3. Deadline Monotonicity (cumulative "by..." events)

For "by deadline" event series (non-NegRisk, cumulative):
- Later deadline should always cost >= earlier deadline.
- Flag inversions (later is cheaper) as potential opportunity.

### 4. Spread/Depth Profiling

For every market: bid-ask spread, depth at 3 price levels, liquidity score, VWAP slippage curve.
Not arb detection — market structure intel that helps size real opportunities.

## Output

### Terminal (default)

```
=== COMPREHENSIVE POLYMARKET SCAN ===
Fetched 623 markets across 47 events
Orderbooks: 1,198/1,246 successful (96.1%)
Scan time: 2m 14s

--- ARBITRAGE OPPORTUNITIES (ranked by net edge) ---
#  Type          Edge(bps) VWAP@$500  Markets  Question
...

--- NEAR-MISS OPPORTUNITIES (10-50 bps) ---
...

--- MARKET STRUCTURE SUMMARY ---
Category     Markets  Avg Spread  Avg Depth  Total 24h Vol
Sports/NHL      32       22 bps      4,200     $740K
...
```

### JSON export (`--export scan_results.json`)

Full structured data: every market, orderbook snapshot, opportunity, rejection reason.

### CSV export (`--export-csv opportunities.csv`)

Flat table of opportunities for spreadsheet analysis.

## CLI Interface

```
arb scan --comprehensive [OPTIONS]

Options:
  --min-edge <BPS>         Minimum net edge to display (default: 0)
  --size-tiers <SIZES>     VWAP analysis sizes (default: 100,500,1000,5000)
  --export <FILE.json>     Full results JSON export
  --export-csv <FILE.csv>  Opportunities CSV export
  --max-concurrent <N>     Max concurrent API requests (default: 10)
  --timeout <SECS>         Per-request timeout (default: 15)
  --verbose                Show per-market scan progress
```

## Code Organization

All new code in existing crates:

| Change | Crate | File |
|---|---|---|
| Paginated market fetcher | arb-data | poller.rs (extend SdkMarketDataSource) |
| Concurrent orderbook fetcher | arb-data | poller.rs (new method) |
| Deadline monotonicity detector | arb-strategy | new deadline.rs |
| Spread/depth profiler | arb-data | new profiler.rs |
| Comprehensive scan command | arb-daemon | extend scan.rs |
| JSON/CSV export | arb-daemon | new export.rs |
| Multi-tier VWAP analysis | arb-data | orderbook.rs (extend OrderbookProcessor) |

No new crates. ~600-800 lines of new code.

## Key Design Constraints

- Edge must be VWAP-based, not theoretical mid-price
- All filtering at net edge (after slippage + fees)
- Partial failures tolerated (missing orderbooks logged, not fatal)
- Rate limiting to avoid API throttling
