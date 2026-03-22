use arb_core::types::ExecutionReport;

use crate::state::AppState;

pub fn append_history(state: &AppState, report: &ExecutionReport) {
    if let Ok(mut history) = state.execution_history.write() {
        history.push_front(report.clone());
        history.truncate(500);
    }
}

pub fn broadcast_event<T: serde::Serialize>(
    state: &AppState,
    event_type: &str,
    data: &T,
) -> bool {
    let event = serde_json::json!({
        "type": event_type,
        "data": data
    });
    match serde_json::to_string(&event) {
        Ok(json) => state.ws_tx.send(json).is_ok(),
        Err(_) => false,
    }
}

pub fn broadcast_positions(state: &AppState) {
    let rl = state.risk_limits.lock().unwrap();
    if let Ok(tracker) = rl.positions().lock() {
        let positions: Vec<_> = tracker.all_positions().into_iter().cloned().collect();
        let _ = broadcast_event(state, "position_update", &positions);
    }
}
