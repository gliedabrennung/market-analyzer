use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures::stream::BoxStream;
use futures::{SinkExt, Stream, StreamExt};
use reqwest::StatusCode;
use serde::Deserialize;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;

use ma_core::{Interval, Kline, MarketEvent, Symbol, Trade};

use crate::error::ExchangeError;
use crate::live::{parse_market_event, CombinedEnvelope, RawAggTrade};
use crate::model::parse_kline_row;
use crate::ratelimit::{backoff_delay, build_limiter, parse_retry_after, BackoffConfig, Limiter};
use crate::{ExchangeSource, Instrument, StreamKind, TimeRange};

const EXCHANGE_ID: &str = "binance";
/// Binance's own cap on `limit` for `/api/v3/klines` and `/api/v3/aggTrades`.
const REST_PAGE_LIMIT: u32 = 1000;

/// Binance Spot connection settings.
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    pub base_url: String,
    pub ws_base_url: String,
    pub requests_per_second: u32,
    pub backoff: BackoffConfig,
}

impl Default for BinanceConfig {
    fn default() -> Self {
        Self {
            base_url: "https://api.binance.com".to_string(),
            ws_base_url: "wss://stream.binance.com:9443".to_string(),
            requests_per_second: 5,
            backoff: BackoffConfig::default(),
        }
    }
}

struct Inner {
    http: reqwest::Client,
    base_url: String,
    ws_base_url: String,
    limiter: Limiter,
    backoff: BackoffConfig,
}

/// `ExchangeSource` implementation for Binance Spot (FR-1.1).
///
/// Cheap to clone (an `Arc` handle): `subscribe()` needs to hand back a
/// `'static` stream that keeps its own REST-calling capability for the
/// mandatory post-reconnect gap-fill (FR-1.5), without borrowing from the
/// caller.
#[derive(Clone)]
pub struct BinanceSpot {
    inner: Arc<Inner>,
}

impl BinanceSpot {
    /// Builds a client from `config`. Fails only if the HTTP client or rate
    /// limiter can't be constructed — no network I/O happens here.
    pub fn new(config: BinanceConfig) -> Result<Self, ExchangeError> {
        let http = reqwest::Client::builder()
            .user_agent(concat!("market-analyzer/", env!("CARGO_PKG_VERSION")))
            .build()
            .map_err(ExchangeError::Http)?;
        let limiter = build_limiter(config.requests_per_second)?;
        Ok(Self {
            inner: Arc::new(Inner {
                http,
                base_url: config.base_url,
                ws_base_url: config.ws_base_url,
                limiter,
                backoff: config.backoff,
            }),
        })
    }

