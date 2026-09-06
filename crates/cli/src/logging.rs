/// Initialize `tracing` output. `--verbose` raises the default level
/// (FR-4.3); `RUST_LOG` always wins when set.
pub fn init(verbose: bool) {
    let default_level = if verbose { "debug" } else { "info" };
    let filter = tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(default_level));
    tracing_subscriber::fmt().with_env_filter(filter).init();
}
