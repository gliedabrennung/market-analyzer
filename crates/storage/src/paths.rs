use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::error::StorageError;

pub fn partition_dir(data_root: &Path, dataset: &str, symbol: &str, dt: NaiveDate) -> PathBuf {
    data_root
        .join(dataset)
        .join(format!("symbol={symbol}"))
        .join(format!("dt={}", dt.format("%Y-%m-%d")))
}

pub fn next_part_index(dir: &Path) -> Result<u32, StorageError> {
    let mut max_idx: Option<u32> = None;
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            if let Some(idx) = parse_part_index(&entry.file_name().to_string_lossy()) {
                max_idx = Some(max_idx.map_or(idx, |m| m.max(idx)));
            }
        }
    }
    Ok(max_idx.map_or(0, |m| m + 1))
}

pub fn has_existing_parts(dir: &Path) -> Result<bool, StorageError> {
    if !dir.exists() {
        return Ok(false);
    }
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        if parse_part_index(&entry.file_name().to_string_lossy()).is_some() {
            return Ok(true);
        }
    }
    Ok(false)
}

fn parse_part_index(name: &str) -> Option<u32> {
    name.strip_prefix("part-")?
        .strip_suffix(".parquet")?
        .parse::<u32>()
        .ok()
}

pub fn list_part_files(dir: &Path) -> Result<Vec<PathBuf>, StorageError> {
    let mut files: Vec<(u32, PathBuf)> = Vec::new();
    if dir.exists() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let name = entry.file_name().to_string_lossy().into_owned();
            if let Some(idx) = parse_part_index(&name) {
                files.push((idx, entry.path()));
            }
        }
    }
    files.sort_by_key(|(idx, _)| *idx);
    Ok(files.into_iter().map(|(_, p)| p).collect())
}

pub fn discover_partitions(
    data_root: &Path,
    dataset: &str,
    symbol_filter: Option<&str>,
    date_filter: Option<NaiveDate>,
) -> Result<Vec<PathBuf>, StorageError> {
    let dataset_dir = data_root.join(dataset);
    let mut out = Vec::new();
    if !dataset_dir.exists() {
        return Ok(out);
    }
    for symbol_entry in fs::read_dir(&dataset_dir)? {
        let symbol_entry = symbol_entry?;
        let symbol_name = symbol_entry.file_name().to_string_lossy().into_owned();
        let Some(symbol) = symbol_name.strip_prefix("symbol=") else {
            continue;
        };
        if let Some(filter) = symbol_filter {
            if symbol != filter {
                continue;
            }
        }
        let symbol_dir = symbol_entry.path();
        if !symbol_dir.is_dir() {
            continue;
        }
        for dt_entry in fs::read_dir(&symbol_dir)? {
            let dt_entry = dt_entry?;
            let dt_name = dt_entry.file_name().to_string_lossy().into_owned();
            let Some(dt_str) = dt_name.strip_prefix("dt=") else {
                continue;
            };
            if let Some(filter) = date_filter {
                if dt_str != filter.format("%Y-%m-%d").to_string() {
                    continue;
                }
            }
            let dt_dir = dt_entry.path();
            if dt_dir.is_dir() {
                out.push(dt_dir);
            }
        }
    }
    out.sort();
    Ok(out)
}

pub fn cleanup_incomplete_writes(data_root: &Path) -> Result<usize, StorageError> {
    let mut removed = 0usize;
    if !data_root.exists() {
        return Ok(0);
    }
    let mut stack = vec![data_root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("tmp") {
                fs::remove_file(&path)?;
                removed += 1;
                tracing::info!(path = %path.display(), "removed incomplete write from a previous run");
            }
        }
    }
    Ok(removed)
}

pub fn sql_quote_path(path: &Path) -> String {
    path.to_string_lossy().replace('\'', "''")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn partition_dir_matches_fr_2_1_layout() {
        let root = PathBuf::from("data");
        let dt = NaiveDate::from_ymd_opt(2026, 8, 1).unwrap();
        let dir = partition_dir(&root, "klines", "BTCUSDT", dt);
        assert_eq!(
            dir,
            PathBuf::from("data/klines/symbol=BTCUSDT/dt=2026-08-01")
        );
    }

    #[test]
    fn next_part_index_starts_at_zero_and_increments() {
        let tmp = tempdir().unwrap();
        assert_eq!(next_part_index(tmp.path()).unwrap(), 0);
        fs::write(tmp.path().join("part-0000.parquet"), b"").unwrap();
        fs::write(tmp.path().join("part-0003.parquet"), b"").unwrap();
        fs::write(tmp.path().join("not-a-part.parquet"), b"").unwrap();
        assert_eq!(next_part_index(tmp.path()).unwrap(), 4);
    }

    #[test]
    fn cleanup_removes_only_tmp_files() {
        let tmp = tempdir().unwrap();
        let sub = tmp.path().join("symbol=BTCUSDT").join("dt=2026-08-01");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("part-0000.parquet"), b"keep").unwrap();
        fs::write(sub.join("part-0001.parquet.tmp"), b"drop").unwrap();

        let removed = cleanup_incomplete_writes(tmp.path()).unwrap();
        assert_eq!(removed, 1);
        assert!(sub.join("part-0000.parquet").exists());
        assert!(!sub.join("part-0001.parquet.tmp").exists());
    }

    #[test]
    fn discover_partitions_filters_by_symbol_and_date() {
        let tmp = tempdir().unwrap();
        for (symbol, dt) in [
            ("BTCUSDT", "2026-08-01"),
            ("BTCUSDT", "2026-08-02"),
            ("ETHUSDT", "2026-08-01"),
        ] {
            fs::create_dir_all(
                tmp.path()
                    .join("klines")
                    .join(format!("symbol={symbol}"))
                    .join(format!("dt={dt}")),
            )
            .unwrap();
        }

        let all = discover_partitions(tmp.path(), "klines", None, None).unwrap();
        assert_eq!(all.len(), 3);

        let btc_only = discover_partitions(tmp.path(), "klines", Some("BTCUSDT"), None).unwrap();
        assert_eq!(btc_only.len(), 2);

        let one_day = discover_partitions(
            tmp.path(),
            "klines",
            None,
            NaiveDate::from_ymd_opt(2026, 8, 1),
        )
        .unwrap();
        assert_eq!(one_day.len(), 2);

        let both_filters = discover_partitions(
            tmp.path(),
            "klines",
            Some("BTCUSDT"),
            NaiveDate::from_ymd_opt(2026, 8, 2),
        )
        .unwrap();
        assert_eq!(both_filters.len(), 1);
        assert!(both_filters[0].ends_with("dt=2026-08-02"));
    }

    #[test]
    fn discover_partitions_missing_dataset_dir_returns_empty() {
        let tmp = tempdir().unwrap();
        let result = discover_partitions(tmp.path(), "klines", None, None).unwrap();
        assert!(result.is_empty());
    }
}
