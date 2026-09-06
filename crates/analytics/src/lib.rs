//! Analytical SQL query library (FR-3.1..FR-3.8).
//!
//! All computation is DuckDB SQL, loaded via `include_str!` from `sql/` and
//! executed only through prepared-statement parameters — never string
//! concatenation of data values (FR-3.8). Every function here expects an
//! open connection whose catalog already has the `klines`/`trades` views
//! (see `ma_storage::MetaStore`).

/// [`volume_anomalies`]: rolling z-score outliers (FR-3.4).
pub mod anomaly;
/// [`correlation_matrix`]: pairwise log-return correlation (FR-3.6).
pub mod correlation;
/// [`AnalyticsError`], the error type for every analytics query.
pub mod error;
/// [`order_flow_imbalance`]: buy/sell volume imbalance from trades (FR-3.5).
pub mod ofi;
/// [`resample_ohlcv`]: trades resampled into OHLCV bars (FR-3.1).
pub mod resample;
/// Shared row-decoding helpers (`Decimal`/`TIMESTAMP` columns) and the
/// dynamic-`IN`-list placeholder builder used by [`correlation`].
pub mod rowutil;
/// [`cross_spread`]: cross-series spread via `ASOF JOIN` (FR-3.7).
pub mod spread;
/// [`realized_volatility`]: rolling stddev of log returns (FR-3.3).
pub mod volatility;
/// [`vwap()`]: rolling volume-weighted average price (FR-3.2).
pub mod vwap;

pub use anomaly::{volume_anomalies, VolumeAnomaly};
pub use correlation::{correlation_matrix, CorrelationPair};
pub use error::AnalyticsError;
pub use ofi::{order_flow_imbalance, OfiBucket};
pub use resample::{resample_ohlcv, ResampledBar};
pub use spread::{cross_spread, SpreadPoint};
pub use volatility::{realized_volatility, VolatilityPoint};
pub use vwap::{vwap, VwapPoint};
