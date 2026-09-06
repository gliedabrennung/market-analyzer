use std::sync::Arc;
use std::time::Instant;

use axum::extract::{MatchedPath, State};
use axum::http::{Method, Request};
use axum::middleware::Next;
use axum::response::Response;
use prometheus::{HistogramOpts, HistogramVec, IntCounterVec, Opts, Registry, TextEncoder};

use crate::state::AppState;

/// FR-6.3: request counts, a request-duration histogram, and error counts
/// by code, in Prometheus text format at `/metrics`.
pub struct Metrics {
    registry: Registry,
    pub http_requests_total: IntCounterVec,
    pub http_request_duration_seconds: HistogramVec,
    pub http_errors_total: IntCounterVec,
}

impl Metrics {
    /// Creates and registers the three FR-6.3 collectors.
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let http_requests_total = IntCounterVec::new(
            Opts::new("http_requests_total", "Total HTTP requests handled"),
            &["method", "route", "status"],
        )?;
        let http_request_duration_seconds = HistogramVec::new(
            HistogramOpts::new(
                "http_request_duration_seconds",
                "HTTP request duration in seconds",
            ),
            &["method", "route"],
        )?;
        let http_errors_total = IntCounterVec::new(
            Opts::new("http_errors_total", "Total HTTP error responses by code"),
            &["code"],
        )?;

        registry.register(Box::new(http_requests_total.clone()))?;
        registry.register(Box::new(http_request_duration_seconds.clone()))?;
        registry.register(Box::new(http_errors_total.clone()))?;

        Ok(Self {
            registry,
            http_requests_total,
            http_request_duration_seconds,
            http_errors_total,
        })
    }

    /// Renders the registry as Prometheus text format for `GET /metrics`.
    pub fn render(&self) -> String {
        let families = self.registry.gather();
        TextEncoder::new()
            .encode_to_string(&families)
            .unwrap_or_default()
    }
}

/// The standard HTTP methods this API ever routes; anything else collapses
/// to `"other"` so a client sending arbitrary method tokens (any ASCII
/// token is a legal HTTP method per RFC 7230) can't blow up the
/// `method` label's cardinality.
fn normalize_method(method: &Method) -> &'static str {
    match *method {
        Method::GET => "GET",
        Method::POST => "POST",
        Method::PUT => "PUT",
        Method::DELETE => "DELETE",
        Method::PATCH => "PATCH",
        Method::HEAD => "HEAD",
        Method::OPTIONS => "OPTIONS",
        _ => "other",
    }
}

/// Records request count, latency, and (for non-2xx) error-count metrics
/// for every request (FR-6.3). The route *pattern* is used as a label
/// (e.g. `/ohlcv/:symbol`), never the concrete path — using the literal
/// symbol would blow up Prometheus label cardinality with one series per
/// distinct value ever requested.
pub async fn track_metrics(
    State(state): State<Arc<AppState>>,
    req: Request<axum::body::Body>,
    next: Next,
) -> Response {
    let method = normalize_method(req.method());
    let route = req
        .extensions()
        .get::<MatchedPath>()
        .map(|p| p.as_str().to_string())
        .unwrap_or_else(|| "unmatched".to_string());

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status();

    state
        .metrics
        .http_requests_total
        .with_label_values(&[method, &route, status.as_str()])
        .inc();
    state
        .metrics
        .http_request_duration_seconds
        .with_label_values(&[method, &route])
        .observe(elapsed);
    if status.is_client_error() || status.is_server_error() {
        state
            .metrics
            .http_errors_total
            .with_label_values(&[status.as_str()])
            .inc();
    }

    response
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_prometheus_text_format() {
        let metrics = Metrics::new().unwrap();
        metrics
            .http_requests_total
            .with_label_values(&["GET", "/health", "200"])
            .inc();
        let text = metrics.render();
        assert!(text.contains("http_requests_total"));
        assert!(text.contains("# HELP"));
        assert!(text.contains("# TYPE"));
    }
}
