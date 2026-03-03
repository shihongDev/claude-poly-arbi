use axum::{
    extract::State,
    http::{StatusCode, header},
    response::IntoResponse,
};

use crate::state::AppState;

/// Returns pre-serialized metrics JSON from the engine loop cache.
/// No mutex lock needed — the engine pre-computes metrics every cycle.
pub async fn get_metrics(State(state): State<AppState>) -> impl IntoResponse {
    let json = state.cached_metrics_json.read().unwrap().clone();
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, "application/json"),
            (header::CACHE_CONTROL, "max-age=5"),
        ],
        json,
    )
}
