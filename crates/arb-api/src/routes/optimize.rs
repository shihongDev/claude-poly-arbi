use arb_core::Side;
use arb_data::impact::MarketImpactEstimator;
use arb_risk::limits::kelly_criterion;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use rust_decimal::Decimal;
use serde::Deserialize;

use crate::state::AppState;

// ── Market impact endpoint ───────────────────────────────────

#[derive(Deserialize)]
pub struct ImpactRequest {
    pub token_id: String,
    pub side: String,
    pub size: Decimal,
}

pub async fn impact(
    State(state): State<AppState>,
    Json(req): Json<ImpactRequest>,
) -> impl IntoResponse {
    let side = match req.side.to_lowercase().as_str() {
        "buy" => Side::Buy,
        "sell" => Side::Sell,
        _ => {
            return (
                StatusCode::BAD_REQUEST,
                Json(serde_json::json!({ "error": "side must be 'buy' or 'sell'" })),
            )
                .into_response();
        }
    };

    // Find the market containing this token_id
    let markets = state.market_cache.all_markets();
    let market = markets.iter().find(|m| m.token_ids.contains(&req.token_id));

    let market = match market {
        Some(m) => m,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({ "error": "Token ID not found in any cached market" })),
            )
                .into_response();
        }
    };

    // Find the orderbook for this specific token
    let book = market
        .orderbooks
        .iter()
        .find(|ob| ob.token_id == req.token_id);

    let book = match book {
        Some(b) => b,
        None => {
            return (
                StatusCode::NOT_FOUND,
                Json(serde_json::json!({
                    "error": "No orderbook data available for this token"
                })),
            )
                .into_response();
        }
    };

    let estimator = MarketImpactEstimator::default_config();
    let estimate = estimator.estimate(book, side, req.size, market.volume_24hr);

    // Also compute VWAP estimate by walking the book
    let vwap_estimate = estimate.effective_price;

    let result = serde_json::json!({
        "impact_bps": estimate.impact_bps.to_string(),
        "effective_price": estimate.effective_price.to_string(),
        "depth_consumed_pct": estimate.depth_consumed_pct,
        "vwap_estimate": vwap_estimate.to_string(),
        "market_volume_24hr": market.volume_24hr.map(|v| v.to_string()),
    });

    (StatusCode::OK, Json(result)).into_response()
}

// ── Portfolio optimization endpoint ──────────────────────────

#[derive(Deserialize)]
pub struct OptimizeRequest {
    pub candidates: Vec<CandidateOpportunity>,
    pub kelly_multiplier: Option<f64>,
    #[serde(default)]
    pub risk_preferences: Option<RiskPreferences>,
}

#[derive(Deserialize)]
pub struct CandidateOpportunity {
    pub condition_id: String,
    pub net_edge: Decimal,
    pub confidence: f64,
    pub loss_if_wrong: Decimal,
}

#[derive(Deserialize)]
pub struct RiskPreferences {
    pub max_position_per_market: Option<Decimal>,
    pub max_total_exposure: Option<Decimal>,
}

pub async fn optimize(
    State(state): State<AppState>,
    Json(req): Json<OptimizeRequest>,
) -> impl IntoResponse {
    let kelly_mult = req.kelly_multiplier.unwrap_or(0.25);

    // Read current bankroll from risk limits (equity = starting_equity for now)
    let bankroll = match state.config.read() {
        Ok(c) => c.general.starting_equity,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": "Config lock poisoned" })),
            )
                .into_response();
        }
    };

    let max_pos = req
        .risk_preferences
        .as_ref()
        .and_then(|p| p.max_position_per_market);
    let max_total = req
        .risk_preferences
        .as_ref()
        .and_then(|p| p.max_total_exposure);

    let mut allocations = Vec::new();
    let mut total_allocated = Decimal::ZERO;

    for candidate in &req.candidates {
        let result = kelly_criterion(
            candidate.confidence,
            candidate.net_edge,
            candidate.loss_if_wrong,
            bankroll,
            kelly_mult,
        );

        let mut suggested = result.suggested_size;
        let mut capped_by: Option<&str> = None;

        // Apply per-market cap
        if let Some(max_per_market) = max_pos
            && suggested > max_per_market
        {
            suggested = max_per_market;
            capped_by = Some("max_position");
        }

        // Apply total exposure cap
        if let Some(max_exposure) = max_total {
            let remaining = max_exposure - total_allocated;
            if suggested > remaining {
                suggested = remaining.max(Decimal::ZERO);
                capped_by = Some("max_exposure");
            }
        }

        total_allocated += suggested;

        allocations.push(serde_json::json!({
            "condition_id": candidate.condition_id,
            "suggested_size": suggested.to_string(),
            "kelly_fraction": result.kelly_fraction,
            "adjusted_fraction": result.adjusted_fraction,
            "capped_by": capped_by,
        }));
    }

    let result = serde_json::json!({
        "allocations": allocations,
        "total_allocated": total_allocated.to_string(),
        "bankroll": bankroll.to_string(),
    });

    (StatusCode::OK, Json(result)).into_response()
}
