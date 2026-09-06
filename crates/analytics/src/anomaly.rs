use chrono::{DateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::{decimal_col, timestamp_col};

/// One detected volume anomaly (FR-3.4).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolumeAnomaly {
    pub open_time: DateTime<Utc>,
    /// Stored trade volume, so `Decimal`, never `f64`.
    pub volume: Decimal,
    /// A genuinely-derived statistic (standard scores over log-free window
    /// stats), not a stored quantity — `f64` is correct here.
    pub z_score: f64,
}

/// Bars whose volume z-score (relative to the preceding `window` bars,
/// excluding itself) exceeds `threshold` (defaults 100 / 3.0 per FR-3.4).
pub fn volume_anomalies(
    conn: &Connection,
    exchange: &str,
    symbol: &Symbol,
    interval: Interval,
    window: u32,
    threshold: f64,
) -> Result<Vec<VolumeAnomaly>, AnalyticsError> {
    if window == 0 {
        return Err(AnalyticsError::InvalidParam {
            name: "window",
            reason: "must be at least 1".to_string(),
        });
    }

    let mut stmt = conn.prepare(include_str!("../sql/volume_anomaly.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        i64::from(window),
        i64::from(window),
        exchange,
        symbol.as_str(),
        interval.as_str(),
        threshold
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(VolumeAnomaly {
            open_time: timestamp_col(row, 0)?,
            volume: decimal_col(row, 1)?,
            z_score: row.get(2)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(volumes: &[&str]) -> Connection {
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
                         CAST('1' AS DECIMAL(18,8)), CAST('1' AS DECIMAL(18,8)), CAST('1' AS DECIMAL(18,8)), CAST('1' AS DECIMAL(18,8)),
                         CAST(? AS DECIMAL(28,8)), CAST('1' AS DECIMAL(28,8)), 1, true)",
            )
            .unwrap();
        for (i, vol) in volumes.iter().enumerate() {
            let ts = format!("2026-01-01 00:{i:02}:00");
            stmt.execute(duckdb::params![ts, ts, vol]).unwrap();
        }
        conn
    }

    #[test]
    fn zero_variance_baseline_yields_no_anomaly_not_a_crash() {
        // Baseline of 10,10,10,10,10 has zero stddev; nullif(sd_vol, 0)
        // makes z_score NULL for the following point regardless of its
        // volume, and `NULL > threshold` is not true in SQL — so a spike
        // must NOT be (falsely) flagged here. Real detection over a
        // *varying* baseline is asserted numerically in the next test.
        let conn = setup(&["10", "10", "10", "10", "10", "1000"]);
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let anomalies =
            volume_anomalies(&conn, "binance", &symbol, Interval::OneMinute, 5, 3.0).unwrap();
        assert_eq!(anomalies.len(), 0);
    }

    #[test]
    fn z_score_matches_hand_computation() {
        // Baseline 8,10,12,10,10 (mean=10, sample stddev = sqrt(2)) then a
        // spike of 20: z = (20-10)/sqrt(2) ≈ 7.0710678.
        let conn = setup(&["8", "10", "12", "10", "10", "20"]);
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let anomalies =
            volume_anomalies(&conn, "binance", &symbol, Interval::OneMinute, 5, 3.0).unwrap();
        assert_eq!(anomalies.len(), 1);
        let expected_z = (20.0 - 10.0) / 2.0f64.sqrt();
        assert!((anomalies[0].z_score - expected_z).abs() < 1e-6);
    }

    #[test]
    fn rejects_zero_window() {
        let conn = setup(&["1"]);
        let symbol = Symbol::new("BTCUSDT").unwrap();
        assert!(volume_anomalies(&conn, "binance", &symbol, Interval::OneMinute, 0, 3.0).is_err());
    }
}
