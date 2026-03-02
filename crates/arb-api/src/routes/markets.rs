use axum::{
    Json,
    extract::{Path, Query, State},
    http::StatusCode,
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
) -> Json<serde_json::Value> {
    let limit = query.limit.unwrap_or(500).min(2000);
    let with_orderbooks = query.with_orderbooks.unwrap_or(false);

    let markets = state.market_cache.active_markets();

    let filtered: Vec<_> = if with_orderbooks {
        // Return markets with orderbooks first (most useful for arb)
        markets
            .into_iter()
            .filter(|m| !m.orderbooks.is_empty())
            .take(limit)
            .collect()
    } else {
        // Return markets with orderbooks first, then others
        let (with_books, without_books): (Vec<_>, Vec<_>) =
            markets.into_iter().partition(|m| !m.orderbooks.is_empty());
        with_books
            .into_iter()
            .chain(without_books)
            .take(limit)
            .collect()
    };

    Json(serde_json::to_value(&filtered).unwrap_or_default())
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
