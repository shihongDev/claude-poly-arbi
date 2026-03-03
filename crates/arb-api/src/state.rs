use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};
use std::time::Instant;

use arb_core::config::ArbConfig;
use arb_core::{ExecutionReport, Opportunity};
use arb_data::market_cache::MarketCache;
use arb_risk::limits::RiskLimits;
use tokio::sync::broadcast;

#[derive(Clone)]
pub struct AppState {
    pub market_cache: Arc<MarketCache>,
    pub risk_limits: Arc<Mutex<RiskLimits>>,
    /// Lock-free kill switch mirror — checked on the engine hot path
    /// every cycle without acquiring the `risk_limits` mutex.
    pub kill_switch_active: Arc<AtomicBool>,
    pub config: Arc<RwLock<ArbConfig>>,
    pub ws_tx: broadcast::Sender<String>,
    pub opportunities: Arc<RwLock<Vec<Opportunity>>>,
    pub execution_history: Arc<RwLock<Vec<ExecutionReport>>>,
    /// Pre-serialized metrics JSON — updated each engine cycle to avoid
    /// mutex lock + serialization on every `/api/metrics` request.
    pub cached_metrics_json: Arc<RwLock<String>>,
    pub start_time: Instant,
}
