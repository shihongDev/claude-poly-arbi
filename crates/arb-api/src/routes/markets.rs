use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
};

use crate::state::AppState;

pub async fn list_markets(State(state): State<AppState>) -> Json<serde_json::Value> {
    let markets = state.market_cache.all_markets();
    Json(serde_json::to_value(&markets).unwrap_or_default())
}

pub async fn get_market(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.market_cache.get(&id) {
        Some(market) => {
            let json = serde_json::to_value(&market).unwrap_or_default();
            (StatusCode::OK, Json(json)).into_response()
        }
        None => {
            let json = serde_json::json!({"error": "market not found"});
            (StatusCode::NOT_FOUND, Json(json)).into_response()
        }
    }
}
