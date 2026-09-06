use std::time::Instant;

use ma_exchanges::binance::BinanceSpot;

use crate::metrics::Metrics;
use crate::pool::DbPool;

/// Runtime knobs the router needs but that don't belong on any one request
/// (FR-5.3's date-range width cap, FR-5.5's pagination bounds).
#[derive(Debug, Clone, Copy)]
pub struct ApiLimits {
    pub max_date_range_days: i64,
    pub default_pagination_limit: i64,
    pub max_pagination_limit: i64,
    /// Caps `/analytics/correlation`'s symbol count: the query is O(n^2) in
    /// pairwise joins, so an unbounded symbol list is a self-inflicted DoS.
    pub max_correlation_symbols: usize,
}

/// Everything a route handler needs, shared behind one `Arc` (FR-5.4).
pub struct AppState {
    pub pool: DbPool,
    pub exchange: BinanceSpot,
    pub metrics: Metrics,
    pub start_time: Instant,
    pub limits: ApiLimits,
}
