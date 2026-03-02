//! Background task that runs the arb engine loop, feeding data into AppState.
//!
//! Fetches live market data from Polymarket, runs arb detectors,
//! and broadcasts events to WebSocket clients.

use std::sync::Arc;
use std::time::Duration;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, MarketDataSource, RiskManager, SlippageEstimator};
use arb_core::types::ArbType;
use arb_data::correlation::CorrelationGraph;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::poller::{
    ConcurrentFetchConfig, MarketPoller, SdkMarketDataSource, classify_markets,
};
use arb_strategy::cross_market::CrossMarketDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use rust_decimal::Decimal;
use tracing::{debug, error, info, warn};

use crate::state::AppState;

/// Spawn the background engine loop. Shares state with the API handlers.
pub fn spawn_engine(state: AppState) {
    tokio::spawn(async move {
        if let Err(e) = run_engine_loop(state).await {
            error!(error = %e, "Engine loop exited with error");
        }
    });
}

async fn run_engine_loop(state: AppState) -> anyhow::Result<()> {
    let config = state.config.read().unwrap().clone();

    let data_source = SdkMarketDataSource::new();
    let mut poller = MarketPoller::new(config.polling.clone());
    let fetch_config = ConcurrentFetchConfig::default();

    let slippage_estimator: Arc<dyn SlippageEstimator> =
        Arc::new(OrderbookProcessor::new(config.slippage.clone()));

    // Build detectors
    let mut detectors: Vec<Box<dyn ArbDetector>> = Vec::new();

    if config.strategy.intra_market_enabled {
        detectors.push(Box::new(IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        )));
    }

    if config.strategy.multi_outcome_enabled {
        detectors.push(Box::new(MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage_estimator.clone(),
        )));
    }

    if config.strategy.cross_market_enabled {
        let graph = if let Some(ref file) = config.strategy.cross_market.correlation_file {
            let path = ArbConfig::config_dir().join(file);
            if path.exists() {
                CorrelationGraph::load(&path).unwrap_or_else(|e| {
                    warn!("Failed to load correlation file: {e}");
                    CorrelationGraph::empty()
                })
            } else {
                CorrelationGraph::empty()
            }
        } else {
            CorrelationGraph::empty()
        };

        detectors.push(Box::new(CrossMarketDetector::new(
            config.strategy.cross_market.clone(),
            config.strategy.clone(),
            Arc::new(graph),
            state.market_cache.clone(),
            slippage_estimator.clone(),
        )));
    }

    let edge_calculator = EdgeCalculator::default_with_config(config.slippage.clone());

    // Initial market fetch
    info!("Engine: fetching initial market data from Polymarket...");
    match data_source.fetch_all_active_markets().await {
        Ok(markets) => {
            info!(count = markets.len(), "Engine: initial market fetch complete");
            state.market_cache.update(&markets);
            broadcast_event(&state, "market_count_update", &serde_json::json!({
                "count": markets.len()
            }));
        }
        Err(e) => {
            error!(error = %e, "Engine: failed initial market fetch");
        }
    }

    // Fetch orderbooks for classified markets
    let classified = classify_markets(&state.market_cache.active_markets());
    info!(
        binary = classified.binary.len(),
        neg_risk = classified.neg_risk.len(),
        tokens = classified.all_token_ids.len(),
        "Engine: fetching orderbooks..."
    );

    match data_source
        .fetch_orderbooks_concurrent(&classified.all_token_ids, &fetch_config)
        .await
    {
        Ok(books) => {
            info!(count = books.len(), "Engine: orderbooks fetched");
            // Attach orderbooks to cached markets
            for market in state.market_cache.active_markets() {
                let mut updated = market.clone();
                let mut new_books = Vec::new();
                for tid in &market.token_ids {
                    if let Some(book) = books.get(tid) {
                        new_books.push(book.clone());
                    }
                }
                if !new_books.is_empty() {
                    updated.orderbooks = new_books;
                    state.market_cache.update_one(updated);
                }
            }
        }
        Err(e) => {
            warn!(error = %e, "Engine: orderbook fetch failed");
        }
    }

    // Broadcast initial market data to connected clients
    let all_markets = state.market_cache.active_markets();
    for m in &all_markets {
        let _ = broadcast_event(&state, "market_update", m);
    }

    info!("Engine: entering main loop");

    loop {
        // Check kill switch
        let kill_active = {
            let rl = state.risk_limits.lock().unwrap();
            rl.is_kill_switch_active()
        };
        if kill_active {
            tokio::time::sleep(Duration::from_secs(5)).await;
            continue;
        }

        // Poll markets due for refresh
        let all_markets = state.market_cache.active_markets();
        let due_markets = poller.filter_due(&all_markets);

        if !due_markets.is_empty() {
            debug!(count = due_markets.len(), "Engine: refreshing due markets");
            for market in &due_markets {
                match data_source.fetch_orderbooks(&market.token_ids).await {
                    Ok(books) => {
                        let mut updated = market.clone();
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

        // Run detectors
        let markets_snapshot = state.market_cache.active_markets();
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

        // Filter and sort
        let min_edge = Decimal::from(config.strategy.min_edge_bps);
        opportunities.retain(|o| o.net_edge_bps() >= min_edge);
        opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

        // Broadcast opportunities
        for opp in &opportunities {
            let _ = broadcast_event(&state, "opportunity_detected", opp);
        }

        // Update shared state
        if let Ok(mut opps) = state.opportunities.write() {
            for opp in &opportunities {
                opps.insert(0, opp.clone());
            }
            opps.truncate(200);
        }

        // Broadcast periodic metrics + positions update
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
                "peak_equity": "10000",
                "current_equity": (Decimal::from(10_000) + rl.metrics().total_pnl()).to_string(),
                "pnl_by_type": {
                    "IntraMarket": rl.metrics().pnl_for_type(ArbType::IntraMarket).to_string(),
                    "CrossMarket": rl.metrics().pnl_for_type(ArbType::CrossMarket).to_string(),
                    "MultiOutcome": rl.metrics().pnl_for_type(ArbType::MultiOutcome).to_string(),
                }
            });
            let _ = broadcast_event(&state, "metrics_update", &metrics);

            if let Ok(tracker) = rl.positions().lock() {
                let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
                let _ = broadcast_event(&state, "position_update", &positions);
            }
        }

        // Sleep before next tick
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

fn broadcast_event<T: serde::Serialize>(state: &AppState, event_type: &str, data: &T) -> bool {
    let event = serde_json::json!({
        "type": event_type,
        "data": data
    });
    match serde_json::to_string(&event) {
        Ok(json) => state.ws_tx.send(json).is_ok(),
        Err(_) => false,
    }
}
