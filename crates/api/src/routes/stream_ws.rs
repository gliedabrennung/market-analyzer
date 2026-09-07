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

const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(20);

#[derive(Debug, Deserialize)]
pub struct StreamQuery {
    pub interval: Option<String>,
}

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
    heartbeat.tick().await;

    loop {
        tokio::select! {
            biased;

            incoming = socket.recv() => {
                match incoming {
                    Some(Ok(_)) => {}
                    _ => break,
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
        return true;
    };
    socket.send(Message::Text(payload)).await.is_ok()
}
