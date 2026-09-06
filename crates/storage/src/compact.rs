use std::fs;
use std::path::{Path, PathBuf};

use duckdb::Connection;

use crate::error::StorageError;
use crate::paths::{list_part_files, next_part_index, sql_quote_path};

/// Suffix for a compaction's write-ahead manifest — never matches
/// `part-*.parquet`, so it's invisible to `list_part_files`/`next_part_index`
/// and to the DuckDB glob views. See [`recover_incomplete_compaction`].
const MANIFEST_SUFFIX: &str = ".compacted-manifest";

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
/// leaves both on disk — a transient over-count a query might see, and
/// which this function's own next run resolves automatically: a
/// write-ahead manifest naming exactly the files this compaction replaces
/// is durably in place *before* the merge is written, so
/// [`recover_incomplete_compaction`] can always finish an interrupted
/// cleanup — deleting exactly those files, nothing more — before computing
/// a fresh "before" checksum. Without this, a second run could otherwise
/// compute its checksum over the still-duplicated glob and "verify" a merge
/// that silently bakes the duplicate in permanently.
pub fn compact_partition(
    dir: &Path,
    order_by_column: &str,
) -> Result<CompactionResult, StorageError> {
    recover_incomplete_compaction(dir)?;

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

    let manifest_path = write_manifest(dir, new_index, &existing)?;

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
        let _ = fs::remove_file(&manifest_path);
        return Err(StorageError::CompactionMismatch {
            path: dir.display().to_string(),
            count_before,
            count_after,
            checksum_before,
            checksum_after,
        });
    }

    fs::rename(&tmp_path, &final_path)?;
    delete_manifested_files(&manifest_path)?;

    let bytes_after = fs::metadata(&final_path).map(|m| m.len()).unwrap_or(0);

    Ok(CompactionResult {
        files_before: existing.len(),
        files_after: 1,
        rows: count_after as usize,
        bytes_before,
        bytes_after,
    })
}

/// Finishes any compaction whose merged file was already written (and
/// checksum-verified) but whose original-file cleanup was interrupted by a
/// crash. Run automatically at the start of every [`compact_partition`]
/// call, so a dangling manifest is always resolved before any new
/// compaction computes a "before" checksum over the same directory.
fn recover_incomplete_compaction(dir: &Path) -> Result<(), StorageError> {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.into()),
    };
    for entry in entries {
        let path = entry?.path();
        let is_manifest = path
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.ends_with(MANIFEST_SUFFIX));
        if is_manifest {
            tracing::info!(path = %path.display(), "resuming compaction cleanup left incomplete by a previous run");
            delete_manifested_files(&path)?;
        }
    }
    Ok(())
}

/// Durably records, *before* the merged file is written, exactly which
/// files this compaction will make redundant — written via the usual
/// tmp-then-rename pattern so the manifest itself never appears half-written.
fn write_manifest(dir: &Path, new_index: u32, files: &[PathBuf]) -> Result<PathBuf, StorageError> {
    let tmp_path = dir.join(format!("part-{new_index:04}{MANIFEST_SUFFIX}.tmp"));
    let final_path = dir.join(format!("part-{new_index:04}{MANIFEST_SUFFIX}"));
    let names: Vec<&str> = files
        .iter()
        .filter_map(|p| p.file_name().and_then(|n| n.to_str()))
        .collect();
    fs::write(&tmp_path, names.join("\n"))?;
    fs::rename(&tmp_path, &final_path)?;
    Ok(final_path)
}

/// Deletes every file named in `manifest_path` (tolerating one already
/// having been removed by a prior, interrupted attempt), then the manifest
/// itself — the manifest's presence is exactly what future runs use to
/// detect that this cleanup hasn't finished yet.
fn delete_manifested_files(manifest_path: &Path) -> Result<(), StorageError> {
    let dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let contents = fs::read_to_string(manifest_path)?;
    for name in contents.lines().filter(|l| !l.is_empty()) {
        match fs::remove_file(dir.join(name)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    fs::remove_file(manifest_path)?;
    Ok(())
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
    fn recovers_from_manifest_left_by_a_crash_between_rename_and_cleanup() {
        let tmp = tempdir().unwrap();
        let store = crate::KlineStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        for minute in 0..3i64 {
            store.write_klines(&[kline_at(base, minute)]).unwrap();
        }
        let dir = crate::paths::partition_dir(
            tmp.path(),
            "klines",
            "BTCUSDT",
            chrono::NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        let existing = list_part_files(&dir).unwrap();
        assert_eq!(existing.len(), 3);

        // Hand-construct exactly the "crashed between rename and cleanup"
        // state: the merged file is already written and correct, its
        // manifest (naming the 3 originals it replaces) is in place, but
        // none of the originals has been deleted yet.
        let manifest_path = write_manifest(&dir, 3, &existing).unwrap();
        let engine = Connection::open_in_memory().unwrap();
        let glob = sql_quote_path(&dir.join("part-*.parquet"));
        let final_path = dir.join("part-0003.parquet");
        engine
            .execute_batch(&format!(
                "COPY (SELECT * FROM read_parquet('{glob}') ORDER BY open_time)
                 TO '{dest}' (FORMAT PARQUET, COMPRESSION zstd)",
                dest = sql_quote_path(&final_path)
            ))
            .unwrap();
        assert!(manifest_path.exists());
        // Transient duplicate: the 3 originals plus the merged file that
        // already contains their rows.
        assert_eq!(list_part_files(&dir).unwrap().len(), 4);

        // A retried `compact_partition` must finish the interrupted cleanup
        // *before* computing any fresh checksum, so it must never see —
        // let alone silently verify and bake in — that transient duplicate.
        let result = compact_partition(&dir, "open_time").unwrap();
        assert_eq!(result.files_before, 1);
        assert_eq!(result.files_after, 1);
        assert_eq!(
            result.rows, 3,
            "must recover to the true row count, not the transient double-count"
        );

        assert!(!manifest_path.exists());
        assert_eq!(list_part_files(&dir).unwrap(), vec![final_path]);
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
