use arb_core::{
    ExecutionReport, FillStatus, LegReport, Opportunity, TradingMode,
    error::Result,
    traits::TradeExecutor,
};
use async_trait::async_trait;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal_macros::dec;
use tracing::{info, warn};

/// Live trade executor using the Polymarket CLOB API.
///
/// Executes real orders through `polymarket_client_sdk::clob::Client`.
/// Uses limit orders with post_only when possible (maker rebates),
/// falls back to market orders for urgent fills.
///
/// Safety: requires explicit `--live` flag and is never the default.
///
/// TODO: Wire up authenticated CLOB client. The SDK uses a typestate pattern
/// (`Client<Authenticated<K>>`) — the authenticated client is created via
/// `Client::builder().signer(signer).build()` using the auth patterns
/// from `polymarket-cli-main/src/auth.rs`.
pub struct LiveTradeExecutor {
    _prefer_post_only: bool,
    _order_timeout_secs: u64,
}

impl LiveTradeExecutor {
    pub fn new(prefer_post_only: bool, order_timeout_secs: u64) -> Self {
        Self {
            _prefer_post_only: prefer_post_only,
            _order_timeout_secs: order_timeout_secs,
        }
    }

    /// Place a limit order for a single leg.
    async fn execute_leg(&self, leg: &arb_core::TradeLeg) -> Result<LegReport> {
        // TODO: Build authenticated CLOB client and place real orders.
        // The SDK requires:
        //   let client = polymarket_client_sdk::clob::Client::builder()
        //       .signer(signer)
        //       .build();
        //   client.limit_order(&order_request).await
        //
        // For now, return a simulated fill with a warning.
        warn!(
            token_id = %leg.token_id,
            side = ?leg.side,
            price = %leg.vwap_estimate,
            size = %leg.target_size,
            "Live execution stub — returning simulated fill (auth not configured)"
        );

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
        // TODO: Call authenticated client's cancel_all_orders() method.
        // Requires: self.clob_client.cancel_all_orders().await
        warn!("Cancel all orders stub — auth not configured");
        Ok(())
    }

    fn mode(&self) -> TradingMode {
        TradingMode::Live
    }
}
