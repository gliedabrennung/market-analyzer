use std::fmt;
use std::str::FromStr;

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::error::CoreError;
use crate::symbol::Symbol;

/// Kline / candle aggregation interval. FR-1.2.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Interval {
    #[serde(rename = "1m")]
    OneMinute,
    #[serde(rename = "5m")]
    FiveMinutes,
    #[serde(rename = "15m")]
    FifteenMinutes,
    #[serde(rename = "1h")]
    OneHour,
    #[serde(rename = "4h")]
    FourHours,
    #[serde(rename = "1d")]
    OneDay,
}

impl Interval {
    /// Every supported interval, in ascending order.
    pub const ALL: [Interval; 6] = [
        Interval::OneMinute,
        Interval::FiveMinutes,
        Interval::FifteenMinutes,
        Interval::OneHour,
        Interval::FourHours,
        Interval::OneDay,
    ];

    /// Binance's own wire string for this interval, e.g. `"1m"`.
    pub fn as_str(self) -> &'static str {
        match self {
            Interval::OneMinute => "1m",
            Interval::FiveMinutes => "5m",
            Interval::FifteenMinutes => "15m",
            Interval::OneHour => "1h",
            Interval::FourHours => "4h",
            Interval::OneDay => "1d",
        }
    }

    /// Nominal duration of one bar of this interval.
    pub fn duration(self) -> chrono::Duration {
        match self {
            Interval::OneMinute => chrono::Duration::minutes(1),
            Interval::FiveMinutes => chrono::Duration::minutes(5),
            Interval::FifteenMinutes => chrono::Duration::minutes(15),
            Interval::OneHour => chrono::Duration::hours(1),
            Interval::FourHours => chrono::Duration::hours(4),
            Interval::OneDay => chrono::Duration::days(1),
        }
    }
}

impl FromStr for Interval {
    type Err = CoreError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "1m" => Ok(Interval::OneMinute),
            "5m" => Ok(Interval::FiveMinutes),
            "15m" => Ok(Interval::FifteenMinutes),
            "1h" => Ok(Interval::OneHour),
            "4h" => Ok(Interval::FourHours),
            "1d" => Ok(Interval::OneDay),
            other => Err(CoreError::InvalidInterval(other.to_string())),
        }
    }
}

impl fmt::Display for Interval {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// OHLCV candle. Model per TZ 5.2. Money fields are `Decimal`, never `f64`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Kline {
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub symbol: Symbol,
    pub exchange: String,
    pub interval: Interval,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    /// Base-asset volume.
    pub volume: Decimal,
    /// Quote-asset volume.
    pub quote_volume: Decimal,
    pub trades_count: i32,
    pub taker_buy_base: Option<Decimal>,
    pub is_closed: bool,
}

/// Single trade / tick. Model per TZ 5.1.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Trade {
    pub ts: DateTime<Utc>,
    pub symbol: Symbol,
    pub exchange: String,
    pub trade_id: i64,
    pub price: Decimal,
    pub qty: Decimal,
    /// `true` when the aggressor (taker) was the seller.
    pub is_buyer_maker: bool,
}

/// Order book delta. Schema is not fixed by the TZ data model (section 5
/// only specifies `trades`/`klines`); this is a minimal shape sufficient to
/// normalize Binance `depthUpdate` events for FR-1.4/FR-1.6, to be extended
/// when the order-book analytics FRs require more detail.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DepthUpdate {
    pub ts: DateTime<Utc>,
    pub symbol: Symbol,
    pub exchange: String,
    pub first_update_id: i64,
    pub final_update_id: i64,
    pub bids: Vec<(Decimal, Decimal)>,
    pub asks: Vec<(Decimal, Decimal)>,
}

/// Normalized live-stream event, exchange-independent (FR-1.6).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum MarketEvent {
    Trade(Trade),
    Kline(Kline),
    DepthUpdate(DepthUpdate),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_roundtrips_through_str() {
        for i in Interval::ALL {
            assert_eq!(Interval::from_str(i.as_str()).unwrap(), i);
        }
    }

    #[test]
    fn interval_rejects_unknown() {
        assert!(Interval::from_str("2m").is_err());
    }
}
