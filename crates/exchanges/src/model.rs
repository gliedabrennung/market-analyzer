use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;

use ma_core::{rescale_checked, Interval, Kline, Symbol};

use crate::error::ExchangeError;

const MONEY_SCALE: u32 = 8;

pub fn parse_kline_row(
    row: &[serde_json::Value],
    symbol: &Symbol,
    interval: Interval,
    exchange: &str,
    now: DateTime<Utc>,
) -> Result<Kline, ExchangeError> {
    if row.len() < 11 {
        return Err(ExchangeError::Parse(format!(
            "kline row has {} fields, expected at least 11",
            row.len()
        )));
    }

    let open_time = ts_millis(&row[0])?;
    let open = rescale_checked(dec_str(&row[1])?, MONEY_SCALE)?;
    let high = rescale_checked(dec_str(&row[2])?, MONEY_SCALE)?;
    let low = rescale_checked(dec_str(&row[3])?, MONEY_SCALE)?;
    let close = rescale_checked(dec_str(&row[4])?, MONEY_SCALE)?;
    let volume = rescale_checked(dec_str(&row[5])?, MONEY_SCALE)?;
    let close_time = ts_millis(&row[6])?;
    let quote_volume = rescale_checked(dec_str(&row[7])?, MONEY_SCALE)?;
    let raw_trades_count = row[8]
        .as_i64()
        .ok_or_else(|| ExchangeError::Parse("trades_count is not an integer".to_string()))?;

    let trades_count = i32::try_from(raw_trades_count).map_err(|_| {
        ExchangeError::Parse(format!(
            "trades_count {raw_trades_count} does not fit in i32"
        ))
    })?;
    let taker_buy_base = Some(rescale_checked(dec_str(&row[9])?, MONEY_SCALE)?);

    Ok(Kline {
        open_time,
        close_time,
        symbol: symbol.clone(),
        exchange: exchange.to_string(),
        interval,
        open,
        high,
        low,
        close,
        volume,
        quote_volume,
        trades_count,
        taker_buy_base,
        is_closed: close_time <= now,
    })
}

fn ts_millis(v: &serde_json::Value) -> Result<DateTime<Utc>, ExchangeError> {
    let ms = v
        .as_i64()
        .ok_or_else(|| ExchangeError::Parse("timestamp field is not an integer".to_string()))?;
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or_else(|| ExchangeError::Parse(format!("timestamp {ms} out of range")))
}

fn dec_str(v: &serde_json::Value) -> Result<Decimal, ExchangeError> {
    let s = v
        .as_str()
        .ok_or_else(|| ExchangeError::Parse("expected a string-encoded decimal".to_string()))?;
    Decimal::from_str(s).map_err(|e| ExchangeError::Parse(format!("invalid decimal '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn sample_row() -> Vec<serde_json::Value> {
        vec![
            json!(1_690_000_000_000i64),
            json!("50000.12345678"),
            json!("50100.00000000"),
            json!("49900.00000000"),
            json!("50050.00000000"),
            json!("12.50000000"),
            json!(1_690_000_059_999i64),
            json!("625625.00000000"),
            json!(308),
            json!("6.25000000"),
            json!("312812.50000000"),
            json!("0"),
        ]
    }

    #[test]
    fn parses_valid_row() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let now = Utc::now();
        let k =
            parse_kline_row(&sample_row(), &symbol, Interval::OneMinute, "binance", now).unwrap();
        assert_eq!(k.symbol.as_str(), "BTCUSDT");
        assert_eq!(k.exchange, "binance");
        assert_eq!(k.trades_count, 308);
        assert_eq!(k.open.to_string(), "50000.12345678");
        assert_eq!(k.taker_buy_base.unwrap().to_string(), "6.25000000");
        assert!(k.is_closed);
    }

    #[test]
    fn open_kline_is_not_closed() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let far_future_close = Utc
            .timestamp_millis_opt(9_999_999_999_999)
            .single()
            .unwrap();
        let mut row = sample_row();
        row[6] = json!(far_future_close.timestamp_millis());
        let k = parse_kline_row(&row, &symbol, Interval::OneMinute, "binance", Utc::now()).unwrap();
        assert!(!k.is_closed);
    }

    #[test]
    fn rejects_short_row() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let row = vec![json!(1), json!("1")];
        assert!(
            parse_kline_row(&row, &symbol, Interval::OneMinute, "binance", Utc::now()).is_err()
        );
    }

    #[test]
    fn rejects_non_numeric_decimal() {
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let mut row = sample_row();
        row[1] = json!("not-a-number");
        assert!(
            parse_kline_row(&row, &symbol, Interval::OneMinute, "binance", Utc::now()).is_err()
        );
    }
}
