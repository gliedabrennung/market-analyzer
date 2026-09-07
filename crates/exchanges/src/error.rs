use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExchangeError {
    #[error("http transport error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("rate limited after {attempts} attempt(s)")]
    RateLimited { attempts: u32 },

    #[error("banned by exchange (HTTP 418), retry after {retry_after_secs}s")]
    Banned { retry_after_secs: u64 },

    #[error("unexpected HTTP status {status}: {body}")]
    UnexpectedStatus { status: u16, body: String },

    #[error("failed to parse exchange response: {0}")]
    Parse(String),

    #[error("failed to parse exchange JSON payload: {0}")]
    Json(#[from] serde_json::Error),

    #[error(transparent)]
    Domain(#[from] ma_core::CoreError),

    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    #[error("invalid configuration: {0}")]
    Config(String),
}
