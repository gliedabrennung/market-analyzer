use std::sync::Arc;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, State};
use axum::response::IntoResponse;
use futures::StreamExt;

use ma_core::{MarketEvent, Symbol};
use ma_exchanges::{ExchangeSource, StreamKind};

use crate::error::{error_envelope, ApiError};
use crate::state::AppState;
use crate::validation;

/// `WS /stream/{symbol}` (FR-5.1): proxies live trade events for `symbol`.
///
/// This is a fresh upstream subscription per connected client (via
/// `ma_exchanges`, independent of anything the `stream` CLI collector is
/// doing) — simplest correct implementation. It does not fan a single
/// upstream subscription out to multiple clients, so many simultaneous
/// viewers of the same symbol each open their own Binance connection; that
/// would be the next optimization if this needs to scale to many
/// concurrent WS clients.
pub async fn stream_ws(
    ws: WebSocketUpgrade,
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
) -> Result<impl IntoResponse, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    Ok(ws.on_upgrade(move |socket| handle_socket(socket, state, symbol)))
}

async fn handle_socket(mut socket: WebSocket, state: Arc<AppState>, symbol: Symbol) {
    let mut events = match state
        .exchange
        .subscribe(std::slice::from_ref(&symbol), &[StreamKind::Trade])
        .await
    {
        Ok(events) => events,
        Err(e) => {
            let _ = socket
                .send(Message::Text(
                    error_envelope("internal_error", e.to_string()).to_string(),
                ))
                .await;
            return;
        }
    };

    loop {
        tokio::select! {
            biased;

            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(_)) => {}
                    _ => break, // client closed, errored, or sent a close frame
                }
            }

            maybe_event = events.next() => {
                match maybe_event {
                    Some(Ok(event)) => {
                        if !forward(&mut socket, &event).await {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        tracing::warn!(symbol = %symbol, error = %e, "upstream stream error while proxying to ws client");
                    }
                    None => break,
                }
            }
        }
    }
}

async fn forward(socket: &mut WebSocket, event: &MarketEvent) -> bool {
    let Ok(payload) = serde_json::to_string(event) else {
        return true; // skip an unserializable event, keep the connection open
    };
    socket.send(Message::Text(payload)).await.is_ok()
}
