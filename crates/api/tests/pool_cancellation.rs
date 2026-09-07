//! A cancelled request must not cost the pool a connection. Axum drops a
//! handler's future when the client disconnects, so anything that returns
//! the handle *after* the blocking task is awaited never runs — and the
//! frontend cancels in-flight requests on every symbol/interval switch,
//! which used to be enough to empty the pool and hang every later request
//! forever (with `/health` still answering 200, since it touches no
//! database).

use std::time::Duration;

use ma_api::{ApiError, DbPool};
use ma_storage::MetaStore;

const POOL_SIZE: usize = 2;

fn pool() -> (DbPool, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("tempdir");
    let meta_path = dir.path().join("meta.duckdb");
    drop(MetaStore::open_writable(&meta_path, dir.path()).expect("creating meta"));
    let pool = DbPool::new(&meta_path, dir.path(), POOL_SIZE).expect("opening pool");
    (pool, dir)
}

#[tokio::test]
async fn cancelling_requests_does_not_drain_the_pool() {
    let (pool, _dir) = pool();

    // Every handle checked out, then abandoned mid-flight — the same shape
    // as a browser navigating away from an in-flight fetch.
    for _ in 0..POOL_SIZE * 3 {
        let fut = pool.with_meta(|meta| {
            std::thread::sleep(Duration::from_millis(50));
            meta.list_symbols().map_err(ApiError::from)
        });
        tokio::time::timeout(Duration::from_millis(1), fut)
            .await
            .expect_err("the request is meant to be abandoned before it finishes");
    }

    // Give the abandoned blocking tasks time to finish and hand their
    // handles back.
    tokio::time::sleep(Duration::from_millis(500)).await;

    for i in 0..POOL_SIZE * 2 {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            pool.with_meta(|meta| meta.list_symbols().map_err(ApiError::from)),
        )
        .await;
        assert!(
            result.is_ok(),
            "request {i} hung: the pool lost a connection to a cancelled request"
        );
        result.unwrap().expect("query should succeed");
    }
}

#[tokio::test]
async fn a_panicking_query_does_not_drain_the_pool() {
    let (pool, _dir) = pool();

    for _ in 0..POOL_SIZE * 2 {
        let result = pool
            .with_meta(|_meta| -> Result<(), ApiError> { panic!("query blew up") })
            .await;
        assert!(
            result.is_err(),
            "a panicking query must surface as an error"
        );
    }

    for i in 0..POOL_SIZE * 2 {
        let result = tokio::time::timeout(
            Duration::from_secs(5),
            pool.with_meta(|meta| meta.list_symbols().map_err(ApiError::from)),
        )
        .await;
        assert!(result.is_ok(), "request {i} hung after a panicking query");
    }
}
