use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use serde::Deserialize;
use tracing::info;
use uuid::Uuid;

use arb_core::types::{
    ArbType, ExecutionReport, FillStatus, LegReport, Side, TradingMode,
};
use arb_core::traits::RiskManager;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct OrderRequest {
    pub token_id: String,
    pub condition_id: String,
    pub side: Side,
    pub price: Decimal,
    pub size: Decimal,
}

pub async fn place_order(
    State(state): State<AppState>,
    Json(req): Json<OrderRequest>,
) -> impl IntoResponse {
    // Validate price in (0, 1]
    if req.price <= dec!(0) || req.price > dec!(1) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "price must be in (0, 1]" })),
        );
    }
    // Validate size > 0
    if req.size <= dec!(0) {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "size must be > 0" })),
        );
    }

    let leg = LegReport {
        order_id: Uuid::new_v4().to_string(),
        token_id: req.token_id.clone(),
        condition_id: req.condition_id.clone(),
        side: req.side,
        expected_vwap: req.price,
        actual_fill_price: req.price,
        filled_size: req.size,
        status: FillStatus::FullyFilled,
    };

    let report = ExecutionReport {
        opportunity_id: Uuid::new_v4(),
        legs: vec![leg],
        realized_edge: dec!(0),
        slippage: dec!(0),
        total_fees: dec!(0),
        timestamp: Utc::now(),
        mode: TradingMode::Paper,
    };

    // Update risk limits + position tracker
    {
        let mut rl = state.risk_limits.lock().unwrap();
        rl.record_execution(&report, ArbType::IntraMarket);
    }

    // Append to execution history
    if let Ok(mut history) = state.execution_history.write() {
        history.insert(0, report.clone());
        history.truncate(500);
    }

    // Broadcast trade_executed event
    let _ = broadcast_event(&state, "trade_executed", &report);

    // Broadcast position_update event
    {
        let rl = state.risk_limits.lock().unwrap();
        if let Ok(tracker) = rl.positions().lock() {
            let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
            let _ = broadcast_event(&state, "position_update", &positions);
        }
    }

    info!(
        mode = "paper",
        side = ?req.side,
        token_id = %req.token_id,
        price = %req.price,
        size = %req.size,
        "Manual paper order placed"
    );

    match serde_json::to_value(&report) {
        Ok(json) => (StatusCode::OK, Json(json)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        ),
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
