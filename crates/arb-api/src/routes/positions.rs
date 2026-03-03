use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use crate::state::AppState;

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
