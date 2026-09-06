use anyhow::{Context, Result};
use chrono::Utc;

use ma_exchanges::binance::{BinanceConfig, BinanceSpot};
use ma_exchanges::ExchangeSource;
use ma_storage::{MetaStore, SymbolRecord};

use crate::config::AppConfig;

use super::SymbolsArgs;

/// FR-2.6: `symbols --refresh` resyncs the registry from the exchange's
/// instrument list; without `--refresh`, just prints what's registered.
pub async fn run(args: SymbolsArgs, config: &AppConfig) -> Result<()> {
    if args.refresh {
        let exchange = BinanceSpot::new(BinanceConfig {
            base_url: config.binance_base_url.clone(),
            requests_per_second: config.requests_per_second,
            ..Default::default()
        })?;
        let instruments = exchange
            .instruments()
            .await
            .context("fetching instruments from exchange")?;
        let records: Vec<SymbolRecord> = instruments
            .into_iter()
            .map(|i| SymbolRecord {
                exchange: exchange.id().to_string(),
                symbol: i.symbol.as_str().to_string(),
                base_asset: i.base_asset,
                quote_asset: i.quote_asset,
                status: i.status,
            })
            .collect();

        let meta = MetaStore::open_writable(&config.meta_db_path, &config.data_dir)
            .context("opening meta.duckdb for writing")?;
        let count = records.len();
        meta.replace_symbols(exchange.id(), &records, Utc::now())
            .context("writing symbol registry")?;
        println!("refreshed {count} symbols from {}", exchange.id());
    } else {
        let meta = MetaStore::open_read_only(&config.meta_db_path)
            .context("opening meta.duckdb for reading")?;
        for record in meta.list_symbols().context("listing symbols")? {
            println!(
                "{}\t{}\t{}/{}\t{}",
                record.exchange,
                record.symbol,
                record.base_asset,
                record.quote_asset,
                record.status
            );
        }
    }
    Ok(())
}
