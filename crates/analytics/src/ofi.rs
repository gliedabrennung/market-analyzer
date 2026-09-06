use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::Symbol;

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, timestamp_col};

/// Order Flow Imbalance for one time bucket (FR-3.5).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OfiBucket {
    pub bucket: DateTime<Utc>,
    /// Stored trade quantity, so `Decimal`, never `f64`.
    pub buy_volume: Decimal,
    /// Stored trade quantity, so `Decimal`, never `f64`.
    pub sell_volume: Decimal,
    /// `(buy_volume - sell_volume) / total_volume`: a genuinely-derived
    /// ratio in `[-1, 1]`, not a stored quantity — `f64` is correct here.
    pub ofi: f64,
}

/// Order flow imbalance = (buy_volume - sell_volume) / total_volume,
/// bucketed by `bucket_seconds`, over `[from, to)`.
pub fn order_flow_imbalance(
    conn: &Connection,
    exchange: &str,
    symbol: &Symbol,
    bucket_seconds: i64,
    from: DateTime<Utc>,
    to: DateTime<Utc>,
) -> Result<Vec<OfiBucket>, AnalyticsError> {
    if bucket_seconds <= 0 {
        return Err(AnalyticsError::InvalidParam {
            name: "bucket_seconds",
            reason: "must be positive".to_string(),
        });
    }

    let mut stmt = conn.prepare(include_str!("../sql/ofi.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        bucket_seconds,
        exchange,
        symbol.as_str(),
        from.naive_utc(),
        to.naive_utc()
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(OfiBucket {
            bucket: timestamp_col(row, 0)?,
            buy_volume: decimal_col(row, 1)?,
            sell_volume: decimal_col(row, 2)?,
            ofi: row.get(3)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

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
            .prepare("INSERT INTO trades VALUES (?, 'BTCUSDT', 'binance', ?, CAST('1' AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), ?)")
            .unwrap();
        // Bucket [0,60): buyer-taker (is_buyer_maker=false) qty 3 and 2 -> buy=5
        //                seller-taker (is_buyer_maker=true) qty 1 -> sell=1
        // ofi = (5-1)/(5+1) = 4/6 = 0.6666...
        let rows: &[(&str, i64, &str, bool)] = &[
            ("2026-01-01 00:00:05", 1, "3", false),
            ("2026-01-01 00:00:10", 2, "2", false),
            ("2026-01-01 00:00:15", 3, "1", true),
        ];
        for (ts, id, qty, is_maker) in rows {
            stmt.execute(duckdb::params![ts, id, qty, is_maker])
                .unwrap();
        }
        conn
    }

    #[test]
    fn ofi_matches_hand_computed_value() {
        use std::str::FromStr;

        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();

        let buckets = order_flow_imbalance(&conn, "binance", &symbol, 60, from, to).unwrap();
        assert_eq!(buckets.len(), 1);
        assert_eq!(
            buckets[0].buy_volume,
            Decimal::from_str("5.00000000").unwrap()
        );
        assert_eq!(
            buckets[0].sell_volume,
            Decimal::from_str("1.00000000").unwrap()
        );
        assert!((buckets[0].ofi - (4.0 / 6.0)).abs() < 1e-6);
    }

    #[test]
    fn rejects_non_positive_bucket() {
        let conn = setup();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let from = Utc.with_ymd_and_hms(2026, 1, 1, 0, 0, 0).unwrap();
        let to = Utc.with_ymd_and_hms(2026, 1, 1, 0, 1, 0).unwrap();
        assert!(order_flow_imbalance(&conn, "binance", &symbol, 0, from, to).is_err());
    }
}
