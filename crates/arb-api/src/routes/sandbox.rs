use std::collections::{BTreeSet, HashMap};

use std::sync::Arc;
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_core::traits::{ArbDetector, SlippageEstimator, TradeExecutor};
use arb_core::types::{Opportunity, SandboxConfigOverrides};
use arb_core::{MarketState, OrderbookSnapshot};
use arb_data::correlation::CorrelationGraph;
use arb_data::orderbook::OrderbookProcessor;
use arb_data::vwap_cache::CachedSlippageEstimator;
use arb_execution::paper_trade::PaperTradeExecutor;
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
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::state::AppState;

// ─── Shared detector construction ────────────────────────────────────────────

/// Build all enabled strategy detectors from config.
///
/// ProbModelDetector is intentionally excluded: it requires an
/// `Arc<dyn ProbabilityEstimator>` from arb-simulation which depends on
/// historical data that the sandbox doesn't maintain. The engine's own
/// estimator instance is not accessible from the API layer. Callers get
/// a `prob_model_note` in the response explaining this.
fn build_detectors(
    config: &ArbConfig,
    state: &AppState,
    slippage: Arc<dyn SlippageEstimator>,
) -> Vec<Box<dyn ArbDetector>> {
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
        let mut graph =
            if let Some(ref file) = config.strategy.cross_market.correlation_file {
                let path = ArbConfig::config_dir().join(file);
                if path.exists() {
                    CorrelationGraph::load(&path).unwrap_or_else(|e| {
                        tracing::warn!("Failed to load correlation file: {e}");
                        CorrelationGraph::empty()
                    })
                } else {
                    CorrelationGraph::empty()
                }
            } else {
                CorrelationGraph::empty()
            };

        // Auto-detect correlation pairs from cached markets (same logic as engine.rs)
        let all_markets = state.market_cache.active_markets();
        let plain_markets: Vec<_> = all_markets.iter().map(|m| m.as_ref().clone()).collect();
        let auto_pairs = CorrelationGraph::auto_detect(&plain_markets);
        graph.merge(auto_pairs);

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
    if config.strategy.prob_model_enabled && state.prob_estimator.get().is_some() {
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
    // NOTE: prob_model is only added above when the engine has initialized
    // its ProbabilityEstimator (see doc comment and conditional above).

    detectors
}

/// Build a slippage estimator from config.
fn build_slippage(config: &ArbConfig) -> Arc<dyn SlippageEstimator> {
    Arc::new(CachedSlippageEstimator::new(OrderbookProcessor::new(
        config.slippage.clone(),
    )))
}

/// Build the config_used JSON block for detect/sweep responses.
fn config_used_json(config: &ArbConfig) -> serde_json::Value {
    serde_json::json!({
        "min_edge_bps": config.strategy.min_edge_bps,
        "fee_rate": config.fees.effective_rate(config.slippage.prefer_post_only).to_string(),
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
    })
}

// ─── detect ──────────────────────────────────────────────────────────────────

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

    let slippage = build_slippage(&config);
    let detectors = build_detectors(&config, &state, slippage.clone());

    let edge_calculator = EdgeCalculator::from_config(
        &config.fees,
        config.slippage.prefer_post_only,
        slippage,
    );

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
            && m.orderbooks
                .iter()
                .any(|b| !b.asks.is_empty() || !b.bids.is_empty());
        if has_books {
            markets_with_orderbooks += 1;
        }
        if m.neg_risk {
            neg_risk_markets += 1;
        }
        if m.token_ids.len() == 2 && !m.neg_risk {
            binary_markets += 1;
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
    let mut detector_errors: Vec<serde_json::Value> = Vec::new();
    let warnings: Vec<String> = Vec::new();

    for detector in &detectors {
        match detector.scan(&markets).await {
            Ok(opps) => opportunities.extend(opps),
            Err(e) => {
                detector_errors.push(serde_json::json!({
                    "detector": detector.arb_type().to_string(),
                    "error": e.to_string(),
                }));
            }
        }
    }

    let pre_filter_count = opportunities.len();

    for opp in &mut opportunities {
        if edge_calculator.refine_with_vwap(opp, &state.market_cache).is_err() {
            opp.net_edge = Decimal::ZERO;
        }
    }

    let min_edge = Decimal::from(config.strategy.min_edge_bps);
    opportunities.retain(|o| o.net_edge_bps() >= min_edge);
    opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

    let elapsed = start.elapsed().as_millis();

    let mut result = serde_json::json!({
        "opportunities": opportunities,
        "detection_time_ms": elapsed,
        "markets_scanned": markets_scanned,
        "config_used": config_used_json(&config),
        "diagnostics": {
            "binary_markets": binary_markets,
            "neg_risk_markets": neg_risk_markets,
            "markets_with_orderbooks": markets_with_orderbooks,
            "closest_binary_ask_sum": closest_ask_sum.to_string(),
            "closest_binary_bid_sum": closest_bid_sum.to_string(),
            "pre_filter_count": pre_filter_count,
            "post_filter_count": opportunities.len(),
            "detector_errors": detector_errors,
        },
    });

    if !warnings.is_empty() {
        result["warnings"] = serde_json::json!(warnings);
    }

    // Warn callers when prob_model is enabled but not available in sandbox
    if config.strategy.prob_model_enabled {
        result["prob_model_note"] = serde_json::json!(
            "ProbModelDetector requires a ProbabilityEstimator which is only available \
             in the live engine. It is skipped in sandbox mode."
        );
    }

    (StatusCode::OK, Json(result)).into_response()
}

// ─── sweep ───────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SweepRequest {
    #[serde(default)]
    pub base: SandboxConfigOverrides,
    pub param: String,
    pub values: Vec<serde_json::Value>,
}

