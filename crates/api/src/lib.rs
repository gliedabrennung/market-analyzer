pub mod arrow_ipc;

pub mod error;

pub mod metrics;

pub mod pool;

pub mod queries;

pub mod routes;

pub mod state;

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

pub fn build_app(
    pool: DbPool,
    exchange: BinanceSpot,
    limits: ApiLimits,
    cors_origin: &str,
) -> Result<Router, ApiError> {
    let allowed_origin = cors_origin.to_string();
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
        ws_slots: Arc::new(tokio::sync::Semaphore::new(limits.max_ws_connections)),
        cors_origin: allowed_origin,
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
