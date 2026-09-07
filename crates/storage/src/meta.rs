use std::cell::RefCell;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use duckdb::{AccessMode, Config, Connection};

use crate::error::StorageError;
use crate::paths::sql_quote_path;

const SCHEMA_VERSION: i64 = 1;

const DATASETS: [&str; 2] = ["klines", "trades"];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

pub struct MetaStore {
    conn: Connection,
    data_root: PathBuf,

    views_created: RefCell<HashSet<&'static str>>,
}

impl MetaStore {
    pub fn open_writable(
        path: impl AsRef<Path>,
        data_root: impl AsRef<Path>,
    ) -> Result<Self, StorageError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path.as_ref())?;
        let store = Self::new(conn, data_root.as_ref())?;
        store.migrate()?;
        store.drop_persisted_views()?;
        store.ensure_views()?;
        Ok(store)
    }

    pub fn open_read_only(
        path: impl AsRef<Path>,
        data_root: impl AsRef<Path>,
    ) -> Result<Self, StorageError> {
        let config = Config::default().access_mode(AccessMode::ReadOnly)?;
        let conn = Connection::open_with_flags(path.as_ref(), config)?;
        let store = Self::new(conn, data_root.as_ref())?;
        store.ensure_views()?;
        Ok(store)
    }

    fn new(conn: Connection, data_root: &Path) -> Result<Self, StorageError> {
        Ok(Self {
            conn,

            data_root: std::path::absolute(data_root)?,
            views_created: RefCell::new(HashSet::new()),
        })
    }

    fn drop_persisted_views(&self) -> Result<(), StorageError> {
        for dataset in DATASETS {
            self.conn
                .execute_batch(&format!("DROP VIEW IF EXISTS main.{dataset}"))?;
        }
        Ok(())
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.conn
            .execute_batch(include_str!("../sql/migrations/0001_init.sql"))?;
        let version: i64 = self
            .conn
            .query_row("SELECT version FROM schema_version", [], |r| r.get(0))?;
        if version != SCHEMA_VERSION {
            return Err(StorageError::SchemaVersion {
                found: version,
                expected: SCHEMA_VERSION,
            });
        }
        Ok(())
    }

    pub fn ensure_views(&self) -> Result<(), StorageError> {
        for dataset in DATASETS {
            if self.views_created.borrow().contains(dataset) {
                continue;
            }
            if self.try_create_glob_view(dataset, &self.data_root.join(dataset))? {
                self.views_created.borrow_mut().insert(dataset);
            }
        }
        Ok(())
    }

    fn try_create_glob_view(
        &self,
        view_name: &str,
        dataset_dir: &Path,
    ) -> Result<bool, StorageError> {
        let glob = format!("{}/**/*.parquet", sql_quote_path(dataset_dir));

        let sql = format!(
            "CREATE OR REPLACE TEMP VIEW {view_name} AS
             SELECT * FROM read_parquet('{glob}', hive_partitioning = true)"
        );
        match self.conn.execute_batch(&sql) {
            Ok(()) => Ok(true),
            Err(duckdb::Error::DuckDBFailure(_, Some(msg)))
                if msg.contains("No files found that match the pattern") =>
            {
                tracing::debug!(view = view_name, %glob, "no parquet files yet, view not created");
                Ok(false)
            }
            Err(e) => Err(e.into()),
        }
    }

    pub fn upsert_collector_state(
        &self,
        exchange: &str,
        symbol: &str,
        dataset: &str,
        interval: Option<&str>,
        last_ts: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let last_ts = last_ts.naive_utc();
        let now = now.naive_utc();
        self.conn.execute(
            "INSERT INTO collector_state (exchange, symbol, dataset, interval, last_ts, updated_at)
             SELECT ?, ?, ?, ?, ?, ?
             WHERE NOT EXISTS (
                 SELECT 1 FROM collector_state
                 WHERE exchange = ? AND symbol = ? AND dataset = ? AND interval IS NOT DISTINCT FROM ?
             )",
            duckdb::params![exchange, symbol, dataset, interval, last_ts, now, exchange, symbol, dataset, interval],
        )?;
        self.conn.execute(
            "UPDATE collector_state
             SET last_ts = GREATEST(last_ts, ?), updated_at = ?
             WHERE exchange = ? AND symbol = ? AND dataset = ? AND interval IS NOT DISTINCT FROM ?",
            duckdb::params![last_ts, now, exchange, symbol, dataset, interval],
        )?;
        Ok(())
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn replace_symbols(
        &self,
        exchange: &str,
        records: &[SymbolRecord],
        now: DateTime<Utc>,
    ) -> Result<(), StorageError> {
        let now = now.naive_utc();
        self.conn.execute(
            "DELETE FROM symbols WHERE exchange = ?",
            duckdb::params![exchange],
        )?;
        let mut stmt = self.conn.prepare(
            "INSERT INTO symbols (exchange, symbol, base_asset, quote_asset, status, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)",
        )?;
        for r in records {
            stmt.execute(duckdb::params![
                exchange,
                r.symbol,
                r.base_asset,
                r.quote_asset,
                r.status,
                now
            ])?;
        }
        Ok(())
    }

    pub fn list_symbols(&self) -> Result<Vec<SymbolRecord>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT exchange, symbol, base_asset, quote_asset, status FROM symbols ORDER BY symbol",
        )?;
        let mut rows = stmt.query([])?;
        let mut out = Vec::new();
        while let Some(row) = rows.next()? {
            out.push(SymbolRecord {
                exchange: row.get(0)?,
                symbol: row.get(1)?,
                base_asset: row.get(2)?,
                quote_asset: row.get(3)?,
                status: row.get(4)?,
            });
        }
        Ok(out)
    }

    pub fn symbols_with_data(&self) -> Result<HashSet<String>, StorageError> {
        let mut stmt = self
            .conn
            .prepare("SELECT DISTINCT symbol FROM collector_state")?;
        let mut rows = stmt.query([])?;
        let mut out = HashSet::new();
        while let Some(row) = rows.next()? {
            out.insert(row.get::<_, String>(0)?);
        }
        Ok(out)
    }

    pub fn symbol_exists(&self, symbol: &str) -> Result<bool, StorageError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM symbols WHERE symbol = ?",
            duckdb::params![symbol],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn last_ts(
        &self,
        exchange: &str,
        symbol: &str,
        dataset: &str,
        interval: Option<&str>,
    ) -> Result<Option<DateTime<Utc>>, StorageError> {
        let mut stmt = self.conn.prepare(
            "SELECT last_ts FROM collector_state
             WHERE exchange = ? AND symbol = ? AND dataset = ? AND interval IS NOT DISTINCT FROM ?",
        )?;
        let mut rows = stmt.query(duckdb::params![exchange, symbol, dataset, interval])?;
        match rows.next()? {
            Some(row) => {
                let naive: chrono::NaiveDateTime = row.get(0)?;
                Ok(Some(DateTime::from_naive_utc_and_offset(naive, Utc)))
            }
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use tempfile::tempdir;

    #[test]
    fn migrate_is_idempotent_across_reopen() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("meta.duckdb");
        {
            let store = MetaStore::open_writable(&path, dir.path()).unwrap();
            drop(store);
        }

        let store = MetaStore::open_writable(&path, dir.path()).unwrap();
        let count: i64 = store
            .conn
            .query_row("SELECT count(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn klines_view_absent_without_data_then_appears_and_stays_fresh() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join("data");
        let meta_path = dir.path().join("meta.duckdb");

        let store = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        assert!(store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get::<_, i64>(0))
            .is_err());
        drop(store);

        let part_dir = data_root.join("klines/symbol=BTCUSDT/dt=2026-08-01");
        std::fs::create_dir_all(&part_dir).unwrap();
        let scratch = Connection::open_in_memory().unwrap();
        scratch
            .execute_batch(&format!(
                "COPY (SELECT 1 AS x) TO '{}' (FORMAT PARQUET)",
                sql_quote_path(&part_dir.join("part-0000.parquet"))
            ))
            .unwrap();

        let store = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        let count: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);

        let part_dir2 = data_root.join("klines/symbol=ETHUSDT/dt=2026-08-01");
        std::fs::create_dir_all(&part_dir2).unwrap();
        scratch
            .execute_batch(&format!(
                "COPY (SELECT 2 AS x) TO '{}' (FORMAT PARQUET)",
                sql_quote_path(&part_dir2.join("part-0000.parquet"))
            ))
            .unwrap();
        let count: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn ensure_views_publishes_files_written_after_open() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join("data");
        let meta_path = dir.path().join("meta.duckdb");

        let store = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        assert!(store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get::<_, i64>(0))
            .is_err());

        let part_dir = data_root.join("klines/symbol=BTCUSDT/dt=2026-08-01");
        std::fs::create_dir_all(&part_dir).unwrap();
        Connection::open_in_memory()
            .unwrap()
            .execute_batch(&format!(
                "COPY (SELECT 1 AS x) TO '{}' (FORMAT PARQUET)",
                sql_quote_path(&part_dir.join("part-0000.parquet"))
            ))
            .unwrap();

        store.ensure_views().unwrap();

        let count: i64 = store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn read_only_open_builds_its_own_views_and_can_query_them() {
        let dir = tempdir().unwrap();
        let data_root = dir.path().join("data");
        let meta_path = dir.path().join("meta.duckdb");

        let part_dir = data_root.join("klines/symbol=BTCUSDT/dt=2026-08-01");
        std::fs::create_dir_all(&part_dir).unwrap();
        Connection::open_in_memory()
            .unwrap()
            .execute_batch(&format!(
                "COPY (SELECT 1 AS x) TO '{}' (FORMAT PARQUET)",
                sql_quote_path(&part_dir.join("part-0000.parquet"))
            ))
            .unwrap();

        let writer = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        drop(writer);

        let reader = MetaStore::open_read_only(&meta_path, &data_root).unwrap();
        let count: i64 = reader
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn data_moved_to_another_path_is_still_queryable() {
        let dir = tempdir().unwrap();
        let original = dir.path().join("host/data");
        let part_dir = original.join("klines/symbol=BTCUSDT/dt=2026-08-01");
        std::fs::create_dir_all(&part_dir).unwrap();
        Connection::open_in_memory()
            .unwrap()
            .execute_batch(&format!(
                "COPY (SELECT 1 AS x) TO '{}' (FORMAT PARQUET)",
                sql_quote_path(&part_dir.join("part-0000.parquet"))
            ))
            .unwrap();
        let meta_path = original.join("meta.duckdb");
        drop(MetaStore::open_writable(&meta_path, &original).unwrap());

        let mounted = dir.path().join("container/data");
        std::fs::create_dir_all(mounted.parent().unwrap()).unwrap();
        std::fs::rename(&original, &mounted).unwrap();

        let reader = MetaStore::open_read_only(mounted.join("meta.duckdb"), &mounted).unwrap();
        let count: i64 = reader
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn collector_state_upsert_keeps_max_timestamp() {
        let dir = tempdir().unwrap();
        let store = MetaStore::open_writable(dir.path().join("meta.duckdb"), dir.path()).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        let t2 = Utc.with_ymd_and_hms(2026, 8, 2, 0, 0, 0).unwrap();

        store
            .upsert_collector_state("binance", "BTCUSDT", "klines", Some("1m"), t1, t1)
            .unwrap();
        assert_eq!(
            store
                .last_ts("binance", "BTCUSDT", "klines", Some("1m"))
                .unwrap(),
            Some(t1)
        );

        store
            .upsert_collector_state("binance", "BTCUSDT", "klines", Some("1m"), t2, t2)
            .unwrap();
        assert_eq!(
            store
                .last_ts("binance", "BTCUSDT", "klines", Some("1m"))
                .unwrap(),
            Some(t2)
        );

        store
            .upsert_collector_state("binance", "BTCUSDT", "klines", Some("1m"), t1, t2)
            .unwrap();
        assert_eq!(
            store
                .last_ts("binance", "BTCUSDT", "klines", Some("1m"))
                .unwrap(),
            Some(t2)
        );
    }

    #[test]
    fn collector_state_handles_null_interval_for_trades() {
        let dir = tempdir().unwrap();
        let store = MetaStore::open_writable(dir.path().join("meta.duckdb"), dir.path()).unwrap();
        let t1 = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();
        store
            .upsert_collector_state("binance", "BTCUSDT", "trades", None, t1, t1)
            .unwrap();
        assert_eq!(
            store.last_ts("binance", "BTCUSDT", "trades", None).unwrap(),
            Some(t1)
        );
    }

    #[test]
    fn replace_symbols_is_a_full_resync_and_exists_check_works() {
        let dir = tempdir().unwrap();
        let store = MetaStore::open_writable(dir.path().join("meta.duckdb"), dir.path()).unwrap();
        let now = Utc.with_ymd_and_hms(2026, 8, 1, 0, 0, 0).unwrap();

        let first = vec![
            SymbolRecord {
                exchange: "binance".to_string(),
                symbol: "BTCUSDT".to_string(),
                base_asset: "BTC".to_string(),
                quote_asset: "USDT".to_string(),
                status: "TRADING".to_string(),
            },
            SymbolRecord {
                exchange: "binance".to_string(),
                symbol: "ETHUSDT".to_string(),
                base_asset: "ETH".to_string(),
                quote_asset: "USDT".to_string(),
                status: "TRADING".to_string(),
            },
        ];
        store.replace_symbols("binance", &first, now).unwrap();
        assert!(store.symbol_exists("BTCUSDT").unwrap());
        assert!(store.symbol_exists("ETHUSDT").unwrap());
        assert!(!store.symbol_exists("SOLUSDT").unwrap());
        assert_eq!(store.list_symbols().unwrap().len(), 2);

        let second = vec![SymbolRecord {
            exchange: "binance".to_string(),
            symbol: "SOLUSDT".to_string(),
            base_asset: "SOL".to_string(),
            quote_asset: "USDT".to_string(),
            status: "TRADING".to_string(),
        }];
        store.replace_symbols("binance", &second, now).unwrap();
        assert!(!store.symbol_exists("BTCUSDT").unwrap());
        assert!(store.symbol_exists("SOLUSDT").unwrap());
        assert_eq!(store.list_symbols().unwrap().len(), 1);
    }
}
