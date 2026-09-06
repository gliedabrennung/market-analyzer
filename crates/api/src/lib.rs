//! HTTP API (FR-5.x): `axum` over a pooled read-only DuckDB connection
//! (FR-5.4: all synchronous DuckDB work runs in `spawn_blocking`).

/// Arrow IPC stream responses (frontend-tz.md BE-1) alongside the existing
/// JSON ones — [`arrow_ipc::respond_rows`] picks the format from `Accept`.
pub mod arrow_ipc;
/// [`ApiError`]: the FR-5.2 JSON error envelope and its HTTP status mapping.
pub mod error;
/// [`metrics::Metrics`] (Prometheus registry) and the request-tracking middleware (FR-6.3).
pub mod metrics;
/// [`DbPool`]: checkout/return pool of read-only DuckDB handles (FR-5.4).
pub mod pool;
/// `/ohlcv`'s row type and query — the one endpoint with no `ma_analytics` equivalent.
pub mod queries;
/// The FR-5.1 HTTP/WS route handlers.
pub mod routes;
/// [`AppState`]/[`ApiLimits`]: shared server state and validation bounds.
pub mod state;
/// FR-5.3 input validation, run before any DB/analytics call.
pub mod validation;

use std::sync::Arc;
use std::time::Instant;

use axum::http::HeaderValue;
use axum::routing::get;
use axum::Router;
use tower_http::cors::CorsLayer;
use tower_http::trace::TraceLayer;

use ma_exchanges::binance::BinanceSpot;

pub use error::ApiError;
pub use pool::DbPool;
pub use state::{ApiLimits, AppState};

/// Builds the full router. `serve` (the CLI command) owns binding a
/// listener and running it with graceful shutdown. `cors_origin` is the
/// single allowed `Access-Control-Allow-Origin` (frontend-tz.md BE-2) —
/// the frontend is a single first-party client, so one configured origin
/// rather than a wildcard or a dynamic allowlist.
pub fn build_app(
    pool: DbPool,
    exchange: BinanceSpot,
    limits: ApiLimits,
    cors_origin: &str,
) -> Result<Router, ApiError> {
    let cors_origin: HeaderValue = cors_origin
        .parse()
        .map_err(|_| ApiError::Internal(format!("invalid CORS origin '{cors_origin}'")))?;
    let metrics = metrics::Metrics::new()
        .map_err(|e| ApiError::Internal(format!("initializing metrics: {e}")))?;

    let state = Arc::new(AppState {
        pool,
        exchange,
        metrics,
        start_time: Instant::now(),
        limits,
    });

    Ok(Router::new()
        .route("/health", get(routes::health::health))
        .route("/symbols", get(routes::symbols::list_symbols))
        .route("/ohlcv/:symbol", get(routes::ohlcv::ohlcv))
        .route("/analytics/:symbol/vwap", get(routes::analytics::vwap))
        .route(
            "/analytics/:symbol/volatility",
            get(routes::analytics::volatility),
        )
        .route(
            "/analytics/:symbol/anomalies",
            get(routes::analytics::anomalies),
        )
        .route("/analytics/:symbol/ofi", get(routes::analytics::ofi))
        .route(
            "/analytics/correlation",
            get(routes::analytics::correlation),
        )
        .route("/stream/:symbol", get(routes::stream_ws::stream_ws))
        .route("/metrics", get(metrics_endpoint))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            metrics::track_metrics,
        ))
        .layer(TraceLayer::new_for_http())
        .layer(
            CorsLayer::new()
                .allow_origin(cors_origin)
                .allow_methods([axum::http::Method::GET])
                .allow_headers([axum::http::header::ACCEPT]),
        )
        .with_state(state))
}

async fn metrics_endpoint(
    axum::extract::State(state): axum::extract::State<Arc<AppState>>,
) -> impl axum::response::IntoResponse {
    (
        [(
            axum::http::header::CONTENT_TYPE,
            "text/plain; version=0.0.4",
        )],
        state.metrics.render(),
    )
}
