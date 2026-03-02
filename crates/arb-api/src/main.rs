mod routes;
mod state;
mod ws;

use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_data::market_cache::MarketCache;
use arb_risk::limits::RiskLimits;
use axum::routing::{get, post};
use axum::Router;
use rust_decimal::Decimal;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(tracing_subscriber::fmt::layer())
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "arb_api=debug,tower_http=debug".into()),
        )
        .init();

    let config = ArbConfig::load();
    let (ws_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    let state = state::AppState {
        market_cache: Arc::new(MarketCache::new()),
        risk_limits: Arc::new(Mutex::new(RiskLimits::new(
            config.risk.clone(),
            Decimal::from(10_000),
        ))),
        config: Arc::new(RwLock::new(config)),
        ws_tx,
        opportunities: Arc::new(RwLock::new(Vec::new())),
        execution_history: Arc::new(RwLock::new(Vec::new())),
        start_time: Instant::now(),
    };

    let app = Router::new()
        .route("/api/status", get(routes::status::get_status))
        .route("/api/opportunities", get(routes::opportunities::list))
        .route("/api/positions", get(routes::positions::list))
        .route("/api/metrics", get(routes::metrics::get_metrics))
        .route("/api/markets", get(routes::markets::list_markets))
        .route("/api/markets/{id}", get(routes::markets::get_market))
        .route("/api/history", get(routes::history::list))
        .route(
            "/api/config",
            get(routes::config::get_config).put(routes::config::update_config),
        )
        .route("/api/kill", post(routes::control::kill))
        .route("/api/resume", post(routes::control::resume))
        .route(
            "/api/simulate/{condition_id}",
            post(routes::simulate::run_simulation),
        )
        .route("/ws", get(ws::ws_handler))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("arb-api server listening on http://0.0.0.0:8080");
    axum::serve(listener, app).await?;
    Ok(())
}
