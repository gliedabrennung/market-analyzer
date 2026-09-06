//! FR-5.1 route handlers, one module per endpoint group.

/// `/analytics/{symbol}/{vwap,volatility,anomalies,ofi}`, `/analytics/correlation`.
pub mod analytics;
/// `GET /health`.
pub mod health;
/// `GET /ohlcv/{symbol}`.
pub mod ohlcv;
/// `WS /stream/{symbol}`.
pub mod stream_ws;
/// `GET /symbols`.
pub mod symbols;
