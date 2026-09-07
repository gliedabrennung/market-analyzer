use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use futures::StreamExt;

use ma_core::{Interval, Kline, MarketEvent, Symbol, Trade};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};
use ma_exchanges::{ExchangeSource, StreamKind};
use ma_storage::{BatchBuffer, BatchConfig, KlineStore, MetaStore, TradeStore};

use crate::config::AppConfig;

use super::{Dataset, StreamArgs};

/// FR-1.4/FR-1.5: subscribe to Binance's combined live stream and write
/// events to the Parquet layout, batched per FR-2.2, with graceful
/// shutdown on SIGINT/SIGTERM (FR-6.4).
///
/// The CLI grammar (FR-4.1) has no `--interval` flag for `stream`; klines
/// are streamed at `1m`, the finest granularity, matching `backfill`'s
/// typical default.
pub async fn run(args: StreamArgs, config: &AppConfig) -> Result<()> {
    let symbols: Vec<Symbol> = args
        .symbols
        .iter()
        .map(|s| Symbol::new(s).map_err(|e| anyhow::anyhow!("--symbol '{s}': {e}")))
        .collect::<Result<_>>()?;

    let interval = Interval::OneMinute;
    let stream_kind = match args.dataset {
        Dataset::Klines => StreamKind::Kline(interval),
        Dataset::Trades => StreamKind::Trade,
    };

    let exchange = BinanceSpot::new(BinanceConfig {
        base_url: config.binance_base_url.clone(),
        requests_per_second: config.requests_per_second,
        ..Default::default()
    })?;

    let meta = MetaStore::open_writable(&config.meta_db_path, &config.data_dir)
        .context("opening meta.duckdb for writing")?;
    let kline_store = KlineStore::new(&config.data_dir)?;
    let trade_store = TradeStore::new(&config.data_dir)?;

    let event_stream = exchange
        .subscribe(&symbols, &[stream_kind])
        .await
        .context("subscribing to binance combined stream")?;
    tokio::pin!(event_stream);

    tracing::info!(symbols = ?args.symbols, dataset = ?args.dataset, "stream started");

    let mut kline_buf: BatchBuffer<Kline> = BatchBuffer::new(BatchConfig::default());
    let mut trade_buf: BatchBuffer<Trade> = BatchBuffer::new(BatchConfig::default());

    let mut tick = tokio::time::interval(Duration::from_secs(5));
    let shutdown = crate::shutdown::signal();
    tokio::pin!(shutdown);
    loop {
        tokio::select! {
            biased;

            _ = &mut shutdown => {
                tracing::info!("shutdown signal received, flushing buffers (FR-6.4)");
                flush_klines(&kline_store, &meta, &config.data_dir, exchange.id(), interval, &mut kline_buf)?;
                flush_trades(&trade_store, &meta, &config.data_dir, exchange.id(), &mut trade_buf)?;
                break;
            }

            _ = tick.tick() => {
                if kline_buf.should_flush() {
                    flush_klines(&kline_store, &meta, &config.data_dir, exchange.id(), interval, &mut kline_buf)?;
                }
                if trade_buf.should_flush() {
                    flush_trades(&trade_store, &meta, &config.data_dir, exchange.id(), &mut trade_buf)?;
                }
            }

            maybe_event = event_stream.next() => {
                match maybe_event {
                    // The live kline stream re-emits the same still-forming
                    // candle roughly once a second (same open_time, growing
                    // o/h/l/c/v) until it closes. Parquet files are
                    // immutable, so a partial snapshot written now could
                    // never be corrected later — the dedup key would just
                    // see `open_time` already on disk and skip the real,
                    // final update forever. Only persist once `is_closed`.
                    Some(Ok(MarketEvent::Kline(k))) if !k.is_closed => {}
                    Some(Ok(MarketEvent::Kline(k))) => {
                        kline_buf.push(k);
                        if kline_buf.should_flush() {
                            flush_klines(&kline_store, &meta, &config.data_dir, exchange.id(), interval, &mut kline_buf)?;
                        }
                    }
                    Some(Ok(MarketEvent::Trade(t))) => {
                        trade_buf.push(t);
                        if trade_buf.should_flush() {
                            flush_trades(&trade_store, &meta, &config.data_dir, exchange.id(), &mut trade_buf)?;
                        }
                    }
                    // No FR/data-model table stores order-book depth; the
                    // exchange abstraction can subscribe to it (FR-1.4) but
                    // `stream`'s two datasets are only trades/klines (FR-4.1).
                    Some(Ok(MarketEvent::DepthUpdate(_))) => {}
                    Some(Err(e)) => tracing::warn!(error = %e, "stream event error"),
                    None => {
                        tracing::warn!("event stream ended unexpectedly, shutting down");
                        flush_klines(&kline_store, &meta, &config.data_dir, exchange.id(), interval, &mut kline_buf)?;
                        flush_trades(&trade_store, &meta, &config.data_dir, exchange.id(), &mut trade_buf)?;
                        break;
                    }
                }
            }
        }
    }

    Ok(())
}

fn flush_klines(
    store: &KlineStore,
    meta: &MetaStore,
    data_dir: &Path,
    exchange_id: &str,
    interval: Interval,
    buf: &mut BatchBuffer<Kline>,
) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let batch = buf.drain();
    let fetched = batch.len();
    let written = store.write_klines(&batch).context("writing kline batch")?;
    if written > 0 {
        // See `backfill`: on a fresh data directory the view could not be
        // created when the store was opened, and only a writer can create
        // it — so a long-running `stream` is otherwise the one process that
        // fills the directory while leaving the API unable to see any of it.
        meta.refresh_views(data_dir)
            .context("refreshing meta.duckdb views over the new Parquet files")?;
    }

    let now = Utc::now();
    for (symbol, max_ts) in
        max_timestamp_per_symbol(batch.iter().map(|k| (k.symbol.as_str(), k.open_time)))
    {
        meta.upsert_collector_state(
            exchange_id,
            &symbol,
            "klines",
            Some(interval.as_str()),
            max_ts,
            now,
        )
        .with_context(|| format!("updating collector_state for {symbol}"))?;
    }
    tracing::info!(fetched, written, "flushed kline batch");
    Ok(())
}

fn flush_trades(
    store: &TradeStore,
    meta: &MetaStore,
    data_dir: &Path,
    exchange_id: &str,
    buf: &mut BatchBuffer<Trade>,
) -> Result<()> {
    if buf.is_empty() {
        return Ok(());
    }
    let batch = buf.drain();
    let fetched = batch.len();
    let written = store.write_trades(&batch).context("writing trade batch")?;
    if written > 0 {
        meta.refresh_views(data_dir)
            .context("refreshing meta.duckdb views over the new Parquet files")?;
    }

    let now = Utc::now();
    for (symbol, max_ts) in
        max_timestamp_per_symbol(batch.iter().map(|t| (t.symbol.as_str(), t.ts)))
    {
        meta.upsert_collector_state(exchange_id, &symbol, "trades", None, max_ts, now)
            .with_context(|| format!("updating collector_state for {symbol}"))?;
    }
    tracing::info!(fetched, written, "flushed trade batch");
    Ok(())
}

fn max_timestamp_per_symbol<'a>(
    rows: impl Iterator<Item = (&'a str, DateTime<Utc>)>,
) -> HashMap<String, DateTime<Utc>> {
    let mut out: HashMap<String, DateTime<Utc>> = HashMap::new();
    for (symbol, ts) in rows {
        out.entry(symbol.to_string())
            .and_modify(|max| {
                if ts > *max {
                    *max = ts;
                }
            })
            .or_insert(ts);
    }
    out
}
