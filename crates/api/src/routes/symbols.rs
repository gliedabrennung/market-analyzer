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

    pub has_data: bool,
}

pub async fn list_symbols(
    State(state): State<Arc<AppState>>,
) -> Result<Json<Vec<SymbolDto>>, ApiError> {
    let (records, with_data) = state
        .pool
        .with_meta(|meta| {
            let records = meta.list_symbols()?;
            let with_data = meta.symbols_with_data()?;
            Ok((records, with_data))
        })
        .await?;
    Ok(Json(
        records
            .into_iter()
            .map(|r| SymbolDto {
                has_data: with_data.contains(&r.symbol),
                exchange: r.exchange,
                symbol: r.symbol,
                base_asset: r.base_asset,
                quote_asset: r.quote_asset,
                status: r.status,
            })
            .collect(),
    ))
}
