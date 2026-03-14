use axum::{
    Json,
    extract::{Path, Query, State},
    http::{StatusCode, header},
    response::IntoResponse,
};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct MarketsQuery {
    /// Maximum number of markets to return (default 500)
    pub limit: Option<usize>,
    /// If true, only return markets with orderbooks
    pub with_orderbooks: Option<bool>,
}

pub async fn list_markets(
    State(state): State<AppState>,
    Query(query): Query<MarketsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(500).min(2000);
    let with_orderbooks = query.with_orderbooks.unwrap_or(false);

    let markets = state.market_cache.active_markets();

    let filtered: Vec<_> = if with_orderbooks {
        markets
            .into_iter()
            .filter(|m| !m.orderbooks.is_empty())
            .take(limit)
            .collect()
    } else {
        let (with_books, without_books): (Vec<_>, Vec<_>) =
            markets.into_iter().partition(|m| !m.orderbooks.is_empty());
        with_books
            .into_iter()
            .chain(without_books)
            .take(limit)
            .collect()
    };

    match serde_json::to_value(&filtered) {
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

pub async fn get_market(
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> impl IntoResponse {
    match state.market_cache.get(&id) {
        Some(market) => match serde_json::to_value(&market) {
            Ok(json) => (StatusCode::OK, Json(json)).into_response(),
            Err(e) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({"error": format!("Serialization failed: {e}")})),
            )
                .into_response(),
        },
        None => {
            let json = serde_json::json!({"error": "market not found"});
            (StatusCode::NOT_FOUND, Json(json)).into_response()
        }
    }
}
