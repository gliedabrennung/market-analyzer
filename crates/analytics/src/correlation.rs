use duckdb::Connection;
use serde::Serialize;

use ma_core::{Interval, Symbol};

use crate::error::AnalyticsError;
use crate::rowutil::placeholders;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CorrelationPair {
    pub symbol_a: String,
    pub symbol_b: String,

    pub correlation: Option<f64>,
}

pub fn correlation_matrix(
    conn: &Connection,
    exchange: &str,
    symbols: &[Symbol],
    interval: Interval,
) -> Result<Vec<CorrelationPair>, AnalyticsError> {
    if symbols.len() < 2 {
        return Err(AnalyticsError::InvalidParam {
            name: "symbols",
            reason: "need at least 2 symbols to correlate".to_string(),
        });
    }

    let sql = include_str!("../sql/correlation.sql")
        .replace("__SYMBOL_PLACEHOLDERS__", &placeholders(symbols.len()));
    let mut stmt = conn.prepare(&sql)?;

    let interval_str = interval.as_str();
    let symbol_strs: Vec<&str> = symbols.iter().map(Symbol::as_str).collect();
    let params: Vec<&dyn duckdb::ToSql> = std::iter::once(&exchange as &dyn duckdb::ToSql)
        .chain(std::iter::once(&interval_str as &dyn duckdb::ToSql))
        .chain(symbol_strs.iter().map(|s| s as &dyn duckdb::ToSql))
        .collect();
    let mut rows = stmt.query(duckdb::params_from_iter(params))?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(CorrelationPair {
            symbol_a: row.get(0)?,
            symbol_b: row.get(1)?,
            correlation: row.get::<_, Option<f64>>(2)?,
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
                 VALUES (?, ?, ?, 'binance', '1d',
                         CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)), CAST(? AS DECIMAL(18,8)),
                         CAST('1' AS DECIMAL(28,8)), CAST('1' AS DECIMAL(28,8)), 1, true)",
            )
            .unwrap();

        let btc = ["100", "200", "100", "400"];
        let eth = ["10", "20", "10", "40"];
        let sol = ["4", "2", "4", "1"];
        for (i, ((b, e), s)) in btc.iter().zip(eth.iter()).zip(sol.iter()).enumerate() {
            let ts = format!("2026-01-{:02} 00:00:00", i + 1);
            for (symbol, price) in [("BTCUSDT", b), ("ETHUSDT", e), ("SOLUSDT", s)] {
                stmt.execute(duckdb::params![ts, ts, symbol, price, price, price, price])
                    .unwrap();
            }
        }
        conn
    }

    #[test]
    fn perfectly_correlated_and_anti_correlated_pairs() {
        let conn = setup();
        let symbols = vec![
            Symbol::new("BTCUSDT").unwrap(),
            Symbol::new("ETHUSDT").unwrap(),
            Symbol::new("SOLUSDT").unwrap(),
        ];
        let pairs = correlation_matrix(&conn, "binance", &symbols, Interval::OneDay).unwrap();
        assert_eq!(pairs.len(), 3);

        let find = |a: &str, b: &str| {
            pairs
                .iter()
                .find(|p| p.symbol_a == a && p.symbol_b == b)
                .unwrap()
        };
        assert!((find("BTCUSDT", "ETHUSDT").correlation.unwrap() - 1.0).abs() < 1e-6);
        assert!((find("BTCUSDT", "SOLUSDT").correlation.unwrap() - (-1.0)).abs() < 1e-6);
        assert!((find("ETHUSDT", "SOLUSDT").correlation.unwrap() - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn rejects_fewer_than_two_symbols() {
        let conn = setup();
        let symbols = vec![Symbol::new("BTCUSDT").unwrap()];
        assert!(correlation_matrix(&conn, "binance", &symbols, Interval::OneDay).is_err());
    }
}
