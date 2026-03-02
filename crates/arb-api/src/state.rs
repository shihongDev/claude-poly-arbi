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
    pub config: Arc<RwLock<ArbConfig>>,
    pub ws_tx: broadcast::Sender<String>,
    pub opportunities: Arc<RwLock<Vec<Opportunity>>>,
    pub execution_history: Arc<RwLock<Vec<ExecutionReport>>>,
    pub start_time: Instant,
}
