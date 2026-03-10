use std::sync::Arc;
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, SlippageEstimator};
use arb_core::types::{Opportunity, SandboxConfigOverrides};
use arb_data::correlation::CorrelationGraph;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::vwap_cache::CachedSlippageEstimator;
use arb_simulation::estimator::EnsembleEstimator;
use arb_strategy::cross_market::CrossMarketDetector;
use arb_strategy::edge::EdgeCalculator;
use arb_strategy::intra_market::IntraMarketDetector;
use arb_strategy::liquidity_sniping::LiquiditySnipingDetector;
use arb_strategy::market_making::MarketMakingDetector;
use arb_strategy::multi_outcome::MultiOutcomeDetector;
use arb_strategy::prob_model::ProbModelDetector;
use arb_strategy::resolution_sniping::ResolutionSnipingDetector;
use arb_strategy::stale_market::StaleMarketDetector;
use arb_strategy::volume_spike::VolumeSpikeDetector;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct DetectRequest {
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
}

pub async fn detect(
    State(state): State<AppState>,
    Json(req): Json<DetectRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    let base_config = state.config.read().unwrap().clone();
    let config = base_config.with_overrides(&req.config_overrides);

    let estimator = Arc::new(CachedSlippageEstimator::new(
        OrderbookProcessor::new(config.slippage.clone()),
    ));
    let slippage: Arc<dyn SlippageEstimator> = estimator;

    let mut detectors: Vec<Box<dyn ArbDetector>> = Vec::new();

    if config.strategy.intra_market_enabled {
        detectors.push(Box::new(IntraMarketDetector::new(
            config.strategy.intra_market.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.multi_outcome_enabled {
        detectors.push(Box::new(MultiOutcomeDetector::new(
            config.strategy.multi_outcome.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.cross_market_enabled {
        let graph = if let Some(ref file) = config.strategy.cross_market.correlation_file {
            let path = ArbConfig::config_dir().join(file);
            if path.exists() {
                CorrelationGraph::load(&path).unwrap_or_else(|_| CorrelationGraph::empty())
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
            slippage.clone(),
        )));
    }
    if config.strategy.resolution_sniping_enabled {
        detectors.push(Box::new(ResolutionSnipingDetector::new(
            config.strategy.resolution_sniping.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.stale_market_enabled {
        detectors.push(Box::new(StaleMarketDetector::new(
            config.strategy.stale_market.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.volume_spike_enabled {
        detectors.push(Box::new(VolumeSpikeDetector::new(
            config.strategy.volume_spike.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.prob_model_enabled
        && state.prob_estimator.get().is_some()
    {
        // Create a fresh estimator for the sandbox (don't share mutable state)
        let sandbox_estimator = EnsembleEstimator::from_config(
            config.simulation.monte_carlo_paths,
            config.simulation.particle_count,
        );
        detectors.push(Box::new(ProbModelDetector::new(
            config.strategy.prob_model.clone(),
            config.strategy.clone(),
            slippage.clone(),
            Arc::new(sandbox_estimator),
        )));
    }
    if config.strategy.liquidity_sniping_enabled {
        detectors.push(Box::new(LiquiditySnipingDetector::new(
            config.strategy.liquidity_sniping.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }
    if config.strategy.market_making_enabled {
        detectors.push(Box::new(MarketMakingDetector::new(
            config.strategy.market_making.clone(),
            config.strategy.clone(),
            slippage.clone(),
        )));
    }

    let edge_calculator = EdgeCalculator::default_with_estimator(slippage);

    let markets = state.market_cache.active_markets();
    let markets_scanned = markets.len();

    // ── Diagnostic counters ──
    let mut binary_markets = 0usize;
    let mut neg_risk_markets = 0usize;
    let mut markets_with_orderbooks = 0usize;
    let mut closest_ask_sum = Decimal::from(999);
    let mut closest_bid_sum = Decimal::ZERO;

    for m in &markets {
        let has_books = !m.orderbooks.is_empty()
            && m.orderbooks.iter().any(|b| !b.asks.is_empty() || !b.bids.is_empty());
        if has_books {
            markets_with_orderbooks += 1;
        }
        if m.neg_risk {
            neg_risk_markets += 1;
        }
        if m.token_ids.len() == 2 && !m.neg_risk {
            binary_markets += 1;
            // Track closest ask sum for binary markets (YES ask + NO ask)
            if m.orderbooks.len() == 2
                && !m.orderbooks[0].asks.is_empty()
                && !m.orderbooks[1].asks.is_empty()
            {
                let ask_sum = m.orderbooks[0].asks[0].price + m.orderbooks[1].asks[0].price;
                if ask_sum < closest_ask_sum {
                    closest_ask_sum = ask_sum;
                }
            }
            if m.orderbooks.len() == 2
                && !m.orderbooks[0].bids.is_empty()
                && !m.orderbooks[1].bids.is_empty()
            {
                let bid_sum = m.orderbooks[0].bids[0].price + m.orderbooks[1].bids[0].price;
                if bid_sum > closest_bid_sum {
                    closest_bid_sum = bid_sum;
                }
            }
        }
    }

    let mut opportunities: Vec<Opportunity> = Vec::new();

    for detector in &detectors {
        if let Ok(opps) = detector.scan(&markets).await {
            opportunities.extend(opps);
        }
    }

    let pre_filter_count = opportunities.len();

    for opp in &mut opportunities {
        let _ = edge_calculator.refine_with_vwap(opp, &state.market_cache);
    }

    let min_edge = Decimal::from(config.strategy.min_edge_bps);
    opportunities.retain(|o| o.net_edge_bps() >= min_edge);
    opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

    let elapsed = start.elapsed().as_millis();

    let result = serde_json::json!({
        "opportunities": opportunities,
        "detection_time_ms": elapsed,
        "markets_scanned": markets_scanned,
        "config_used": {
            "min_edge_bps": config.strategy.min_edge_bps,
            "intra_market_enabled": config.strategy.intra_market_enabled,
            "cross_market_enabled": config.strategy.cross_market_enabled,
            "multi_outcome_enabled": config.strategy.multi_outcome_enabled,
            "resolution_sniping_enabled": config.strategy.resolution_sniping_enabled,
            "stale_market_enabled": config.strategy.stale_market_enabled,
            "volume_spike_enabled": config.strategy.volume_spike_enabled,
            "prob_model_enabled": config.strategy.prob_model_enabled,
            "liquidity_sniping_enabled": config.strategy.liquidity_sniping_enabled,
            "market_making_enabled": config.strategy.market_making_enabled,
            "intra_min_deviation": config.strategy.intra_market.min_deviation.to_string(),
            "multi_min_deviation": config.strategy.multi_outcome.min_deviation.to_string(),
        },
        "diagnostics": {
            "binary_markets": binary_markets,
            "neg_risk_markets": neg_risk_markets,
            "markets_with_orderbooks": markets_with_orderbooks,
            "closest_binary_ask_sum": closest_ask_sum.to_string(),
            "closest_binary_bid_sum": closest_bid_sum.to_string(),
            "pre_filter_count": pre_filter_count,
            "post_filter_count": opportunities.len(),
        },
    });

    (StatusCode::OK, Json(result)).into_response()
}

#[derive(Deserialize)]
pub struct BacktestRequest {
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
}

pub async fn backtest(
    State(state): State<AppState>,
    Json(req): Json<BacktestRequest>,
) -> impl IntoResponse {
    let base_config = state.config.read().unwrap().clone();
    let config = base_config.with_overrides(&req.config_overrides);

    let history = state.execution_history.read().unwrap().clone();
    let total_original = history.len();
    let min_edge_bps = Decimal::from(config.strategy.min_edge_bps);

    let mut trades = Vec::new();
    let mut cumulative_exposure = Decimal::ZERO;
    let mut daily_pnl_tracker: std::collections::BTreeMap<String, (Decimal, usize)> =
        std::collections::BTreeMap::new();
    let mut aggregate_pnl = Decimal::ZERO;
    let mut aggregate_pnl_original = Decimal::ZERO;

    for report in &history {
        let net_pnl = report.realized_edge - report.total_fees;
        aggregate_pnl_original += net_pnl;

        let edge_bps = if report.realized_edge != Decimal::ZERO {
            report.realized_edge * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        let trade_size: Decimal = report.legs.iter().map(|l| l.filled_size).sum();

        let would_exceed_exposure =
            cumulative_exposure + trade_size > config.risk.max_total_exposure;
        let below_min_edge = edge_bps.abs() < min_edge_bps;

        let (included, rejection_reason) = if below_min_edge {
            (false, Some(format!(
                "edge {edge_bps} below min_edge_bps ({})",
                config.strategy.min_edge_bps
            )))
        } else if would_exceed_exposure {
            (false, Some(format!(
                "would exceed max_total_exposure ({})",
                config.risk.max_total_exposure
            )))
        } else {
            (true, None)
        };

        if included {
            aggregate_pnl += net_pnl;
            cumulative_exposure += trade_size;
        }

        let date = report.timestamp.format("%Y-%m-%d").to_string();
        let entry = daily_pnl_tracker.entry(date).or_insert((Decimal::ZERO, 0));
        if included {
            entry.0 += net_pnl;
            entry.1 += 1;
        }

        trades.push(serde_json::json!({
            "opportunity_id": report.opportunity_id.to_string(),
            "realized_edge": report.realized_edge.to_string(),
            "total_fees": report.total_fees.to_string(),
            "net_pnl": net_pnl.to_string(),
            "timestamp": report.timestamp.to_rfc3339(),
            "included": included,
            "rejection_reason": rejection_reason,
        }));
    }

    let total_filtered = trades.iter().filter(|t| t["included"] == true).count();

    let daily_breakdown: Vec<_> = daily_pnl_tracker
        .into_iter()
        .map(|(date, (pnl, count))| {
            serde_json::json!({
                "date": date,
                "pnl": pnl.to_string(),
                "trade_count": count,
            })
        })
        .collect();

    let result = serde_json::json!({
        "total_trades_original": total_original,
        "total_trades_filtered": total_filtered,
        "trades_rejected": total_original - total_filtered,
        "aggregate_pnl": aggregate_pnl.to_string(),
        "aggregate_pnl_original": aggregate_pnl_original.to_string(),
        "daily_breakdown": daily_breakdown,
        "trades": trades,
    });

    (StatusCode::OK, Json(result)).into_response()
}
