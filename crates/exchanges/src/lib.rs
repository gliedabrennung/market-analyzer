pub mod binance;

pub mod error;

pub mod live;

pub mod model;

pub mod ratelimit;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;

use ma_core::{Interval, Kline, MarketEvent, Symbol};

pub use error::ExchangeError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeRange {
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Instrument {
    pub symbol: Symbol,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamKind {
    Trade,
    Kline(Interval),
    Depth,
}

#[async_trait]
pub trait ExchangeSource: Send + Sync {
    fn id(&self) -> &'static str;

    async fn instruments(&self) -> Result<Vec<Instrument>, ExchangeError>;

    async fn klines(
        &self,
        symbol: &Symbol,
        interval: Interval,
        range: TimeRange,
    ) -> Result<Vec<Kline>, ExchangeError>;

    async fn subscribe(
        &self,
        symbols: &[Symbol],
        streams: &[StreamKind],
    ) -> Result<BoxStream<'static, Result<MarketEvent, ExchangeError>>, ExchangeError>;
}
