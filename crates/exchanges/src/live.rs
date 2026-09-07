use std::str::FromStr;

use chrono::{DateTime, TimeZone, Utc};
use rust_decimal::Decimal;
use serde::Deserialize;

use ma_core::{rescale_checked, DepthUpdate, Interval, Kline, MarketEvent, Symbol, Trade};

use crate::error::ExchangeError;

const MONEY_SCALE: u32 = 8;

#[derive(Debug, Deserialize)]
pub struct CombinedEnvelope {
    #[allow(dead_code)]
    pub stream: String,
    pub data: serde_json::Value,
}

pub fn parse_market_event(
    data: serde_json::Value,
    exchange: &str,
    now: DateTime<Utc>,
) -> Result<Option<MarketEvent>, ExchangeError> {
    let event_type = data
        .get("e")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();

    match event_type.as_str() {
        "trade" => {
            let raw: RawTradeEvent = serde_json::from_value(data)?;
            Ok(Some(MarketEvent::Trade(raw.into_trade(exchange)?)))
        }
        "kline" => {
            let raw: RawKlineEvent = serde_json::from_value(data)?;
            Ok(Some(MarketEvent::Kline(
                raw.kline.into_kline(exchange, now)?,
            )))
        }
        "depthUpdate" => {
            let raw: RawDepthEvent = serde_json::from_value(data)?;
            Ok(Some(MarketEvent::DepthUpdate(
                raw.into_depth_update(exchange)?,
            )))
        }
        other => {
            tracing::debug!(
                event_type = other,
                "ignoring unrecognized stream event type"
            );
            Ok(None)
        }
    }
}

#[derive(Debug, Deserialize)]
struct RawTradeEvent {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "t")]
    trade_id: i64,
    #[serde(rename = "p")]
    price: String,
    #[serde(rename = "q")]
    qty: String,
    #[serde(rename = "T")]
    trade_time_ms: i64,
    #[serde(rename = "m")]
    is_buyer_maker: bool,
}

