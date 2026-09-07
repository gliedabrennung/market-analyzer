use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::Response;
use serde::Deserialize;

use crate::arrow_ipc::respond_rows;
use crate::error::ApiError;
use crate::queries::fetch_ohlcv;
use crate::state::AppState;
use crate::validation;

#[derive(Debug, Deserialize)]
pub struct OhlcvQuery {
    pub interval: Option<String>,
    pub from: Option<String>,
    pub to: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

pub async fn ohlcv(
    State(state): State<Arc<AppState>>,
    Path(symbol): Path<String>,
    Query(q): Query<OhlcvQuery>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let symbol = validation::validate_symbol(&state, &symbol).await?;
    let interval = validation::parse_interval(q.interval.as_deref())?;
    let (from, to) =
        validation::parse_date_range(q.from.as_deref(), q.to.as_deref(), &state.limits)?;
    let (limit, offset) = validation::parse_pagination(q.limit, q.offset, &state.limits)?;

    let rows = state
        .pool
        .with_meta(move |meta| {
            fetch_ohlcv(
                meta.connection(),
                ma_exchanges::binance::EXCHANGE_ID,
                symbol.as_str(),
                interval.as_str(),
                from,
                to,
                limit,
                offset,
            )
            .map_err(ApiError::from)
        })
        .await?;

    respond_rows(&headers, rows)
}
