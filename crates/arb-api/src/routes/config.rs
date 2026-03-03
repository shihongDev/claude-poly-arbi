use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};
use arb_core::config::ArbConfig;

use crate::state::AppState;

pub async fn get_config(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().unwrap();
    match serde_json::to_value(&*config) {
        Ok(json) => (StatusCode::OK, Json(json)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        )
            .into_response(),
    }
}

pub async fn update_config(
    State(state): State<AppState>,
    Json(new_config): Json<ArbConfig>,
) -> impl IntoResponse {
    let errors = new_config.validate();
    if !errors.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({"errors": errors})),
        )
            .into_response();
    }

    let mut config = state.config.write().unwrap();
    *config = new_config.clone();
    drop(config);

    // Persist to disk so changes survive restarts
    if let Err(e) = new_config.save() {
        tracing::warn!(error = %e, "Config updated in-memory but failed to persist to disk");
    }

    (StatusCode::OK, Json(serde_json::json!({"status": "updated"}))).into_response()
}
