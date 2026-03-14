use axum::{
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::IntoResponse,
};
use futures_util::{SinkExt, StreamExt};

use crate::state::AppState;

pub async fn ws_handler(ws: WebSocketUpgrade, State(state): State<AppState>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.ws_tx.subscribe();

    // Forward broadcast messages to this client
    let mut send_task = tokio::spawn(async move {
        while let Ok(msg) = rx.recv().await {
            if sender.send(Message::Text(msg.into())).await.is_err() {
                break;
            }
        }
    });

    // Handle client messages (ping/pong, close)
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(_msg)) = receiver.next().await {
            // Client messages ignored for now
        }
    });

    // Wait for either task to end. When the client disconnects (recv_task ends),
    // explicitly abort send_task so the broadcast receiver is cleaned up and
    // doesn't fill the channel buffer. Previously, dropping a JoinHandle merely
    // detaches the task, leaving its broadcast receiver alive indefinitely.
    tokio::select! {
        _ = &mut recv_task => {
            send_task.abort();
        },
        _ = &mut send_task => {
            // send_task ended first (e.g., broadcast channel closed or send error)
        },
    }
}
