use axum::{Json, extract::State};

use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let opps = state.opportunities.read().unwrap();
    Json(serde_json::to_value(&*opps).unwrap_or_default())
}
