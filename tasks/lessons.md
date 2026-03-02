# Lessons Learned

## SDK Integration
- **polymarket-client-sdk types are not always Strings**: `condition_id` is `Option<B256>`, `clob_token_ids` is `Option<Vec<U256>>`. Always check the actual source types in the SDK, not assume String.
- **Import paths**: `OrderBookSummaryRequest` is at `clob::types::request::OrderBookSummaryRequest`, not `clob::types::OrderBookSummaryRequest`. The SDK uses sub-modules for request types.
- **SDK typestate auth**: `Client` uses `Client<Unauthenticated>` vs `Client<Authenticated<K>>`. Methods like `cancel_all_orders()` are only on authenticated clients. Can't store a default `Client` and call auth methods on it.
- **cancel method**: The SDK method is `cancel_all_orders()`, not `cancel_all()`.

## Quantitative / Simulation
- **GBM ATM probability**: Under GBM with drift μ=0, P(S_T > S_0) ≠ 0.50. The log-normal median is S_0·exp((μ - σ²/2)T) < S_0 for σ > 0. For a true 50/50 test, set drift = σ²/2.
- **ABM kyle_lambda scaling**: Kyle's lambda must be scaled by 1/total_agents (or similar). If λ × net_order_flow > price range, the model oscillates wildly between bounds.

## Testing
- **File-based kill switch leaks between tests**: `KillSwitch::new()` reads from disk. Tests that activate the kill switch must deactivate it in cleanup, or other tests running in parallel will fail.
- **Stochastic test tolerance**: Monte Carlo tests with RNG should have tolerances wide enough to handle statistical noise. For 100K paths with p=0.5, SE ≈ 0.0016, so ±0.02 is about 12σ — generally safe but exact drift matters.

## Build
- **alloy 1.7.3 requires rustc ≥ 1.91**: If compilation fails on alloy, `rustup update stable` to get a newer rustc.
- **Workspace profile warning**: The nested CLI's Cargo.toml has its own `[profile]` sections which emit a warning. Harmless — profiles must be at workspace root.
