use std::sync::Arc;
use std::time::Instant;

use ma_exchanges::binance::BinanceSpot;
use tokio::sync::Semaphore;

use crate::metrics::Metrics;
use crate::pool::DbPool;

#[derive(Debug, Clone, Copy)]
pub struct ApiLimits {
    pub max_date_range_days: i64,
    pub default_pagination_limit: i64,
    pub max_pagination_limit: i64,

    pub max_correlation_symbols: usize,

    pub max_ws_connections: usize,
}

pub struct AppState {
    pub pool: DbPool,
    pub exchange: BinanceSpot,
    pub metrics: Metrics,
    pub start_time: Instant,
    pub limits: ApiLimits,

    pub ws_slots: Arc<Semaphore>,

    pub cors_origin: String,
}
