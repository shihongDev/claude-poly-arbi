use axum::{Json, extract::{Query, State}, http::StatusCode, response::IntoResponse};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct HistoryParams {
    pub limit: Option<usize>,
}

pub async fn list(
    Query(params): Query<HistoryParams>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    let limit = params.limit.unwrap_or(50);
    let history = state.execution_history.read().unwrap();

    let entries: Vec<_> = history.iter().rev().take(limit).cloned().collect();
    match serde_json::to_value(&entries) {
        Ok(json) => (StatusCode::OK, Json(json)).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
        )
            .into_response(),
    }
}
