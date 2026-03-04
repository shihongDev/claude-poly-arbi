use std::sync::atomic::Ordering;

use axum::{Json, extract::State};
use serde::Serialize;

use crate::state::AppState;

#[derive(Serialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub uptime_secs: u64,
    pub markets_loaded: usize,
    pub kill_switch_active: bool,
    pub warnings: Vec<String>,
}

pub async fn health_check(State(state): State<AppState>) -> Json<HealthStatus> {
    let kill_switch_active = state.kill_switch_active.load(Ordering::Relaxed);
    let markets_loaded = state.market_cache.len();
    let uptime_secs = state.start_time.elapsed().as_secs();

    let mut warnings = Vec::new();
    if kill_switch_active {
        warnings.push("Kill switch is active".to_string());
    }
    if markets_loaded == 0 {
        warnings.push("No markets loaded".to_string());
    }

    let healthy = !kill_switch_active && markets_loaded > 0;

    Json(HealthStatus {
        healthy,
        uptime_secs,
        markets_loaded,
        kill_switch_active,
        warnings,
    })
}
