use axum::{Json, extract::State};
use arb_core::traits::RiskManager;

use crate::state::AppState;

pub async fn get_metrics(State(state): State<AppState>) -> Json<serde_json::Value> {
    let risk = state.risk_limits.lock().unwrap();
    let metrics = risk.metrics();

    let brier_score = metrics.brier_score();
    let drawdown_pct = metrics.drawdown_pct();
    let execution_quality = metrics.execution_quality();
    let total_pnl = metrics.total_pnl();
    let trade_count = metrics.trade_count();

    let daily_pnl = risk.daily_pnl();
    let current_exposure = risk.current_exposure();

    let pnl_intra = metrics.pnl_for_type(arb_core::ArbType::IntraMarket);
    let pnl_cross = metrics.pnl_for_type(arb_core::ArbType::CrossMarket);
    let pnl_multi = metrics.pnl_for_type(arb_core::ArbType::MultiOutcome);

    Json(serde_json::json!({
        "brier_score": brier_score,
        "drawdown_pct": drawdown_pct,
        "execution_quality": execution_quality,
        "total_pnl": total_pnl,
        "daily_pnl": daily_pnl,
        "trade_count": trade_count,
        "pnl_by_type": {
            "intra_market": pnl_intra,
            "cross_market": pnl_cross,
            "multi_outcome": pnl_multi,
        },
        "current_exposure": current_exposure,
    }))
}
