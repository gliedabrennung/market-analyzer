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

        let incomplete = fetched_klines.iter().filter(|k| !k.is_closed).count();
        let klines: Vec<_> = fetched_klines.into_iter().filter(|k| k.is_closed).collect();

        let written = store
            .write_klines(&klines)
            .with_context(|| format!("writing klines for {symbol}"))?;

        if written > 0 {
            meta.ensure_views()
                .context("creating meta.duckdb views over the new Parquet files")?;
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
