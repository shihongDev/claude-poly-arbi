use arb_core::traits::RiskManager;
use axum::{Json, extract::State};

use crate::state::AppState;

pub async fn get_status(State(state): State<AppState>) -> Json<serde_json::Value> {
    let risk = state.risk_limits.lock().unwrap();
    let kill_active = risk.is_kill_switch_active();
    drop(risk);

    let config = state.config.read().unwrap();
    let mode = config.general.trading_mode.clone();
    drop(config);

    let market_count = state.market_cache.len();
    let uptime_secs = state.start_time.elapsed().as_secs();

    // Capitalize mode to match frontend TradingMode type ("Paper" | "Live")
    let mode_display = match mode.as_str() {
        "live" => "Live",
        _ => "Paper",
    };

    Json(serde_json::json!({
        "mode": mode_display,
        "kill_switch_active": kill_active,
        "kill_switch_reason": serde_json::Value::Null,
        "market_count": market_count,
        "uptime_secs": uptime_secs,
    }))
}
