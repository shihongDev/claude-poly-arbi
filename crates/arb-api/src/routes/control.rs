use axum::{Json, extract::State};
use arb_core::traits::RiskManager;
use arb_risk::kill_switch::KillSwitch;
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

    let event = serde_json::json!({
        "event": "kill_switch_activated",
        "reason": body.reason,
    });
    let _ = state.ws_tx.send(event.to_string());

    Json(serde_json::json!({"status": "kill switch activated", "reason": body.reason}))
}

pub async fn resume(State(state): State<AppState>) -> Json<serde_json::Value> {
    // KillSwitch is file-based; create a new instance to deactivate the file
    let mut ks = KillSwitch::new();
    ks.deactivate();

    // The file-based kill switch will be re-read by RiskLimits on next tick

    let event = serde_json::json!({
        "event": "kill_switch_deactivated",
    });
    let _ = state.ws_tx.send(event.to_string());

    Json(serde_json::json!({"status": "kill switch deactivated"}))
}
