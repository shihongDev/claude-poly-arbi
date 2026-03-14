use std::time::Instant;

use arb_execution::auth;
use axum::Json;
use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;

use crate::state::AppState;

/// Live verification preflight endpoint.
///
/// Tests the full live trading pipeline without executing real trades:
/// 1. Checks if the system is in live mode
/// 2. Authenticates with the SDK (times it)
/// 3. Optionally places a tiny order guaranteed not to fill, then cancels it
///
/// If not in live mode, returns early with a SKIP result.
pub async fn verify_live(State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.read().unwrap().clone();

    // If not in live mode, skip
    if !config.is_live() {
        return (
            StatusCode::OK,
            Json(serde_json::json!({
                "mode": "paper",
                "auth": null,
                "order_test": null,
                "cancel_test": null,
                "overall": "SKIP",
                "reason": "Not in live mode"
            })),
        )
            .into_response();
    }

    // Resolve key path
    let key_path = config.general.key_file.as_ref().map(std::path::Path::new);

    // Step 1: Authenticate
    let auth_start = Instant::now();
    let auth_result = auth::authenticate_from_key_file(key_path).await;
    let auth_latency = auth_start.elapsed().as_millis();

    let (client, signer) = match auth_result {
        Ok((c, s)) => {
            // Auth succeeded, continue to order test
            (c, s)
        }
        Err(e) => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "mode": "live",
                    "auth": {
                        "success": false,
                        "latency_ms": auth_latency,
                        "error": format!("{e}"),
                    },
                    "order_test": null,
                    "cancel_test": null,
                    "overall": "FAIL",
                })),
            )
                .into_response();
        }
    };

    // Step 2: Find the most liquid market
    let markets = state.market_cache.active_markets();
    let best_market = markets
        .iter()
        .max_by_key(|m| m.volume_24hr.unwrap_or(rust_decimal::Decimal::ZERO));

    let best_market = match best_market {
        Some(m) => m,
        None => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "mode": "live",
                    "auth": {
                        "success": true,
                        "latency_ms": auth_latency,
                    },
                    "order_test": {
                        "success": false,
                        "error": "No active markets in cache for order test",
                    },
                    "cancel_test": null,
                    "overall": "PARTIAL",
                })),
            )
                .into_response();
        }
    };

    // Pick the first token and find the best bid to price 5 ticks below
    let token_id_str = match best_market.token_ids.first() {
        Some(t) => t.clone(),
        None => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "mode": "live",
                    "auth": { "success": true, "latency_ms": auth_latency },
                    "order_test": { "success": false, "error": "No token IDs on market" },
                    "cancel_test": null,
                    "overall": "PARTIAL",
                })),
            )
                .into_response();
        }
    };

    // Find best bid from orderbook, or use a very low price
    let best_bid = best_market
        .orderbooks
        .iter()
        .find(|ob| ob.token_id == token_id_str)
        .and_then(|ob| ob.bids.first())
        .map(|l| l.price)
        .unwrap_or(rust_decimal::Decimal::new(5, 2)); // 0.05 fallback

    // Price 5 ticks below best bid (each tick = 0.01)
    let order_price =
        (best_bid - rust_decimal::Decimal::new(5, 2)).max(rust_decimal::Decimal::new(1, 2)); // min 0.01
    let order_size = rust_decimal::Decimal::new(1, 2); // 0.01 USDC

    // Step 3: Place a tiny limit order
    let order_start = Instant::now();
    let token_id = match token_id_str.parse::<alloy::primitives::U256>() {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "mode": "live",
                    "auth": { "success": true, "latency_ms": auth_latency },
                    "order_test": { "success": false, "error": format!("Invalid token ID: {e}") },
                    "cancel_test": null,
                    "overall": "FAIL",
                })),
            )
                .into_response();
        }
    };

    let order_result = async {
        let order = client
            .limit_order()
            .token_id(token_id)
            .side(polymarket_client_sdk::clob::types::Side::Buy)
            .price(order_price)
            .size(order_size)
            .order_type(polymarket_client_sdk::clob::types::OrderType::GTC)
            .build()
            .await
            .map_err(|e| format!("Build failed: {e}"))?;

        let signed = client
            .sign(&signer, order)
            .await
            .map_err(|e| format!("Sign failed: {e}"))?;

        client
            .post_order(signed)
            .await
            .map_err(|e| format!("Post failed: {e}"))
    }
    .await;
    let order_latency = order_start.elapsed().as_millis();

    let order_id = match order_result {
        Ok(response) => {
            if response.success {
                response.order_id
            } else {
                let err_msg = response
                    .error_msg
                    .unwrap_or_else(|| "Unknown error".to_string());
                return (
                    StatusCode::OK,
                    Json(serde_json::json!({
                        "mode": "live",
                        "auth": { "success": true, "latency_ms": auth_latency },
                        "order_test": {
                            "success": false,
                            "latency_ms": order_latency,
                            "error": err_msg,
                        },
                        "cancel_test": null,
                        "overall": "FAIL",
                    })),
                )
                    .into_response();
            }
        }
        Err(e) => {
            return (
                StatusCode::OK,
                Json(serde_json::json!({
                    "mode": "live",
                    "auth": { "success": true, "latency_ms": auth_latency },
                    "order_test": {
                        "success": false,
                        "latency_ms": order_latency,
                        "error": e,
                    },
                    "cancel_test": null,
                    "overall": "FAIL",
                })),
            )
                .into_response();
        }
    };

    // Step 4: Cancel the order
    let cancel_start = Instant::now();
    let cancel_result = client.cancel_order(&order_id).await;
    let cancel_latency = cancel_start.elapsed().as_millis();

    let cancel_success = cancel_result.is_ok();
    let cancel_error = cancel_result.err().map(|e| format!("{e}"));

    let overall = if cancel_success { "PASS" } else { "FAIL" };

    let mut cancel_json = serde_json::json!({
        "success": cancel_success,
        "latency_ms": cancel_latency,
    });
    if let Some(err) = cancel_error {
        cancel_json["error"] = serde_json::json!(err);
    }

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "mode": "live",
            "auth": { "success": true, "latency_ms": auth_latency },
            "order_test": {
                "success": true,
                "order_id": order_id,
                "latency_ms": order_latency,
            },
            "cancel_test": cancel_json,
            "overall": overall,
        })),
    )
        .into_response()
}
