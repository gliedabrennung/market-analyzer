//! Exchange abstraction (FR-1.1) and concrete implementations.
//!
//! Adding a new exchange means implementing [`ExchangeSource`] in a new
//! module here — no changes required elsewhere in the workspace (goal Ц4).

/// Binance Spot: REST + combined-stream WebSocket ([`ExchangeSource`] impl).
pub mod binance;
/// [`ExchangeError`], the error type for every exchange operation.
pub mod error;
/// Live combined-stream event parsing and the REST gap-fill trade type.
pub mod live;
/// REST `/api/v3/klines` response parsing.
pub mod model;
/// Request pacing ([`ratelimit::Limiter`]) and 429/418 backoff.
pub mod ratelimit;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;

use ma_core::{Interval, Kline, MarketEvent, Symbol};

pub use error::ExchangeError;

/// Inclusive UTC time range for a historical data request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

/// Tradable instrument metadata (FR-1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    pub symbol: Symbol,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

/// Live-stream event kinds an exchange can be subscribed to (FR-1.4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamKind {
    Trade,
    Kline(Interval),
    Depth,
}

/// Uniform abstraction over a market-data source.
#[async_trait]
pub trait ExchangeSource: Send + Sync {
    /// Short, stable identifier, e.g. `"binance"`. Used as the `exchange`
    /// column value and in log output.
    fn id(&self) -> &'static str;

    /// Registered instruments/symbols known to the exchange.
    async fn instruments(&self) -> Result<Vec<Instrument>, ExchangeError>;

    /// Historical candles for `symbol`/`interval` covering `range`,
    /// paginating internally (FR-1.2).
    async fn klines(
        &self,
        symbol: &Symbol,
        interval: Interval,
        range: TimeRange,
    ) -> Result<Vec<Kline>, ExchangeError>;

    /// Subscribe to a combined live stream of the given event kinds (FR-1.4).
    async fn subscribe(
        &self,
        symbols: &[Symbol],
        streams: &[StreamKind],
    ) -> Result<BoxStream<'static, Result<MarketEvent, ExchangeError>>, ExchangeError>;
}
