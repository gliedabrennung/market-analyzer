use std::path::Path;

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
    tx: mpsc::Sender<MetaStore>,
    rx: Mutex<mpsc::Receiver<MetaStore>>,
}

impl DbPool {
    /// Opens `size` (at least 1) independent read-only handles on `meta_db_path`.
    pub fn new(meta_db_path: impl AsRef<Path>, size: usize) -> Result<Self, ApiError> {
        let (tx, rx) = mpsc::channel(size.max(1));
        for _ in 0..size.max(1) {
            let store = MetaStore::open_read_only(meta_db_path.as_ref())
                .map_err(|e| ApiError::Internal(format!("opening db pool connection: {e}")))?;
            tx.try_send(store).map_err(|_| {
                ApiError::Internal("db pool channel capacity exceeded during init".to_string())
            })?;
        }
        Ok(Self {
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
        let joined = tokio::task::spawn_blocking(move || {
            let result = f(&store);
            (store, result)
        })
        .await;
        match joined {
            Ok((store, result)) => {
                // Best-effort return: if the channel is somehow full (it
                // never should be — we only ever hold as many handles as
                // capacity), drop it rather than block or panic.
                let _ = tx.try_send(store);
                result
            }
            Err(join_error) => Err(ApiError::Internal(format!("db task failed: {join_error}"))),
        }
    }
}
