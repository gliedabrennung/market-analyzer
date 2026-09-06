use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::Deserialize;

/// Runtime configuration: defaults, overridden by a TOML file (`--config`),
/// overridden by `MA_*` environment variables (FR-6.1).
#[derive(Debug, Clone, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub data_dir: PathBuf,
    pub meta_db_path: PathBuf,
    pub binance_base_url: String,
    pub requests_per_second: u32,
    /// Maximum width, in days, of a `from`/`to` range the API will accept
    /// (FR-5.3) — also the default lookback when `from` is omitted.
    pub api_max_date_range_days: i64,
    /// Read-only DuckDB connections held open for the API's request pool
    /// (FR-5.4: queries run in `spawn_blocking`, never on the async
    /// executor thread).
    pub api_db_pool_size: usize,
    /// CORS `Access-Control-Allow-Origin` for the API (frontend-tz.md BE-2).
    /// Defaults to the Vite dev server; set to the deployed frontend origin
    /// in production.
    pub api_cors_origin: String,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            data_dir: PathBuf::from("data"),
            meta_db_path: PathBuf::from("data/meta.duckdb"),
            binance_base_url: "https://api.binance.com".to_string(),
            requests_per_second: 5,
            api_max_date_range_days: 366,
            api_db_pool_size: 4,
            api_cors_origin: "http://localhost:5173".to_string(),
        }
    }
}

impl AppConfig {
    pub fn load(config_path: Option<&Path>) -> Result<Self> {
        let mut cfg = match config_path {
            Some(path) => {
                let text = std::fs::read_to_string(path)
                    .with_context(|| format!("reading config file {}", path.display()))?;
                toml::from_str(&text)
                    .with_context(|| format!("parsing config file {}", path.display()))?
            }
            None => AppConfig::default(),
        };

        if let Ok(v) = std::env::var("MA_DATA_DIR") {
            cfg.data_dir = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("MA_META_DB_PATH") {
            cfg.meta_db_path = PathBuf::from(v);
        }
        if let Ok(v) = std::env::var("MA_BINANCE_BASE_URL") {
            cfg.binance_base_url = v;
        }
        if let Ok(v) = std::env::var("MA_REQUESTS_PER_SECOND") {
            cfg.requests_per_second = v
                .parse()
                .context("MA_REQUESTS_PER_SECOND must be a positive integer")?;
        }
        if let Ok(v) = std::env::var("MA_API_MAX_DATE_RANGE_DAYS") {
            cfg.api_max_date_range_days = v
                .parse()
                .context("MA_API_MAX_DATE_RANGE_DAYS must be a positive integer")?;
        }
        if let Ok(v) = std::env::var("MA_API_DB_POOL_SIZE") {
            cfg.api_db_pool_size = v
                .parse()
                .context("MA_API_DB_POOL_SIZE must be a positive integer")?;
        }
        if let Ok(v) = std::env::var("MA_API_CORS_ORIGIN") {
            cfg.api_cors_origin = v;
        }

        Ok(cfg)
    }
}
