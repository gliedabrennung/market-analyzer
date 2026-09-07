use std::sync::Arc;
use std::time::Duration;

use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures::StreamExt;
use serde::Deserialize;

use ma_core::{Interval, MarketEvent, Symbol};
use ma_exchanges::{ExchangeSource, StreamKind};

use crate::error::{error_envelope, ApiError};
use crate::state::AppState;
use crate::validation;

/// BE-5 (frontend-tz.md §7): a heartbeat at least this often lets the
/// client tell "quiet market" apart from "connection silently died".
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub interval: Option<String>,
}

/// `WS /stream/{symbol}` (FR-5.1): proxies live trade *and* kline events
/// for `symbol` (`?interval=`, default `1m` — the frontend's live candle
/// update, FR-3.3, needs the kline stream at whatever interval it's
/// displaying, not just raw trades).
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
    Query(q): Query<StreamQuery>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, ApiError> {
    check_origin(&state, &headers)?;
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let interval = validation::parse_interval(q.interval.as_deref())?;
    // Taken before the upgrade, released when `handle_socket` returns: an
    // accepted socket must already own its upstream budget, since after the
    // upgrade there is no longer an HTTP status to refuse with.
    let slot = Arc::clone(&state.ws_slots)
        .try_acquire_owned()
        .map_err(|_| {
            tracing::warn!(
                limit = state.limits.max_ws_connections,
                "refusing ws client: live stream connection limit reached"
            );
            ApiError::RateLimited
        })?;
    Ok(ws.on_upgrade(move |socket| async move {
        handle_socket(socket, state, symbol, interval).await;
        drop(slot);
    }))
}

/// Browsers do not apply CORS to WebSocket handshakes, so the `Origin`
/// header has to be checked by hand — otherwise any page a user visits
/// could open a stream against this API from their browser, which both
/// reads data the CORS policy meant to fence off and burns the connection
/// budget above with connections the operator never asked for. Requests
/// with no `Origin` at all (curl, the CLI, server-side clients) are left
/// alone: `Origin` is a browser-supplied header, and its absence means no
/// browser page is behind the request.
fn check_origin(state: &AppState, headers: &HeaderMap) -> Result<(), ApiError> {
    let Some(origin) = headers.get(axum::http::header::ORIGIN) else {
        return Ok(());
    };
    let origin = origin.to_str().unwrap_or_default();
    if origin == state.cors_origin {
        return Ok(());
    }
    tracing::warn!(%origin, allowed = %state.cors_origin, "refusing ws client from a disallowed origin");
    Err(ApiError::bad_request(
        "origin",
        "websocket connections from this origin are not allowed",
    ))
}

async fn handle_socket(
    mut socket: WebSocket,
    state: Arc<AppState>,
    symbol: Symbol,
    interval: Interval,
) {
    let mut events = match state
        .exchange
        .subscribe(
            std::slice::from_ref(&symbol),
            &[StreamKind::Trade, StreamKind::Kline(interval)],
        )
        .await
    {
        Ok(events) => events,
        Err(e) => {
            // Same redaction rule as the HTTP `500` path: the upstream
            // error text (exchange URLs, transport details) goes to the
            // log, the client just learns the subscription failed.
            tracing::error!(symbol = %symbol, error = %e, "failed to subscribe upstream for ws client");
            let _ = socket
                .send(Message::Text(
                    error_envelope("internal_error", "internal error").to_string(),
                ))
                .await;
            return;
        }
    };

    let mut heartbeat = tokio::time::interval(HEARTBEAT_INTERVAL);
    heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    heartbeat.tick().await; // first tick fires immediately; not a real interval

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

            _ = heartbeat.tick() => {
                if socket.send(Message::Text(r#"{"type":"heartbeat"}"#.to_string())).await.is_err() {
                    break;
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
