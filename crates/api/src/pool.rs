use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use ma_storage::MetaStore;
use tokio::sync::{mpsc, Mutex};

use crate::error::ApiError;

pub struct DbPool {
    meta_db_path: PathBuf,
    data_root: PathBuf,
    tx: mpsc::Sender<MetaStore>,
    rx: Mutex<mpsc::Receiver<MetaStore>>,
}

impl DbPool {
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
            let result = catch_unwind(AssertUnwindSafe(|| {
                store
                    .ensure_views()
                    .map_err(ApiError::from)
                    .and_then(|()| f(&store))
            }));

            match &result {
                Ok(_) => {
                    let _ = tx.try_send(store);
                }
                Err(_) => {
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
