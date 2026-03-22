use std::sync::atomic::Ordering;

use arb_core::traits::RiskManager;
use axum::{Json, extract::State};
use serde::Deserialize;

use crate::state::AppState;

#[derive(Deserialize)]
pub struct KillRequest {
    pub reason: String,
}

pub async fn kill(
    State(state): State<AppState>,
    Json(body): Json<KillRequest>,
) -> Json<serde_json::Value> {
    {
        let mut risk = state.risk_limits.lock().unwrap();
        risk.activate_kill_switch(&body.reason);
    }

    // Mirror to lock-free AtomicBool so engine loop doesn't need the mutex
    state.kill_switch_active.store(true, Ordering::Relaxed);

    // Broadcast kill switch change via WS using the standard {type, data} format
    let event = serde_json::json!({
        "type": "kill_switch_change",
        "data": {
            "active": true,
            "reason": body.reason,
        }
    });
    let _ = state.ws_tx.send(event.to_string());

    Json(serde_json::json!({"status": "kill switch activated", "reason": body.reason}))
}

pub async fn resume(State(state): State<AppState>) -> Json<serde_json::Value> {
    // Deactivate kill switch in the shared RiskLimits (authoritative state)
    {
        let mut risk = state.risk_limits.lock().unwrap();
        risk.deactivate_kill_switch();
    }

    // Mirror to lock-free AtomicBool so engine loop sees the change immediately
    state.kill_switch_active.store(false, Ordering::Relaxed);

    let event = serde_json::json!({
        "type": "kill_switch_change",
        "data": {
            "active": false,
        }
    });
    let _ = state.ws_tx.send(event.to_string());

    Json(serde_json::json!({"status": "kill switch deactivated"}))
}