/// Apply a single parameter value to a `SandboxConfigOverrides` by name.
/// Returns `Err` with a message if the param name is unknown or the value
/// cannot be parsed to the expected type.
fn apply_param(
    overrides: &mut SandboxConfigOverrides,
    param: &str,
    value: &serde_json::Value,
) -> std::result::Result<(), String> {
    match param {
        // ── Global ──
        "min_edge_bps" => {
            overrides.min_edge_bps = Some(
                value.as_u64().ok_or_else(|| format!("min_edge_bps: expected u64, got {value}"))?,
            );
        }
        "fee_rate_override" => {
            overrides.fee_rate_override = Some(parse_decimal(value, "fee_rate_override")?);
        }
        // ── Strategy toggles ──
        "intra_market_enabled" => {
            overrides.intra_market_enabled = Some(parse_bool(value, "intra_market_enabled")?);
        }
        "cross_market_enabled" => {
            overrides.cross_market_enabled = Some(parse_bool(value, "cross_market_enabled")?);
        }
        "multi_outcome_enabled" => {
            overrides.multi_outcome_enabled = Some(parse_bool(value, "multi_outcome_enabled")?);
        }
        "resolution_sniping_enabled" => {
            overrides.resolution_sniping_enabled =
                Some(parse_bool(value, "resolution_sniping_enabled")?);
        }
        "stale_market_enabled" => {
            overrides.stale_market_enabled = Some(parse_bool(value, "stale_market_enabled")?);
        }
        "volume_spike_enabled" => {
            overrides.volume_spike_enabled = Some(parse_bool(value, "volume_spike_enabled")?);
        }
        "prob_model_enabled" => {
            overrides.prob_model_enabled = Some(parse_bool(value, "prob_model_enabled")?);
        }
        "liquidity_sniping_enabled" => {
            overrides.liquidity_sniping_enabled =
                Some(parse_bool(value, "liquidity_sniping_enabled")?);
        }
        "market_making_enabled" => {
            overrides.market_making_enabled = Some(parse_bool(value, "market_making_enabled")?);
        }
        // ── Per-strategy params ──
        // Intra-market
        "intra_min_deviation" => {
            overrides.intra_min_deviation = Some(parse_decimal(value, "intra_min_deviation")?);
        }
        // Cross-market
        "cross_min_implied_edge" => {
            overrides.cross_min_implied_edge =
                Some(parse_decimal(value, "cross_min_implied_edge")?);
        }
        // Multi-outcome
        "multi_min_deviation" => {
            overrides.multi_min_deviation = Some(parse_decimal(value, "multi_min_deviation")?);
        }
        // Resolution sniping
        "res_min_price" => {
            overrides.res_min_price = Some(parse_decimal(value, "res_min_price")?);
        }
        "res_max_price" => {
            overrides.res_max_price = Some(parse_decimal(value, "res_max_price")?);
        }
        "res_max_hours" => {
            overrides.res_max_hours = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("res_max_hours: expected u64, got {value}"))?,
            );
        }
        "res_min_volume" => {
            overrides.res_min_volume = Some(parse_decimal(value, "res_min_volume")?);
        }
        // Stale market
        "stale_max_hours" => {
            overrides.stale_max_hours = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("stale_max_hours: expected u64, got {value}"))?,
            );
        }
        "stale_min_divergence_bps" => {
            overrides.stale_min_divergence_bps = Some(
                value
                    .as_u64()
                    .ok_or_else(|| {
                        format!("stale_min_divergence_bps: expected u64, got {value}")
                    })?,
            );
        }
        // Volume spike
        "vol_spike_multiplier" => {
            overrides.vol_spike_multiplier = Some(parse_f64(value, "vol_spike_multiplier")?);
        }
        "vol_min_absolute_volume" => {
            overrides.vol_min_absolute_volume =
                Some(parse_decimal(value, "vol_min_absolute_volume")?);
        }
        // Prob model
        "prob_min_deviation_bps" => {
            overrides.prob_min_deviation_bps = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("prob_min_deviation_bps: expected u64, got {value}"))?,
            );
        }
        "prob_min_confidence" => {
            overrides.prob_min_confidence = Some(parse_f64(value, "prob_min_confidence")?);
        }
        // Liquidity sniping
        "liq_min_depth_change_pct" => {
            overrides.liq_min_depth_change_pct =
                Some(parse_f64(value, "liq_min_depth_change_pct")?);
        }
        // Market making
        "mm_target_spread_bps" => {
            overrides.mm_target_spread_bps = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("mm_target_spread_bps: expected u64, got {value}"))?,
            );
        }
        "mm_max_inventory" => {
            overrides.mm_max_inventory = Some(parse_decimal(value, "mm_max_inventory")?);
        }
        "mm_min_volume" => {
            overrides.mm_min_volume = Some(parse_decimal(value, "mm_min_volume")?);
        }
        // ── Slippage ──
        "max_slippage_bps" => {
            overrides.max_slippage_bps = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("max_slippage_bps: expected u64, got {value}"))?,
            );
        }
        "vwap_depth_levels" => {
            overrides.vwap_depth_levels = Some(
                value
                    .as_u64()
                    .ok_or_else(|| format!("vwap_depth_levels: expected u64, got {value}"))?
                    as usize,
            );
        }
        // ── Risk ──
        "max_position_per_market" => {
            overrides.max_position_per_market =
                Some(parse_decimal(value, "max_position_per_market")?);
        }
        "max_total_exposure" => {
            overrides.max_total_exposure = Some(parse_decimal(value, "max_total_exposure")?);
        }
        "daily_loss_limit" => {
            overrides.daily_loss_limit = Some(parse_decimal(value, "daily_loss_limit")?);
        }
        other => {
            return Err(format!("unknown sweep parameter: {other}"));
        }
    }
    Ok(())
}

