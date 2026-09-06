use std::path::Path;

use chrono::{DateTime, Utc};
use duckdb::{AccessMode, Config, Connection};

use crate::error::StorageError;
use crate::paths::sql_quote_path;

const SCHEMA_VERSION: i64 = 1;

/// One row of the `symbols` registry (FR-2.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SymbolRecord {
    pub exchange: String,
    pub symbol: String,
    pub base_asset: String,
    pub quote_asset: String,
    pub status: String,
}

/// `meta.duckdb`: symbol registry, collector bookkeeping, schema version,
/// and the `klines`/`trades` views over the Parquet layout (FR-2.6, FR-2.7).
///
/// Per NFR-4.1, exactly one process may hold a writable `MetaStore` on a
/// given file at a time; every other process must use
/// [`MetaStore::open_read_only`].
pub struct MetaStore {
    conn: Connection,
}

impl MetaStore {
    /// Open for writing, creating the file and/or applying migrations as
    /// needed (FR-2.7). This is the single writer connection for the file.
    /// `data_root` is the same directory `KlineStore`/trade-writer use.
    pub fn open_writable(
        path: impl AsRef<Path>,
        data_root: impl AsRef<Path>,
    ) -> Result<Self, StorageError> {
        if let Some(parent) = path.as_ref().parent() {
            std::fs::create_dir_all(parent)?;
        }
        let conn = Connection::open(path.as_ref())?;
        let store = Self { conn };
        store.migrate()?;
        store.refresh_views(data_root.as_ref())?;
        Ok(store)
    }

    /// Open read-only — safe for any number of concurrent readers while a
    /// writer holds the file elsewhere (NFR-4.1).
    ///
    /// Does *not* (re)create the `klines`/`trades` views: DuckDB refuses any
    /// `CREATE` statement on a read-only-attached database, full stop, no
    /// exception for views (confirmed the hard way — an earlier version of
    /// this function assumed otherwise). Views are only ever created by
    /// [`MetaStore::open_writable`] (i.e. by `backfill`); a reader just
    /// queries whatever already exists, which stays fresh on its own since
    /// an existing view re-globs its Parquet files on every query.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let config = Config::default().access_mode(AccessMode::ReadOnly)?;
        let conn = Connection::open_with_flags(path.as_ref(), config)?;
        Ok(Self { conn })
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

    /// (Re)create the `klines`/`trades` views over their Parquet glob
    /// (FR-2.6). DuckDB validates the glob eagerly at `CREATE VIEW` time —
    /// if a dataset has no files yet (e.g. `trades` before Этап 3 ever
    /// writes one), that dataset's view is simply left absent for now.
    /// Once created, a view re-globs on every query, so it stays fresh as
    /// new partitions land without needing to be recreated (verified
    /// empirically against DuckDB's `read_parquet` before relying on it).
    ///
    /// `data_root` is resolved to an absolute path first — a relative glob
    /// gets re-evaluated against whatever the *querying* process's current
    /// directory happens to be, not the one that created the view (found
    /// this the hard way: `serve`'s own load test runs with its crate
    /// directory as CWD, not the repo root every manual CLI run in this
    /// project happened to share, and every `/ohlcv` call failed with "No
    /// files found" against a `data/klines/...` glob that was never wrong
    /// until the CWD actually differed).
    fn refresh_views(&self, data_root: &Path) -> Result<(), StorageError> {
        let data_root = std::path::absolute(data_root)?;
        self.try_create_glob_view("klines", &data_root.join("klines"))?;
        self.try_create_glob_view("trades", &data_root.join("trades"))?;
        Ok(())
    }

    fn try_create_glob_view(
        &self,
        view_name: &str,
        dataset_dir: &Path,
    ) -> Result<(), StorageError> {
        let glob = format!("{}/**/*.parquet", sql_quote_path(dataset_dir));
        // No `union_by_name`: every file here is written by our own
        // `KlineStore`/`TradeStore` with one fixed schema, so there is
        // nothing to reconcile — and asking DuckDB to check anyway means
        // re-reading every matched file's schema on every single query.
        // Measured impact under Этап 4's 100 RPS load test: ~268ms mean
        // server-side latency on a query that takes microseconds without
        // it; removing it is why that test passes at all.
        let sql = format!(
            "CREATE OR REPLACE VIEW {view_name} AS
             SELECT * FROM read_parquet('{glob}', hive_partitioning = true)"
        );
        match self.conn.execute_batch(&sql) {
            Ok(()) => Ok(()),
            Err(duckdb::Error::DuckDBFailure(_, Some(msg)))
                if msg.contains("No files found that match the pattern") =>
            {
                tracing::debug!(view = view_name, %glob, "no parquet files yet, view not (re)created");
                Ok(())
            }
            Err(e) => Err(e.into()),
        }
    }

    /// Record that `[exchange, symbol, dataset, interval]` has been
    /// collected up to `last_ts`, keeping the maximum across calls.
    /// `interval` is `None` for the `trades` dataset (TZ 5.3).
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

    /// Raw connection, for callers (e.g. `ma-analytics`) that need to run
    /// their own queries against the `klines`/`trades` views.
    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    /// Full resync of the symbol registry for `exchange` (FR-2.6): replaces
    /// every row for that exchange with `records` (`symbols --refresh`
    /// reflects the exchange's current `exchangeInfo`, not an incremental
    /// diff).
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

    /// All registered symbols (FR-5.1 `/symbols`).
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

    /// Whether `symbol` is in the registry, on any exchange (FR-5.3: the
    /// API validates a path symbol against the registry before touching
    /// the database proper).
    pub fn symbol_exists(&self, symbol: &str) -> Result<bool, StorageError> {
        let count: i64 = self.conn.query_row(
            "SELECT count(*) FROM symbols WHERE symbol = ?",
            duckdb::params![symbol],
            |r| r.get(0),
        )?;
        Ok(count > 0)
    }

    /// Last recorded timestamp for `[exchange, symbol, dataset, interval]`,
    /// if any collection has happened yet.
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
        // Reopening an already-migrated file must not fail or re-seed version rows.
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

        // No klines files yet: opening must succeed, view must not exist.
        let store = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        assert!(store
            .connection()
            .query_row("SELECT count(*) FROM klines", [], |r| r.get::<_, i64>(0))
            .is_err());
        drop(store);

        // Write one partition file directly (bypassing KlineStore, which
        // isn't under test here), then reopen: the view must now exist and
        // reflect it.
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

        // A second file added after the view exists must be picked up
        // without recreating it (views re-glob per query).
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
    fn read_only_open_does_not_attempt_create_and_can_query_existing_view() {
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

        // The writer creates the view...
        let writer = MetaStore::open_writable(&meta_path, &data_root).unwrap();
        drop(writer);

        // ...a completely separate read-only connection must be able to
        // open the same file and query that view (NFR-4.1: one writer, any
        // number of readers) without itself trying to CREATE anything,
        // which DuckDB rejects outright on a read-only-attached database.
        let reader = MetaStore::open_read_only(&meta_path).unwrap();
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

        // An older timestamp must not regress last_ts.
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

        // A second refresh with a different set must replace, not accumulate.
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
