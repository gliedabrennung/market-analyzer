use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;
use duckdb::Connection;

use ma_core::Trade;

use crate::error::StorageError;
use crate::paths::{has_existing_parts, next_part_index, partition_dir, sql_quote_path};

const DATASET: &str = "trades";

type DedupKey = (String, i64);

/// Writes `Trade`s to the Parquet/hive layout from FR-2.1, deduplicating by
/// `(exchange, trade_id)` within each symbol/day partition (FR-2.3, TZ 5.1).
/// Mirrors `KlineStore` — see there for why writes go through an in-memory
/// DuckDB "SQL-to-Parquet engine" (architecture §3.4 variant A) rather than
/// a raw Arrow/Parquet writer, and why the dedup key set is cached in memory
/// after its first disk read per partition rather than re-scanned on every
/// write (a long-running `stream` process flushes far more often than
/// `backfill` writes once).
pub struct TradeStore {
    data_root: PathBuf,
    engine: Connection,
    dedup_cache: RefCell<HashMap<PathBuf, HashSet<DedupKey>>>,
}

impl TradeStore {
    /// Opens the in-memory DuckDB engine used to write into `data_root`.
    pub fn new(data_root: impl Into<PathBuf>) -> Result<Self, StorageError> {
        let engine = Connection::open_in_memory()?;
        Ok(Self {
            data_root: data_root.into(),
            engine,
            dedup_cache: RefCell::new(HashMap::new()),
        })
    }

    /// Write `trades`, skipping rows that already exist on disk under the
    /// `(exchange, trade_id)` dedup key. Returns the number of rows
    /// actually written.
    pub fn write_trades(&self, trades: &[Trade]) -> Result<usize, StorageError> {
        if trades.is_empty() {
            return Ok(0);
        }

        let mut groups: BTreeMap<(String, NaiveDate), Vec<&Trade>> = BTreeMap::new();
        for t in trades {
            let dt = t.ts.date_naive();
            groups
                .entry((t.symbol.as_str().to_string(), dt))
                .or_default()
                .push(t);
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
        trades: &[&Trade],
    ) -> Result<usize, StorageError> {
        let dir = partition_dir(&self.data_root, DATASET, symbol, dt);
        fs::create_dir_all(&dir)?;

        let mut existing = self.take_dedup_set(&dir)?;
        let fresh: Vec<&Trade> = trades
            .iter()
            .copied()
            .filter(|t| existing.insert((t.exchange.clone(), t.trade_id)))
            .collect();
        self.dedup_cache.borrow_mut().insert(dir.clone(), existing);

        if fresh.is_empty() {
            return Ok(0);
        }

        let index = next_part_index(&dir)?;
        let final_path = dir.join(format!("part-{index:04}.parquet"));
        let tmp_path = dir.join(format!("part-{index:04}.parquet.tmp"));

        self.copy_to_parquet(&fresh, &tmp_path)?;
        fs::rename(&tmp_path, &final_path)?;

        Ok(fresh.len())
    }

    /// Takes ownership of the cached dedup set for `dir` (loading it from
    /// disk on first touch); the caller reinserts the (possibly updated)
    /// set afterward. See `KlineStore::take_dedup_set` for why this shape
    /// (`remove` + reinsert, no `get_mut`) — it avoids a fallible lookup
    /// that would otherwise need an `unwrap`/`expect` (NFR-3.4).
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
        let sql = format!("SELECT exchange, trade_id FROM read_parquet('{glob}')");
        let mut stmt = self.engine.prepare(&sql)?;
        let mut rows = stmt.query([])?;
        while let Some(row) = rows.next()? {
            let exchange: String = row.get(0)?;
            let trade_id: i64 = row.get(1)?;
            set.insert((exchange, trade_id));
        }
        Ok(set)
    }

    fn copy_to_parquet(&self, trades: &[&Trade], dest: &Path) -> Result<(), StorageError> {
        self.engine.execute_batch(
            "CREATE OR REPLACE TEMP TABLE staging_trades (
                ts TIMESTAMP NOT NULL,
                symbol VARCHAR NOT NULL,
                exchange VARCHAR NOT NULL,
                trade_id BIGINT NOT NULL,
                price VARCHAR NOT NULL,
                qty VARCHAR NOT NULL,
                is_buyer_maker BOOLEAN NOT NULL
            )",
        )?;

        {
            let mut appender = self.engine.appender("staging_trades")?;
            for t in trades.iter().copied() {
                appender.append_row(duckdb::params![
                    t.ts.naive_utc(),
                    t.symbol.as_str(),
                    t.exchange.as_str(),
                    t.trade_id,
                    t.price.to_string(),
                    t.qty.to_string(),
                    t.is_buyer_maker,
                ])?;
            }
            appender.flush()?;
        }

        let dest_literal = sql_quote_path(dest);
        let copy_sql = format!(
            "COPY (
                SELECT ts, symbol, exchange, trade_id,
                       CAST(price AS DECIMAL(18,8)) AS price,
                       CAST(qty AS DECIMAL(18,8)) AS qty,
                       is_buyer_maker
                FROM staging_trades
                ORDER BY ts
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
    use ma_core::Symbol;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use tempfile::tempdir;

    fn trade_at(base: chrono::DateTime<Utc>, offset_secs: i64, trade_id: i64) -> Trade {
        Trade {
            ts: base + chrono::Duration::seconds(offset_secs),
            symbol: Symbol::new("BTCUSDT").unwrap(),
            exchange: "binance".to_string(),
            trade_id,
            price: Decimal::from_str("50000.00000000").unwrap(),
            qty: Decimal::from_str("0.50000000").unwrap(),
            is_buyer_maker: false,
        }
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
    fn writes_trades_and_dedups_on_rerun() {
        let tmp = tempdir().unwrap();
        let store = TradeStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let trades: Vec<Trade> = (0..5).map(|i| trade_at(base, i, i)).collect();

        assert_eq!(store.write_trades(&trades).unwrap(), 5);
        assert_eq!(store.write_trades(&trades).unwrap(), 0);

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
    fn dedup_key_is_exchange_and_trade_id_not_price_or_time() {
        // Two different trade_ids at the exact same instant must both survive.
        let tmp = tempdir().unwrap();
        let store = TradeStore::new(tmp.path()).unwrap();
        let base = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let trades = vec![trade_at(base, 0, 1), trade_at(base, 0, 2)];
        assert_eq!(store.write_trades(&trades).unwrap(), 2);
    }

    #[test]
    fn empty_input_writes_nothing() {
        let tmp = tempdir().unwrap();
        let store = TradeStore::new(tmp.path()).unwrap();
        assert_eq!(store.write_trades(&[]).unwrap(), 0);
    }
}
