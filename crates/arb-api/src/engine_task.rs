//! Background task that runs the arb engine loop, feeding data into AppState.
//!
//! Fetches live market data from Polymarket, runs arb detectors,
//! and broadcasts events to WebSocket clients.

use std::sync::Arc;
use std::time::Duration;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, MarketDataSource, ProbabilityEstimator, RiskManager, SlippageEstimator, TradeExecutor};
use arb_core::types::{ArbType, RiskDecision};
use arb_execution::executor::LiveTradeExecutor;
use arb_execution::paper_trade::PaperTradeExecutor;
use arb_data::correlation::CorrelationGraph;
use arb_data::local_book::OrderBookStore;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::{
    ConcurrentFetchConfig, MarketPoller, SdkMarketDataSource, classify_markets,
};
use arb_data::price_history::PriceHistoryStore;
use arb_data::vwap_cache::CachedSlippageEstimator;
use arb_data::ws_feed::{BookUpdate, WsFeedClient};
use tokio::sync::mpsc;
use arb_simulation::estimator::EnsembleEstimator;
use arb_strategy::cross_market::CrossMarketDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use arb_strategy::resolution_sniping::ResolutionSnipingDetector;
use arb_strategy::liquidity_sniping::LiquiditySnipingDetector;
use arb_strategy::market_making::MarketMakingDetector;
use arb_strategy::prob_model::ProbModelDetector;
use arb_strategy::stale_market::StaleMarketDetector;
use arb_strategy::volume_spike::VolumeSpikeDetector;
use rust_decimal::Decimal;
use tracing::{debug, error, info, warn};

use crate::routes::helpers::{append_history, broadcast_event};
use crate::state::AppState;

/// Maximum number of token IDs to fetch orderbooks for in one pass.
/// Keeps startup time reasonable (~30s instead of 5+ minutes).
const MAX_ORDERBOOK_TOKENS: usize = 400;

/// Maximum markets to send in a single WS bulk event.
const MAX_WS_BULK_MARKETS: usize = 500;

/// Maximum backoff delay between engine restart attempts.
const MAX_RESTART_BACKOFF_SECS: u64 = 30;

/// Spawn the background engine loop. Shares state with the API handlers.
/// If the engine crashes, it will be automatically restarted with exponential backoff.
pub fn spawn_engine(state: AppState) {
    tokio::spawn(async move {
        let mut backoff_secs: u64 = 1;

        loop {
            match run_engine_loop(state.clone()).await {
                Ok(()) => {
                    info!("Engine loop exited cleanly");
                    break;
                }
                Err(e) => {
                    error!(
                        error = %e,
                        restart_in_secs = backoff_secs,
                        "Engine loop crashed — will restart with backoff"
                    );

                    tokio::time::sleep(Duration::from_secs(backoff_secs)).await;
                    backoff_secs = (backoff_secs * 2).min(MAX_RESTART_BACKOFF_SECS);

                    warn!("Restarting engine loop...");
                }
            }
        }
    });
}