    /// GET `path` with query params, honoring the rate limiter and the
    /// FR-1.3 backoff rules for HTTP 429/418.
    async fn get_with_retry(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<reqwest::Response, ExchangeError> {
        let url = format!("{}{}", self.inner.base_url, path);
        let mut attempt = 0u32;
        loop {
            attempt += 1;
            self.inner.limiter.until_ready().await;
            let resp = self.inner.http.get(&url).query(query).send().await?;
            match resp.status() {
                StatusCode::OK => return Ok(resp),
                StatusCode::TOO_MANY_REQUESTS => {
                    if attempt >= self.inner.backoff.max_attempts {
                        return Err(ExchangeError::RateLimited { attempts: attempt });
                    }
                    let delay = parse_retry_after(resp.headers())
                        .unwrap_or_else(|| backoff_delay(&self.inner.backoff, attempt));
                    tracing::warn!(
                        attempt,
                        delay_ms = delay.as_millis() as u64,
                        "binance HTTP 429, backing off"
                    );
                    tokio::time::sleep(delay).await;
                }
                StatusCode::IM_A_TEAPOT => {
                    let retry_after =
                        parse_retry_after(resp.headers()).unwrap_or(self.inner.backoff.max);
                    tracing::warn!(
                        retry_after_secs = retry_after.as_secs(),
                        "binance HTTP 418, IP temporarily banned"
                    );
                    if attempt >= self.inner.backoff.max_attempts {
                        return Err(ExchangeError::Banned {
                            retry_after_secs: retry_after.as_secs(),
                        });
                    }
                    tokio::time::sleep(retry_after).await;
                }
                status => {
                    let body = resp.text().await.unwrap_or_default();
                    return Err(ExchangeError::UnexpectedStatus {
                        status: status.as_u16(),
                        body,
                    });
                }
            }
        }
    }

    async fn fetch_klines_page(
        &self,
        symbol: &Symbol,
        interval: Interval,
        start_ms: i64,
        end_ms: i64,
        limit: u32,
    ) -> Result<Vec<Kline>, ExchangeError> {
        let query = [
            ("symbol", symbol.as_str().to_string()),
            ("interval", interval.as_str().to_string()),
            ("startTime", start_ms.to_string()),
            ("endTime", end_ms.to_string()),
            ("limit", limit.to_string()),
        ];
        let resp = self.get_with_retry("/api/v3/klines", &query).await?;
        let rows: Vec<Vec<serde_json::Value>> = resp.json().await?;
        let now = Utc::now();
        rows.iter()
            .map(|row| parse_kline_row(row, symbol, interval, EXCHANGE_ID, now))
            .collect()
    }

    /// Historical aggregated trades for `[from, to]`, used only for the
    /// mandatory post-reconnect gap-fill (FR-1.5). Not part of
    /// `ExchangeSource`: Binance has no time-ranged REST endpoint for
    /// individual raw trades, only aggregated ones — see
    /// `live::RawAggTrade` for the id-space caveat this implies.
    pub async fn agg_trades(
        &self,
        symbol: &Symbol,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
    ) -> Result<Vec<Trade>, ExchangeError> {
        let mut out = Vec::new();
        let mut cursor = from.timestamp_millis();
        let end_ms = to.timestamp_millis();
        while cursor <= end_ms {
            let query = [
                ("symbol", symbol.as_str().to_string()),
                ("startTime", cursor.to_string()),
                ("endTime", end_ms.to_string()),
                ("limit", REST_PAGE_LIMIT.to_string()),
            ];
            let resp = self.get_with_retry("/api/v3/aggTrades", &query).await?;
            let rows: Vec<RawAggTrade> = resp.json().await?;
            let page_len = rows.len();
            if page_len == 0 {
                break;
            }
            let last_ts = rows.last().map(|r| r.trade_time_ms).unwrap_or(cursor);
            for r in rows {
                out.push(r.into_trade(symbol, EXCHANGE_ID)?);
            }
            cursor = last_ts + 1;
            if (page_len as u32) < REST_PAGE_LIMIT {
                break;
            }
        }
        Ok(out)
    }

    fn stream_names(symbols: &[Symbol], streams: &[StreamKind]) -> Vec<String> {
        let mut names = Vec::with_capacity(symbols.len() * streams.len());
        for symbol in symbols {
            let lower = symbol.as_str().to_lowercase();
            for kind in streams {
                let suffix = match kind {
                    StreamKind::Trade => "trade".to_string(),
                    StreamKind::Kline(interval) => format!("kline_{}", interval.as_str()),
                    StreamKind::Depth => "depth".to_string(),
                };
                names.push(format!("{lower}@{suffix}"));
            }
        }
        names
    }
}

#[async_trait]
impl ExchangeSource for BinanceSpot {
    fn id(&self) -> &'static str {
        EXCHANGE_ID
    }

    async fn instruments(&self) -> Result<Vec<Instrument>, ExchangeError> {
        let resp = self.get_with_retry("/api/v3/exchangeInfo", &[]).await?;
        let raw: RawExchangeInfo = resp.json().await?;
        Ok(raw
            .symbols
            .into_iter()
            .filter_map(|s| match Symbol::new(&s.symbol) {
                Ok(symbol) => Some(Instrument {
                    symbol,
                    base_asset: s.base_asset,
                    quote_asset: s.quote_asset,
                    status: s.status,
                }),
                Err(e) => {
                    tracing::warn!(symbol = %s.symbol, error = %e, "skipping symbol with unexpected format");
                    None
                }
            })
            .collect())
    }

    async fn klines(
        &self,
        symbol: &Symbol,
        interval: Interval,
        range: TimeRange,
    ) -> Result<Vec<Kline>, ExchangeError> {
        let mut out = Vec::new();
        let mut cursor = range.from.timestamp_millis();
        let end_ms = range.to.timestamp_millis();
        while cursor <= end_ms {
            let page = self
                .fetch_klines_page(symbol, interval, cursor, end_ms, REST_PAGE_LIMIT)
                .await?;
            let page_len = page.len();
            if page_len == 0 {
                break;
            }
            cursor = page[page_len - 1].open_time.timestamp_millis() + 1;
            out.extend(page);
            if (page_len as u32) < REST_PAGE_LIMIT {
                break;
            }
        }
        Ok(out)
    }

    async fn subscribe(
        &self,
        symbols: &[Symbol],
        streams: &[StreamKind],
    ) -> Result<BoxStream<'static, Result<MarketEvent, ExchangeError>>, ExchangeError> {
        if symbols.is_empty() || streams.is_empty() {
            return Err(ExchangeError::Config(
                "subscribe needs at least one symbol and one stream kind".to_string(),
            ));
        }
        let client = self.clone();
        let symbols = symbols.to_vec();
        let streams = streams.to_vec();
        Ok(Box::pin(live_stream(client, symbols, streams)))
    }
}

