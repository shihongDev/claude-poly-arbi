use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tracing::info;
use uuid::Uuid;

use arb_core::traits::RiskManager;
use arb_core::types::{
    ArbType, ExecutionReport, FillStatus, LegReport, Position, Side, TradingMode,
};

use crate::state::AppState;

use super::helpers::{append_history, broadcast_event, broadcast_positions};

// ---------------------------------------------------------------------------
// List all positions
// ---------------------------------------------------------------------------

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let risk = state.risk_limits.lock().unwrap();
    let tracker = risk.positions();
    drop(risk);

    let tracker = tracker.lock().unwrap();
    let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
    match serde_json::to_value(&positions) {
        Ok(json) => (StatusCode::OK, Json(json)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Close a single position entirely
// ---------------------------------------------------------------------------

pub async fn close_position(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
) -> impl IntoResponse {
    // Snapshot the position while holding the lock briefly
    let position = {
        let rl = state.risk_limits.lock().unwrap();
        let positions_arc = rl.positions();
        let tracker = positions_arc.lock().unwrap();
        tracker.get(&token_id).cloned()
    };

    let Some(pos) = position else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Position not found"})),
        );
    };

    if pos.size <= Decimal::ZERO {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Position size is zero"})),
        );
    }

    let report = build_close_report(&pos, pos.size);

    {
        let mut rl = state.risk_limits.lock().unwrap();
        rl.record_execution(&report, ArbType::IntraMarket);
    }

    append_history(&state, &report);
    let _ = broadcast_event(&state, "trade_executed", &report);
    broadcast_positions(&state);

    info!(
        mode = "paper",
        token_id = %token_id,
        size = %pos.size,
        "Position closed"
    );

    match serde_json::to_value(&report) {
        Ok(json) => (StatusCode::OK, Json(json)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Close all active positions
// ---------------------------------------------------------------------------

#[derive(serde::Serialize)]
pub struct CloseAllResponse {
    pub closed: usize,
    pub reports: Vec<ExecutionReport>,
}

pub async fn close_all(State(state): State<AppState>) -> impl IntoResponse {
    // Snapshot active positions, build reports, and record all executions
    // under a single lock acquisition to avoid TOCTOU races.
    let reports: Vec<ExecutionReport> = {
        let mut rl = state.risk_limits.lock().unwrap();
        let active: Vec<Position> = {
            let positions_arc = rl.positions();
            let tracker = positions_arc.lock().unwrap();
            tracker
                .all_positions()
                .into_iter()
                .filter(|p| p.size > Decimal::ZERO)
                .cloned()
                .collect()
        };

        let mut reports = Vec::with_capacity(active.len());
        for pos in &active {
            let report = build_close_report(pos, pos.size);
            rl.record_execution(&report, ArbType::IntraMarket);
            reports.push(report);
        }
        reports
    };

    // Append history and broadcast outside the lock
    for report in &reports {
        append_history(&state, report);
    }
    let _ = broadcast_event(&state, "trade_executed", &reports);
    broadcast_positions(&state);

    let closed = reports.len();
    info!(mode = "paper", closed = closed, "All positions closed");

    let resp = CloseAllResponse { closed, reports };
    match serde_json::to_value(&resp) {
        Ok(json) => (StatusCode::OK, Json(json)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        )
            .into_response(),
    }
}

// ---------------------------------------------------------------------------
// Reduce (partial close) a position
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ReduceRequest {
    pub size: Decimal,
}

pub async fn reduce_position(
    State(state): State<AppState>,
    Path(token_id): Path<String>,
    Json(req): Json<ReduceRequest>,
) -> impl IntoResponse {
    if req.size <= dec!(0) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "size must be > 0"})),
        );
    }

    let position = {
        let rl = state.risk_limits.lock().unwrap();
        let positions_arc = rl.positions();
        let tracker = positions_arc.lock().unwrap();
        tracker.get(&token_id).cloned()
    };

    let Some(pos) = position else {
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "Position not found"})),
        );
    };

    if pos.size <= Decimal::ZERO {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Position size is zero"})),
        );
    }

    if req.size > pos.size {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"error": "Reduce size exceeds position size"})),
        );
    }

    let report = build_close_report(&pos, req.size);

    {
        let mut rl = state.risk_limits.lock().unwrap();
        rl.record_execution(&report, ArbType::IntraMarket);
    }

    append_history(&state, &report);
    let _ = broadcast_event(&state, "trade_executed", &report);
    broadcast_positions(&state);

    info!(
        mode = "paper",
        token_id = %token_id,
        size = %req.size,
        "Position reduced"
    );

    match serde_json::to_value(&report) {
        Ok(json) => (StatusCode::OK, Json(json)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        ),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn build_close_report(pos: &Position, close_size: Decimal) -> ExecutionReport {
    let realized = (pos.current_price - pos.avg_entry_price) * close_size;

    let leg = LegReport {
        order_id: Uuid::new_v4().to_string(),
        token_id: pos.token_id.clone(),
        condition_id: pos.condition_id.clone(),
        side: Side::Sell,
        expected_vwap: pos.current_price,
        actual_fill_price: pos.current_price,
        filled_size: close_size,
        status: FillStatus::FullyFilled,
    };

    ExecutionReport {
        opportunity_id: Uuid::new_v4(),
        legs: vec![leg],
        realized_edge: realized,
        slippage: dec!(0),
        total_fees: dec!(0),
        timestamp: Utc::now(),
        mode: TradingMode::Paper,
    }
}
