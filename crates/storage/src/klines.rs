use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use duckdb::Connection;

use ma_core::Kline;

use crate::error::StorageError;
use crate::paths::{has_existing_parts, next_part_index, partition_dir, sql_quote_path};

const DATASET: &str = "klines";

type DedupKey = (String, String, i64);

/// Writes `Kline`s to the Parquet/hive layout from FR-2.1, deduplicating
/// against whatever is already on disk (FR-2.3) so re-running the same
/// backfill never duplicates rows (NFR-2.3).
///
/// Writing goes through an in-memory DuckDB connection used purely as a
/// SQL-to-Parquet engine (architecture §3.4 variant A) — it never touches
/// `meta.duckdb` and is unrelated to the NFR-4.1 single-writer rule, which
/// governs the metabase file, not Parquet files.
///
/// The dedup key set for a partition is read from disk at most once per
/// process lifetime and kept in memory after that (`dedup_cache`), updated
/// in place as new rows are written. A long-running `stream` process calls
/// `write_klines` far more often than `backfill` does (every flush, for
/// hours), and that partition's Parquet files only grow over the session —
/// re-scanning them from disk on every flush would make each flush cost
/// grow with everything written so far (measured: RSS climbing over
/// minutes of a real 20-symbol stream instead of leveling off).
pub struct KlineStore {
    data_root: PathBuf,
    engine: Connection,
    dedup_cache: RefCell<HashMap<PathBuf, HashSet<DedupKey>>>,
}

impl KlineStore {
    /// Opens the in-memory DuckDB engine used to write into `data_root`.
    pub fn new(data_root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let engine = Connection::open_in_memory()?;
        Ok(Self {
            data_root: data_root.into(),
            engine,
            dedup_cache: RefCell::new(HashMap::new()),
        })
    }

    /// Write `klines`, skipping rows that already exist on disk under the
    /// `(exchange, interval, open_time)` dedup key (FR-2.3). Returns the
    /// number of rows actually written.
    pub fn write_klines(&self, klines: &[Kline]) -> Result<usize, StorageError> {
        if klines.is_empty() {
            return Ok(0);
        }

        let mut groups: BTreeMap<(String, NaiveDate), Vec<&Kline>> = BTreeMap::new();
        for k in klines {
            let dt = k.open_time.date_naive();
            groups
                .entry((k.symbol.as_str().to_string(), dt))
                .or_default()
                .push(k);
        }

        let mut total_written = 0usize;
        for ((symbol, dt), group) in groups {
            total_written += self.write_partition(&symbol, dt, &group)?;
        }
        Ok(total_written)
    }

    fn write_partition(
        &self,
        symbol: &str,
        dt: NaiveDate,
        klines: &[&Kline],
    ) -> Result<usize, StorageError> {
        let dir = partition_dir(&self.data_root, DATASET, symbol, dt);
        fs::create_dir_all(&dir)?;

        let mut existing = self.take_dedup_set(&dir)?;
        let fresh: Vec<&Kline> = klines
            .iter()
            .copied()
            .filter(|k| {
                let key = (
                    k.exchange.clone(),
                    k.interval.as_str().to_string(),
                    k.open_time.timestamp_micros(),
                );
                existing.insert(key)
            })
            .collect();
        self.dedup_cache.borrow_mut().insert(dir.clone(), existing);

        if fresh.is_empty() {
            return Ok(0);
        }

        let index = next_part_index(&dir)?;
        let final_path = dir.join(format!("part-{index:04}.parquet"));
        let tmp_path = dir.join(format!("part-{index:04}.parquet.tmp"));

        self.copy_to_parquet(&fresh, &tmp_path)?;
        // Atomic-ish: readers only ever see the final, fully-written name;
        // a crash before rename leaves a `.tmp` cleaned up at next startup
        // (NFR-2.1).
        fs::rename(&tmp_path, &final_path)?;

        Ok(fresh.len())
    }

