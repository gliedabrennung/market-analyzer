//! Parquet + DuckDB storage layer (FR-2.x).
//!
//! - [`klines`] / [`trades`]: write `Kline`s/`Trade`s to the hive-partitioned
//!   Parquet layout, deduplicating against what's already on disk.
//! - [`batch`]: in-memory flush-threshold bookkeeping for streaming writers
//!   (FR-2.2).
//! - [`compact`]: merges a partition's `part-*.parquet` files into one
//!   (FR-2.5).
//! - [`meta`]: the single-writer `meta.duckdb` metabase (symbol registry,
//!   collector bookkeeping, schema migrations, views over Parquet).

/// [`BatchBuffer`]/[`BatchConfig`]: flush-threshold bookkeeping (FR-2.2).
pub mod batch;
/// [`compact_partition`]: merge a partition's files into one (FR-2.5).
pub mod compact;
/// [`StorageError`], the error type for every storage operation.
pub mod error;
/// [`KlineStore`]: writes deduplicated `Kline`s to the Parquet layout.
pub mod klines;
/// [`MetaStore`]: the single-writer `meta.duckdb` metabase.
pub mod meta;
/// Hive-partition path helpers and crash-recovery cleanup.
pub mod paths;
/// [`TradeStore`]: writes deduplicated `Trade`s to the Parquet layout.
pub mod trades;

pub use batch::{BatchBuffer, BatchConfig};
pub use compact::{compact_partition, CompactionResult};
pub use error::StorageError;
pub use klines::KlineStore;
pub use meta::{MetaStore, SymbolRecord};
pub use paths::{cleanup_incomplete_writes, discover_partitions};
pub use trades::TradeStore;
