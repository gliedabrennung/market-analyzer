//! Load test for the Этап 4 acceptance criterion: "100 RPS on `/ohlcv`
//! holds without a growing error rate." Builds the real app in-process
//! (against real local data from earlier stages) and drives it with a
//! connection-reusing `reqwest::Client` — a `curl` process per request was
//! tried first and the *client* became the bottleneck (~69 RPS) long
//! before the server would have.
//!
//! Depends on local state (`data/meta.duckdb` from a prior `backfill`),
//! not just network, so it is `#[ignore]`d like the other real-dependency
//! tests in this workspace. Run in release for a representative number:
//!   cargo test -p ma-api --release --test load_test -- --ignored --nocapture

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use ma_api::{build_app, ApiLimits, DbPool};
use ma_exchanges::binance::{BinanceConfig, BinanceSpot};

const TARGET_RPS: u64 = 100;
const DURATION_SECS: u64 = 20;
const CONCURRENCY: u64 = 20;

#[tokio::test]
#[ignore]
async fn sustains_100_rps_on_ohlcv_without_growing_errors() {
    let meta_db_path = std::path::PathBuf::from("../../data/meta.duckdb");
    assert!(
        meta_db_path.exists(),
        "run `market-analyzer backfill` first so {} exists",
        meta_db_path.display()
    );

    let pool = DbPool::new(&meta_db_path, 8).expect("opening db pool");
    let exchange = BinanceSpot::new(BinanceConfig::default()).expect("building exchange client");
    let limits = ApiLimits {
        max_date_range_days: 366,
        default_pagination_limit: 1000,
        max_pagination_limit: 10_000,
        max_correlation_symbols: 10,
        max_ws_connections: 64,
    };
    let app = build_app(pool, exchange, limits, "http://localhost:5173").expect("building app");

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("binding ephemeral port");
    let addr = listener.local_addr().expect("reading bound address");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serving");
    });

    let url =
        format!("http://{addr}/ohlcv/BTCUSDT?interval=1m&from=2026-08-01&to=2026-08-02&limit=100");
    let client = reqwest::Client::new();

    let total = Arc::new(AtomicU64::new(0));
    let errors = Arc::new(AtomicU64::new(0));
    let errors_per_second: Arc<Mutex<Vec<u64>>> =
        Arc::new(Mutex::new(vec![0; DURATION_SECS as usize + 1]));

    let start = Instant::now();
    let deadline = start + Duration::from_secs(DURATION_SECS);

    // CONCURRENCY workers, each pacing itself to TARGET_RPS/CONCURRENCY
    // req/sec, for TARGET_RPS in aggregate — deliberately fewer than
    // TARGET_RPS *simultaneous* in-flight requests. 100 fully independent
    // concurrent workers (each doing 1 req/s) measured *worse* throughput
    // than this despite a bigger DB pool, i.e. the ceiling here is
    // something that gets worse with raw concurrent-connection count, not
    // with request rate — plausibly many separate DuckDB read-only handles
    // to the same file contending internally. Rate, not raw concurrency,
    // is what FR-5.1's "100 RPS" actually asks for.
    let per_worker_interval = Duration::from_secs_f64(CONCURRENCY as f64 / TARGET_RPS as f64);
    let mut workers = Vec::new();
    for _ in 0..CONCURRENCY {
        let client = client.clone();
        let url = url.clone();
        let total = total.clone();
        let errors = errors.clone();
        let errors_per_second = errors_per_second.clone();
        workers.push(tokio::spawn(async move {
            while Instant::now() < deadline {
                let tick_start = Instant::now();
                match client.get(&url).send().await {
                    Ok(resp) if resp.status().is_success() => {}
                    other => {
                        let prev = errors.fetch_add(1, Ordering::Relaxed);
                        if prev < 3 {
                            match other {
                                Ok(resp) => {
                                    let status = resp.status();
                                    let body = resp.text().await.unwrap_or_default();
                                    eprintln!("sample failure: HTTP {status} body={body}");
                                }
                                Err(e) => eprintln!("sample failure: request error: {e}"),
                            }
                        }
                        let sec = tick_start.duration_since(start).as_secs() as usize;
                        if let Some(bucket) = errors_per_second.lock().unwrap().get_mut(sec) {
                            *bucket += 1;
                        }
                    }
                }
                total.fetch_add(1, Ordering::Relaxed);
                let remaining = per_worker_interval.saturating_sub(tick_start.elapsed());
                tokio::time::sleep(remaining).await;
            }
        }));
    }
    for w in workers {
        let _ = w.await;
    }

    if let Ok(resp) = client.get(format!("http://{addr}/metrics")).send().await {
        if let Ok(text) = resp.text().await {
            for line in text.lines() {
                if line.contains("http_request_duration_seconds") {
                    println!("{line}");
                }
            }
        }
    }

    let elapsed = start.elapsed().as_secs_f64();
    let total = total.load(Ordering::Relaxed);
    let errors = errors.load(Ordering::Relaxed);
    let achieved_rps = total as f64 / elapsed;

    println!("sent {total} requests in {elapsed:.1}s = {achieved_rps:.1} RPS, {errors} errors");
    println!("errors per second: {:?}", errors_per_second.lock().unwrap());

    assert!(
        achieved_rps >= TARGET_RPS as f64 * 0.9,
        "achieved only {achieved_rps:.1} RPS, target was {TARGET_RPS}"
    );
    assert_eq!(
        errors, 0,
        "error rate must not grow under sustained 100 RPS load"
    );
}