    /// Takes ownership of the cached dedup set for `dir` (loading it from
    /// disk on first touch), leaving the cache momentarily without an entry
    /// for it — the caller is expected to insert the (possibly updated) set
    /// back in. Structured this way (`remove` + reinsert, no `get_mut`) so
    /// there is no fallible-lookup-after-ensuring-presence step that would
    /// need an `unwrap`/`expect` (NFR-3.4 forbids both in library code).
    fn take_dedup_set(&self, dir: &Path) -> Result<HashSet<DedupKey>, StorageError> {
        if let Some(set) = self.dedup_cache.borrow_mut().remove(dir) {
            return Ok(set);
        }
        self.existing_keys(dir)
    }

    fn existing_keys(&self, dir: &Path) -> Result<HashSet<DedupKey>, StorageError> {
        let mut set = HashSet::new();
        if !has_existing_parts(dir)? {
            return Ok(set);
        }
        let glob = sql_quote_path(&dir.join("part-*.parquet"));
        let sql =
            format!("SELECT exchange, interval, epoch_us(open_time) FROM read_parquet('{glob}')");
        let mut stmt = self.engine.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let exchange: String = row.get(0)?;
            let interval: String = row.get(1)?;
            let open_time_us: i64 = row.get(2)?;
            set.insert((exchange, interval, open_time_us));
        }
        Ok(set)
    }

    /// Bulk-loads `klines` into a staging table and copies it out to
    /// Parquet. Money columns are staged as `VARCHAR` and cast to `DECIMAL`
    /// only in the final `COPY ... SELECT`, because the DuckDB `Appender`
    /// (the fast bulk-load path — a row-by-row `INSERT`, each in its own
    /// implicit transaction, measured at minutes for tens of thousands of
    /// rows and would blow NFR-1.5) does not target typed `DECIMAL` columns
    /// directly.
    fn copy_to_parquet(&self, klines: &[&Kline], dest: &Path) -> Result<(), StorageError> {
        self.engine.execute_batch(
            "CREATE OR REPLACE TEMP TABLE staging_klines (
                open_time TIMESTAMP NOT NULL,
                close_time TIMESTAMP NOT NULL,
                symbol VARCHAR NOT NULL,
                exchange VARCHAR NOT NULL,
                interval VARCHAR NOT NULL,
                open VARCHAR NOT NULL,
                high VARCHAR NOT NULL,
                low VARCHAR NOT NULL,
                close VARCHAR NOT NULL,
                volume VARCHAR NOT NULL,
                quote_volume VARCHAR NOT NULL,
                trades_count INTEGER NOT NULL,
                taker_buy_base VARCHAR,
                is_closed BOOLEAN NOT NULL
            )",
        )?;

        {
            let mut appender = self.engine.appender("staging_klines")?;
            for k in klines.iter().copied() {
                appender.append_row(duckdb::params![
                    k.open_time.naive_utc(),
                    k.close_time.naive_utc(),
                    k.symbol.as_str(),
                    k.exchange.as_str(),
                    k.interval.as_str(),
                    k.open.to_string(),
                    k.high.to_string(),
                    k.low.to_string(),
                    k.close.to_string(),
                    k.volume.to_string(),
                    k.quote_volume.to_string(),
                    k.trades_count,
                    k.taker_buy_base.map(|d| d.to_string()),
                    k.is_closed,
                ])?;
            }
            appender.flush()?;
        }

        let dest_literal = sql_quote_path(dest);
        let copy_sql = format!(
            "COPY (
                SELECT open_time, close_time, symbol, exchange, interval,
                       CAST(open AS DECIMAL(18,8)) AS open,
                       CAST(high AS DECIMAL(18,8)) AS high,
                       CAST(low AS DECIMAL(18,8)) AS low,
                       CAST(close AS DECIMAL(18,8)) AS close,
                       CAST(volume AS DECIMAL(28,8)) AS volume,
                       CAST(quote_volume AS DECIMAL(28,8)) AS quote_volume,
                       trades_count,
                       CAST(taker_buy_base AS DECIMAL(28,8)) AS taker_buy_base,
                       is_closed
                FROM staging_klines
                ORDER BY open_time
            ) TO '{dest_literal}' (FORMAT PARQUET, COMPRESSION zstd)"
        );
        self.engine.execute_batch(&copy_sql)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};
    use ma_core::{Interval, Symbol};
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

    fn sample_kline(minute: u32) -> Kline {
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        kline_at(base, minute as i64)
    }

    fn count_parquet_rows(engine: &Connection, dir: &Path) -> i64 {
        let glob = sql_quote_path(&dir.join("part-*.parquet"));
        engine
            .query_row(
                &format!("SELECT count(*) FROM read_parquet('{glob}')"),
                [],
                |r| r.get(0),
            )
            .unwrap()
    }

    #[test]
    fn writes_klines_and_dedups_on_rerun() {
        let tmp = tempdir().unwrap();
        let store = KlineStore::new(tmp.path()).unwrap();
        let klines: Vec<Kline> = (0..5).map(sample_kline).collect();

        let written_first = store.write_klines(&klines).unwrap();
        assert_eq!(written_first, 5);

        // Re-running the exact same backfill must not duplicate rows.
        let written_second = store.write_klines(&klines).unwrap();
        assert_eq!(written_second, 0);

        let dir = partition_dir(
            tmp.path(),
            DATASET,
            "BTCUSDT",
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        let check_engine = Connection::open_in_memory().unwrap();
        assert_eq!(count_parquet_rows(&check_engine, &dir), 5);
    }

    #[test]
    fn appends_new_rows_alongside_existing_partition() {
        let tmp = tempdir().unwrap();
        let store = KlineStore::new(tmp.path()).unwrap();

        store.write_klines(&[sample_kline(0)]).unwrap();
        let written = store
            .write_klines(&[sample_kline(0), sample_kline(1)])
            .unwrap();
        assert_eq!(
            written, 1,
            "only the genuinely new minute should be written"
        );

        let dir = partition_dir(
            tmp.path(),
            DATASET,
            "BTCUSDT",
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        let check_engine = Connection::open_in_memory().unwrap();
        assert_eq!(count_parquet_rows(&check_engine, &dir), 2);
    }

    #[test]
    fn empty_input_writes_nothing() {
        let tmp = tempdir().unwrap();
        let store = KlineStore::new(tmp.path()).unwrap();
        assert_eq!(store.write_klines(&[]).unwrap(), 0);
    }

    /// Simulates a long-running `stream` process: many small, separate
    /// flushes into the *same* partition (unlike a single backfill batch).
    /// Each call must still dedup correctly against everything written by
    /// earlier calls in this process, via the in-memory cache rather than a
    /// disk re-scan per call.
    #[test]
    fn many_sequential_flushes_into_same_partition_dedup_correctly() {
        let tmp = tempdir().unwrap();
        let store = KlineStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

        for minute in 0..20i64 {
            // Each flush re-sends the previous minute too (as a live kline
            // stream's own reconnect-gap overlap might), plus one new one.
            let batch = vec![kline_at(base, minute), kline_at(base, minute + 1)];
            let written = store.write_klines(&batch).unwrap();
            let expected = if minute == 0 { 2 } else { 1 };
            assert_eq!(
                written, expected,
                "flush {minute} wrote unexpected row count"
            );
        }

        let dir = partition_dir(
            tmp.path(),
            DATASET,
            "BTCUSDT",
            NaiveDate::from_ymd_opt(2026, 8, 1).unwrap(),
        );
        let check_engine = Connection::open_in_memory().unwrap();
        assert_eq!(count_parquet_rows(&check_engine, &dir), 21);
    }

    /// One month of 1m candles (44_640 rows, spanning 31 daily partitions)
    /// must write well within NFR-1.5's year-per-5-minutes budget. Guards
    /// against regressing to a row-by-row INSERT, which measured at
    /// several minutes for this same input.
    #[test]
    fn writes_one_month_of_1m_candles_quickly() {
        let tmp = tempdir().unwrap();
        let store = KlineStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let klines: Vec<Kline> = (0..44_640i64).map(|i| kline_at(base, i)).collect();

        let start = std::time::Instant::now();
        let written = store.write_klines(&klines).unwrap();
        let elapsed = start.elapsed();

        assert_eq!(written, 44_640);
        assert!(
            elapsed.as_secs() < 15,
            "writing 44_640 rows took {elapsed:?}, too slow for NFR-1.5"
        );
    }
}
