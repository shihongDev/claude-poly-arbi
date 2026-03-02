use axum::{Json, extract::{Query, State}};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct HistoryParams {
    pub limit: Option<usize>,
}

pub async fn list(
    Query(params): Query<HistoryParams>,
    State(state): State<AppState>,
) -> Json<serde_json::Value> {
    let limit = params.limit.unwrap_or(50);
    let history = state.execution_history.read().unwrap();

    let entries: Vec<_> = history.iter().rev().take(limit).cloned().collect();
    Json(serde_json::to_value(&entries).unwrap_or_default())
}
