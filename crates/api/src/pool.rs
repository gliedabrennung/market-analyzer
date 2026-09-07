use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use ma_storage::MetaStore;
use tokio::sync::{mpsc, Mutex};

use crate::error::ApiError;

/// A small pool of read-only [`MetaStore`] handles (NFR-4.1: readers are
/// read-only; any number of them may coexist alongside the one writer).
///
/// Checkout/return is a channel, not a lock held across `.await`: a handle
/// is taken out of the channel, moved into [`tokio::task::spawn_blocking`]
/// for the actual (synchronous) DuckDB work — FR-5.4 forbids blocking the
/// async executor — and returned to the channel afterward.
pub struct DbPool {
    meta_db_path: PathBuf,
    data_root: PathBuf,
    tx: mpsc::Sender<MetaStore>,
    rx: Mutex<mpsc::Receiver<MetaStore>>,
}

impl DbPool {
    /// Opens `size` (at least 1) independent read-only handles on
    /// `meta_db_path`. `data_root` is the Parquet directory each handle
    /// builds its own views over — see `MetaStore::ensure_views`.
    pub fn new(
        meta_db_path: impl AsRef<Path>,
        data_root: impl AsRef<Path>,
        size: usize,
    ) -> Result<Self, ApiError> {
        let (tx, rx) = mpsc::channel(size.max(1));
        for _ in 0..size.max(1) {
            let store = MetaStore::open_read_only(meta_db_path.as_ref(), data_root.as_ref())
                .map_err(|e| ApiError::Internal(format!("opening db pool connection: {e}")))?;
            tx.try_send(store).map_err(|_| {
                ApiError::Internal("db pool channel capacity exceeded during init".to_string())
            })?;
        }
        Ok(Self {
            meta_db_path: meta_db_path.as_ref().to_path_buf(),
            data_root: data_root.as_ref().to_path_buf(),
            tx,
            rx: Mutex::new(rx),
        })
    }

    /// Run `f` against a pooled connection on the blocking thread pool.
    pub async fn with_meta<F, T>(&self, f: F) -> Result<T, ApiError>
    where
        F: FnOnce(&MetaStore) -> Result<T, ApiError> + Send + 'static,
        T: Send + 'static,
    {
        let store = {
            let mut rx = self.rx.lock().await;
            rx.recv()
                .await
                .ok_or_else(|| ApiError::Internal("db pool closed".to_string()))?
        };
        let tx = self.tx.clone();
        let meta_db_path = self.meta_db_path.clone();
        let data_root = self.data_root.clone();
        let joined = tokio::task::spawn_blocking(move || {
            // A dataset that had no Parquet files when this handle was
            // opened has no view yet; `serve` outlives the first `backfill`
            // that creates one, so the check has to happen per use rather
            // than once at startup. It costs a set lookup once both views
            // exist.
            let result = catch_unwind(AssertUnwindSafe(|| {
                store
                    .ensure_views()
                    .map_err(ApiError::from)
                    .and_then(|()| f(&store))
            }));

            // The handle goes back from *inside* the task, not after the
            // await below: a client that disconnects mid-request causes
            // axum to drop this handler's future, and then nothing after
            // that await ever runs. Returning it there leaked one handle
            // per cancelled request — and the frontend cancels in-flight
            // requests on every symbol/interval switch, so a few switches
            // emptied the pool and left every later request waiting on
            // `rx.recv()` forever, with `/health` (no database) still
            // answering 200 the whole time.
            match &result {
                Ok(_) => {
                    // Best-effort: the channel holds at most the handles
                    // this pool created, so it cannot actually be full.
                    let _ = tx.try_send(store);
                }
                Err(_) => {
                    // A panicked closure can leave its connection mid-
                    // statement; replace the handle rather than hand a
                    // questionable one to the next caller. Failing that,
                    // the pool runs one handle smaller — reduced
                    // concurrency, never wrong data, and still not a
                    // permanent stall.
                    drop(store);
                    if let Ok(fresh) = MetaStore::open_read_only(&meta_db_path, &data_root) {
                        let _ = tx.try_send(fresh);
                    }
                }
            }
            result
        })
        .await;

        match joined {
            Ok(Ok(result)) => result,
            Ok(Err(_panic)) => {
                tracing::error!("db pool worker panicked; pool slot replenished");
                Err(ApiError::Internal("db task panicked".to_string()))
            }
            Err(join_error) => Err(ApiError::Internal(format!("db task failed: {join_error}"))),
        }
    }
}