impl RawTradeEvent {
    fn into_trade(self, exchange: &str) -> Result<Trade, ExchangeError> {
        Ok(Trade {
            ts: ts_millis(self.trade_time_ms)?,
            symbol: Symbol::new(&self.symbol)?,
            exchange: exchange.to_string(),
            trade_id: self.trade_id,
            price: rescale_checked(dec(&self.price)?, MONEY_SCALE)?,
            qty: rescale_checked(dec(&self.qty)?, MONEY_SCALE)?,
            is_buyer_maker: self.is_buyer_maker,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawKlineEvent {
    #[serde(rename = "k")]
    kline: RawKlinePayload,
}

#[derive(Debug, Deserialize)]
struct RawKlinePayload {
    #[serde(rename = "t")]
    open_time_ms: i64,
    #[serde(rename = "T")]
    close_time_ms: i64,
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "i")]
    interval: String,
    #[serde(rename = "o")]
    open: String,
    #[serde(rename = "h")]
    high: String,
    #[serde(rename = "l")]
    low: String,
    #[serde(rename = "c")]
    close: String,
    #[serde(rename = "v")]
    volume: String,
    #[serde(rename = "q")]
    quote_volume: String,
    #[serde(rename = "n")]
    trades_count: i32,
    #[serde(rename = "x")]
    is_closed: bool,
    #[serde(rename = "V")]
    taker_buy_base: String,
}

impl RawKlinePayload {
    fn into_kline(self, exchange: &str, _now: DateTime<Utc>) -> Result<Kline, ExchangeError> {
        Ok(Kline {
            open_time: ts_millis(self.open_time_ms)?,
            close_time: ts_millis(self.close_time_ms)?,
            symbol: Symbol::new(&self.symbol)?,
            exchange: exchange.to_string(),
            interval: Interval::from_str(&self.interval)?,
            open: rescale_checked(dec(&self.open)?, MONEY_SCALE)?,
            high: rescale_checked(dec(&self.high)?, MONEY_SCALE)?,
            low: rescale_checked(dec(&self.low)?, MONEY_SCALE)?,
            close: rescale_checked(dec(&self.close)?, MONEY_SCALE)?,
            volume: rescale_checked(dec(&self.volume)?, MONEY_SCALE)?,
            quote_volume: rescale_checked(dec(&self.quote_volume)?, MONEY_SCALE)?,
            trades_count: self.trades_count,
            taker_buy_base: Some(rescale_checked(dec(&self.taker_buy_base)?, MONEY_SCALE)?),

            is_closed: self.is_closed,
        })
    }
}

#[derive(Debug, Deserialize)]
struct RawDepthEvent {
    #[serde(rename = "s")]
    symbol: String,
    #[serde(rename = "E")]
    event_time_ms: i64,
    #[serde(rename = "U")]
    first_update_id: i64,
    #[serde(rename = "u")]
    final_update_id: i64,
    #[serde(rename = "b")]
    bids: Vec<(String, String)>,
    #[serde(rename = "a")]
    asks: Vec<(String, String)>,
}

impl RawDepthEvent {
    fn into_depth_update(self, exchange: &str) -> Result<DepthUpdate, ExchangeError> {
        Ok(DepthUpdate {
            ts: ts_millis(self.event_time_ms)?,
            symbol: Symbol::new(&self.symbol)?,
            exchange: exchange.to_string(),
            first_update_id: self.first_update_id,
            final_update_id: self.final_update_id,
            bids: levels(self.bids)?,
            asks: levels(self.asks)?,
        })
    }
}

#[derive(Debug, Deserialize)]
pub struct RawAggTrade {
    #[serde(rename = "a")]
    pub agg_trade_id: i64,
    #[serde(rename = "p")]
    pub price: String,
    #[serde(rename = "q")]
    pub qty: String,
    #[serde(rename = "T")]
    pub trade_time_ms: i64,
    #[serde(rename = "m")]
    pub is_buyer_maker: bool,
}

impl RawAggTrade {
    pub fn into_trade(self, symbol: &Symbol, exchange: &str) -> Result<Trade, ExchangeError> {
        Ok(Trade {
            ts: ts_millis(self.trade_time_ms)?,
            symbol: symbol.clone(),
            exchange: exchange.to_string(),
            trade_id: self.agg_trade_id,
            price: rescale_checked(dec(&self.price)?, MONEY_SCALE)?,
            qty: rescale_checked(dec(&self.qty)?, MONEY_SCALE)?,
            is_buyer_maker: self.is_buyer_maker,
        })
    }
}

fn levels(raw: Vec<(String, String)>) -> Result<Vec<(Decimal, Decimal)>, ExchangeError> {
    raw.into_iter()
        .map(|(p, q)| Ok((dec(&p)?, dec(&q)?)))
        .collect()
}

fn ts_millis(ms: i64) -> Result<DateTime<Utc>, ExchangeError> {
    Utc.timestamp_millis_opt(ms)
        .single()
        .ok_or_else(|| ExchangeError::Parse(format!("timestamp {ms} out of range")))
}

fn dec(s: &str) -> Result<Decimal, ExchangeError> {
    Decimal::from_str(s).map_err(|e| ExchangeError::Parse(format!("invalid decimal '{s}': {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_trade_event() {
        let data = json!({
            "e": "trade", "E": 1_690_000_000_000i64, "s": "BTCUSDT", "t": 12345,
            "p": "50000.12345678", "q": "0.50000000", "T": 1_690_000_000_100i64,
            "m": true, "M": true
        });
        let event = parse_market_event(data, "binance", Utc::now())
            .unwrap()
            .unwrap();
        match event {
            MarketEvent::Trade(t) => {
                assert_eq!(t.symbol.as_str(), "BTCUSDT");
                assert_eq!(t.trade_id, 12345);
                assert!(t.is_buyer_maker);
            }
            other => panic!("expected Trade, got {other:?}"),
        }
    }

    #[test]
    fn parses_kline_event_open_and_closed() {
        let mut payload = json!({
            "e": "kline", "E": 1_690_000_000_000i64, "s": "BTCUSDT",
            "k": {
                "t": 1_690_000_000_000i64, "T": 1_690_000_059_999i64, "s": "BTCUSDT", "i": "1m",
                "f": 100, "L": 200, "o": "100.00000000", "c": "101.00000000",
                "h": "102.00000000", "l": "99.00000000", "v": "10.00000000",
                "n": 5, "x": false, "q": "1000.00000000", "V": "5.00000000",
                "Q": "500.00000000", "B": "0"
            }
        });
        let event = parse_market_event(payload.clone(), "binance", Utc::now())
            .unwrap()
            .unwrap();
        match event {
            MarketEvent::Kline(k) => assert!(!k.is_closed, "x=false must map to is_closed=false"),
            other => panic!("expected Kline, got {other:?}"),
        }

        payload["k"]["x"] = json!(true);
        let event = parse_market_event(payload, "binance", Utc::now())
            .unwrap()
            .unwrap();
        match event {
            MarketEvent::Kline(k) => assert!(k.is_closed, "x=true must map to is_closed=true"),
            other => panic!("expected Kline, got {other:?}"),
        }
    }

    #[test]
    fn parses_depth_event() {
        let data = json!({
            "e": "depthUpdate", "E": 1_690_000_000_000i64, "s": "BTCUSDT",
            "U": 157, "u": 160,
            "b": [["100.00000000", "1.00000000"]],
            "a": [["101.00000000", "2.00000000"]]
        });
        let event = parse_market_event(data, "binance", Utc::now())
            .unwrap()
            .unwrap();
        match event {
            MarketEvent::DepthUpdate(d) => {
                assert_eq!(d.first_update_id, 157);
                assert_eq!(d.final_update_id, 160);
                assert_eq!(d.bids.len(), 1);
                assert_eq!(d.asks.len(), 1);
            }
            other => panic!("expected DepthUpdate, got {other:?}"),
        }
    }

    #[test]
    fn unknown_event_type_is_skipped_not_an_error() {
        let data = json!({"e": "somethingNew"});
        let result = parse_market_event(data, "binance", Utc::now()).unwrap();
        assert!(result.is_none());
    }

    #[test]
    #[ignore]
    fn throughput_benchmark() {
        const N: usize = 500_000;
        let payload = json!({
            "e": "trade", "E": 1_690_000_000_000i64, "s": "BTCUSDT", "t": 12345,
            "p": "50000.12345678", "q": "0.50000000", "T": 1_690_000_000_100i64,
            "m": true, "M": true
        });
        let now = Utc::now();

        let start = std::time::Instant::now();
        for _ in 0..N {
            let event = parse_market_event(payload.clone(), "binance", now)
                .unwrap()
                .unwrap();
            std::hint::black_box(event);
        }
        let elapsed = start.elapsed();
        let per_sec = N as f64 / elapsed.as_secs_f64();
        println!(
            "parsed {N} events in {elapsed:?} = {per_sec:.0} events/sec (NFR-1.1 target: >= 10000)"
        );
    }

    #[test]
    fn agg_trade_maps_aggregate_id_as_trade_id() {
        let raw: RawAggTrade = serde_json::from_value(json!({
            "a": 26129, "p": "0.01633102", "q": "4.70443515",
            "f": 27781, "l": 27781, "T": 1_498_793_709_153i64, "m": true, "M": true
        }))
        .unwrap();
        let symbol = Symbol::new("BTCUSDT").unwrap();
        let trade = raw.into_trade(&symbol, "binance").unwrap();
        assert_eq!(trade.trade_id, 26129);
        assert_eq!(trade.price.to_string(), "0.01633102");
    }
}
