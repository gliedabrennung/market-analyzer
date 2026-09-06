mod backfill;
mod compact;
pub mod daterange;
mod query;
mod serve;
mod stream;
mod symbols;

use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use crate::config::AppConfig;

/// `market-analyzer` — сбор и аналитика биржевых данных (FR-4.1).
#[derive(Parser)]
#[command(name = "market-analyzer", version, about)]
pub struct Cli {
    /// Повысить уровень логирования (FR-4.3).
    #[arg(long, global = true)]
    pub verbose: bool,

    /// Файл конфигурации TOML (FR-4.3, FR-6.1).
    #[arg(long, global = true, value_name = "PATH")]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Историческая загрузка свечей (FR-1.2, FR-2.1..FR-2.3, Этап 1).
    Backfill(BackfillArgs),
    /// Live-поток в реальном времени (FR-1.4, Этап 3).
    Stream(StreamArgs),
    /// Аналитический запрос (FR-3.x, Этап 2).
    Query(QueryArgs),
    /// Склейка part-файлов партиции (FR-2.5, Этап 5).
    Compact(CompactArgs),
    /// Реестр символов (FR-2.6).
    Symbols(SymbolsArgs),
    /// HTTP API (FR-5.x, Этап 4).
    Serve(ServeArgs),
}

#[derive(clap::Args)]
pub struct BackfillArgs {
    #[arg(long = "symbol", required = true)]
    pub symbols: Vec<String>,
    #[arg(long)]
    pub interval: String,
    #[arg(long)]
    pub from: chrono::NaiveDate,
    #[arg(long)]
    pub to: Option<chrono::NaiveDate>,
}

#[derive(clap::Args)]
pub struct StreamArgs {
    #[arg(long = "symbol", required = true)]
    pub symbols: Vec<String>,
    #[arg(long, value_enum, default_value = "klines")]
    pub dataset: Dataset,
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Dataset {
    Trades,
    Klines,
}

#[derive(clap::Args)]
pub struct QueryArgs {
    #[arg(long = "name")]
    pub name: String,
    #[arg(long = "symbol")]
    pub symbol: String,
    #[arg(long = "param", value_parser = parse_key_val)]
    pub params: Vec<(String, String)>,
    #[arg(long, value_enum, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Clone, clap::ValueEnum)]
pub enum OutputFormat {
    Table,
    Json,
    Csv,
}

fn parse_key_val(s: &str) -> Result<(String, String), String> {
    let (k, v) = s
        .split_once('=')
        .ok_or_else(|| format!("expected key=value, got '{s}'"))?;
    Ok((k.to_string(), v.to_string()))
}

#[derive(clap::Args)]
pub struct CompactArgs {
    #[arg(long)]
    pub symbol: Option<String>,
    #[arg(long)]
    pub date: Option<chrono::NaiveDate>,
}

#[derive(clap::Args)]
pub struct SymbolsArgs {
    #[arg(long)]
    pub refresh: bool,
}

#[derive(clap::Args)]
pub struct ServeArgs {
    #[arg(long, default_value_t = 8080)]
    pub port: u16,
}

pub async fn run(cli: Cli) -> Result<()> {
    crate::logging::init(cli.verbose);

    let config = AppConfig::load(cli.config.as_deref())?;
    ma_storage::cleanup_incomplete_writes(&config.data_dir)
        .context("cleaning up incomplete writes from a previous run")?;

    match cli.command {
        Command::Backfill(args) => backfill::run(args, &config).await,
        Command::Stream(args) => stream::run(args, &config).await,
        Command::Query(args) => query::run(args, &config),
        Command::Compact(args) => compact::run(args, &config),
        Command::Symbols(args) => symbols::run(args, &config).await,
        Command::Serve(args) => serve::run(args, &config).await,
    }
}
