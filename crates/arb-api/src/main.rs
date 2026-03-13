mod engine_task;
mod routes;
mod state;
mod ws;

use std::path::Path;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, OnceLock, RwLock};
use std::time::Instant;

use arb_core::ExecutionReport;
use arb_core::config::ArbConfig;
use arb_core::traits::TradeExecutor;
use arb_data::market_cache::MarketCache;
use arb_data::price_history::PriceHistoryStore;
use arb_risk::limits::RiskLimits;
use arb_risk::position_tracker::PositionTracker;
use axum::Router;
use axum::routing::{delete, get, post};
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

use crate::state::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| {
                "arb_api=info,arb_core=info,arb_data=info,arb_execution=info,arb_risk=info,arb_strategy=info,arb_simulation=info,tower_http=debug".into()
            }),
        )
        .with(fmt::layer().json())
        .init();

    let config = ArbConfig::load();
    let (ws_tx, _) = tokio::sync::broadcast::channel::<String>(256);

    // ── Persistence paths ──────────────────────────────────────
    let config_dir = ArbConfig::config_dir();
    let positions_path = config_dir.join("positions.json");
    let history_path = config_dir.join("history.json");

    // ── Load persisted positions ───────────────────────────────
    let mut risk_limits = RiskLimits::new(config.risk.clone(), config.general.starting_equity);

    if positions_path.exists() {
        match PositionTracker::load(&positions_path) {
            Ok(tracker) => {
                let count = tracker.active_count();
                risk_limits.load_positions(tracker);
                tracing::info!(
                    path = %positions_path.display(),
                    active_positions = count,
                    "Loaded positions from disk"
                );
            }
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %positions_path.display(),
                    "Failed to load positions, starting fresh"
                );
            }
        }
    }

    // ── Load persisted execution history ───────────────────────
    let initial_history: Vec<ExecutionReport> = if history_path.exists() {
        match std::fs::read_to_string(&history_path) {
            Ok(content) => match serde_json::from_str::<Vec<ExecutionReport>>(&content) {
                Ok(history) => {
                    tracing::info!(
                        path = %history_path.display(),
                        trade_count = history.len(),
                        "Loaded execution history from disk"
                    );
                    history
                }
                Err(e) => {
                    tracing::error!(
                        error = %e,
                        path = %history_path.display(),
                        bytes = content.len(),
                        "Failed to parse execution history — backing up corrupt file"
                    );
                    let bak = history_path.with_extension("json.bak");
                    let _ = std::fs::rename(&history_path, &bak);
                    Vec::new()
                }
            },
            Err(e) => {
                tracing::warn!(
                    error = %e,
                    path = %history_path.display(),
                    "Failed to load history, starting fresh"
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };

    // Check if the file-based kill switch is already active at startup
    let kill_switch_initial = arb_risk::kill_switch::KillSwitch::new().is_active();

    // ── Create executor based on configured trading mode ─────────
    let executor: Arc<dyn TradeExecutor> = if config.is_live() {
        tracing::info!("Starting in LIVE trading mode");
        let key_path = config.general.key_file.as_ref().map(std::path::Path::new);
        let live = arb_execution::executor::LiveTradeExecutor::from_key_file(
            key_path,
            config.slippage.prefer_post_only,
            config.risk.order_timeout_secs,
        )
        .await
        .map_err(|e| {
            anyhow::anyhow!(
                "Live mode requested but authentication failed: {e}. \
                 Fix your key_file config or remove --live to use paper mode."
            )
        })?;
        Arc::new(live)
    } else {
        tracing::info!("Starting in PAPER trading mode");
        Arc::new(arb_execution::paper_trade::PaperTradeExecutor::default_pessimism())
    };

    // ── Open price history store ─────────────────────────────────
    let price_store = match PriceHistoryStore::open(&config_dir.join("price_history.db")) {
        Ok(store) => {
            tracing::info!("Price history store opened");
            Some(Arc::new(store))
        }
        Err(e) => {
            tracing::warn!(error = %e, "Failed to open price history store");
            None
        }
    };

    let state = AppState {
        market_cache: Arc::new(MarketCache::new()),
        risk_limits: Arc::new(Mutex::new(risk_limits)),
        kill_switch_active: Arc::new(AtomicBool::new(kill_switch_initial)),
        config: Arc::new(RwLock::new(config)),
        ws_tx,
        opportunities: Arc::new(RwLock::new(Vec::new())),
        execution_history: Arc::new(RwLock::new(initial_history)),
        cached_metrics_json: Arc::new(RwLock::new("{}".to_string())),
        start_time: Instant::now(),
        executor: executor.clone(),
        price_store: price_store.clone(),
        prob_estimator: Arc::new(OnceLock::new()),
    };

    let app = Router::new()
        .route("/api/health", get(routes::health::health_check))
        .route("/api/status", get(routes::status::get_status))
        .route("/api/opportunities", get(routes::opportunities::list))
        .route("/api/positions", get(routes::positions::list))
        .route(
            "/api/positions/close-all",
            post(routes::positions::close_all),
        )
        .route(
            "/api/positions/{token_id}/close",
            post(routes::positions::close_position),
        )
        .route(
            "/api/positions/{token_id}/reduce",
            post(routes::positions::reduce_position),
        )
        .route("/api/metrics", get(routes::metrics::get_metrics))
        .route("/api/markets", get(routes::markets::list_markets))
        .route("/api/markets/{id}", get(routes::markets::get_market))
        .route("/api/history", get(routes::history::list))
        .route(
            "/api/config",
            get(routes::config::get_config).put(routes::config::update_config),
        )
        .route("/api/order", post(routes::order::place_order))
        .route(
            "/api/orders",
            get(routes::orders::list_orders).delete(routes::orders::cancel_all_orders),
        )
        .route("/api/orders/{id}", delete(routes::orders::cancel_order))
        .route("/api/kill", post(routes::control::kill))
        .route("/api/resume", post(routes::control::resume))
        .route(
            "/api/simulate/{condition_id}",
            post(routes::simulate::run_simulation),
        )
        .route("/api/sandbox/detect", post(routes::sandbox::detect))
        .route("/api/sandbox/backtest", post(routes::sandbox::backtest))
        .route(
            "/api/sandbox/backtest-historical",
            post(routes::sandbox::backtest_historical),
        )
        .route("/api/sandbox/impact", post(routes::optimize::impact))
        .route("/api/sandbox/optimize", post(routes::optimize::optimize))
        .route("/api/verify-live", post(routes::verify::verify_live))
        .route("/api/stress-test", post(routes::stress::run_stress_test))
        .route(
            "/api/simulation/status",
            get(routes::stress::simulation_status),
        )
        .route("/ws", get(ws::ws_handler))
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    // Spawn background engine that fetches live Polymarket data
    engine_task::spawn_engine(state.clone(), executor);

    // ── Periodic auto-save (every 60s) ─────────────────────────
    let autosave_state = state.clone();
    let autosave_pos = positions_path.clone();
    let autosave_hist = history_path.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        // The first tick completes immediately — skip it so we don't
        // save right at startup (we just loaded).
        interval.tick().await;
        loop {
            interval.tick().await;
            save_state(&autosave_state, &autosave_pos, &autosave_hist);
            tracing::debug!("Auto-save complete");
        }
    });

    // ── Start server with graceful shutdown ─────────────────────
    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await?;
    tracing::info!("arb-api server listening on http://0.0.0.0:8080");

    let shutdown_state = state.clone();
    let shutdown_positions = positions_path.clone();
    let shutdown_history = history_path.clone();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    // Server has stopped — save state before exit
    tracing::info!("Shutdown signal received, saving state...");
    save_state(&shutdown_state, &shutdown_positions, &shutdown_history);
    tracing::info!("State saved. Goodbye.");

    Ok(())
}

