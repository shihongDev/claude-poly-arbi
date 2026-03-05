use std::str::FromStr;
use std::time::Duration;

use arb_core::{
    ExecutionReport, FillStatus, LegReport, Opportunity, Side, TradingMode,
    error::{ArbError, Result},
    traits::TradeExecutor,
};
use alloy::primitives::U256;
use alloy::signers::local::PrivateKeySigner;
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
/// Holds an authenticated `Client<Authenticated<Normal>>` and the `LocalSigner`
/// used during authentication. Both are needed for live order placement:
/// the client provides the API, and the signer is required for
/// `client.sign(&signer, order)` before posting.
///
/// Uses limit orders (GTC) when `prefer_post_only` is set for maker rebates,
/// otherwise uses FOK for immediate fills.
///
/// Safety: requires explicit `--live` flag and is never the default.
pub struct LiveTradeExecutor {
    clob_client: clob::Client<Authenticated<Normal>>,
    signer: PrivateKeySigner,
    prefer_post_only: bool,
    order_timeout_secs: u64,
}

impl LiveTradeExecutor {
    /// Create from an already-authenticated client and its signer.
    pub fn new(
        clob_client: clob::Client<Authenticated<Normal>>,
        signer: PrivateKeySigner,
        prefer_post_only: bool,
        order_timeout_secs: u64,
    ) -> Self {
        Self {
            clob_client,
            signer,
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
        let (client, signer) = auth::authenticate_from_key_file(key_path).await?;
        Ok(Self::new(client, signer, prefer_post_only, order_timeout_secs))
    }

    /// Returns a reference to the signer for order signing.
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Place a limit order for a single leg via the Polymarket CLOB SDK.
    ///
    /// Flow: parse token_id -> build limit order -> sign -> post -> map response.
    /// The entire build+sign+post sequence is wrapped in a `tokio::time::timeout`
    /// to prevent orders from hanging indefinitely.
    async fn execute_leg(&self, leg: &arb_core::TradeLeg) -> Result<LegReport> {
        let sdk_side = match leg.side {
            Side::Buy => polymarket_client_sdk::clob::types::Side::Buy,
            Side::Sell => polymarket_client_sdk::clob::types::Side::Sell,
        };

        let order_type = if self.prefer_post_only {
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

        // Parse the string token_id into the SDK's U256 type
        let token_id = U256::from_str(&leg.token_id)
            .map_err(|e| ArbError::Execution(format!("Invalid token ID '{}': {e}", leg.token_id)))?;

        // Wrap the entire build → sign → post sequence in a timeout
        let timeout_dur = Duration::from_secs(self.order_timeout_secs);
        let order_result = tokio::time::timeout(timeout_dur, async {
            // Build the limit order
            let order = self
                .clob_client
                .limit_order()
                .token_id(token_id)
                .side(sdk_side)
                .price(leg.vwap_estimate)
                .size(leg.target_size)
                .order_type(order_type)
                .build()
                .await
                .map_err(|e| ArbError::Execution(format!("Failed to build order: {e}")))?;

            // Sign the order with our private key
            let signed = self
                .clob_client
                .sign(&self.signer, order)
                .await
                .map_err(|e| ArbError::Execution(format!("Failed to sign order: {e}")))?;

            // Post the signed order to the CLOB API
            self.clob_client
                .post_order(signed)
                .await
                .map_err(|e| ArbError::Execution(format!("Failed to post order: {e}")))
        })
        .await;

        match order_result {
            Ok(Ok(response)) => {
                info!(
                    order_id = %response.order_id,
                    success = response.success,
                    status = %response.status,
                    "Order posted"
                );

                // Map SDK response to our LegReport
                let status = if response.success {
                    FillStatus::FullyFilled
                } else {
                    warn!(
                        order_id = %response.order_id,
                        error = ?response.error_msg,
                        "Order was not successful"
                    );
                    FillStatus::Rejected
                };

                // The taking_amount represents what we received (the fill price * size).
                // For a successful fill, approximate fill price from taking/making amounts.
                let actual_fill_price =
                    if response.success && response.taking_amount > Decimal::ZERO {
                        // taking_amount / making_amount gives effective price for buys;
                        // for sells it's making_amount / taking_amount.
                        // Fall back to our VWAP estimate if amounts are zero.
                        match leg.side {
                            Side::Buy => response.taking_amount / response.making_amount,
                            Side::Sell => response.making_amount / response.taking_amount,
                        }
                    } else {
                        leg.vwap_estimate
                    };

                let filled_size = if response.success {
                    leg.target_size
                } else {
                    Decimal::ZERO
                };

                Ok(LegReport {
                    order_id: response.order_id,
                    token_id: leg.token_id.clone(),
                    condition_id: String::new(),
                    side: leg.side,
                    expected_vwap: leg.vwap_estimate,
                    actual_fill_price,
                    filled_size,
                    status,
                })
            }
            Ok(Err(e)) => {
                warn!(token_id = %leg.token_id, error = %e, "Order placement failed");
                Ok(LegReport {
                    order_id: String::new(),
                    token_id: leg.token_id.clone(),
                    condition_id: String::new(),
                    side: leg.side,
                    expected_vwap: leg.vwap_estimate,
                    actual_fill_price: Decimal::ZERO,
                    filled_size: Decimal::ZERO,
                    status: FillStatus::Rejected,
                })
            }
            Err(_elapsed) => {
                warn!(
                    token_id = %leg.token_id,
                    timeout_secs = self.order_timeout_secs,
                    "Order timed out"
                );
                Ok(LegReport {
                    order_id: String::new(),
                    token_id: leg.token_id.clone(),
                    condition_id: String::new(),
                    side: leg.side,
                    expected_vwap: leg.vwap_estimate,
                    actual_fill_price: Decimal::ZERO,
                    filled_size: Decimal::ZERO,
                    status: FillStatus::Cancelled,
                })
            }
        }
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
