# Market Analyzer

Сервис сбора и аналитики биржевых данных на Rust + DuckDB. Собирает OHLCV-свечи
и сделки с Binance (REST backfill + WebSocket live-поток), хранит их в
партиционированном Parquet, считает аналитику (VWAP, волатильность, аномалии
объёма, OFI, корреляции, межбиржевой спред) через DuckDB SQL и отдаёт всё
через CLI и HTTP API.

Источники требований: `market-analyzer-tz.md` (ТЗ, приоритетный) и
`market-analyzer-architecture.md` (архитектура).

## Требования

- Rust ≥ 1.80 (см. `rust-toolchain`/`rust-version` в `Cargo.toml`)
- Linux или macOS (NFR-4.3)
- Компилятор C++ (для сборки DuckDB из исходников, `features = ["bundled"]`) —
  на большинстве систем уже стоит; первая сборка компилирует DuckDB с нуля и
  занимает **10-20 минут**, дальше пересборки быстрые (инкрементальные)
- Никаких внешних сервисов не требуется — только файловая система (NFR-4.2)

## Сборка

```bash
cargo build --release
# бинарник: target/release/market-analyzer
```

## Быстрый старт

```bash
# 1. Историческая загрузка (Этап 1)
market-analyzer backfill --symbol BTCUSDT --interval 1m \
  --from 2026-08-01 --to 2026-08-31

# 2. Реестр символов — нужен для API-валидации (FR-5.3) и справки
market-analyzer symbols --refresh

# 3. Аналитика прямо из CLI (Этап 2)
market-analyzer query --name vwap --symbol BTCUSDT \
  --param interval=1m --param window=20 --format table

# 4. Live-поток в реальном времени (Этап 3), Ctrl+C — грациозная остановка
market-analyzer stream --symbol BTCUSDT --symbol ETHUSDT --dataset trades

# 5. HTTP API (Этап 4)
market-analyzer serve --port 8080
curl 'http://localhost:8080/ohlcv/BTCUSDT?interval=1m&from=2026-08-01&to=2026-08-02'

# 6. Склейка part-файлов партиции (Этап 5)
market-analyzer compact --symbol BTCUSDT
```

Данные по умолчанию лежат в `./data/` (`data/klines/`, `data/trades/`,
`data/meta.duckdb`).

## Конфигурация (FR-6.1)

Настройки берутся из значений по умолчанию → TOML-файла (`--config path.toml`)
→ переменных окружения с префиксом `MA_` (наивысший приоритет).

| Переменная | По умолчанию | Смысл |
|---|---|---|
| `MA_DATA_DIR` | `data` | корень Parquet-данных |
| `MA_META_DB_PATH` | `data/meta.duckdb` | путь к метабазе |
| `MA_BINANCE_BASE_URL` | `https://api.binance.com` | REST-эндпоинт биржи |
| `MA_REQUESTS_PER_SECOND` | `5` | лимит собственного rate limiter'а (FR-1.3) |
| `MA_API_MAX_DATE_RANGE_DAYS` | `366` | максимальная ширина `from`/`to` в API (FR-5.3) |
| `MA_API_DB_POOL_SIZE` | `4` | число read-only DuckDB-подключений у `serve` (FR-5.4) |

`--config`/`MA_DATA_DIR` и т.п. должны указывать на один и тот же каталог во
всех командах одного проекта — иначе `backfill`/`stream` и `query`/`serve`
будут смотреть на разные данные.

## Команды (FR-4.1)

### `backfill`
```
market-analyzer backfill --symbol SYM [--symbol SYM ...] --interval {1m,5m,15m,1h,4h,1d} --from YYYY-MM-DD [--to YYYY-MM-DD]
```
Пагинация и обработка rate-limit (429 — бэкофф, 418 — пауза по
`Retry-After`) — внутри, автоматически. Идемпотентно: повторный запуск не
плодит дубли (дедуп по `(exchange, symbol, interval, open_time)`).

### `stream`
```
market-analyzer stream --symbol SYM [--symbol SYM ...] [--dataset klines|trades]
```
Klines стримятся на `1m`. Живёт, пока не придёт `SIGINT`/`SIGTERM` —
флашит буфер и завершается за секунды (FR-6.4). При обрыве соединения —
реконнект с бэкоффом и обязательный REST-добор пропущенного окна (FR-1.5).

### `query`
```
market-analyzer query --name NAME --symbol SYM [--param k=v ...] [--format table|json|csv]
```
`NAME` — один из: `resample`, `vwap`, `volatility`, `anomalies`, `ofi`,
`correlation`, `spread`. Параметры (все опциональны, есть разумные
дефолты) — через `--param`:

