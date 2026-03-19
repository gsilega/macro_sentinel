// WebSocket handler for real-time indicator updates.
//
// Each connected client receives the full indicator state immediately on connect,
// then receives push updates on a fixed interval. The server loop uses tokio::select!
// to concurrently handle both the interval timer and incoming client messages
// (pings, disconnects) without blocking either.

use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket};
use tokio::sync::RwLock;
use tracing::{debug, info};

use crate::state::AppState;

/// How often to push state updates to connected clients.
const WS_PUSH_INTERVAL_SECS: u64 = 10;

/// How often to send a server-initiated ping to keep the connection alive.
/// Browsers and proxies will drop idle WebSocket connections without a heartbeat.
const WS_PING_INTERVAL_SECS: u64 = 30;

/// Public entry point called from routes.rs. Delegates to the internal handler.
pub async fn handle_socket_direct(socket: WebSocket, state: Arc<RwLock<AppState>>) {
    handle_socket(socket, state).await;
}

/// Manages a single WebSocket connection for its full lifetime.
async fn handle_socket(mut socket: WebSocket, state: Arc<RwLock<AppState>>) {
    info!("WebSocket client connected");

    // Send current state immediately so the client isn't waiting for the first interval.
    if let Err(e) = send_state_update(&mut socket, &state).await {
        debug!("Failed to send initial state: {}", e);
        return;
    }

    let mut push_interval = tokio::time::interval(Duration::from_secs(WS_PUSH_INTERVAL_SECS));
    push_interval.tick().await; // consume the first immediate tick

    let mut ping_interval = tokio::time::interval(Duration::from_secs(WS_PING_INTERVAL_SECS));
    ping_interval.tick().await; // consume the first immediate tick

    loop {
        tokio::select! {
            _ = push_interval.tick() => {
                if let Err(e) = send_state_update(&mut socket, &state).await {
                    debug!("WebSocket send failed: {}", e);
                    break;
                }
            }

            _ = ping_interval.tick() => {
                if socket.send(Message::Ping(vec![].into())).await.is_err() {
                    break;
                }
            }

            msg = socket.recv() => {
                match msg {
                    None => {
                        info!("WebSocket client disconnected");
                        break;
                    }
                    Some(Ok(Message::Ping(data))) => {
                        if socket.send(Message::Pong(data)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) => {
                        info!("WebSocket close frame received");
                        break;
                    }
                    Some(Ok(_)) => {} // binary/text messages from client are ignored
                    Some(Err(e)) => {
                        debug!("WebSocket error: {}", e);
                        break;
                    }
                }
            }
        }
    }

    info!("WebSocket connection closed");
}

/// Serialize the current app state to JSON and send it to the client.
/// The read lock is released before the async send to avoid holding it across I/O.
async fn send_state_update(
    socket: &mut WebSocket,
    state: &Arc<RwLock<AppState>>,
) -> Result<(), axum::Error> {
    let json_payload = {
        let guard = state.read().await;
        serde_json::json!({
            "type": "update",
            "readings": guard.all_latest_readings(),
            "ai_interpretation": guard.ai_interpretation,
            "last_updated": guard.last_updated.map(|dt| dt.to_rfc3339()),
        })
        .to_string()
        // guard dropped here — lock released before the await below
    };

    socket.send(Message::Text(json_payload.into())).await
}