fn parse_bool(v: &serde_json::Value, name: &str) -> std::result::Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| format!("{name}: expected bool, got {v}"))
}

fn parse_f64(v: &serde_json::Value, name: &str) -> std::result::Result<f64, String> {
    v.as_f64()
        .ok_or_else(|| format!("{name}: expected number, got {v}"))
}

fn parse_decimal(
    v: &serde_json::Value,
    name: &str,
) -> std::result::Result<Decimal, String> {
    if let Some(n) = v.as_f64() {
        Decimal::try_from(n).map_err(|e| format!("{name}: invalid decimal: {e}"))
    } else if let Some(s) = v.as_str() {
        s.parse::<Decimal>()
            .map_err(|e| format!("{name}: invalid decimal string \"{s}\": {e}"))
    } else {
        Err(format!("{name}: expected number or string, got {v}"))
    }
}

pub async fn sweep(
    State(state): State<AppState>,
    Json(req): Json<SweepRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    if req.values.len() > 100 {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "sweep limited to 100 values per request" })),
        )
            .into_response();
    }

    let base_config = match state.config.read() {
        Ok(c) => c.clone(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "config lock poisoned" })),
            )
                .into_response()
        }
    };

    let markets = state.market_cache.active_markets();
    let markets_scanned = markets.len();

    let mut grid_results = Vec::new();

    for value in &req.values {
        let mut overrides = req.base.clone();
        if let Err(msg) = apply_param(&mut overrides, &req.param, value) {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response();
        }

        let config = base_config.with_overrides(&overrides);
        let slippage = build_slippage(&config);
        let detectors = build_detectors(&config, &state, slippage.clone());

        let edge_calculator = EdgeCalculator::from_config(
            &config.fees,
            config.slippage.prefer_post_only,
            slippage,
        );

        let mut opportunities: Vec<Opportunity> = Vec::new();
        let mut sweep_warnings: Vec<String> = Vec::new();
        for detector in &detectors {
            match detector.scan(&markets).await {
                Ok(opps) => opportunities.extend(opps),
                Err(e) => sweep_warnings.push(format!("detector scan failed: {e}")),
            }
        }

        for opp in &mut opportunities {
            let _ = edge_calculator.refine_with_vwap(opp, &state.market_cache);
        }

        let min_edge = Decimal::from(config.strategy.min_edge_bps);
        opportunities.retain(|o| o.net_edge_bps() >= min_edge);
        opportunities.sort_by(|a, b| b.net_edge.cmp(&a.net_edge));

        let total_net_edge: Decimal = opportunities.iter().map(|o| o.net_edge).sum();
        let best_edge_bps = opportunities
            .first()
            .map(|o| o.net_edge_bps())
            .unwrap_or(Decimal::ZERO);

        let strategies_triggered: BTreeSet<String> = opportunities
            .iter()
            .map(|o| o.strategy_type.to_string())
            .collect();

        let mut entry = serde_json::json!({
            "param_value": value,
            "opportunities_count": opportunities.len(),
            "opportunities": opportunities,
            "total_net_edge": total_net_edge.to_string(),
            "best_edge_bps": best_edge_bps.to_string(),
            "strategies_triggered": strategies_triggered,
        });
        if !sweep_warnings.is_empty() {
            entry["warnings"] = serde_json::json!(sweep_warnings);
        }
        grid_results.push(entry);
    }

    let elapsed = start.elapsed().as_millis();

    let result = serde_json::json!({
        "param": req.param,
        "grid_points": req.values.len(),
        "markets_scanned": markets_scanned,
        "sweep_time_ms": elapsed,
        "results": grid_results,
    });

    (StatusCode::OK, Json(result)).into_response()
}

