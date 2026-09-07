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

/// What a `500` says to the client. The underlying error text is a raw
/// DuckDB/storage message: absolute paths of the data directory, the SQL
/// that failed, sometimes schema details. That belongs in the server log,
/// not in a response any unauthenticated caller can read — the client can
/// do nothing with it either way.
const INTERNAL_MESSAGE: &str = "internal error";

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = self.status();
        let message = if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = %self, "internal error");
            INTERNAL_MESSAGE.to_string()
        } else {
            self.to_string()
        };
        let body = Json(json!({
            "error": {
                "code": self.code(),
                "message": message,
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

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::to_bytes;

    async fn body_json(error: ApiError) -> serde_json::Value {
        let response = error.into_response();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn internal_errors_do_not_leak_their_detail_to_the_client() {
        let secret = "IO Error: No files found that match the pattern \
                      \"/srv/market-analyzer/data/klines/**/*.parquet\"";
        let json = body_json(ApiError::Internal(secret.to_string())).await;

        assert_eq!(json["error"]["code"], "internal_error");
        assert_eq!(json["error"]["message"], "internal error");
        assert!(
            !json.to_string().contains("/srv/market-analyzer"),
            "server-side paths must never reach the response body"
        );
    }

    #[tokio::test]
    async fn client_errors_keep_their_actionable_message() {
        let json = body_json(ApiError::bad_request(
            "window",
            "window must be between 1 and 100000",
        ))
        .await;
        assert_eq!(json["error"]["code"], "invalid_parameter");
        assert_eq!(
            json["error"]["message"],
            "window must be between 1 and 100000"
        );
        assert_eq!(json["error"]["details"]["parameter"], "window");
    }
}
