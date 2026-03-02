use async_trait::async_trait;
use rust_decimal::Decimal;

use crate::error::Result;
use crate::types::*;

#[async_trait]
pub trait MarketDataSource: Send + Sync {
    async fn fetch_markets(&self) -> Result<Vec<MarketState>>;
    async fn fetch_orderbook(&self, token_id: &str) -> Result<OrderbookSnapshot>;
    async fn fetch_orderbooks(&self, token_ids: &[String]) -> Result<Vec<OrderbookSnapshot>>;
}

#[async_trait]
pub trait ArbDetector: Send + Sync {
    fn arb_type(&self) -> ArbType;
    async fn scan(&self, markets: &[MarketState]) -> Result<Vec<Opportunity>>;
}

pub trait SlippageEstimator: Send + Sync {
    fn estimate_vwap(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        size: Decimal,
    ) -> Result<VwapEstimate>;

    fn split_order(
        &self,
        book: &OrderbookSnapshot,
        side: Side,
        total_size: Decimal,
        max_slippage_bps: Decimal,
    ) -> Result<Vec<OrderChunk>>;
}

#[async_trait]
pub trait TradeExecutor: Send + Sync {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport>;
    async fn cancel_all(&self) -> Result<()>;
    fn mode(&self) -> TradingMode;
}

pub trait RiskManager: Send + Sync {
    fn check_opportunity(&self, opp: &Opportunity) -> Result<RiskDecision>;
    fn record_execution(&mut self, report: &ExecutionReport);
    fn is_kill_switch_active(&self) -> bool;
    fn activate_kill_switch(&mut self, reason: &str);
    fn daily_pnl(&self) -> Decimal;
    fn current_exposure(&self) -> Decimal;
}

pub trait ProbabilityEstimator: Send + Sync {
    fn estimate(&self, market: &MarketState) -> Result<ProbEstimate>;
    fn update(&mut self, market: &MarketState, new_data: &MarketState);
}
