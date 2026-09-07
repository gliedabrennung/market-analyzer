//! `WS /stream/{symbol}` opens a *separate* upstream exchange connection
//! per client, so the number of accepted clients has to be bounded — see
//! `ApiLimits::max_ws_connections`. Runs the real router in-process over a
//! temporary `meta.duckdb`; no exchange traffic happens, because the limit
//! is checked before the socket is upgraded (which is the point: after the
//! upgrade there is no HTTP status left to refuse with).

use chrono::Utc;

use ma_api::{build_app, ApiLimits, DbPool};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};
use ma_storage::{MetaStore, SymbolRecord};

fn limits(max_ws_connections: usize) -> ApiLimits {
    ApiLimits {
        max_date_range_days: 366,
        default_pagination_limit: 1000,
        max_pagination_limit: 10_000,
        max_correlation_symbols: 10,
        max_ws_connections,
    }
}

async fn serve_with_ws_limit(
    max_ws_connections: usize,
) -> (std::net::SocketAddr, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let meta_path = dir.path().join("meta.duckdb");
    {
        let meta = MetaStore::open_writable(&meta_path, dir.path()).expect("opening meta");
        meta.replace_symbols(
            "binance",
            &[SymbolRecord {
                exchange: "binance".to_string(),
                symbol: "BTCUSDT".to_string(),
                base_asset: "BTC".to_string(),
                quote_asset: "USDT".to_string(),
                status: "TRADING".to_string(),
            }],
            Utc::now(),
        )
        .expect("seeding symbol registry");
    }

    let pool = DbPool::new(&meta_path, 2).expect("opening db pool");
    let exchange = BinanceSpot::new(BinanceConfig::default()).expect("building exchange client");
    let app = build_app(
        pool,
        exchange,
        limits(max_ws_connections),
        "http://localhost:5173",
    )
    .expect("building app");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binding ephemeral port");
    let addr = listener.local_addr().expect("reading bound address");
    tokio::spawn(async move {
        let _ = axum::serve(listener, app).await;
    });
    (addr, dir)
}

#[tokio::test]
async fn refuses_a_ws_client_once_the_connection_budget_is_spent() {
    // Budget of zero stands in for "every slot already taken" without
    // needing a real upstream subscription to hold one.
    let (addr, _dir) = serve_with_ws_limit(0).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/stream/BTCUSDT?interval=1m"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("sending upgrade request");

    assert_eq!(response.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
    let body: serde_json::Value = response.json().await.expect("error envelope");
    assert_eq!(body["error"]["code"], "rate_limited");
}

/// CORS does not cover WebSocket handshakes, so a page on any other origin
/// could otherwise open a stream from a visitor's browser.
#[tokio::test]
async fn refuses_a_ws_client_from_a_foreign_browser_origin() {
    let (addr, _dir) = serve_with_ws_limit(4).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/stream/BTCUSDT"))
        .header("Origin", "https://evil.example")
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("sending upgrade request");

    assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);
    let body: serde_json::Value = response.json().await.expect("error envelope");
    assert_eq!(body["error"]["details"]["parameter"], "origin");
}

/// A non-browser client (curl, another service) sends no `Origin` at all
/// and must keep working — the check is about browser-initiated requests.
#[tokio::test]
async fn a_request_without_an_origin_header_passes_the_origin_check() {
    let (addr, _dir) = serve_with_ws_limit(0).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/stream/BTCUSDT"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("sending upgrade request");

    // Reaches the connection-budget check (spent here) rather than being
    // turned away as a cross-origin request.
    assert_eq!(response.status(), reqwest::StatusCode::TOO_MANY_REQUESTS);
}

#[tokio::test]
async fn an_unknown_symbol_is_still_rejected_before_any_budget_is_taken() {
    let (addr, _dir) = serve_with_ws_limit(1).await;

    let response = reqwest::Client::new()
        .get(format!("http://{addr}/stream/NOSUCHPAIR"))
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
        .send()
        .await
        .expect("sending upgrade request");

    assert_eq!(response.status(), reqwest::StatusCode::NOT_FOUND);
}
