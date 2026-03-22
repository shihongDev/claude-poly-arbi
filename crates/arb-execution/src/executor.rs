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
use tracing::{debug, info, warn};

use crate::auth;

/// Maximum number of retry attempts for transient order placement failures.
const MAX_RETRIES: u32 = 1;
/// Delay between retry attempts.
const RETRY_DELAY: Duration = Duration::from_millis(500);

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
    /// Fee rate applied to each leg's notional (maker=0%, taker=2%).
    fee_rate: Decimal,
}

impl LiveTradeExecutor {
    /// Create from an already-authenticated client and its signer.
    pub fn new(
        clob_client: clob::Client<Authenticated<Normal>>,
        signer: PrivateKeySigner,
        prefer_post_only: bool,
        order_timeout_secs: u64,
        fee_rate: Decimal,
    ) -> Self {
        Self {
            clob_client,
            signer,
            prefer_post_only,
            order_timeout_secs,
            fee_rate,
        }
    }

    /// Create by reading the private key from a file and authenticating.
    pub async fn from_key_file(
        key_path: Option<&std::path::Path>,
        prefer_post_only: bool,
        order_timeout_secs: u64,
        fee_rate: Decimal,
    ) -> Result<Self> {
        let (client, signer) = auth::authenticate_from_key_file(key_path).await?;
        Ok(Self::new(client, signer, prefer_post_only, order_timeout_secs, fee_rate))
    }

    /// Returns a reference to the signer for order signing.
    pub fn signer(&self) -> &PrivateKeySigner {
        &self.signer
    }

    /// Build, sign, and post a single order. Returns the raw SDK response or an error.
    ///
    /// Extracted so the retry wrapper can call it without duplicating the
    /// build -> sign -> post pipeline.
    async fn post_single_order(
        &self,
        token_id: U256,
        sdk_side: polymarket_client_sdk::clob::types::Side,
        order_type: polymarket_client_sdk::clob::types::OrderType,
        price: Decimal,
        size: Decimal,
    ) -> std::result::Result<clob::types::response::PostOrderResponse, ArbError> {
        let order = self
            .clob_client
            .limit_order()
            .token_id(token_id)
            .side(sdk_side)
            .price(price)
            .size(size)
            .order_type(order_type)
            .build()
            .await
            .map_err(|e| ArbError::Execution(format!("Failed to build order: {e}")))?;

        let signed = self
            .clob_client
            .sign(&self.signer, order)
            .await
            .map_err(|e| ArbError::Execution(format!("Failed to sign order: {e}")))?;

        self.clob_client
            .post_order(signed)
            .await
            .map_err(|e| ArbError::Execution(format!("Failed to post order: {e}")))
    }

