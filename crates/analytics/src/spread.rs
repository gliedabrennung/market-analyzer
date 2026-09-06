use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, timestamp_col};

/// One matched point of a cross-series spread (FR-3.7).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SpreadPoint {
    pub ts: DateTime<Utc>,
    pub price_a: Decimal,
    pub price_b: Decimal,
    pub spread_abs: Decimal,
    pub spread_bps: Option<f64>,
}

/// Matches series A against the most recent point of series B at or before
/// each A timestamp (`ASOF JOIN`, FR-3.7) and computes the spread both in
/// absolute price terms and in basis points. A and B may be the same or
/// different exchanges/symbols/intervals.
#[allow(clippy::too_many_arguments)]
pub fn cross_spread(
    conn: &Connection,
    exchange_a: &str,
    symbol_a: &Symbol,
    interval_a: Interval,
    exchange_b: &str,
    symbol_b: &Symbol,
    interval_b: Interval,
) -> Result<Vec<SpreadPoint>, AnalyticsError> {
    let mut stmt = conn.prepare(include_str!("../sql/spread.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        exchange_a,
        symbol_a.as_str(),
        interval_a.as_str(),
        exchange_b,
        symbol_b.as_str(),
        interval_b.as_str(),
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(SpreadPoint {
            ts: timestamp_col(row, 0)?,
            price_a: decimal_col(row, 1)?,
            price_b: decimal_col(row, 2)?,
            spread_abs: decimal_col(row, 3)?,
            spread_bps: row.get::<_, Option<f64>>(4)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE klines (
                open_time TIMESTAMP, close_time TIMESTAMP, symbol VARCHAR, exchange VARCHAR,
                interval VARCHAR, open DECIMAL(18,8), high DECIMAL(18,8), low DECIMAL(18,8),
                close DECIMAL(18,8), volume DECIMAL(28,8), quote_volume DECIMAL(28,8),
                trades_count INTEGER, taker_buy_base DECIMAL(28,8), is_closed BOOLEAN
            )",
        )
        .unwrap();
        let mut stmt = conn
            .prepare(
                "INSERT INTO klines
                 (open_time, close_time, symbol, exchange, interval, open, high, low, close, volume, quote_volume, trades_count, is_closed)
                 VALUES (?, ?, ?, ?, '1m',
                         CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)),
                         CAST('1' AS DECIMAL(28,8)), CAST('1' AS DECIMAL(28,8)), 1, true)",
            )
            .unwrap();
        // A: BTCUSDT@binance at 00:00 and 00:01, price 100 then 102.
        // B: BTCUSDT@bybit at 00:00 only, price 99 (older, so ASOF-matches both A rows).
        for (ts, symbol, exchange, price) in [
            ("2026-01-01 00:00:00", "BTCUSDT", "binance", "100"),
            ("2026-01-01 00:01:00", "BTCUSDT", "binance", "102"),
            ("2026-01-01 00:00:00", "BTCUSDT", "bybit", "99"),
        ] {
            stmt.execute(duckdb::params![
                ts, ts, symbol, exchange, price, price, price, price
            ])
            .unwrap();
        }
        conn
    }

    #[test]
    fn asof_join_matches_most_recent_prior_point() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let points = cross_spread(
            &conn,
            "binance",
            &symbol,
            Interval::OneMinute,
            "bybit",
            &symbol,
            Interval::OneMinute,
        )
        .unwrap();
        assert_eq!(points.len(), 2);

        assert_eq!(points[0].price_a, Decimal::from_str("100").unwrap());
        assert_eq!(points[0].price_b, Decimal::from_str("99").unwrap());
        assert_eq!(points[0].spread_abs, Decimal::from_str("1").unwrap());
        assert!((points[0].spread_bps.unwrap() - (1.0 / 99.0 * 10000.0)).abs() < 1e-6);

        // Second A point ASOF-matches the SAME (only) B point.
        assert_eq!(points[1].price_a, Decimal::from_str("102").unwrap());
        assert_eq!(points[1].price_b, Decimal::from_str("99").unwrap());
        assert_eq!(points[1].spread_abs, Decimal::from_str("3").unwrap());
    }
}
