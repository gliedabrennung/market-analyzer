use chrono::{DateTime, NaiveDateTime, Utc};
use duckdb::Connection;
use rust_decimal::Decimal;
use serde::Serialize;

use ma_analytics::rowutil::{decimal_col, timestamp_col};

/// One candle for `GET /ohlcv/{symbol}` (FR-5.1). Not an `ma_analytics`
/// type: none of the Этап 2 analytics functions is "just fetch raw
/// klines" — this is API-specific.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OhlcvRow {
    pub open_time: DateTime<Utc>,
    pub close_time: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub quote_volume: Decimal,
    pub trades_count: i32,
    pub is_closed: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn fetch_ohlcv(
    conn: &Connection,
    exchange: &str,
    symbol: &str,
    interval: &str,
    from: NaiveDateTime,
    to: NaiveDateTime,
    limit: i64,
    offset: i64,
) -> Result<Vec<OhlcvRow>, duckdb::Error> {
    let mut stmt = conn.prepare(include_str!("../sql/ohlcv.sql"))?;
    let mut rows = stmt.query(duckdb::params![
        exchange, symbol, interval, from, to, limit, offset
    ])?;

    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(OhlcvRow {
            open_time: timestamp_col(row, 0)?,
            close_time: timestamp_col(row, 1)?,
            open: decimal_col(row, 2).map_err(decimal_err)?,
            high: decimal_col(row, 3).map_err(decimal_err)?,
            low: decimal_col(row, 4).map_err(decimal_err)?,
            close: decimal_col(row, 5).map_err(decimal_err)?,
            volume: decimal_col(row, 6).map_err(decimal_err)?,
            quote_volume: decimal_col(row, 7).map_err(decimal_err)?,
            trades_count: row.get(8)?,
            is_closed: row.get(9)?,
        });
    }
    Ok(out)
}

/// `decimal_col` returns `ma_analytics::AnalyticsError` (it can fail on a
/// malformed string, not just a DuckDB error); this query reports through
/// `duckdb::Error` instead since callers already map that to `ApiError`,
/// so a parse failure is folded in as a generic DuckDB-shaped error.
fn decimal_err(e: ma_analytics::AnalyticsError) -> duckdb::Error {
    duckdb::Error::ToSqlConversionFailure(Box::new(e))
}