/// Wait for SIGINT (Ctrl+C) or SIGTERM.
async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        () = ctrl_c => {},
        () = terminate => {},
    }
}

/// Save positions and execution history to disk.
fn save_state(state: &AppState, positions_path: &Path, history_path: &Path) {
    save_positions(state, positions_path);
    save_history(state, history_path);
}

fn save_positions(state: &AppState, path: &Path) {
    let rl = match state.risk_limits.lock() {
        Ok(rl) => rl,
        Err(e) => {
            tracing::error!(error = %e, "Failed to lock risk_limits for position save");
            return;
        }
    };
    let positions_arc = rl.positions();
    let tracker = match positions_arc.lock() {
        Ok(t) => t,
        Err(e) => {
            tracing::error!(error = %e, "Failed to lock position tracker for save");
            return;
        }
    };
    match tracker.save(path) {
        Ok(()) => {
            tracing::info!(
                path = %path.display(),
                active = tracker.active_count(),
                "Positions saved"
            );
        }
        Err(e) => {
            tracing::error!(error = %e, path = %path.display(), "Failed to save positions");
        }
    }
}

fn save_history(state: &AppState, path: &Path) {
    let history = match state.execution_history.read() {
        Ok(h) => h,
        Err(e) => {
            tracing::error!(error = %e, "Failed to lock execution_history for save");
            return;
        }
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::error!(error = %e, "Failed to create history directory");
        return;
    }

    match serde_json::to_string_pretty(&*history) {
        Ok(json) => match std::fs::write(path, json) {
            Ok(()) => {
                tracing::info!(
                    path = %path.display(),
                    trade_count = history.len(),
                    "Execution history saved"
                );
            }
            Err(e) => {
                tracing::error!(error = %e, path = %path.display(), "Failed to write history file");
            }
        },
        Err(e) => {
            tracing::error!(error = %e, "Failed to serialize execution history");
        }
    }
}