    /// Place a limit order for a single leg via the Polymarket CLOB SDK.
    ///
    /// Flow: parse token_id -> build limit order -> sign -> post -> map response.
    /// The entire build+sign+post sequence is wrapped in a `tokio::time::timeout`
    /// to prevent orders from hanging indefinitely. Transient failures are
    /// retried up to `MAX_RETRIES` times with a short delay between attempts.
    async fn execute_leg(
        &self,
        leg: &arb_core::TradeLeg,
        condition_id: &str,
    ) -> Result<LegReport> {
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

        // Wrap the entire build -> sign -> post sequence in a timeout,
        // with retry logic for transient failures.
        let timeout_dur = Duration::from_secs(self.order_timeout_secs);
        let order_result = tokio::time::timeout(timeout_dur, async {
            let mut last_err = None;
            for attempt in 0..=MAX_RETRIES {
                if attempt > 0 {
                    debug!(
                        token_id = %leg.token_id,
                        attempt = attempt + 1,
                        "Retrying order placement after transient failure"
                    );
                    tokio::time::sleep(RETRY_DELAY).await;
                }

                match self
                    .post_single_order(
                        token_id,
                        sdk_side,
                        order_type.clone(),
                        leg.vwap_estimate,
                        leg.target_size,
                    )
                    .await
                {
                    Ok(response) => return Ok(response),
                    Err(e) => {
                        let is_transient = is_transient_error(&e);
                        warn!(
                            token_id = %leg.token_id,
                            attempt = attempt + 1,
                            transient = is_transient,
                            error = %e,
                            "Order placement attempt failed"
                        );
                        if !is_transient {
                            return Err(e);
                        }
                        last_err = Some(e);
                    }
                }
            }
            // All retries exhausted
            Err(last_err.unwrap_or_else(|| ArbError::Execution("All retries exhausted".into())))
        })
        .await;

        match order_result {
            Ok(Ok(response)) => {
                info!(
                    order_id = %response.order_id,
                    success = response.success,
                    status = %response.status,
                    making_amount = %response.making_amount,
                    taking_amount = %response.taking_amount,
                    "Order posted"
                );

                // Determine fill status: the SDK response does not have a
                // dedicated "partial" flag, so we infer it from amounts.
                // - success=true + making_amount >= target -> FullyFilled
                // - success=true + 0 < making_amount < target -> PartiallyFilled
                // - success=false -> Rejected
                let (status, filled_size) = if response.success {
                    if response.making_amount >= leg.target_size {
                        (FillStatus::FullyFilled, leg.target_size)
                    } else if response.making_amount > Decimal::ZERO {
                        let partial_size = response.making_amount;
                        let remaining = leg.target_size - partial_size;
                        warn!(
                            order_id = %response.order_id,
                            filled = %partial_size,
                            remaining = %remaining,
                            target = %leg.target_size,
                            "Partial fill detected"
                        );
                        (FillStatus::PartiallyFilled, partial_size)
                    } else {
                        // success=true but making_amount is zero; treat as full fill
                        // (some order types report success without amounts)
                        (FillStatus::FullyFilled, leg.target_size)
                    }
                } else {
                    warn!(
                        order_id = %response.order_id,
                        error = ?response.error_msg,
                        "Order was not successful"
                    );
                    (FillStatus::Rejected, Decimal::ZERO)
                };

                // Compute the effective fill price from taking/making amounts.
                // Guard against division by zero: if making_amount is zero,
                // fall back to our VWAP estimate.
                let actual_fill_price =
                    if response.success
                        && response.taking_amount > Decimal::ZERO
                        && response.making_amount > Decimal::ZERO
                    {
                        match leg.side {
                            Side::Buy => response.taking_amount / response.making_amount,
                            Side::Sell => response.making_amount / response.taking_amount,
                        }
                    } else {
                        leg.vwap_estimate
                    };

                Ok(LegReport {
                    order_id: response.order_id,
                    token_id: leg.token_id.clone(),
                    condition_id: condition_id.to_string(),
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
                    condition_id: condition_id.to_string(),
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
                    condition_id: condition_id.to_string(),
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

/// Check whether an execution error is likely transient (network issue, 5xx, timeout)
/// and thus worth retrying.
fn is_transient_error(err: &ArbError) -> bool {
    match err {
        ArbError::Execution(msg) => {
            let lower = msg.to_lowercase();
            lower.contains("timeout")
                || lower.contains("timed out")
                || lower.contains("connection")
                || lower.contains("500")
                || lower.contains("502")
                || lower.contains("503")
                || lower.contains("504")
                || lower.contains("internal server error")
                || lower.contains("bad gateway")
                || lower.contains("service unavailable")
                || lower.contains("gateway timeout")
        }
        _ => false,
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

        // Derive condition_id from the opportunity's markets list.
        // Each leg corresponds to a market by index; fall back to the first
        // market if the index is out of bounds.
        let default_condition_id = opp.markets.first().cloned().unwrap_or_default();

        for (i, leg) in opp.legs.iter().enumerate() {
            let condition_id = opp.markets.get(i).unwrap_or(&default_condition_id);

            let report = self.execute_leg(leg, condition_id).await?;

            let slippage = (report.actual_fill_price - report.expected_vwap).abs()
                * report.filled_size;
            let fee = report.filled_size * report.actual_fill_price * self.fee_rate;

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
