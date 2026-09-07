use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::Symbol;

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, timestamp_col};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ResampledBar {
    pub bucket: DateTime<Utc>,
    pub symbol: String,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub trades_count: i64,
}

pub fn resample_ohlcv(
    conn: &Connection,
    exchange: &str,
    symbol: &Symbol,
    bucket_seconds: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<ResampledBar>, AnalyticsError> {
    if bucket_seconds <= 0 {
        return Err(AnalyticsError::InvalidParam {
            name: "bucket_seconds",
            reason: "must be positive".to_string(),
        });
    }

    let mut stmt = conn.prepare(include_str!("../sql/resample_ohlcv.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        bucket_seconds,
        exchange,
        symbol.as_str(),
        from.naive_utc(),
        to.naive_utc()
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(ResampledBar {
            bucket: timestamp_col(row, 0)?,
            symbol: row.get(1)?,
            open: decimal_col(row, 2)?,
            high: decimal_col(row, 3)?,
            low: decimal_col(row, 4)?,
            close: decimal_col(row, 5)?,
            volume: decimal_col(row, 6)?,
            trades_count: row.get(7)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use std::str::FromStr;

    fn setup() -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE trades (
                ts TIMESTAMP, symbol VARCHAR, exchange VARCHAR, trade_id BIGINT,
                price DECIMAL(18,8), qty DECIMAL(18,8), is_buyer_maker BOOLEAN
            )",
        )
        .unwrap();
        let mut stmt = conn
            .prepare("INSERT INTO trades VALUES (?, 'BTCUSDT', 'binance', ?, CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), false)")
            .unwrap();

        let rows: &[(&str, i64, &str, &str)] = &[
            ("2026-01-01 00:00:00", 1, "100", "1"),
            ("2026-01-01 00:00:10", 2, "101", "2"),
            ("2026-01-01 00:00:50", 3, "99", "3"),
            ("2026-01-01 00:01:00", 4, "200", "4"),
        ];
        for (ts, id, price, qty) in rows {
            stmt.execute(duckdb::params![ts, id, price, qty]).unwrap();
        }
        conn
    }

    #[test]
    fn resamples_into_expected_bars() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 1, 1, 0, 2, 0).unwrap();

        let bars = resample_ohlcv(&conn, "binance", &symbol, 60, from, to).unwrap();
        assert_eq!(bars.len(), 2);

        assert_eq!(bars[0].open, Decimal::from_str("100").unwrap());
        assert_eq!(bars[0].high, Decimal::from_str("101").unwrap());
        assert_eq!(bars[0].low, Decimal::from_str("99").unwrap());
        assert_eq!(bars[0].close, Decimal::from_str("99").unwrap());
        assert_eq!(bars[0].volume, Decimal::from_str("6").unwrap());
        assert_eq!(bars[0].trades_count, 3);

        assert_eq!(bars[1].open, Decimal::from_str("200").unwrap());
        assert_eq!(bars[1].volume, Decimal::from_str("4").unwrap());
        assert_eq!(bars[1].trades_count, 1);
    }

    #[test]
    fn rejects_non_positive_bucket() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 1, 1, 0, 2, 0).unwrap();
        assert!(resample_ohlcv(&conn, "binance", &symbol, 0, from, to).is_err());
    }
}
