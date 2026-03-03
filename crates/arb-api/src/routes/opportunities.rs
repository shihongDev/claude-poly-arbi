use axum::{
    Json,
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};

use crate::state::AppState;

pub async fn list(State(state): State<AppState>) -> impl IntoResponse {
    let opps = state.opportunities.read().unwrap();
    match serde_json::to_value(&*opps) {
        Ok(json) => (
            StatusCode::OK,
            [(header::CACHE_CONTROL, "max-age=5")],
            Json(json),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        )
            .into_response(),
    }
}
