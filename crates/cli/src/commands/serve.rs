use anyhow::{Context, Result};

use ma_api::{build_app, ApiLimits, DbPool};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};

use crate::config::AppConfig;

use super::ServeArgs;

/// FR-5.x: run the HTTP API. Graceful shutdown on SIGINT/SIGTERM (FR-6.4)
/// is `axum`'s own `with_graceful_shutdown`, which stops accepting new
/// connections and waits for in-flight ones to finish.
pub async fn run(args: ServeArgs, config: &AppConfig) -> Result<()> {
    let pool = DbPool::new(&config.meta_db_path, config.api_db_pool_size)
        .context("opening API database pool")?;

    let exchange = BinanceSpot::new(BinanceConfig {
        base_url: config.binance_base_url.clone(),
        requests_per_second: config.requests_per_second,
        ..Default::default()
    })?;

    let limits = ApiLimits {
        max_date_range_days: config.api_max_date_range_days,
        default_pagination_limit: 1000,
        max_pagination_limit: 10_000,
    };

    let app = build_app(pool, exchange, limits).context("building API router")?;

    let addr = format!("0.0.0.0:{}", args.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .with_context(|| format!("binding {addr}"))?;
    tracing::info!(%addr, "API server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(crate::shutdown::signal())
        .await
        .context("serving HTTP")?;

    tracing::info!("API server shut down gracefully (FR-6.4)");
    Ok(())
}
