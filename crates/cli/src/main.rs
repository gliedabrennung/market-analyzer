mod commands;
mod config;
mod logging;
mod output;
mod shutdown;

use anyhow::Result;
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = commands::Cli::parse();
    commands::run(cli).await
}
