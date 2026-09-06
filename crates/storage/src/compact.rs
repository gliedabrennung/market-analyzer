use std::fs;
use std::path::Path;

use duckdb::Connection;

use crate::error::StorageError;
use crate::paths::{list_part_files, next_part_index, sql_quote_path};

/// Outcome of compacting one partition directory (FR-2.5).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionResult {
    pub files_before: usize,
    pub files_after: usize,
    pub rows: usize,
    pub bytes_before: u64,
    pub bytes_after: u64,
}

/// Merges every `part-*.parquet` file in `dir` into one, sorted by
/// `order_by_column` for scan locality. No-op if the partition already has
/// at most one file.
///
/// Crash-safe by construction: the merged file is written under a *new*,
/// never-before-used index and verified (row count + an order-independent
/// row checksum, `sum(hash(row))`, so a `COPY`-induced reorder can't cause
/// a false mismatch) against the originals *before* any original file is
/// deleted. A crash between "merged file written" and "originals deleted"
/// leaves both on disk — a transient over-count an operator can resolve by
/// re-running `compact`, never data loss (NFR-2.1's spirit, applied to a
/// multi-file rewrite rather than a single-file write).
pub fn compact_partition(
    dir: &Path,
    order_by_column: &str,
) -> Result<CompactionResult, StorageError> {
    let existing = list_part_files(dir)?;
    let bytes_before = total_size(&existing);

    if existing.len() <= 1 {
        let rows = if existing.is_empty() {
            0
        } else {
            row_count(dir, "part-*.parquet")?
        };
        return Ok(CompactionResult {
            files_before: existing.len(),
            files_after: existing.len(),
            rows,
            bytes_before,
            bytes_after: bytes_before,
        });
    }

    let engine = Connection::open_in_memory()?;
    let glob = sql_quote_path(&dir.join("part-*.parquet"));

    let (count_before, checksum_before) = aggregate_checksum(&engine, &glob)?;

    let new_index = next_part_index(dir)?;
    let tmp_path = dir.join(format!("part-{new_index:04}.parquet.tmp"));
    let final_path = dir.join(format!("part-{new_index:04}.parquet"));

    let copy_sql = format!(
        "COPY (SELECT * FROM read_parquet('{glob}') ORDER BY {order_by_column})
         TO '{dest}' (FORMAT PARQUET, COMPRESSION zstd)",
        dest = sql_quote_path(&tmp_path)
    );
    engine.execute_batch(&copy_sql)?;

    let merged_glob = sql_quote_path(&tmp_path);
    let (count_after, checksum_after) = aggregate_checksum(&engine, &merged_glob)?;

    if count_before != count_after || checksum_before != checksum_after {
        let _ = fs::remove_file(&tmp_path);
        return Err(StorageError::CompactionMismatch {
            path: dir.display().to_string(),
            count_before,
            count_after,
            checksum_before,
            checksum_after,
        });
    }

    fs::rename(&tmp_path, &final_path)?;
    for f in &existing {
        fs::remove_file(f)?;
    }

    let bytes_after = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);

    Ok(CompactionResult {
        files_before: existing.len(),
        files_after: 1,
        rows: count_after as usize,
        bytes_before,
        bytes_after,
    })
}

fn total_size(paths: &[std::path::PathBuf]) -> u64 {
    paths
        .iter()
        .filter_map(|p| fs::metadata(p).ok())
        .map(|m| m.len())
        .sum()
}

fn row_count(dir: &Path, pattern: &str) -> Result<usize, StorageError> {
    let engine = Connection::open_in_memory()?;
    let glob = sql_quote_path(&dir.join(pattern));
    let count: i64 = engine.query_row(
        &format!("SELECT count(*) FROM read_parquet('{glob}')"),
        [],
        |r| r.get(0),
    )?;
    Ok(count as usize)
}

/// `(row count, order-independent checksum)` for whatever `glob` matches —
/// hashing the whole row as a struct and summing means row order (which
/// `ORDER BY` in the `COPY` changes) can't affect the result.
fn aggregate_checksum(engine: &Connection, glob: &str) -> Result<(i64, String), StorageError> {
    engine
        .query_row(
            &format!("SELECT count(*), sum(hash(t))::VARCHAR FROM (SELECT * FROM read_parquet('{glob}')) t"),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(StorageError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ma_core::{Interval, Kline, Symbol};
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use tempfile::tempdir;

    fn kline_at(base: chrono::DateTime<Utc>, offset_minutes: i64) -> Kline {
        let open_time = base + chrono::Duration::minutes(offset_minutes);
        Kline {
            open_time,
            close_time: open_time + chrono::Duration::seconds(59),
            symbol: Symbol::new("BTCUSDT").unwrap(),
            exchange: "binance".to_string(),
            interval: Interval::OneMinute,
            open: Decimal::from_str("50000.00000000").unwrap(),
            high: Decimal::from_str("50100.00000000").unwrap(),
            low: Decimal::from_str("49900.00000000").unwrap(),
            close: Decimal::from_str("50050.00000000").unwrap(),
            volume: Decimal::from_str("12.50000000").unwrap(),
            quote_volume: Decimal::from_str("625625.00000000").unwrap(),
            trades_count: 100,
            taker_buy_base: Some(Decimal::from_str("6.25000000").unwrap()),
            is_closed: true,
        }
    }

    #[test]
    fn merges_many_files_into_one_preserving_aggregates() {
        let tmp = tempdir().unwrap();
        let store = crate::KlineStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

        // Five separate flushes -> five separate part files, same partition.
        for minute in 0..5i64 {
            store.write_klines(&[kline_at(base, minute)]).unwrap();
        }

        let dir = crate::paths::partition_dir(
            tmp.path(),
            "klines",
            "BTCUSDT",
            chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        assert_eq!(list_part_files(&dir).unwrap().len(), 5);

        let engine = Connection::open_in_memory().unwrap();
        let glob = sql_quote_path(&dir.join("part-*.parquet"));
        let (count_before, checksum_before) = aggregate_checksum(&engine, &glob).unwrap();
        assert_eq!(count_before, 5);

        let result = compact_partition(&dir, "open_time").unwrap();
        assert_eq!(result.files_before, 5);
        assert_eq!(result.files_after, 1);
        assert_eq!(result.rows, 5);

        assert_eq!(list_part_files(&dir).unwrap().len(), 1);

        let (count_after, checksum_after) = aggregate_checksum(&engine, &glob).unwrap();
        assert_eq!(count_before, count_after);
        assert_eq!(checksum_before, checksum_after);
    }

    #[test]
    fn single_file_partition_is_a_no_op() {
        let tmp = tempdir().unwrap();
        let store = crate::KlineStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        store.write_klines(&[kline_at(base, 0)]).unwrap();

        let dir = crate::paths::partition_dir(
            tmp.path(),
            "klines",
            "BTCUSDT",
            chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        let result = compact_partition(&dir, "open_time").unwrap();
        assert_eq!(result.files_before, 1);
        assert_eq!(result.files_after, 1);
        assert_eq!(result.rows, 1);
    }

    #[test]
    fn empty_partition_is_a_no_op() {
        let tmp = tempdir().unwrap();
        let dir = tmp.path().join("symbol=BTCUSDT/dt=2026-08-01");
        fs::create_dir_all(&dir).unwrap();
        let result = compact_partition(&dir, "open_time").unwrap();
        assert_eq!(result.files_before, 0);
        assert_eq!(result.files_after, 0);
        assert_eq!(result.rows, 0);
    }
}
