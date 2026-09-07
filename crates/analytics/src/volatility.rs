use chrono::{DateTime, Utc};
use duckdb::Connection;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::timestamp_col;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VolatilityPoint {
    pub open_time: DateTime<Utc>,

    pub realized_volatility: Option<f64>,
}

pub fn realized_volatility(
    conn: &Connection,
    exchange: &str,
    symbol: &Symbol,
    interval: Interval,
    window: u32,
) -> Result<Vec<VolatilityPoint>, AnalyticsError> {
    if window == 0 {
        return Err(AnalyticsError::InvalidParam {
            name: "window",
            reason: "must be at least 1".to_string(),
        });
    }
    let frame = i64::from(window) - 1;
    let bars_per_day = 86_400.0 / interval.duration().num_seconds() as f64;
    let scale_factor = bars_per_day.sqrt();

    let mut stmt = conn.prepare(include_str!("../sql/volatility.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        exchange,
        symbol.as_str(),
        interval.as_str(),
        frame,
        scale_factor
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(VolatilityPoint {
            open_time: timestamp_col(row, 0)?,
            realized_volatility: row.get::<_, Option<f64>>(1)?,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup(closes: &[(&str, &str)]) -> Connection {
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
                 VALUES (?, ?, 'BTCUSDT', 'binance', '1d',
                         CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)),
                         CAST('1' AS DECIMAL(28,8)), CAST('1' AS DECIMAL(28,8)), 1, true)",
            )
            .unwrap();
        for (t, close) in closes {
            stmt.execute(duckdb::params![t, t, close, close, close, close])
                .unwrap();
        }
        conn
    }

    fn reference_volatility(closes: &[f64], window: usize) -> Vec<Option<f64>> {
        let log_rets: Vec<f64> = closes.windows(2).map(|w| (w[1] / w[0]).ln()).collect();
        (0..log_rets.len())
            .map(|i| {
                let start = i.saturating_sub(window - 1);
                let slice = &log_rets[start..=i];
                if slice.len() < 2 {
                    return None;
                }
                let mean = slice.iter().sum::<f64>() / slice.len() as f64;
                let var = slice.iter().map(|x| (x - mean).powi(2)).sum::<f64>()
                    / (slice.len() as f64 - 1.0);
                Some(var.sqrt())
            })
            .collect()
    }

    #[test]
    fn matches_independently_computed_reference_within_1e_minus_6() {
        let closes = ["100", "105", "100", "110"];
        let closes_f64: Vec<f64> = closes.iter().map(|s| s.parse().unwrap()).collect();
        let rows: Vec<(String, &str)> = closes
            .iter()
            .enumerate()
            .map(|(i, c)| (format!("2026-01-{:02} 00:00:00", i + 1), *c))
            .collect();
        let rows_ref: Vec<(&str, &str)> = rows.iter().map(|(t, c)| (t.as_str(), *c)).collect();
        let conn = setup(&rows_ref);
        let symbol = Symbol::new("BTCUSDT").unwrap();

        let window = 3usize;
        let actual =
            realized_volatility(&conn, "binance", &symbol, Interval::OneDay, window as u32)
                .unwrap();
        let expected = reference_volatility(&closes_f64, window);

        assert_eq!(actual.len(), expected.len());
        for (a, e) in actual.iter().zip(expected.iter()) {
            match (a.realized_volatility, e) {
                (None, None) => {}
                (Some(av), Some(ev)) => assert!(
                    (av - ev).abs() < 1e-6,
                    "actual {av} vs reference {ev} differ by more than 1e-6"
                ),
                (a, e) => panic!("mismatch: sql={a:?} reference={e:?}"),
            }
        }
    }

    #[test]
    fn rejects_zero_window() {
        let conn = setup(&[("2026-01-01 00:00:00", "100")]);
        let symbol = Symbol::new("BTCUSDT").unwrap();
        assert!(realized_volatility(&conn, "binance", &symbol, Interval::OneDay, 0).is_err());
    }
}
