use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;

use ma_analytics::{
    correlation_matrix, order_flow_imbalance, realized_volatility, volume_anomalies,
    vwap as vwap_query, CorrelationPair, OfiBucket, VolatilityPoint, VolumeAnomaly, VwapPoint,
};
use ma_core::Symbol;

use crate::error::ApiError;
use crate::state::AppState;
use crate::validation;

#[derive(Debug, Deserialize)]
pub struct WindowQuery {
    pub interval: Option<String>,
    pub window: Option<u32>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /analytics/{symbol}/vwap` (FR-5.1, default window 20 per FR-3.2).
pub async fn vwap(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Vec<VwapPoint>>, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let interval = validation::parse_interval(q.interval.as_deref())?;
    let window = validation::parse_window(q.window, 20)?;
    let (limit, offset) = validation::parse_pagination(q.limit, q.offset, &state.limits)?;

    let rows = state
        .pool
        .with_meta(move |meta| {
            vwap_query(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                &symbol,
                interval,
                window,
            )
            .map_err(ApiError::from)
        })
        .await?;
    Ok(Json(validation::paginate(rows, limit, offset)))
}

/// `GET /analytics/{symbol}/volatility` (FR-5.1, default window 20 per FR-3.3).
pub async fn volatility(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(q): Query<WindowQuery>,
) -> Result<Json<Vec<VolatilityPoint>>, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let interval = validation::parse_interval(q.interval.as_deref())?;
    let window = validation::parse_window(q.window, 20)?;
    let (limit, offset) = validation::parse_pagination(q.limit, q.offset, &state.limits)?;

    let rows = state
        .pool
        .with_meta(move |meta| {
            realized_volatility(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                &symbol,
                interval,
                window,
            )
            .map_err(ApiError::from)
        })
        .await?;
    Ok(Json(validation::paginate(rows, limit, offset)))
}

#[derive(Debug, Deserialize)]
pub struct AnomaliesQuery {
    pub interval: Option<String>,
    pub window: Option<u32>,
    pub threshold: Option<f64>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /analytics/{symbol}/anomalies` (FR-5.1, defaults window 100 / threshold 3.0 per FR-3.4).
pub async fn anomalies(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(q): Query<AnomaliesQuery>,
) -> Result<Json<Vec<VolumeAnomaly>>, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let interval = validation::parse_interval(q.interval.as_deref())?;
    let window = validation::parse_window(q.window, 100)?;
    let threshold = validation::parse_threshold(q.threshold, 3.0)?;
    let (limit, offset) = validation::parse_pagination(q.limit, q.offset, &state.limits)?;

    let rows = state
        .pool
        .with_meta(move |meta| {
            volume_anomalies(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                &symbol,
                interval,
                window,
                threshold,
            )
            .map_err(ApiError::from)
        })
        .await?;
    Ok(Json(validation::paginate(rows, limit, offset)))
}

#[derive(Debug, Deserialize)]
pub struct OfiQuery {
    pub bucket: Option<i64>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

/// `GET /analytics/{symbol}/ofi` (FR-5.1, default 60s bucket per FR-3.5).
pub async fn ofi(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(q): Query<OfiQuery>,
) -> Result<Json<Vec<OfiBucket>>, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let bucket_seconds = validation::parse_bucket_seconds(q.bucket, 60)?;
    let (from, to) =
        validation::parse_date_range(q.from.as_deref(), q.to.as_deref(), &state.limits)?;
    let (limit, offset) = validation::parse_pagination(q.limit, q.offset, &state.limits)?;
    let from_utc = from.and_utc();
    let to_utc = to.and_utc();

    let rows = state
        .pool
        .with_meta(move |meta| {
            order_flow_imbalance(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                &symbol,
                bucket_seconds,
                from_utc,
                to_utc,
            )
            .map_err(ApiError::from)
        })
        .await?;
    Ok(Json(validation::paginate(rows, limit, offset)))
}

#[derive(Debug, Deserialize)]
pub struct CorrelationQuery {
    pub symbols: String,
    pub interval: Option<String>,
}

/// `GET /analytics/correlation` (FR-5.1). `symbols` is a comma-separated list.
pub async fn correlation(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CorrelationQuery>,
) -> Result<Json<Vec<CorrelationPair>>, ApiError> {
    let raw_symbols: Vec<&str> = q
        .symbols
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if raw_symbols.len() < 2 {
        return Err(ApiError::bad_request(
            "symbols",
            "need at least 2 comma-separated symbols",
        ));
    }
    if raw_symbols.len() > state.limits.max_correlation_symbols {
        return Err(ApiError::bad_request(
            "symbols",
            format!(
                "at most {} symbols allowed, got {}",
                state.limits.max_correlation_symbols,
                raw_symbols.len()
            ),
        ));
    }

    let mut symbols: Vec<Symbol> = Vec::with_capacity(raw_symbols.len());
    for raw in raw_symbols {
        symbols.push(validation::validate_symbol(&state, raw).await?);
    }
    let interval = validation::parse_interval(q.interval.as_deref())?;

    let rows = state
        .pool
        .with_meta(move |meta| {
            correlation_matrix(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                &symbols,
                interval,
            )
            .map_err(ApiError::from)
        })
        .await?;
    Ok(Json(rows))
}
