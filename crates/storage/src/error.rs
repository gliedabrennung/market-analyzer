use thiserror::Error;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("duckdb error: {0}")]
    Db(#[from] duckdb::Error),

    #[error(transparent)]
    Domain(#[from] ma_core::CoreError),

    #[error("meta.duckdb schema version {found} is not supported (expected {expected}); run a compatible build or migrate manually")]
    SchemaVersion { found: i64, expected: i64 },

    #[error(
        "compaction checksum mismatch in {path}: before(count={count_before}, checksum={checksum_before}) != after(count={count_after}, checksum={checksum_after}); original files left untouched"
    )]
    CompactionMismatch {
        path: String,
        count_before: i64,
        count_after: i64,
        checksum_before: String,
        checksum_after: String,
    },
}
