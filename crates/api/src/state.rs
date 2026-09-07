use std::sync::Arc;
use std::time::Instant;

use ma_exchanges::binance::BinanceSpot;
use tokio::sync::Semaphore;

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
    /// Caps concurrently open `/stream/{symbol}` clients. Each one opens
    /// its *own* upstream exchange connection (see `routes::stream_ws`), so
    /// without a ceiling a handful of cheap client requests turn into an
    /// unbounded number of outbound sockets — exhausting local file
    /// descriptors and, well before that, earning the exchange's own
    /// connection-rate ban for the whole deployment.
    pub max_ws_connections: usize,
}

/// Everything a route handler needs, shared behind one `Arc` (FR-5.4).
pub struct AppState {
    pub pool: DbPool,
    pub exchange: BinanceSpot,
    pub metrics: Metrics,
    pub start_time: Instant,
    pub limits: ApiLimits,
    /// One permit per allowed live WS client, held for the lifetime of that
    /// client's socket (`limits.max_ws_connections`).
    pub ws_slots: Arc<Semaphore>,
    /// The single browser origin allowed to talk to this API — the same
    /// value the CORS layer enforces for HTTP. Kept here because the
    /// WebSocket route has to check it itself: CORS is not applied to
    /// WebSocket handshakes by browsers, so without this any page on the
    /// internet could open `/stream/{symbol}` from a visitor's browser.
    pub cors_origin: String,
}
