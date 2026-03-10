use std::sync::Arc;

use async_trait::async_trait;
use rust_decimal::Decimal;
use tokio::sync::{mpsc, watch};

use crate::error::{ArbError, Result};
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
    async fn scan(&self, markets: &[Arc<MarketState>]) -> Result<Vec<Opportunity>>;
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

    /// Cancel a single resting order by ID.
    async fn cancel_order(&self, _order_id: &str) -> Result<()> {
        Err(ArbError::Execution("cancel_order not supported".into()))
    }

    /// Cancel multiple resting orders by ID.
    async fn cancel_orders(&self, _order_ids: &[String]) -> Result<()> {
        Err(ArbError::Execution("cancel_orders not supported".into()))
    }

    /// Execute a batch of opportunities, returning a report for each.
    /// Default implementation calls `execute_opportunity` sequentially.
    async fn execute_batch(&self, opps: &[Opportunity]) -> Result<Vec<ExecutionReport>> {
        let mut reports = Vec::with_capacity(opps.len());
        for opp in opps {
            reports.push(self.execute_opportunity(opp).await?);
        }
        Ok(reports)
    }

    /// List all open (resting) orders.
    async fn open_orders(&self) -> Result<Vec<OpenOrder>> {
        Ok(Vec::new())
    }
}

pub trait RiskManager: Send + Sync {
    fn check_opportunity(&self, opp: &Opportunity) -> Result<RiskDecision>;
    fn record_execution(&mut self, report: &ExecutionReport, arb_type: ArbType);
    fn is_kill_switch_active(&self) -> bool;
    fn activate_kill_switch(&mut self, reason: &str);
    fn daily_pnl(&self) -> Decimal;
    fn current_exposure(&self) -> Decimal;
}

pub trait ProbabilityEstimator: Send + Sync {
    fn estimate(&self, market: &MarketState) -> Result<ProbEstimate>;
    fn update(&mut self, market: &MarketState, new_data: &MarketState);
}

/// Event-driven strategy that runs its own async loop.
///
/// Unlike `ArbDetector` (which is polled each tick), `LiveStrategy` owns
/// its own event loop — subscribing to WS updates, maintaining state, and
/// pushing actions through `action_tx`. The engine spawns each strategy
/// as a separate tokio task and drains the action channel each tick.
#[async_trait]
pub trait LiveStrategy: Send + Sync {
    fn strategy_type(&self) -> StrategyType;
    async fn run(
        &mut self,
        action_tx: mpsc::Sender<StrategyAction>,
        shutdown: watch::Receiver<bool>,
    ) -> Result<()>;
}
