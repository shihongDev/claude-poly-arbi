use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

/// GET /api/orders — list open orders
pub async fn list_orders(State(state): State<AppState>) -> impl IntoResponse {
    match state.executor.open_orders().await {
        Ok(orders) => (
            StatusCode::OK,
            Json(serde_json::json!({ "orders": orders })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// DELETE /api/orders/{id} — cancel single order
pub async fn cancel_order(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    match state.executor.cancel_order(&id).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "cancelled": id }))).into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}

/// DELETE /api/orders — cancel all open orders
pub async fn cancel_all_orders(State(state): State<AppState>) -> impl IntoResponse {
    match state.executor.cancel_all().await {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "cancelled_all": true })),
        )
            .into_response(),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
    }
}
