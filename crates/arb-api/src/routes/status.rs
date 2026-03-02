use axum::{Json, extract::State};
use arb_core::traits::RiskManager;

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

    Json(serde_json::json!({
        "mode": mode,
        "kill_switch_active": kill_active,
        "market_count": market_count,
        "uptime_secs": uptime_secs,
    }))
}