/// Combined-stream connection with reconnect + mandatory REST gap-fill
/// (FR-1.4, FR-1.5). Never terminates on its own — errors are logged and
/// recovered from — so the item type is always `Ok`; it stops only when the
/// consumer drops the stream.
fn live_stream(
    client: BinanceSpot,
    symbols: Vec<Symbol>,
    streams: Vec<StreamKind>,
) -> impl Stream<Item = Result<MarketEvent, ExchangeError>> {
    async_stream::stream! {
        let names = BinanceSpot::stream_names(&symbols, &streams);
        let url = format!("{}/stream?streams={}", client.inner.ws_base_url, names.join("/"));

        let mut last_event_time: Option<DateTime<Utc>> = None;
        let mut attempt: u32 = 0;

        loop {
            match connect_async(url.as_str()).await {
                Ok((ws_stream, _response)) => {
                    attempt = 0;
                    tracing::info!(%url, "connected to binance combined stream");
                    let (mut write, mut read) = ws_stream.split();
                    loop {
                        match read.next().await {
                            Some(Ok(Message::Text(text))) => {
                                match serde_json::from_str::<CombinedEnvelope>(&text) {
                                    Ok(envelope) => {
                                        let now = Utc::now();
                                        match parse_market_event(envelope.data, EXCHANGE_ID, now) {
                                            Ok(Some(event)) => {
                                                last_event_time = Some(event_time(&event));
                                                yield Ok(event);
                                            }
                                            Ok(None) => {}
                                            Err(e) => {
                                                tracing::warn!(error = %e, "skipping unparseable market event");
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::warn!(error = %e, "skipping unparseable stream envelope");
                                    }
                                }
                            }
                            Some(Ok(Message::Ping(payload))) => {
                                if let Err(e) = write.send(Message::Pong(payload)).await {
                                    tracing::warn!(error = %e, "failed to send websocket pong");
                                }
                            }
                            Some(Ok(Message::Close(frame))) => {
                                tracing::warn!(?frame, "binance closed the websocket");
                                break;
                            }
                            Some(Ok(_)) => {}
                            Some(Err(e)) => {
                                tracing::warn!(error = %e, "websocket read error");
                                break;
                            }
                            None => {
                                tracing::warn!("websocket stream ended");
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(error = %e, %url, "failed to connect to binance websocket");
                }
            }

            attempt += 1;
            let reconnect_at = Utc::now();
            if let Some(gap_from) = last_event_time {
                tracing::info!(
                    from = %gap_from,
                    to = %reconnect_at,
                    "reconnect gap detected, backfilling via REST (FR-1.5)"
                );
                for kind in &streams {
                    match kind {
                        StreamKind::Kline(interval) => {
                            for symbol in &symbols {
                                let range = TimeRange { from: gap_from, to: reconnect_at };
                                match client.klines(symbol, *interval, range).await {
                                    Ok(klines) => {
                                        let rows = klines.len();
                                        for k in klines {
                                            yield Ok(MarketEvent::Kline(k));
                                        }
                                        tracing::info!(
                                            symbol = %symbol, interval = %interval, rows,
                                            from = %gap_from, to = %reconnect_at,
                                            "kline gap-fill complete"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(symbol = %symbol, error = %e, "kline gap-fill failed");
                                    }
                                }
                            }
                        }
                        StreamKind::Trade => {
                            for symbol in &symbols {
                                match client.agg_trades(symbol, gap_from, reconnect_at).await {
                                    Ok(trades) => {
                                        let rows = trades.len();
                                        for t in trades {
                                            yield Ok(MarketEvent::Trade(t));
                                        }
                                        tracing::info!(
                                            symbol = %symbol, rows,
                                            from = %gap_from, to = %reconnect_at,
                                            "trade gap-fill complete"
                                        );
                                    }
                                    Err(e) => {
                                        tracing::warn!(symbol = %symbol, error = %e, "trade gap-fill failed");
                                    }
                                }
                            }
                        }
                        StreamKind::Depth => {
                            tracing::debug!("no historical REST source for order book depth, skipping gap-fill");
                        }
                    }
                }
                last_event_time = Some(reconnect_at);
            }

            let delay = backoff_delay(&client.inner.backoff, attempt);
            tracing::warn!(
                attempt,
                delay_ms = delay.as_millis() as u64,
                "reconnecting to binance websocket after delay"
            );
            tokio::time::sleep(delay).await;
        }
    }
}

fn event_time(event: &MarketEvent) -> DateTime<Utc> {
    match event {
        MarketEvent::Trade(t) => t.ts,
        MarketEvent::Kline(k) => k.close_time,
        MarketEvent::DepthUpdate(d) => d.ts,
    }
}

#[derive(Debug, Deserialize)]
struct RawExchangeInfo {
    symbols: Vec<RawSymbol>,
}

#[derive(Debug, Deserialize)]
struct RawSymbol {
    symbol: String,
    #[serde(rename = "baseAsset")]
    base_asset: String,
    #[serde(rename = "quoteAsset")]
    quote_asset: String,
    status: String,
}