async fn run_engine_loop(state: AppState) -> anyhow::Result<()> {
    let config = state.config.read().unwrap().clone();

    let data_source = SdkMarketDataSource::new();
    let mut poller = MarketPoller::new(config.polling.clone());
    let fetch_config = ConcurrentFetchConfig::default();

    let cached_estimator = Arc::new(CachedSlippageEstimator::new(
        OrderbookProcessor::new(config.slippage.clone()),
    ));
    let slippage_estimator: Arc<dyn SlippageEstimator> = cached_estimator.clone();

    let fee_rate = config.fees.effective_rate(config.slippage.prefer_post_only);

    // Build detectors
    let mut detectors: Vec<Box<dyn ArbDetector>> = Vec::new();

    if config.strategy.intra_market_enabled {
        detectors.push(Box::new(IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.multi_outcome_enabled {
        detectors.push(Box::new(MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.cross_market_enabled {
        let graph = if let Some(ref file) = config.strategy.cross_market.correlation_file {
            let path = ArbConfig::config_dir().join(file);
            CorrelationGraph::load(&path).unwrap_or_else(|e| {
                warn!("Failed to load correlation file: {e}");
                CorrelationGraph::empty()
            })
        } else {
            CorrelationGraph::empty()
        };

        detectors.push(Box::new(CrossMarketDetector::new(
            config.strategy.cross_market.clone(),
            config.strategy.clone(),
            Arc::new(graph),
            state.market_cache.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.resolution_sniping_enabled {
        detectors.push(Box::new(ResolutionSnipingDetector::new(
            config.strategy.resolution_sniping.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.stale_market_enabled {
        detectors.push(Box::new(StaleMarketDetector::new(
            config.strategy.stale_market.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.volume_spike_enabled {
        detectors.push(Box::new(VolumeSpikeDetector::new(
            config.strategy.volume_spike.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    let edge_calculator = EdgeCalculator::from_config(
        &config.fees,
        config.slippage.prefer_post_only,
        slippage_estimator.clone(),
    );

    // Probability estimator (ensemble of MC + particle filter)
    let prob_estimator = EnsembleEstimator::from_config(
        config.simulation.monte_carlo_paths,
        config.simulation.particle_count,
    );
    let prob_estimation_enabled = config.simulation.probability_estimation_enabled;

    if config.strategy.prob_model_enabled {
        // Create a separate estimator instance for the detector
        let detector_estimator = EnsembleEstimator::from_config(
            config.simulation.monte_carlo_paths,
            config.simulation.particle_count,
        );
        detectors.push(Box::new(ProbModelDetector::new(
            config.strategy.prob_model.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            Arc::new(detector_estimator),
            fee_rate,
        )));
    }

    if config.strategy.liquidity_sniping_enabled {
        detectors.push(Box::new(LiquiditySnipingDetector::new(
            config.strategy.liquidity_sniping.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    if config.strategy.market_making_enabled {
        detectors.push(Box::new(MarketMakingDetector::new(
            config.strategy.market_making.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
            fee_rate,
        )));
    }

    // Historical price store (SQLite, append-only)
    let price_store = match PriceHistoryStore::open(
        &ArbConfig::config_dir().join("price_history.db"),
    ) {
        Ok(store) => {
            info!("Engine: price history store opened");
            Some(store)
        }
        Err(e) => {
            warn!(error = %e, "Engine: failed to open price history store, recording disabled");
            None
        }
    };

    // Create executor based on configured trading mode (paper or live)
    let executor: Box<dyn TradeExecutor> = if config.is_live() {
        info!("Starting engine in LIVE trading mode");
        let key_path = config.general.key_file.as_ref().map(std::path::Path::new);
        match LiveTradeExecutor::from_key_file(
            key_path,
            config.slippage.prefer_post_only,
            config.risk.order_timeout_secs,
            fee_rate,
        )
        .await
        {
            Ok(live) => Box::new(live),
            Err(e) => {
                let reason = format!("CRITICAL: Live executor init failed: {e}. Refusing silent fallback to paper mode.");
                error!("{reason}");
                // Activate kill switch instead of silently falling back to paper mode.
                // A silent fallback means the operator thinks they are trading live
                // but all trades are simulated — this is a critical safety issue.
                {
                    let mut rl = state.risk_limits.lock().unwrap();
                    rl.activate_kill_switch(&reason);
                }
                state.kill_switch_active.store(true, std::sync::atomic::Ordering::Relaxed);
                Box::new(PaperTradeExecutor::with_fee_rate(fee_rate))
            }
        }
    } else {
        info!("Starting engine in PAPER trading mode");
        Box::new(PaperTradeExecutor::with_fee_rate(fee_rate))
    };

    // ── Phase 1: Fetch all active markets (metadata only, no orderbooks) ──
    info!("Engine: fetching initial market data from Polymarket...");
    match data_source.fetch_all_active_markets().await {
        Ok(markets) => {
            info!(count = markets.len(), "Engine: initial market fetch complete");
            state.market_cache.update(&markets);

            // Immediately broadcast top markets to frontend (before orderbook fetch)
            // so the UI populates within seconds, not minutes
            let top_markets: Vec<_> = markets
                .iter()
                .take(MAX_WS_BULK_MARKETS)
                .collect::<Vec<_>>();
            info!(
                count = top_markets.len(),
                "Engine: broadcasting initial markets to clients"
            );
            let _ = broadcast_event(&state, "markets_loaded", &top_markets);
        }
        Err(e) => {
            error!(error = %e, "Engine: failed initial market fetch");
        }
    }

    // ── Phase 2: Fetch orderbooks for a limited set of classified markets ──
    let active = state.market_cache.active_markets();
    let classified = classify_markets(&active);
    let limited_tokens: Vec<_> = classified
        .all_token_ids
        .iter()
        .take(MAX_ORDERBOOK_TOKENS)
        .cloned()
        .collect();
    info!(
        binary = classified.binary.len(),
        neg_risk = classified.neg_risk.len(),
        total_tokens = classified.all_token_ids.len(),
        fetching_tokens = limited_tokens.len(),
        "Engine: fetching orderbooks (limited set)..."
    );

    match data_source
        .fetch_orderbooks_concurrent(&limited_tokens, &fetch_config)
        .await
    {
        Ok(books) => {
            info!(count = books.len(), "Engine: orderbooks fetched");
            let mut updated_markets = Vec::new();
            for market in state.market_cache.active_markets() {
                let mut new_books = Vec::new();
                for tid in &market.token_ids {
                    if let Some(book) = books.get(tid) {
                        new_books.push(book.clone());
                    }
                }
                if !new_books.is_empty() {
                    let mut updated = (*market).clone();
                    updated.orderbooks = new_books;
                    state.market_cache.update_one(updated.clone());
                    updated_markets.push(updated);
                }
            }

            // Broadcast markets that now have orderbooks
            if !updated_markets.is_empty() {
                info!(
                    count = updated_markets.len(),
                    "Engine: broadcasting markets with orderbooks"
                );
                let _ = broadcast_event(&state, "markets_loaded", &updated_markets);
            }
        }
        Err(e) => {
            warn!(error = %e, "Engine: orderbook fetch failed");
        }
    }

    // ── Phase 3: Spawn WebSocket feed for real-time book updates ──
    let book_store = Arc::new(OrderBookStore::new());
    let (ws_update_tx, mut ws_update_rx) = mpsc::channel::<BookUpdate>(1000);

    // Collect token IDs from classified markets for WS subscription (limit to 200)
    let hot_tokens: Vec<String> = classified
        .all_token_ids
        .iter()
        .take(200)
        .cloned()
        .collect();

    let ws_client = WsFeedClient::new(Arc::clone(&book_store), ws_update_tx);
    let _ws_sub_tx = ws_client.spawn(hot_tokens.clone());
    info!(
        token_count = hot_tokens.len(),
        "WebSocket feed spawned for real-time book updates"
    );

    info!("Engine: entering main loop");
    let mut last_scan_gen: u64 = 0;
    let mut cycle_count: u64 = 0;

    loop {
        // Check kill switch (lock-free atomic read -- no mutex needed on hot path)
        if state.kill_switch_active.load(std::sync::atomic::Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        // Drain WebSocket book updates (applied to book_store automatically by WS client;
        // we drain the notification channel to prevent it from filling up)
        let mut ws_update_count = 0u64;
        while let Ok(_update) = ws_update_rx.try_recv() {
            ws_update_count += 1;
        }
        if ws_update_count > 0 {
            debug!(count = ws_update_count, "Processed WebSocket book updates");
        }

        // Clear VWAP cache each tick (orderbooks may have changed)
        cached_estimator.clear_cache();

        // Poll markets due for refresh
        let all_markets = state.market_cache.active_markets();
        let due_markets = poller.filter_due(&all_markets);

        if !due_markets.is_empty() {
            debug!(count = due_markets.len(), "Engine: refreshing due markets");
            for market in &due_markets {
                match data_source.fetch_orderbooks(&market.token_ids).await {
                    Ok(books) => {
                        let mut updated = (**market).clone();
                        updated.orderbooks = books;
                        state.market_cache.update_one(updated.clone());
                        poller.record_poll(&market.condition_id);
                        let _ = broadcast_event(&state, "market_update", &updated);
                    }
                    Err(e) => {
                        debug!(market = %market.condition_id, error = %e, "Orderbook refresh failed");
                    }
                }
            }
        }

        // Run detectors on changed markets only (skip unchanged for efficiency).
        // Cross-market detector needs the full set for pair lookups, but
        // intra-market and multi-outcome only need changed markets.
        let current_gen = state.market_cache.generation();
        let markets_snapshot = if last_scan_gen == 0 {
            // First scan: use all active markets
            state.market_cache.active_markets()
        } else {
            // Subsequent scans: only changed markets
            let changed = state.market_cache.changed_since(last_scan_gen);
            if changed.is_empty() {
                // Nothing changed, skip detector run entirely
                last_scan_gen = current_gen;
                tokio::time::sleep(Duration::from_secs(5)).await;
                continue;
            }
            changed
        };
        last_scan_gen = current_gen;
        let mut opportunities = Vec::new();

        for detector in &detectors {
            match detector.scan(&markets_snapshot).await {
                Ok(opps) => {
                    if !opps.is_empty() {
                        debug!(detector = %detector.arb_type(), count = opps.len(), "Opportunities found");
                    }
                    opportunities.extend(opps);
                }
                Err(e) => {
                    debug!(detector = %detector.arb_type(), error = %e, "Detector error");
                }
            }
        }

        // Refine edge with VWAP
        for opp in &mut opportunities {
            let _ = edge_calculator.refine_with_vwap(opp, &state.market_cache);
        }

        // Enrich opportunities with ensemble probability estimates
        if prob_estimation_enabled {
            for opp in &mut opportunities {
                if let Some(market) = opp.markets.first().and_then(|cid| state.market_cache.get(cid))
                    && let Ok(est) = prob_estimator.estimate(&market)
                {
                    opp.confidence = est.probabilities.first().copied().unwrap_or(0.0);
                }
            }
        }

        // Record changed market prices to historical store every cycle.
        if let Some(ref store) = price_store
            && !markets_snapshot.is_empty()
        {
            let dereffed: Vec<_> = markets_snapshot.iter().map(|m| (**m).clone()).collect();
            if let Err(e) = store.record_markets(&dereffed) {
                debug!(error = %e, "Price history recording failed");
            }
        }

        // Filter and sort -- read min_edge_bps from live config each tick
        // so runtime changes via PUT /api/config take effect immediately
        let min_edge = {
            let cfg = state.config.read().unwrap();
            Decimal::from(cfg.strategy.min_edge_bps)
        };
        opportunities.retain(|o| o.net_edge_bps() >= min_edge);
        opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

        // ── Auto-execute approved opportunities (paper mode) ──
        for opp in &opportunities {
            // Risk check (acquire lock briefly, release before async call)
            let decision = {
                let rl = state.risk_limits.lock().unwrap();
                rl.check_opportunity(opp)
            };

            match decision {
                Ok(RiskDecision::Approve { max_size }) => {
                    let sized_opp = opp.with_max_size(max_size);
                    match executor.execute_opportunity(&sized_opp).await {
                        Ok(report) => {
                            {
                                let mut rl = state.risk_limits.lock().unwrap();
                                rl.record_execution(&report, opp.arb_type);
                            }
                            append_history(&state, &report);
                            let _ = broadcast_event(&state, "trade_executed", &report);
                            info!(
                                opp_id = %opp.id,
                                arb_type = %opp.arb_type,
                                edge = %report.realized_edge,
                                "Auto-executed paper trade"
                            );
                        }
                        Err(e) => {
                            error!(opp_id = %opp.id, error = %e, "Execution failed");
                        }
                    }
                }
                Ok(RiskDecision::ReduceSize { new_size, .. }) => {
                    let sized_opp = opp.with_max_size(new_size);
                    match executor.execute_opportunity(&sized_opp).await {
                        Ok(report) => {
                            {
                                let mut rl = state.risk_limits.lock().unwrap();
                                rl.record_execution(&report, opp.arb_type);
                            }
                            append_history(&state, &report);
                            let _ = broadcast_event(&state, "trade_executed", &report);
                            info!(
                                opp_id = %opp.id,
                                new_size = %new_size,
                                "Auto-executed paper trade (reduced size)"
                            );
                        }
                        Err(e) => {
                            error!(opp_id = %opp.id, error = %e, "Execution failed");
                        }
                    }
                }
                Ok(RiskDecision::Reject { reason }) => {
                    debug!(opp_id = %opp.id, reason = %reason, "Opportunity rejected by risk manager");
                }
                Err(e) => {
                    debug!(opp_id = %opp.id, error = %e, "Risk check failed");
                }
            }
        }

        // Check daily loss limit (mirrors daemon engine check)
        {
            let rl = state.risk_limits.lock().unwrap();
            let daily_pnl = rl.daily_pnl();
            let daily_limit = {
                let cfg = state.config.read().unwrap();
                cfg.risk.daily_loss_limit
            };
            if daily_pnl < -daily_limit {
                drop(rl);
                let reason = format!(
                    "Daily loss limit hit: {} < -{}",
                    daily_pnl, daily_limit
                );
                error!("{reason}");
                {
                    let mut rl = state.risk_limits.lock().unwrap();
                    rl.activate_kill_switch(&reason);
                }
                state.kill_switch_active.store(true, std::sync::atomic::Ordering::Relaxed);
                let _ = broadcast_event(&state, "kill_switch_change", &serde_json::json!({
                    "active": true,
                    "reason": reason,
                }));
            }
        }

        // Update shared state BEFORE broadcasting so that when clients receive
        // the WS event and query GET /api/opportunities, the data is already there.
        if let Ok(mut opps) = state.opportunities.write() {
            for opp in &opportunities {
                opps.insert(0, opp.clone());
            }
            opps.truncate(200);
        }

        // Broadcast opportunities as a single batch (reduces serialization overhead)
        if !opportunities.is_empty() {
            let _ = broadcast_event(&state, "opportunities_batch", &opportunities);
        }

        // Broadcast periodic metrics + positions update, and cache metrics JSON
        {
            let rl = state.risk_limits.lock().unwrap();
            let metrics = serde_json::json!({
                "brier_score": rl.metrics().brier_score(),
                "drawdown_pct": rl.metrics().drawdown_pct(),
                "execution_quality": rl.metrics().execution_quality().to_string(),
                "total_pnl": rl.metrics().total_pnl().to_string(),
                "daily_pnl": rl.daily_pnl().to_string(),
                "trade_count": rl.metrics().trade_count(),
                "current_exposure": rl.current_exposure().to_string(),
                "peak_equity": rl.metrics().peak_equity().to_string(),
                "current_equity": rl.metrics().current_equity().to_string(),
                "pnl_by_type": {
                    "IntraMarket": rl.metrics().pnl_for_type(ArbType::IntraMarket).to_string(),
                    "CrossMarket": rl.metrics().pnl_for_type(ArbType::CrossMarket).to_string(),
                    "MultiOutcome": rl.metrics().pnl_for_type(ArbType::MultiOutcome).to_string(),
                }
            });

            // Cache the serialized metrics so /api/metrics doesn't need the mutex
            if let Ok(json_str) = serde_json::to_string(&metrics)
                && let Ok(mut cached) = state.cached_metrics_json.write()
            {
                *cached = json_str;
            }

            let _ = broadcast_event(&state, "metrics_update", &metrics);

            if let Ok(tracker) = rl.positions().lock() {
                let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
                let _ = broadcast_event(&state, "position_update", &positions);
            }
        }

        // Periodic maintenance: clean stale poller entries every 100 cycles (~8 min)
        cycle_count += 1;
        if cycle_count.is_multiple_of(100) {
            let active_ids: std::collections::HashSet<String> = state
                .market_cache
                .active_markets()
                .iter()
                .map(|m| m.condition_id.clone())
                .collect();
            poller.cleanup_stale(&active_ids);

            // Purge old price history (30-day rolling window)
            if let Some(ref store) = price_store {
                match store.cleanup(30) {
                    Ok(n) if n > 0 => info!(deleted = n, "Price history: purged old ticks"),
                    Err(e) => debug!(error = %e, "Price history cleanup failed"),
                    _ => {}
                }
            }
        }

        // Sleep before next tick
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}
