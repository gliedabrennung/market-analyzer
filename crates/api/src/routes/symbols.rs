use std::sync::Arc;

use axum::extract::State;
use axum::Json;
use serde::Serialize;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize)]
pub struct SymbolDto {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

/// `GET /symbols` (FR-5.1): the registry populated by `symbols --refresh`.
pub async fn list_symbols(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SymbolDto>>, ApiError> {
    let records = state
        .pool
        .with_meta(|meta| meta.list_symbols().map_err(ApiError::from))
        .await?;
    Ok(Json(
        records
            .into_iter()
            .map(|r| SymbolDto {
                exchange: r.exchange,
                symbol: r.symbol,
                base_asset: r.base_asset,
                quote_asset: r.quote_asset,
                status: r.status,
            })
            .collect(),
    ))
}
