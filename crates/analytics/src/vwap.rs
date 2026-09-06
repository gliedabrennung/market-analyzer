use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, timestamp_col};

/// One point of a rolling VWAP series (FR-3.2).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VwapPoint {
    pub open_time: DateTime<Utc>,
    pub close: Decimal,
    /// `None` for the very first bar(s) of a symbol/interval that has no
    /// volume in its window yet (all-zero window).
    pub vwap: Option<f64>,
}

/// Rolling VWAP over the last `window` klines (default 20 per FR-3.2).
pub fn vwap(
    conn: &Connection,
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
        symbol.as_str(),
        interval.as_str()
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(VwapPoint {
            open_time: timestamp_col(row, 0)?,
            close: decimal_col(row, 1)?,
            vwap: row.get::<_, Option<f64>>(2)?,
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
        // close=100 vol=1 ; close=200 vol=1 ; close=300 vol=2
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
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let points = vwap(&conn, &symbol, Interval::OneMinute, 3).unwrap();
        assert_eq!(points.len(), 3);

        // bar 1: vwap = 100
        assert!((points[0].vwap.unwrap() - 100.0).abs() < 1e-6);
        // bar 2: (100*1 + 200*1) / (1+1) = 150
        assert!((points[1].vwap.unwrap() - 150.0).abs() < 1e-6);
        // bar 3: (100*1 + 200*1 + 300*2) / (1+1+2) = 900/4 = 225
        assert!((points[2].vwap.unwrap() - 225.0).abs() < 1e-6);
    }

    #[test]
    fn rejects_zero_window() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        assert!(vwap(&conn, &symbol, Interval::OneMinute, 0).is_err());
    }
}
