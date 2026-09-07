use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, decimal_col_opt, timestamp_col};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VwapPoint {
    pub open_time: DateTime<Utc>,
    pub close: Decimal,

    pub vwap: Option<Decimal>,
}

pub fn vwap(
    conn: &Connection,
    exchange: &str,
    symbol: &Symbol,
    interval: Interval,
    window: u32,
) -> Result<Vec<VwapPoint>, AnalyticsError> {
    if window == 0 {
        return Err(AnalyticsError::InvalidParam {
            name: "window",
            reason: "must be at least 1".to_string(),
        });
    }
    let frame = i64::from(window) - 1;

    let mut stmt = conn.prepare(include_str!("../sql/vwap.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        frame,
        frame,
        exchange,
        symbol.as_str(),
        interval.as_str()
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(VwapPoint {
            open_time: timestamp_col(row, 0)?,
            close: decimal_col(row, 1)?,
            vwap: decimal_col_opt(row, 2)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

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
                 VALUES (?, ?, 'BTCUSDT', 'binance', '1m',
                         CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)),
                         CAST(? AS DECIMAL(28,8)), CAST(? AS DECIMAL(28,8)), 1, true)",
            )
            .unwrap();

        let rows: &[(&str, &str, &str)] = &[
            ("2026-01-01 00:00:00", "100", "1"),
            ("2026-01-01 00:01:00", "200", "1"),
            ("2026-01-01 00:02:00", "300", "2"),
        ];
        for (t, close, vol) in rows {
            stmt.execute(duckdb::params![t, t, close, close, close, close, vol, vol])
                .unwrap();
        }
        conn
    }

    #[test]
    fn vwap_window_3_matches_hand_computed_value() {
        use std::str::FromStr;

        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let points = vwap(&conn, "binance", &symbol, Interval::OneMinute, 3).unwrap();
        assert_eq!(points.len(), 3);

        assert_eq!(
            points[0].vwap,
            Some(Decimal::from_str("100.00000000").unwrap())
        );

        assert_eq!(
            points[1].vwap,
            Some(Decimal::from_str("150.00000000").unwrap())
        );

        assert_eq!(
            points[2].vwap,
            Some(Decimal::from_str("225.00000000").unwrap())
        );
    }

    #[test]
    fn zero_volume_window_yields_none_not_a_crash() {
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
        conn.execute(
            "INSERT INTO klines
             (open_time, close_time, symbol, exchange, interval, open, high, low, close, volume, quote_volume, trades_count, is_closed)
             VALUES ('2026-01-01 00:00:00', '2026-01-01 00:00:00', 'BTCUSDT', 'binance', '1m',
                     CAST('5' AS DECIMAL(18,8)), CAST('5' AS DECIMAL(18,8)), CAST('5' AS DECIMAL(18,8)), CAST('5' AS DECIMAL(18,8)),
                     CAST('0' AS DECIMAL(28,8)), CAST('0' AS DECIMAL(28,8)), 1, true)",
            [],
        )
        .unwrap();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let points = vwap(&conn, "binance", &symbol, Interval::OneMinute, 3).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!(points[0].vwap, None);
    }

    #[test]
    fn rejects_zero_window() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        assert!(vwap(&conn, "binance", &symbol, Interval::OneMinute, 0).is_err());
    }
}
