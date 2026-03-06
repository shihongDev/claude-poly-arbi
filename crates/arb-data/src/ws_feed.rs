use crate::local_book::OrderBookStore;
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, info, warn};

const WS_URL: &str = "wss://ws-subscriptions-clob.polymarket.com/ws/market";
const RECONNECT_BASE_DELAY: Duration = Duration::from_secs(1);
const RECONNECT_MAX_DELAY: Duration = Duration::from_secs(30);

/// Wire format for subscribing to token feeds.
#[derive(Debug, Clone, Serialize)]
struct SubscribeMessage {
    #[serde(rename = "type")]
    msg_type: String,
    assets_ids: Vec<String>,
}

/// Wire format for incoming WebSocket messages (book snapshots and price changes).
#[derive(Debug, Deserialize)]
struct WsMessage {
    #[serde(rename = "type")]
    msg_type: Option<String>,
    market: Option<String>,
    bids: Option<Vec<WsLevel>>,
    asks: Option<Vec<WsLevel>>,
    price: Option<String>,
    size: Option<String>,
    side: Option<String>,
}

/// A single price level in a book snapshot.
#[derive(Debug, Deserialize)]
struct WsLevel {
    price: String,
    size: String,
}

/// Notification emitted when a local order book has been updated.
#[derive(Debug, Clone)]
pub struct BookUpdate {
    pub token_id: String,
}

/// WebSocket client that connects to Polymarket's CLOB feed and maintains
/// local order books via [`OrderBookStore`].
///
/// Supports:
/// - Automatic reconnection with exponential backoff (1s to 30s)
/// - Dynamic subscription to additional tokens at runtime
/// - Full book snapshots and incremental price-change updates
pub struct WsFeedClient {
    book_store: Arc<OrderBookStore>,
    update_tx: mpsc::Sender<BookUpdate>,
}

impl WsFeedClient {
    pub fn new(book_store: Arc<OrderBookStore>, update_tx: mpsc::Sender<BookUpdate>) -> Self {
        Self {
            book_store,
            update_tx,
        }
    }

    /// Spawn the WebSocket connection loop as a background task.
    ///
    /// Returns a sender that can be used to dynamically subscribe to
    /// additional tokens while the connection is active.
    pub fn spawn(self, initial_tokens: Vec<String>) -> mpsc::Sender<Vec<String>> {
        let (sub_tx, mut sub_rx) = mpsc::channel::<Vec<String>>(32);
        let book_store = self.book_store;
        let update_tx = self.update_tx;

        tokio::spawn(async move {
            let mut subscribed_tokens = initial_tokens;
            let mut retry_count: u32 = 0;

            loop {
                match Self::connect_and_run(
                    &book_store,
                    &update_tx,
                    &mut sub_rx,
                    &mut subscribed_tokens,
                )
                .await
                {
                    Ok(()) => {
                        info!("WebSocket connection closed cleanly");
                        retry_count = 0;
                    }
                    Err(e) => {
                        warn!(error = %e, retry = retry_count, "WebSocket connection error");
                    }
                }

                // Exponential backoff: 1s, 2s, 4s, 8s, 16s, capped at 30s
                let delay = RECONNECT_BASE_DELAY * 2u32.pow(retry_count.min(4));
                let delay = delay.min(RECONNECT_MAX_DELAY);
                info!(delay_ms = delay.as_millis(), "Reconnecting WebSocket...");
                tokio::time::sleep(delay).await;
                retry_count += 1;
            }
        });

        sub_tx
    }

    /// Connect to the WebSocket, subscribe, and process messages until
    /// the connection drops or an error occurs.
    async fn connect_and_run(
        book_store: &OrderBookStore,
        update_tx: &mpsc::Sender<BookUpdate>,
        sub_rx: &mut mpsc::Receiver<Vec<String>>,
        subscribed_tokens: &mut Vec<String>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let (ws_stream, _) = connect_async(WS_URL).await?;
        let (mut write, mut read) = ws_stream.split();
        info!(tokens = subscribed_tokens.len(), "WebSocket connected");

        // Subscribe to initial tokens
        if !subscribed_tokens.is_empty() {
            let msg = SubscribeMessage {
                msg_type: "subscribe".into(),
                assets_ids: subscribed_tokens.clone(),
            };
            let payload = serde_json::to_string(&msg)?;
            write.send(Message::Text(payload.into())).await?;
        }

        loop {
            tokio::select! {
                ws_msg = read.next() => {
                    match ws_msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_message(&text, book_store, update_tx).await {
                                debug!(error = %e, "Failed to handle WS message");
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = write.send(Message::Pong(data)).await;
                        }
                        Some(Err(e)) => return Err(e.into()),
                        None => return Ok(()),
                        _ => {}
                    }
                }
                new_tokens = sub_rx.recv() => {
                    if let Some(tokens) = new_tokens {
                        let msg = SubscribeMessage {
                            msg_type: "subscribe".into(),
                            assets_ids: tokens.clone(),
                        };
                        let payload = serde_json::to_string(&msg)?;
                        write.send(Message::Text(payload.into())).await?;
                        subscribed_tokens.extend(tokens);
                    }
                }
            }
        }
    }

    /// Parse and apply a single WebSocket message to the local book store.
    async fn handle_message(
        text: &str,
        book_store: &OrderBookStore,
        update_tx: &mpsc::Sender<BookUpdate>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let msg: WsMessage = serde_json::from_str(text)?;

        match msg.msg_type.as_deref() {
            Some("book") => {
                if let Some(token_id) = &msg.market {
                    let book_lock = book_store.get_or_create(token_id);
                    let mut book = book_lock.write().await;

                    let bids: Vec<(Decimal, Decimal)> = msg
                        .bids
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|l| Some((l.price.parse().ok()?, l.size.parse().ok()?)))
                        .collect();
                    let asks: Vec<(Decimal, Decimal)> = msg
                        .asks
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|l| Some((l.price.parse().ok()?, l.size.parse().ok()?)))
                        .collect();

                    book.apply_snapshot(bids, asks);
                    let _ = update_tx
                        .send(BookUpdate {
                            token_id: token_id.clone(),
                        })
                        .await;
                }
            }
            Some("price_change") => {
                if let (Some(token_id), Some(price_str), Some(size_str), Some(side_str)) =
                    (&msg.market, &msg.price, &msg.size, &msg.side)
                    && let (Ok(price), Ok(size)) = (
                        price_str.parse::<Decimal>(),
                        size_str.parse::<Decimal>(),
                    )
                {
                    let book_lock = book_store.get_or_create(token_id);
                    let mut book = book_lock.write().await;
                    match side_str.as_str() {
                        "BUY" | "buy" => book.update_bid(price, size),
                        "SELL" | "sell" => book.update_ask(price, size),
                        _ => {}
                    }
                    let _ = update_tx
                        .send(BookUpdate {
                            token_id: token_id.clone(),
                        })
                        .await;
                }
            }
            _ => {}
        }

        Ok(())
    }
}
