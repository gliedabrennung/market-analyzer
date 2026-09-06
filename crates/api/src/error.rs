use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

/// FR-5.2: every error response is `{ "error": { code, message, details } }`
/// with the mandated HTTP status per class.
#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{message}")]
    BadRequest {
        message: String,
        details: serde_json::Value,
    },

    #[error("unknown symbol '{symbol}'")]
    UnknownSymbol { symbol: String },

    #[error("rate limit exceeded")]
    RateLimited,

    #[error("internal error: {0}")]
    Internal(String),
}

impl ApiError {
    /// A `400` for `parameter`, with it recorded in `details` (FR-5.2/5.3).
    pub fn bad_request(parameter: &str, message: impl Into<String>) -> Self {
        ApiError::BadRequest {
            message: message.into(),
            details: json!({ "parameter": parameter }),
        }
    }

    fn code(&self) -> &'static str {
        match self {
            ApiError::BadRequest { .. } => "invalid_parameter",
            ApiError::UnknownSymbol { .. } => "unknown_symbol",
            ApiError::RateLimited => "rate_limited",
            ApiError::Internal(_) => "internal_error",
        }
    }

    /// The FR-5.2-mandated HTTP status for this error's class.
    pub fn status(&self) -> StatusCode {
        match self {
            ApiError::BadRequest { .. } => StatusCode::BAD_REQUEST,
            ApiError::UnknownSymbol { .. } => StatusCode::NOT_FOUND,
            ApiError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            ApiError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    fn details(&self) -> serde_json::Value {
        match self {
            ApiError::BadRequest { details, .. } => details.clone(),
            _ => json!({}),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "internal error");
        }
        let body = Json(json!({
            "error": {
                "code": self.code(),
                "message": self.to_string(),
                "details": self.details(),
            }
        }));
        (status, body).into_response()
    }
}

/// FR-5.2's `{ "error": { code, message, details } }` shape, for the one
/// call site (the WS upstream-subscribe failure) that reports over a raw
/// `Message::Text` frame instead of an `IntoResponse`.
pub fn error_envelope(code: &str, message: impl Into<String>) -> serde_json::Value {
    json!({
        "error": {
            "code": code,
            "message": message.into(),
            "details": {},
        }
    })
}

impl From<ma_analytics::AnalyticsError> for ApiError {
    fn from(e: ma_analytics::AnalyticsError) -> Self {
        match e {
            ma_analytics::AnalyticsError::InvalidParam { name, reason } => {
                ApiError::bad_request(name, format!("invalid parameter '{name}': {reason}"))
            }
            other => ApiError::Internal(other.to_string()),
        }
    }
}

impl From<ma_storage::StorageError> for ApiError {
    fn from(e: ma_storage::StorageError) -> Self {
        ApiError::Internal(e.to_string())
    }
}

impl From<duckdb::Error> for ApiError {
    fn from(e: duckdb::Error) -> Self {
        ApiError::Internal(e.to_string())
    }
}
