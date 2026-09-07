use std::str::FromStr;

use anyhow::{Context, Result};
use chrono::Utc;

use ma_core::{Interval, Symbol};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};
use ma_exchanges::{ExchangeSource, TimeRange};
use ma_storage::{KlineStore, MetaStore};

use crate::config::AppConfig;

use super::daterange::day_range_utc;
use super::BackfillArgs;

/// FR-1.2 / FR-2.1..FR-2.3: fetch historical klines for each symbol and
/// write them to the Parquet layout, deduplicated.
pub async fn run(args: BackfillArgs, config: &AppConfig) -> Result<()> {
    let interval =
        Interval::from_str(&args.interval).map_err(|e| anyhow::anyhow!("--interval: {e}"))?;

    let (from_utc, to_inclusive_utc) = day_range_utc(args.from, args.to)?;

    let exchange = BinanceSpot::new(BinanceConfig {
        base_url: config.binance_base_url.clone(),
        requests_per_second: config.requests_per_second,
        ..Default::default()
    })?;

    let meta = MetaStore::open_writable(&config.meta_db_path, &config.data_dir)
        .context("opening meta.duckdb for writing")?;
    let store = KlineStore::new(&config.data_dir)?;

    for raw_symbol in &args.symbols {
        let symbol =
            Symbol::new(raw_symbol).map_err(|e| anyhow::anyhow!("--symbol '{raw_symbol}': {e}"))?;

        tracing::info!(
            symbol = %symbol,
            interval = %interval,
            from = %from_utc,
            to = %to_inclusive_utc,
            "backfill start"
        );

        let fetched_klines = exchange
            .klines(
                &symbol,
                interval,
                TimeRange {
                    from: from_utc,
                    to: to_inclusive_utc,
                },
            )
            .await
            .with_context(|| format!("fetching klines for {symbol}"))?;

        let fetched = fetched_klines.len();
        // Binance's REST /api/v3/klines returns the still-forming current
        // candle when the requested range includes "now" (e.g. `--to
        // <today>`). Parquet files are immutable and the dedup key is
        // (exchange, interval, open_time), so persisting that partial
        // snapshot would permanently freeze it — no later backfill or
        // stream run could ever correct it, since the dedup check only
        // looks at open_time, not is_closed. Mirrors the same guard in
        // `stream.rs`.
        let incomplete = fetched_klines.iter().filter(|k| !k.is_closed).count();
        let klines: Vec<_> = fetched_klines.into_iter().filter(|k| k.is_closed).collect();

        let written = store
            .write_klines(&klines)
            .with_context(|| format!("writing klines for {symbol}"))?;

        if written > 0 {
            // On a fresh data directory there were no Parquet files when
            // `open_writable` ran, so the `klines` view could not be created
            // then — and `serve` (read-only) can never create it. Without
            // this, the first backfill's data stays invisible to the API
            // until some unrelated later write reopens the store.
            meta.refresh_views(&config.data_dir)
                .context("refreshing meta.duckdb views over the new Parquet files")?;
        }

        if let Some(max_open) = klines.iter().map(|k| k.open_time).max() {
            meta.upsert_collector_state(
                exchange.id(),
                symbol.as_str(),
                "klines",
                Some(interval.as_str()),
                max_open,
                Utc::now(),
            )
            .with_context(|| format!("updating collector_state for {symbol}"))?;
        }

        println!(
            "{symbol} {interval}: fetched {fetched}, written {written}, skipped duplicates {}, skipped incomplete {incomplete}",
            fetched.saturating_sub(written).saturating_sub(incomplete)
        );
    }

    Ok(())
}
