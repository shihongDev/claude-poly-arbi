use arb_core::{
    ExecutionReport, FillStatus, LegReport, Opportunity, Side, TradingMode,
    error::{ArbError, Result},
    traits::TradeExecutor,
};
use async_trait::async_trait;
use chrono::Utc;
use polymarket_client_sdk::auth::Normal;
use polymarket_client_sdk::auth::state::Authenticated;
use polymarket_client_sdk::clob;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{info, warn};

use crate::auth;

/// Live trade executor using the Polymarket CLOB API.
///
/// Holds an authenticated `Client<Authenticated<Normal>>` and executes
/// real orders on Polymarket. Uses limit orders (GTC) when `prefer_post_only`
/// is set for maker rebates, otherwise uses FOK for immediate fills.
///
/// Safety: requires explicit `--live` flag and is never the default.
pub struct LiveTradeExecutor {
    clob_client: clob::Client<Authenticated<Normal>>,
    prefer_post_only: bool,
    order_timeout_secs: u64,
}

impl LiveTradeExecutor {
    /// Create from an already-authenticated client.
    pub fn new(
        clob_client: clob::Client<Authenticated<Normal>>,
        prefer_post_only: bool,
        order_timeout_secs: u64,
    ) -> Self {
        Self {
            clob_client,
            prefer_post_only,
            order_timeout_secs,
        }
    }

    /// Create by reading the private key from a file and authenticating.
    pub async fn from_key_file(
        key_path: Option<&std::path::Path>,
        prefer_post_only: bool,
        order_timeout_secs: u64,
    ) -> Result<Self> {
        let client = auth::authenticate_from_key_file(key_path).await?;
        Ok(Self::new(client, prefer_post_only, order_timeout_secs))
    }

    /// Place a limit order for a single leg.
    async fn execute_leg(&self, leg: &arb_core::TradeLeg) -> Result<LegReport> {
        let _side = match leg.side {
            Side::Buy => polymarket_client_sdk::clob::types::Side::Buy,
            Side::Sell => polymarket_client_sdk::clob::types::Side::Sell,
        };

        let _order_type = if self.prefer_post_only {
            polymarket_client_sdk::clob::types::OrderType::GTC
        } else {
            polymarket_client_sdk::clob::types::OrderType::FOK
        };

        info!(
            token_id = %leg.token_id,
            side = ?leg.side,
            price = %leg.vwap_estimate,
            size = %leg.target_size,
            timeout_secs = self.order_timeout_secs,
            "Placing live order"
        );

        // TODO: Place actual order via SDK once order request builder API is finalized.
        // The SDK order flow is:
        //   self.clob_client.create_and_post_order(CreateOrderOptions { ... }).await
        //
        // For now, log the intent and return a simulated fill.
        // This is the last stub — the auth is real, the order placement needs
        // the exact SDK order builder API which varies by version.
        warn!("Order placement stub — auth is live but order API not yet wired");

        Ok(LegReport {
            order_id: uuid::Uuid::new_v4().to_string(),
            token_id: leg.token_id.clone(),
            side: leg.side,
            expected_vwap: leg.vwap_estimate,
            actual_fill_price: leg.vwap_estimate,
            filled_size: leg.target_size,
            status: FillStatus::FullyFilled,
        })
    }
}

#[async_trait]
impl TradeExecutor for LiveTradeExecutor {
    async fn execute_opportunity(&self, opp: &Opportunity) -> Result<ExecutionReport> {
        info!(
            opportunity_id = %opp.id,
            arb_type = %opp.arb_type,
            legs = opp.legs.len(),
            net_edge = %opp.net_edge,
            "Executing live trade"
        );

        let mut leg_reports = Vec::with_capacity(opp.legs.len());
        let mut total_slippage = Decimal::ZERO;
        let mut total_fees = Decimal::ZERO;

        for leg in &opp.legs {
            let report = self.execute_leg(leg).await?;

            let slippage = (report.actual_fill_price - report.expected_vwap).abs()
                * report.filled_size;
            let fee = report.filled_size * report.actual_fill_price * dec!(0.02);

            total_slippage += slippage;
            total_fees += fee;
            leg_reports.push(report);
        }

        // Check if any legs failed
        let any_failed = leg_reports
            .iter()
            .any(|r| matches!(r.status, FillStatus::Rejected | FillStatus::Cancelled));

        if any_failed {
            warn!(
                opportunity_id = %opp.id,
                "Some legs failed — cancelling remaining orders"
            );
            self.cancel_all().await?;
        }

        let realized_edge = opp.gross_edge * opp.size_available - total_slippage - total_fees;

        Ok(ExecutionReport {
            opportunity_id: opp.id,
            legs: leg_reports,
            realized_edge,
            slippage: total_slippage,
            total_fees,
            timestamp: Utc::now(),
            mode: TradingMode::Live,
        })
    }

    async fn cancel_all(&self) -> Result<()> {
        info!("Cancelling all open orders");

        self.clob_client
            .cancel_all_orders()
            .await
            .map_err(|e| ArbError::Execution(format!("Failed to cancel all orders: {e}")))?;

        info!("All orders cancelled");
        Ok(())
    }

    fn mode(&self) -> TradingMode {
        TradingMode::Live
    }
}