| `--name` | Параметры |
|---|---|
| `vwap` | `interval` (1m), `window` (20) |
| `volatility` | `interval` (1m), `window` (20) |
| `anomalies` | `interval` (1m), `window` (100), `threshold` (3.0) |
| `resample` | `bucket_seconds` (300), `from`, `to` |
| `ofi` | `bucket_seconds` (60), `from`, `to` |
| `correlation` | `symbols=A,B,C` (нужно ≥2), `interval` (1m) |
| `spread` | `interval` (1m), `other_symbol` (обязателен), `exchange`/`other_exchange` (binance) |

### `symbols`
```
market-analyzer symbols [--refresh]
```
Без `--refresh` — печатает текущий реестр; с `--refresh` — синхронизирует
его с `exchangeInfo` биржи (нужно один раз перед первым использованием API).

### `compact`
```
market-analyzer compact [--symbol SYM] [--date YYYY-MM-DD]
```
Сливает `part-*.parquet` каждой затронутой партиции в один файл. Без
флагов — проходит по всем партициям обоих датасетов. Безопасно к падению:
новый файл дописывается и **проверяется агрегатной чек-суммой** против
оригиналов до того, как оригиналы удаляются.

### `serve`
```
market-analyzer serve [--port 8080]
```
Поднимает HTTP API (см. ниже). Требует, чтобы `symbols --refresh` уже был
хоть раз запущен — иначе `/ohlcv`, `/analytics/*` и `/stream/*` будут
всегда отвечать `404 unknown_symbol`.

## HTTP API (FR-5.1)

| Метод | Путь | Параметры |
|---|---|---|
| GET | `/health` | — |
| GET | `/symbols` | — |
| GET | `/ohlcv/{symbol}` | `interval`, `from`, `to`, `limit`, `offset` |
| GET | `/analytics/{symbol}/vwap` | `interval`, `window`, `limit`, `offset` |
| GET | `/analytics/{symbol}/volatility` | `interval`, `window`, `limit`, `offset` |
| GET | `/analytics/{symbol}/anomalies` | `interval`, `window`, `threshold`, `limit`, `offset` |
| GET | `/analytics/{symbol}/ofi` | `bucket`, `from`, `to`, `limit`, `offset` |
| GET | `/analytics/correlation` | `symbols` (через запятую), `interval` |
| WS | `/stream/{symbol}` | проброс живых сделок |
| GET | `/metrics` | Prometheus-метрики (FR-6.3) |

`from`/`to` — `YYYY-MM-DD` или RFC3339. Ошибки — всегда
`{"error":{"code","message","details"}}` (FR-5.2): `400` — невалидные
параметры, `404` — неизвестный символ, `429` — превышен лимит, `500` —
внутренняя ошибка.

```bash
curl 'http://localhost:8080/analytics/BTCUSDT/vwap?window=20&limit=5'
curl 'http://localhost:8080/analytics/correlation?symbols=BTCUSDT,ETHUSDT'
```

## Разработка

```bash
cargo fmt --check                                   # NFR-3.2
cargo clippy --workspace --all-targets -- -D warnings  # NFR-3.1
cargo test --workspace                              # юнит + интеграционные

# сетевые/реально-зависимые тесты (не входят в дефолтный прогон, TЗ §7):
cargo test -p ma-exchanges --test reconnect_gapfill -- --ignored --nocapture
cargo test -p ma-api --release --test load_test -- --ignored --nocapture

# покрытие analytics+storage (NFR-3.3, порог 70%)
cargo llvm-cov -p ma-storage -p ma-analytics --summary-only
```

Замеры производительности (NFR-1.x) и отчёт по compaction — в
[`BENCHMARKS.md`](BENCHMARKS.md).

## Структура workspace

```
crates/
  core/        # доменные типы: Kline, Trade, Symbol, Interval — без знания о Binance/HTTP/Parquet
  exchanges/   # ExchangeSource + binance/ (REST + WS); новая биржа = новый модуль, трейт не меняется
  storage/     # KlineStore/TradeStore (Parquet), BatchBuffer, compact, MetaStore (meta.duckdb)
  analytics/   # SQL-запросы (sql/*.sql + include_str!) и типизированные обёртки
  api/         # axum HTTP API поверх пула read-only DuckDB-подключений
  cli/         # market-analyzer: backfill/stream/query/compact/symbols/serve
```

## Известные ограничения

- REST-добор сделок после реконнекта идёт через `aggTrades` (у Binance нет
  time-ranged эндпоинта для сырых сделок) — id добранных строк в пространстве
  aggTradeId, не совпадает с id живого потока (дублей не даёт, но это не то
  же самое ID-пространство).
- `WS /stream/{symbol}` открывает отдельную подписку на Binance на каждого
  подключённого клиента (не мультиплексируется).
- Полный 12-часовой прогон `stream` без утечек памяти не воспроизведён в
  рамках разработки (см. `BENCHMARKS.md`) — 8-минутный прогон на 20 символах
  показал здоровый профиль (~105 МБ из бюджета 512 МБ), но это не то же
  самое, что 12 часов.
