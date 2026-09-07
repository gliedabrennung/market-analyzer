use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, TimeZone, Utc};
use futures::{SinkExt, StreamExt};
use serde_json::json;
use tokio::net::TcpListener;
use tokio_tungstenite::tungstenite::Message;

use ma_core::{Interval, MarketEvent, Symbol};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};
use ma_exchanges::{ExchangeSource, StreamKind};

fn trunc_ms(dt: DateTime<Utc>) -> DateTime<Utc> {
    Utc.timestamp_millis_opt(dt.timestamp_millis()).unwrap()
}

fn kline_message(symbol: &Symbol, open_time: DateTime<Utc>, is_closed: bool) -> String {
    let open_ms = open_time.timestamp_millis();
    let close_ms = open_ms + 59_999;
    json!({
        "stream": format!("{}@kline_1m", symbol.as_str().to_lowercase()),
        "data": {
            "e": "kline", "E": open_ms, "s": symbol.as_str(),
            "k": {
                "t": open_ms, "T": close_ms, "s": symbol.as_str(), "i": "1m",
                "f": 1, "L": 2, "o": "1.00000000", "c": "1.00000000",
                "h": "1.00000000", "l": "1.00000000", "v": "1.00000000",
                "n": 1, "x": is_closed, "q": "1.00000000", "V": "0.50000000",
                "Q": "0.50000000", "B": "0"
            }
        }
    })
    .to_string()
}

#[tokio::test]
#[ignore]
async fn forced_disconnect_triggers_gapfill_and_resumes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let symbol = Symbol::new("BTCUSDT").unwrap();
    let now = Utc::now();

    let pre_drop_open = trunc_ms(now - ChronoDuration::minutes(6));
    let gap_upper_bound = trunc_ms(now - ChronoDuration::minutes(2));
    let post_reconnect_open = trunc_ms(now - ChronoDuration::minutes(1));

    let server_symbol = symbol.clone();
    let server = tokio::spawn(async move {
        let (tcp, _) = listener.accept().await.unwrap();
        let mut ws = tokio_tungstenite::accept_async(tcp).await.unwrap();
        ws.send(Message::Text(kline_message(
            &server_symbol,
            pre_drop_open,
            true,
        )))
        .await
        .unwrap();
        drop(ws);

        let (tcp2, _) = listener.accept().await.unwrap();
        let mut ws2 = tokio_tungstenite::accept_async(tcp2).await.unwrap();
        ws2.send(Message::Text(kline_message(
            &server_symbol,
            post_reconnect_open,
            true,
        )))
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_secs(5)).await;
    });

    let client = BinanceSpot::new(BinanceConfig {
        ws_base_url: format!("ws://127.0.0.1:{port}"),
        ..Default::default()
    })
    .unwrap();

    let mut events = client
        .subscribe(
            std::slice::from_ref(&symbol),
            &[StreamKind::Kline(Interval::OneMinute)],
        )
        .await
        .unwrap();

    let mut klines = Vec::new();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(45);
    while tokio::time::Instant::now() < deadline {
        match tokio::time::timeout(Duration::from_secs(15), events.next()).await {
            Ok(Some(Ok(MarketEvent::Kline(k)))) => {
                let is_post_reconnect = k.open_time == post_reconnect_open;
                klines.push(k);
                if is_post_reconnect {
                    break;
                }
            }
            Ok(Some(Ok(_))) => {}
            Ok(Some(Err(e))) => panic!("unexpected stream error: {e}"),
            _ => break,
        }
    }

    server.abort();

    assert!(
        klines.iter().any(|k| k.open_time == pre_drop_open),
        "must have received the pre-drop synthetic event; got {klines:#?}"
    );
    assert!(
        klines
            .iter()
            .any(|k| k.open_time > pre_drop_open && k.open_time <= gap_upper_bound),
        "must have gap-filled at least one real candle from the downtime window (FR-1.5); got {klines:#?}"
    );
    assert!(
        klines.iter().any(|k| k.open_time == post_reconnect_open),
        "must have resumed live flow after reconnecting; got {klines:#?}"
    );
}
