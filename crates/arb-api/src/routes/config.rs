use axum::{Json, extract::State};
use arb_core::config::ArbConfig;

use crate::state::AppState;

pub async fn get_config(State(state): State<AppState>) -> Json<serde_json::Value> {
    let config = state.config.read().unwrap();
    Json(serde_json::to_value(&*config).unwrap_or_default())
}

pub async fn update_config(
    State(state): State<AppState>,
    Json(new_config): Json<ArbConfig>,
) -> Json<serde_json::Value> {
    let mut config = state.config.write().unwrap();
    *config = new_config;
    Json(serde_json::json!({"status": "updated"}))
}
