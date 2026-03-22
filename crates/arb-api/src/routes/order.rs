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

use super::helpers::{append_history, broadcast_event, broadcast_positions};

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
    // Reject orders when kill switch is active
    if state.kill_switch_active.load(std::sync::atomic::Ordering::Relaxed) {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(serde_json::json!({ "error": "Kill switch is active — trading is halted" })),
        );
    }

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

    // For sell orders, compute realized PnL against the existing position's
    // average entry price. Buy orders have no realized PnL at placement time.
    let realized = if req.side == Side::Sell {
        let rl = state.risk_limits.lock().unwrap();
        if let Ok(tracker) = rl.positions().lock() {
            if let Some(pos) = tracker.get(&req.token_id) {
                (req.price - pos.avg_entry_price) * req.size
            } else {
                dec!(0)
            }
        } else {
            dec!(0)
        }
    } else {
        dec!(0)
    };

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

    let trading_mode = {
        let cfg = state.config.read().unwrap();
        if cfg.is_live() { TradingMode::Live } else { TradingMode::Paper }
    };

    let report = ExecutionReport {
        opportunity_id: Uuid::new_v4(),
        legs: vec![leg],
        realized_edge: realized,
        slippage: dec!(0),
        total_fees: dec!(0),
        timestamp: Utc::now(),
        mode: trading_mode,
    };

    // Update risk limits + position tracker
    {
        let mut rl = state.risk_limits.lock().unwrap();
        rl.record_execution(&report, ArbType::IntraMarket);
    }

    append_history(&state, &report);
    let _ = broadcast_event(&state, "trade_executed", &report);
    broadcast_positions(&state);

    let mode_str = if trading_mode == TradingMode::Live { "live" } else { "paper" };
    info!(
        mode = mode_str,
        side = ?req.side,
        token_id = %req.token_id,
        price = %req.price,
        size = %req.size,
        "Manual order placed"
    );

    match serde_json::to_value(&report) {
        Ok(json) => (StatusCode::OK, Json(json)),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        ),
    }
}
