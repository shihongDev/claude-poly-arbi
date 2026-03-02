use axum::{Json, extract::State};

use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> Json<serde_json::Value> {
    let risk = state.risk_limits.lock().unwrap();
    let tracker = risk.positions();
    drop(risk);

    let tracker = tracker.lock().unwrap();
    let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
    Json(serde_json::to_value(&positions).unwrap_or_default())
}
