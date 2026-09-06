//! Domain types and errors shared across the Market Analyzer workspace.
//!
//! Exchange-independent: nothing here knows about Binance, HTTP, or Parquet.

/// Money-safe decimal rescaling (FR-2.4: no `f64` for prices/volumes).
pub mod decimal;
/// [`CoreError`], the error type for every domain-validation failure here.
pub mod error;
/// [`Symbol`], the validated trading-pair identifier.
pub mod symbol;
/// The data model: [`Kline`], [`Trade`], [`DepthUpdate`], [`MarketEvent`], [`Interval`].
pub mod types;

pub use decimal::rescale_checked;
pub use error::CoreError;
pub use symbol::Symbol;
pub use types::{DepthUpdate, Interval, Kline, MarketEvent, Trade};