// ─── explain ─────────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ExplainRequest {
    pub condition_id: String,
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
}

pub async fn explain(
    State(state): State<AppState>,
    Json(req): Json<ExplainRequest>,
) -> impl IntoResponse {
    let start = Instant::now();

    let base_config = match state.config.read() {
        Ok(c) => c.clone(),
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "config lock poisoned" })),
            )
                .into_response()
        }
    };
    let config = base_config.with_overrides(&req.config_overrides);

    // Look up the target market
    let target = match state.market_cache.get(&req.condition_id) {
        Some(m) => m,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": format!("market {} not found in cache", req.condition_id)
                })),
            )
                .into_response();
        }
    };

    let slippage = build_slippage(&config);

    // All active markets (needed for context-dependent strategies)
    let all_markets = state.market_cache.active_markets();
    // Single-market slice for single-market strategies
    let single = vec![target.clone()];

    // Strategy names, enabled flags, and whether they need context (all markets)
    struct StrategySpec {
        name: &'static str,
        enabled: bool,
        /// true = pass all_markets (context-dependent), false = pass single market
        context: bool,
    }

    let specs = [
        StrategySpec {
            name: "intra_market",
            enabled: config.strategy.intra_market_enabled,
            context: false,
        },
        StrategySpec {
            name: "multi_outcome",
            enabled: config.strategy.multi_outcome_enabled,
            context: true,
        },
        StrategySpec {
            name: "cross_market",
            enabled: config.strategy.cross_market_enabled,
            context: true,
        },
        StrategySpec {
            name: "resolution_sniping",
            enabled: config.strategy.resolution_sniping_enabled,
            context: false,
        },
        StrategySpec {
            name: "stale_market",
            enabled: config.strategy.stale_market_enabled,
            context: true,
        },
        StrategySpec {
            name: "volume_spike",
            enabled: config.strategy.volume_spike_enabled,
            context: false,
        },
        StrategySpec {
            name: "prob_model",
            enabled: config.strategy.prob_model_enabled,
            context: false,
        },
        StrategySpec {
            name: "liquidity_sniping",
            enabled: config.strategy.liquidity_sniping_enabled,
            context: false,
        },
        StrategySpec {
            name: "market_making",
            enabled: config.strategy.market_making_enabled,
            context: false,
        },
    ];

    // Build individual detectors to run them one-by-one
    let mut strategy_results = Vec::new();

    for spec in &specs {
        if !spec.enabled {
            strategy_results.push(serde_json::json!({
                "strategy": spec.name,
                "enabled": false,
                "result": "skipped",
                "reason": "strategy disabled in config",
            }));
            continue;
        }

        // ProbModel needs special handling — skip with explanation
        if spec.name == "prob_model" {
            strategy_results.push(serde_json::json!({
                "strategy": spec.name,
                "enabled": true,
                "result": "skipped",
                "reason": "ProbModelDetector requires a ProbabilityEstimator \
                           only available in the live engine",
            }));
            continue;
        }

        // Build a single-strategy detector
        let detector: Box<dyn ArbDetector> = match spec.name {
            "intra_market" => Box::new(IntraMarketDetector::new(
                config.strategy.intra_market.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "multi_outcome" => Box::new(MultiOutcomeDetector::new(
                config.strategy.multi_outcome.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "cross_market" => {
                let mut graph = if let Some(ref file) =
                    config.strategy.cross_market.correlation_file
                {
                    let path = ArbConfig::config_dir().join(file);
                    if path.exists() {
                        CorrelationGraph::load(&path)
                            .unwrap_or_else(|_| CorrelationGraph::empty())
                    } else {
                        CorrelationGraph::empty()
                    }
                } else {
                    CorrelationGraph::empty()
                };
                let plain: Vec<_> =
                    all_markets.iter().map(|m| m.as_ref().clone()).collect();
                let auto_pairs = CorrelationGraph::auto_detect(&plain);
                graph.merge(auto_pairs);

                Box::new(CrossMarketDetector::new(
                    config.strategy.cross_market.clone(),
                    config.strategy.clone(),
                    Arc::new(graph),
                    state.market_cache.clone(),
                    slippage.clone(),
                ))
            }
            "resolution_sniping" => Box::new(ResolutionSnipingDetector::new(
                config.strategy.resolution_sniping.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "stale_market" => Box::new(StaleMarketDetector::new(
                config.strategy.stale_market.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "volume_spike" => Box::new(VolumeSpikeDetector::new(
                config.strategy.volume_spike.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "liquidity_sniping" => Box::new(LiquiditySnipingDetector::new(
                config.strategy.liquidity_sniping.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            "market_making" => Box::new(MarketMakingDetector::new(
                config.strategy.market_making.clone(),
                config.strategy.clone(),
                slippage.clone(),
            )),
            _ => unreachable!(),
        };

        let scan_markets = if spec.context {
            &all_markets
        } else {
            &single
        };

        match detector.scan(scan_markets).await {
            Ok(opps) => {
                // Filter to only opportunities involving our target market
                let relevant: Vec<&Opportunity> = opps
                    .iter()
                    .filter(|o| o.markets.contains(&req.condition_id))
                    .collect();

                if relevant.is_empty() {
                    strategy_results.push(serde_json::json!({
                        "strategy": spec.name,
                        "enabled": true,
                        "result": "no_opportunity",
                        "reason": format!(
                            "detector ran successfully but found no opportunity for {}",
                            req.condition_id
                        ),
                    }));
                } else {
                    strategy_results.push(serde_json::json!({
                        "strategy": spec.name,
                        "enabled": true,
                        "result": "opportunity_found",
                        "opportunities": relevant,
                        "reason": format!(
                            "{} {} found for {}",
                            relevant.len(),
                            if relevant.len() == 1 { "opportunity" } else { "opportunities" },
                            req.condition_id
                        ),
                    }));
                }
            }
            Err(e) => {
                strategy_results.push(serde_json::json!({
                    "strategy": spec.name,
                    "enabled": true,
                    "result": "error",
                    "reason": format!("detector error: {e}"),
                }));
            }
        }
    }

    // Build market info summary
    let best_bid = target
        .orderbooks
        .first()
        .and_then(|b| b.bids.first())
        .map(|l| l.price);
    let best_ask = target
        .orderbooks
        .first()
        .and_then(|b| b.asks.first())
        .map(|l| l.price);
    let spread = match (best_bid, best_ask) {
        (Some(bid), Some(ask)) => Some(ask - bid),
        _ => None,
    };
    let total_bid_depth: Decimal = target
        .orderbooks
        .iter()
        .flat_map(|b| &b.bids)
        .map(|l| l.size)
        .sum();
    let total_ask_depth: Decimal = target
        .orderbooks
        .iter()
        .flat_map(|b| &b.asks)
        .map(|l| l.size)
        .sum();

    let elapsed = start.elapsed().as_millis();

    let result = serde_json::json!({
        "condition_id": req.condition_id,
        "market": {
            "question": target.question,
            "outcomes": target.outcomes,
            "prices": target.outcome_prices,
            "volume_24hr": target.volume_24hr,
            "active": target.active,
            "neg_risk": target.neg_risk,
            "best_bid": best_bid,
            "best_ask": best_ask,
            "spread": spread,
            "total_bid_depth": total_bid_depth.to_string(),
            "total_ask_depth": total_ask_depth.to_string(),
            "end_date_iso": target.end_date_iso,
            "event_id": target.event_id,
        },
        "strategies": strategy_results,
        "config_applied": config_used_json(&config),
        "explain_time_ms": elapsed,
    });

    (StatusCode::OK, Json(result)).into_response()
}

// ─── backtest (unchanged) ────────────────────────────────────────────────────

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

        let trade_size: Decimal = report.legs.iter().map(|l| l.filled_size).sum();

        // edge_bps = (realized_edge / notional) * 10000
        // realized_edge is a dollar amount, so normalize by trade notional first
        let notional: Decimal = report
            .legs
            .iter()
            .map(|l| l.filled_size * l.actual_fill_price)
            .sum();
        let edge_bps = if notional > Decimal::ZERO {
            (report.realized_edge / notional) * Decimal::from(10_000)
        } else {
            Decimal::ZERO
        };

        let would_exceed_exposure =
            cumulative_exposure + trade_size > config.risk.max_total_exposure;
        let below_min_edge = edge_bps.abs() < min_edge_bps;

        let (included, rejection_reason) = if below_min_edge {
            (
                false,
                Some(format!(
                    "edge {edge_bps} below min_edge_bps ({})",
                    config.strategy.min_edge_bps
                )),
            )
        } else if would_exceed_exposure {
            (
                false,
                Some(format!(
                    "would exceed max_total_exposure ({})",
                    config.risk.max_total_exposure
                )),
            )
        } else {
            (true, None)
        };

        if included {
            aggregate_pnl += net_pnl;
            cumulative_exposure += trade_size;
        }

        let date = report.timestamp.format("%Y-%m-%d").to_string();
        let entry = daily_pnl_tracker
            .entry(date)
            .or_insert((Decimal::ZERO, 0));
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

// ── Historical backtest ──────────────────────────────────────

#[derive(Deserialize)]
pub struct HistoricalBacktestRequest {
    pub since: DateTime<Utc>,
    pub until: DateTime<Utc>,
    #[serde(default)]
    pub config_overrides: SandboxConfigOverrides,
    pub resample_interval_secs: Option<u64>,
}

pub async fn backtest_historical(
    State(state): State<AppState>,
    Json(req): Json<HistoricalBacktestRequest>,
) -> impl IntoResponse {
    // Require the price history store
    let price_store = match &state.price_store {
        Some(store) => Arc::clone(store),
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(serde_json::json!({
                    "error": "Price history store not available"
                })),
            )
                .into_response();
        }
    };

    let base_config = state.config.read().unwrap().clone();
    let config = base_config.with_overrides(&req.config_overrides);
    let resample_secs = req.resample_interval_secs.unwrap_or(300);

    // Load all ticks in the time range
    let all_ticks = match price_store.get_all_ticks_in_range(req.since, req.until) {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Failed to load price ticks: {e}") })),
            )
                .into_response();
        }
    };

    if all_ticks.is_empty() {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "buckets": [],
                "total_opportunities": 0,
                "total_trades": 0,
                "final_pnl": "0",
                "time_range": {
                    "since": req.since.to_rfc3339(),
                    "until": req.until.to_rfc3339(),
                },
                "resample_interval_secs": resample_secs,
            })),
        )
            .into_response();
    }

    // Group ticks into time buckets
    let bucket_duration_ms = (resample_secs as i64) * 1000;
    let start_ms = req.since.timestamp_millis();

    // Determine the bucket index for each tick
    let mut bucket_map: std::collections::BTreeMap<i64, Vec<&arb_data::price_history::PriceTick>> =
        std::collections::BTreeMap::new();
    for tick in &all_ticks {
        let tick_ms = tick.timestamp.timestamp_millis();
        let bucket_idx = (tick_ms - start_ms) / bucket_duration_ms;
        bucket_map.entry(bucket_idx).or_default().push(tick);
    }

    // Cap at 1000 buckets
    let max_buckets = 1000usize;
    let bucket_entries: Vec<_> = bucket_map.into_iter().take(max_buckets).collect();

    // Set up detectors for scanning (reuse shared build_detectors helper)
    let slippage = build_slippage(&config);
    let detectors = build_detectors(&config, &state, slippage.clone());

    let edge_calculator = EdgeCalculator::from_config(
        &config.fees,
        config.slippage.prefer_post_only,
        slippage,
    );
    let fee_rate = config.fees.effective_rate(config.slippage.prefer_post_only);
    let paper_executor = PaperTradeExecutor::with_fee_rate(fee_rate);
    let min_edge = Decimal::from(config.strategy.min_edge_bps);

    let mut total_opportunities = 0usize;
    let mut total_trades = 0usize;
    let mut cumulative_pnl = Decimal::ZERO;
    let mut buckets_result = Vec::new();

    for (bucket_idx, ticks) in &bucket_entries {
        let bucket_ts_ms = start_ms + bucket_idx * bucket_duration_ms;
        let bucket_ts = chrono::TimeZone::timestamp_millis_opt(&Utc, bucket_ts_ms)
            .single()
            .unwrap_or_default();

        // Reconstruct MarketState per condition_id from ticks in this bucket
        let mut market_map: HashMap<String, MarketState> = HashMap::new();
        for tick in ticks {
            let ms = market_map
                .entry(tick.condition_id.clone())
                .or_insert_with(|| MarketState {
                    condition_id: tick.condition_id.clone(),
                    question: String::new(),
                    outcomes: vec![],
                    token_ids: vec![],
                    outcome_prices: vec![],
                    orderbooks: vec![],
                    volume_24hr: tick.volume_24h,
                    liquidity: None,
                    active: true,
                    neg_risk: false,
                    best_bid: tick.best_bid,
                    best_ask: tick.best_ask,
                    spread: match (tick.best_ask, tick.best_bid) {
                        (Some(a), Some(b)) => Some(a - b),
                        _ => None,
                    },
                    last_trade_price: Some(tick.price),
                    description: None,
                    end_date_iso: None,
                    slug: None,
                    one_day_price_change: None,
                    event_id: None,
                    last_updated_gen: 0,
                });

            // Add this token's data
            if !ms.token_ids.contains(&tick.token_id) {
                ms.token_ids.push(tick.token_id.clone());
                ms.outcome_prices.push(tick.price);
                ms.outcomes.push(format!("outcome_{}", ms.outcomes.len()));
                ms.orderbooks.push(OrderbookSnapshot {
                    token_id: tick.token_id.clone(),
                    bids: if let Some(bid) = tick.best_bid {
                        vec![arb_core::OrderbookLevel {
                            price: bid,
                            size: Decimal::from(100),
                        }]
                    } else {
                        vec![]
                    },
                    asks: if let Some(ask) = tick.best_ask {
                        vec![arb_core::OrderbookLevel {
                            price: ask,
                            size: Decimal::from(100),
                        }]
                    } else {
                        vec![]
                    },
                    timestamp: tick.timestamp,
                });
            }
        }

        let markets: Vec<Arc<MarketState>> = market_map.into_values().map(Arc::new).collect();

        // Run detectors
        let mut bucket_opps: Vec<Opportunity> = Vec::new();
        let mut detector_errors = 0usize;
        for detector in &detectors {
            match detector.scan(&markets).await {
                Ok(opps) => bucket_opps.extend(opps),
                Err(e) => {
                    tracing::debug!(
                        detector = %detector.arb_type(),
                        error = %e,
                        "Historical backtest detector error"
                    );
                    detector_errors += 1;
                }
            }
        }

        // Build a temporary cache from reconstructed historical data
        // instead of using the live market cache for VWAP refinement.
        let temp_cache = arb_data::market_cache::MarketCache::new();
        for m in &markets {
            temp_cache.update_one((**m).clone());
        }
        for opp in &mut bucket_opps {
            if edge_calculator.refine_with_vwap(opp, &temp_cache).is_err() {
                opp.net_edge = Decimal::ZERO;
            }
        }
        bucket_opps.retain(|o| o.net_edge_bps() >= min_edge);

        let detected = bucket_opps.len();
        total_opportunities += detected;

        // Execute through paper trader
        let mut bucket_trades = 0usize;
        let mut bucket_exec_errors = 0usize;
        for opp in &bucket_opps {
            match paper_executor.execute_opportunity(opp).await {
                Ok(report) => {
                    let net_pnl = report.realized_edge - report.total_fees;
                    cumulative_pnl += net_pnl;
                    bucket_trades += 1;
                }
                Err(e) => {
                    tracing::debug!(opp_id = %opp.id, error = %e, "Backtest execution failed");
                    bucket_exec_errors += 1;
                }
            }
        }
        total_trades += bucket_trades;

        buckets_result.push(serde_json::json!({
            "ts": bucket_ts.to_rfc3339(),
            "opportunities_detected": detected,
            "trades_executed": bucket_trades,
            "execution_errors": bucket_exec_errors,
            "detector_errors": detector_errors,
            "cumulative_pnl": cumulative_pnl.to_string(),
        }));
    }

    let result = serde_json::json!({
        "buckets": buckets_result,
        "total_opportunities": total_opportunities,
        "total_trades": total_trades,
        "final_pnl": cumulative_pnl.to_string(),
        "time_range": {
            "since": req.since.to_rfc3339(),
            "until": req.until.to_rfc3339(),
        },
        "resample_interval_secs": resample_secs,
    });

    (StatusCode::OK, Json(result)).into_response()
}
