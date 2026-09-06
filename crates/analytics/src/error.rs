use thiserror::Error;

#[derive(Debug, Error)]
pub enum AnalyticsError {
    #[error("duckdb error: {0}")]
    Db(#[from] duckdb::Error),

    #[error(transparent)]
    Domain(#[from] ma_core::CoreError),

    #[error("invalid parameter '{name}': {reason}")]
    InvalidParam { name: &'static str, reason: String },

    #[error("failed to parse decimal '{value}': {source}")]
    Decimal {
        value: String,
        #[source]
        source: rust_decimal::Error,
    },
}
