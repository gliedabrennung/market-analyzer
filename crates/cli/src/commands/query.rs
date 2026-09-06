use std::collections::HashMap;
use std::str::FromStr;

use anyhow::{bail, Context, Result};
use chrono::NaiveDate;

use ma_analytics::{
    correlation_matrix, cross_spread, order_flow_imbalance, realized_volatility, resample_ohlcv,
    volume_anomalies, vwap,
};
use ma_core::{Interval, Symbol};
use ma_storage::MetaStore;

use crate::config::AppConfig;
use crate::output::print_rows;

use super::daterange::day_range_utc_exclusive_end;
use super::QueryArgs;

/// FR-3.1..FR-3.7 dispatch: run the named analytics query and print its
/// rows. See each `--name` arm below for the `--param` keys it reads.
pub fn run(args: QueryArgs, config: &AppConfig) -> Result<()> {
    let params: HashMap<String, String> = args.params.into_iter().collect();
    let symbol = Symbol::new(&args.symbol).map_err(|e| anyhow::anyhow!("--symbol: {e}"))?;

    let meta = MetaStore::open_read_only(&config.meta_db_path)
        .context("opening meta.duckdb for reading")?;
    let conn = meta.connection();

    match args.name.as_str() {
        "resample" => {
            let exchange = exchange_param(&params);
            let bucket_seconds: i64 = param_or(&params, "bucket_seconds", 300)?;
            let (from, to) = date_range_param(&params)?;
            let rows = resample_ohlcv(conn, &exchange, &symbol, bucket_seconds, from, to)?;
            print_rows(&rows, args.format)?;
        }
        "vwap" => {
            let exchange = exchange_param(&params);
            let interval = interval_param(&params)?;
            let window: u32 = param_or(&params, "window", 20)?;
            let rows = vwap(conn, &exchange, &symbol, interval, window)?;
            print_rows(&rows, args.format)?;
        }
        "volatility" => {
            let exchange = exchange_param(&params);
            let interval = interval_param(&params)?;
            let window: u32 = param_or(&params, "window", 20)?;
            let rows = realized_volatility(conn, &exchange, &symbol, interval, window)?;
            print_rows(&rows, args.format)?;
        }
        "anomalies" => {
            let exchange = exchange_param(&params);
            let interval = interval_param(&params)?;
            let window: u32 = param_or(&params, "window", 100)?;
            let threshold: f64 = param_or(&params, "threshold", 3.0)?;
            let rows = volume_anomalies(conn, &exchange, &symbol, interval, window, threshold)?;
            print_rows(&rows, args.format)?;
        }
        "ofi" => {
            let exchange = exchange_param(&params);
            let bucket_seconds: i64 = param_or(&params, "bucket_seconds", 60)?;
            let (from, to) = date_range_param(&params)?;
            let rows = order_flow_imbalance(conn, &exchange, &symbol, bucket_seconds, from, to)?;
            print_rows(&rows, args.format)?;
        }
        "correlation" => {
            let exchange = exchange_param(&params);
            let interval = interval_param(&params)?;
            let symbols_str = params
                .get("symbols")
                .cloned()
                .unwrap_or_else(|| args.symbol.clone());
            let symbols: Vec<Symbol> = symbols_str
                .split(',')
                .map(|s| Symbol::new(s.trim()).map_err(|e| anyhow::anyhow!("--param symbols: {e}")))
                .collect::<Result<_>>()?;
            let rows = correlation_matrix(conn, &exchange, &symbols, interval)?;
            print_rows(&rows, args.format)?;
        }
        "spread" => {
            let interval = interval_param(&params)?;
            let exchange_a = exchange_param(&params);
            let other_symbol_str = params
                .get("other_symbol")
                .context("query --name spread needs --param other_symbol=<SYM>")?;
            let other_symbol = Symbol::new(other_symbol_str)
                .map_err(|e| anyhow::anyhow!("--param other_symbol: {e}"))?;
            let other_exchange = params
                .get("other_exchange")
                .cloned()
                .unwrap_or_else(|| exchange_a.clone());
            let rows = cross_spread(
                conn,
                &exchange_a,
                &symbol,
                interval,
                &other_exchange,
                &other_symbol,
                interval,
            )?;
            print_rows(&rows, args.format)?;
        }
        other => bail!(
            "unknown --name '{other}': expected one of resample, vwap, volatility, anomalies, ofi, correlation, spread"
        ),
    }

    Ok(())
}

fn param_or<T: FromStr>(params: &HashMap<String, String>, key: &str, default: T) -> Result<T>
where
    T::Err: std::fmt::Display,
{
    match params.get(key) {
        Some(v) => v.parse().map_err(|e| anyhow::anyhow!("--param {key}: {e}")),
        None => Ok(default),
    }
}

fn exchange_param(params: &HashMap<String, String>) -> String {
    params
        .get("exchange")
        .cloned()
        .unwrap_or_else(|| "binance".to_string())
}

fn interval_param(params: &HashMap<String, String>) -> Result<Interval> {
    let raw = params.get("interval").map(String::as_str).unwrap_or("1m");
    Interval::from_str(raw).map_err(|e| anyhow::anyhow!("--param interval: {e}"))
}

fn date_range_param(
    params: &HashMap<String, String>,
) -> Result<(chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>)> {
    let from = params
        .get("from")
        .context("this query needs --param from=<YYYY-MM-DD>")?;
    let from = NaiveDate::from_str(from).map_err(|e| anyhow::anyhow!("--param from: {e}"))?;
    let to = params
        .get("to")
        .map(|s| NaiveDate::from_str(s).map_err(|e| anyhow::anyhow!("--param to: {e}")))
        .transpose()?;
    day_range_utc_exclusive_end(from, to)
}
