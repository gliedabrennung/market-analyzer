# Market Analyzer

Сбор и аналитика биржевых данных (Rust + DuckDB) с веб-дашбордом (SolidJS).
Собирает OHLCV-свечи и сделки с Binance, считает VWAP/волатильность/аномалии/OFI/корреляции/спред, отдаёт через CLI и HTTP API.

## Требования

- Rust ≥ 1.80, компилятор C++ (DuckDB собирается из исходников, первая сборка 10-20 мин)
- Node.js `^20.19.0 || >=22.12.0` — только для фронтенда
- Linux/macOS, без внешних сервисов

## Запуск

### Docker

```bash
docker compose up --build
```

Бэкенд на `:8080`, фронтенд на `:8081`. Данные — в `./data/`. Первый запуск сам синхронизирует реестр символов; историю грузить отдельно (см. ниже).

### Нативно

```bash
cargo build --release
target/release/market-analyzer symbols --refresh
target/release/market-analyzer backfill --symbol BTCUSDT --interval 1m --from 2026-08-01 --to 2026-08-31
target/release/market-analyzer serve --port 8080

cd frontend && npm install && npm run dev   # http://localhost:5173
```

Каждый интервал (`1m/5m/15m/1h/4h/1d`) грузится отдельно — `backfill` не ресемплит.

## Конфигурация

Переменные окружения с префиксом `MA_` (см. `crates/cli/src/config.rs` за полным списком и дефолтами): `MA_DATA_DIR`, `MA_META_DB_PATH`, `MA_BINANCE_BASE_URL`, `MA_REQUESTS_PER_SECOND`, `MA_API_MAX_DATE_RANGE_DAYS`, `MA_API_DB_POOL_SIZE`, `MA_API_CORS_ORIGIN`, `MA_API_MAX_WS_CONNECTIONS`.

Фронтенд: `VITE_API_BASE_URL` (запекается в бандл на этапе сборки).

## Команды CLI

`backfill` · `stream` · `query` · `symbols [--refresh]` · `compact` · `serve [--port]`

## HTTP API

| Метод | Путь |
|---|---|
| GET | `/health`, `/symbols`, `/metrics` |
| GET | `/ohlcv/{symbol}` |
| GET | `/analytics/{symbol}/{vwap,volatility,anomalies,ofi}`, `/analytics/correlation` |
| WS | `/stream/{symbol}` |

Arrow IPC при `Accept: application/vnd.apache.arrow.stream`, иначе JSON. Ошибки — `{"error":{code,message,details}}`.

## Структура

```
crates/{core,exchanges,storage,analytics,api,cli}
frontend/   SolidJS + Vite + Arrow
```

## Тесты

```bash
cargo fmt --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace
cd frontend && npm run typecheck && npm run lint && npm test && npm run test:e2e
```

Замеры производительности — в [`BENCHMARKS.md`](BENCHMARKS.md).
