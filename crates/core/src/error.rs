use thiserror::Error;

/// Errors produced by domain-level validation in `ma-core`.
#[derive(Debug, Error)]
pub enum CoreError {
    #[error("invalid symbol '{0}': must be 1-32 uppercase ASCII alphanumeric characters")]
    InvalidSymbol(String),

    #[error("invalid interval '{0}'")]
    InvalidInterval(String),

    #[error("decimal value '{value}' has scale {actual} which exceeds target scale {target}")]
    DecimalScale {
        value: String,
        actual: u32,
        target: u32,
    },
}
