# Market Analyzer: архитектура на Rust + DuckDB

Сервис собирает рыночные данные с криптобирж (REST + WebSocket), укладывает их в
колоночное хранилище (Parquet + DuckDB) и отдаёт аналитику через SQL и HTTP API.

---

## 1. Общая схема

```
                   ┌──────────────────────────────────────┐
   Binance REST ──▶│  collector (REST backfill)           │
                   │  - klines, exchangeInfo              │
                   └──────────────┬───────────────────────┘
                                  │  Vec<Kline>
                   ┌──────────────▼───────────────────────┐
   Binance WS   ──▶│  streamer (WebSocket live)           │
   (trades,        │  - trade / kline / depth streams     │
    depth)         │  - reconnect + backfill gaps         │
                   └──────────────┬───────────────────────┘
                                  │  mpsc channel
                   ┌──────────────▼───────────────────────┐
                   │  writer                              │
                   │  - буферизация (N строк / T секунд)  │
                   │  - Arrow RecordBatch                 │
                   │  - COPY → Parquet (партиции)         │
                   └──────────────┬───────────────────────┘
                                  │
              ┌───────────────────▼────────────────────┐
              │  storage                               │
              │  data/trades/symbol=BTCUSDT/dt=.../    │
              │  data/klines/symbol=.../dt=.../        │
              │  meta.duckdb  (агрегаты, views, кэш)   │
              └───────────────────┬────────────────────┘
                                  │  duckdb-rs (embedded)
              ┌───────────────────▼────────────────────┐
              │  analytics                             │
              │  - VWAP / volatility / OFI             │
              │  - anomaly detection                   │
              │  - корреляции, ASOF JOIN               │
              └───────────────────┬────────────────────┘
                                  │
              ┌───────────────────▼────────────────────┐
              │  api (axum)  +  cli (clap)             │
              └────────────────────────────────────────┘
```

Ключевая идея разделения: **Rust держит на себе I/O и надёжность, DuckDB — всю
математику и агрегации.** Не нужно писать скользящие окна руками — они есть в SQL.

---

## 2. Слой сбора данных (ingestion)

### 2.1 REST backfill

Нужен для двух задач: первичная загрузка истории и закрытие дыр после
переподключения WebSocket.

- Endpoint: `GET /api/v3/klines?symbol=BTCUSDT&interval=1m&startTime=...&limit=1000`
- Ответ — массив массивов, поэтому парсить лучше в промежуточную структуру и
  маппить вручную, а не через `#[derive(Deserialize)]` по именам полей.
- Пагинация: идём окнами по 1000 свечей, сдвигая `startTime` по последнему
  `close_time + 1`.
- Rate limit у Binance — весовой (weight per minute), а не «запросов в секунду».
  Заводим простой лимитер (`governor` или самописный token bucket) и
  экспоненциальный бэкофф на `429`/`418`.

```rust
pub struct RestClient {
    http: reqwest::Client,
    base_url: String,
    limiter: RateLimiter,
}

impl RestClient {
    pub async fn klines(
        &self,
        symbol: &str,
        interval: Interval,
        range: TimeRange,
    ) -> Result<Vec<Kline>>;

    pub async fn exchange_info(&self) -> Result<Vec<SymbolMeta>>;
}
```

### 2.2 WebSocket streaming

- Combined stream: `wss://stream.binance.com:9443/stream?streams=btcusdt@trade/ethusdt@trade`
- Крейты: `tokio-tungstenite` + `futures-util`.
- Обязательно: ping/pong keepalive, реконнект с бэкоффом, и **после реконнекта —
  REST-добор пропущенного интервала**. Это то, что отличает игрушку от рабочего
  коллектора.
- Каждое сообщение нормализуется в общий внутренний тип и уходит в
  `tokio::sync::mpsc` канал к writer'у.

```rust
pub enum MarketEvent {
    Trade(Trade),
    Kline(Kline),
    DepthUpdate(DepthUpdate),
}
```

### 2.3 Абстракция над биржей

Чтобы потом добавить Bybit/OKX без переписывания:

```rust
#[async_trait]
pub trait ExchangeSource: Send + Sync {
    fn name(&self) -> &'static str;
    async fn fetch_klines(&self, req: KlineRequest) -> Result<Vec<Kline>>;
    async fn subscribe(&self, symbols: &[String]) -> Result<EventStream>;
}
```

---

## 3. Слой хранения

### 3.1 Почему Parquet + DuckDB, а не «просто БД»

- Parquet — долговременное сырьё, сжимается в 5–10 раз, читается кем угодно
  (pandas, Polars, Spark).
- DuckDB читает Parquet **напрямую**, без импорта: `read_parquet('data/**/*.parquet')`.
  Отдельного ETL-шага нет.
- Партиционирование по символу и дате даёт partition pruning — запрос за один день
  не читает весь архив.

### 3.2 Layout на диске

```
data/
  trades/
    symbol=BTCUSDT/dt=2026-09-05/part-0001.parquet
    symbol=ETHUSDT/dt=2026-09-05/part-0001.parquet
  klines/
    interval=1m/symbol=BTCUSDT/dt=2026-09-05/part-0001.parquet
meta.duckdb          # views, материализованные агрегаты, состояние коллектора
```

### 3.3 Схемы

**trades**

| поле | тип | комментарий |
|---|---|---|
| `ts` | `TIMESTAMP` | время сделки (мс с биржи) |
| `symbol` | `VARCHAR` | партиционный ключ |
| `trade_id` | `BIGINT` | для дедупликации |
| `price` | `DECIMAL(18,8)` | не `DOUBLE` — важно для денег |
| `qty` | `DECIMAL(18,8)` | |
| `is_buyer_maker` | `BOOLEAN` | сторона агрессора |

**klines**

| поле | тип |
|---|---|
| `open_time` | `TIMESTAMP` |
| `symbol` | `VARCHAR` |
| `interval` | `VARCHAR` |
| `open`, `high`, `low`, `close` | `DECIMAL(18,8)` |
| `volume`, `quote_volume` | `DECIMAL(28,8)` |
| `trades_count` | `INTEGER` |
| `taker_buy_base` | `DECIMAL(28,8)` |

### 3.4 Запись

Два рабочих варианта:

**А. Через DuckDB (проще).** Копим батч в памяти, вставляем в temp-таблицу через
`Appender` из `duckdb-rs`, затем выгружаем:

```sql
COPY (SELECT * FROM staging_trades)
TO 'data/trades'
(FORMAT PARQUET, PARTITION_BY (symbol, dt), OVERWRITE_OR_IGNORE, COMPRESSION zstd);
```

**Б. Через arrow/parquet напрямую (быстрее, больше кода).** Собираем
`RecordBatch`, пишем `ArrowWriter` в файл. Даёт полный контроль над row group size.

Для MVP берём вариант А.

**Важное ограничение:** файл DuckDB может держать на запись только **один
процесс**. Если API и коллектор — разные процессы, то либо API открывает базу
`read_only`, либо (лучше) они живут в одном процессе, а DuckDB-коннекты берутся
из пула. Parquet-файлов это не касается — их читать может кто угодно параллельно.

### 3.5 Буферизация

```rust
pub struct BatchWriter {
    buf: Vec<Trade>,
    max_rows: usize,        // напр. 50_000
    max_age: Duration,      // напр. 30s
    conn: duckdb::Connection,
}
```
Флаш по первому из условий. Мелкие Parquet-файлы — главный враг производительности,
поэтому раз в сутки полезен compaction-джоб, склеивающий `part-*.parquet` одной
партиции в один файл.

---

## 4. Аналитический слой

Здесь весь смысл проекта. Всё это — SQL, Rust только подставляет параметры.

### 4.1 Ресемплинг тиков в свечи любого таймфрейма

```sql
SELECT
    time_bucket(INTERVAL '5 minutes', ts) AS bucket,
    symbol,
    first(price ORDER BY ts)  AS open,
    max(price)                AS high,
    min(price)                AS low,
    last(price ORDER BY ts)   AS close,
    sum(qty)                  AS volume,
    count(*)                  AS trades
FROM read_parquet('data/trades/**/*.parquet', hive_partitioning = true)
WHERE symbol = ? AND ts BETWEEN ? AND ?
GROUP BY 1, 2
ORDER BY 1;
```

### 4.2 VWAP и скользящая волатильность

```sql
WITH base AS (
    SELECT open_time, symbol, close, volume,
           ln(close / lag(close) OVER w) AS log_ret
    FROM klines
    WHERE symbol = ?
    WINDOW w AS (PARTITION BY symbol ORDER BY open_time)
)
SELECT
    open_time,
    close,
    sum(close * volume) OVER w20 / sum(volume) OVER w20  AS vwap_20,
    stddev_samp(log_ret) OVER w20 * sqrt(1440)           AS realized_vol_daily
FROM base
WINDOW w20 AS (
    PARTITION BY symbol ORDER BY open_time
    ROWS BETWEEN 19 PRECEDING AND CURRENT ROW
);
```

### 4.3 Детекция аномалий объёма

```sql
WITH stats AS (
    SELECT open_time, symbol, volume, close,
           avg(volume)    OVER w AS avg_vol,
           stddev_samp(volume) OVER w AS sd_vol
    FROM klines
    WHERE symbol = ?
    WINDOW w AS (
        PARTITION BY symbol ORDER BY open_time
        ROWS BETWEEN 100 PRECEDING AND 1 PRECEDING
    )
)
SELECT *, (volume - avg_vol) / nullif(sd_vol, 0) AS z_score
FROM stats
WHERE (volume - avg_vol) / nullif(sd_vol, 0) > 3
ORDER BY open_time DESC;
```

### 4.4 Order Flow Imbalance по сделкам

```sql
SELECT
    time_bucket(INTERVAL '1 minute', ts) AS bucket,
    sum(CASE WHEN NOT is_buyer_maker THEN qty ELSE 0 END) AS buy_vol,
    sum(CASE WHEN     is_buyer_maker THEN qty ELSE 0 END) AS sell_vol,
    (buy_vol - sell_vol) / nullif(buy_vol + sell_vol, 0)  AS ofi
FROM trades
WHERE symbol = ?
GROUP BY 1 ORDER BY 1;
```

### 4.5 Матрица корреляций между инструментами

```sql
WITH rets AS (
    SELECT open_time, symbol,
           ln(close / lag(close) OVER (PARTITION BY symbol ORDER BY open_time)) AS r
    FROM klines WHERE interval = '1m'
)
SELECT a.symbol AS sym_a, b.symbol AS sym_b, corr(a.r, b.r) AS correlation
FROM rets a JOIN rets b
  ON a.open_time = b.open_time AND a.symbol < b.symbol
WHERE a.r IS NOT NULL AND b.r IS NOT NULL
GROUP BY 1, 2
ORDER BY abs(correlation) DESC;
```

### 4.6 ASOF JOIN — сопоставление разнесённых по времени рядов

Свечи разных бирж/инструментов почти никогда не совпадают по меткам времени.
DuckDB решает это нативно:

```sql
SELECT t.ts, t.price AS btc_price, e.price AS eth_price
FROM btc_trades t
ASOF JOIN eth_trades e
  ON e.ts <= t.ts;
```

### 4.7 Организация в коде

Не размазываем SQL по всему проекту:

```
src/analytics/
  queries/
    vwap.sql
    volume_anomaly.sql
    correlation.sql
  mod.rs        // include_str! + типизированные параметры + маппинг в структуры
```

```rust
pub fn volume_anomalies(
    conn: &Connection,
    symbol: &str,
    threshold: f64,
) -> Result<Vec<VolumeAnomaly>> {
    let mut stmt = conn.prepare(include_str!("queries/volume_anomaly.sql"))?;
    let rows = stmt.query_map(params![symbol, threshold], |r| { /* ... */ })?;
    rows.collect()
}
```

---

## 5. API-слой

`axum` поверх пула DuckDB-коннектов (`r2d2` или собственный `Arc<Mutex<Connection>>`
для MVP — DuckDB-коннекты дёшево клонируются от одного инстанса базы).

```
GET  /symbols
GET  /ohlcv/{symbol}?interval=5m&from=...&to=...
GET  /analytics/{symbol}/vwap?window=20
GET  /analytics/{symbol}/anomalies?z=3
GET  /analytics/correlation?symbols=BTCUSDT,ETHUSDT,SOLUSDT
GET  /health
WS   /stream/{symbol}          # проброс live-событий клиенту
```

Тяжёлые запросы гоняем через `tokio::task::spawn_blocking` — DuckDB синхронный,
и блокировать executor нельзя.

---

## 6. Структура проекта

```
market-analyzer/
├── Cargo.toml                 # workspace
├── crates/
│   ├── core/                  # доменные типы: Trade, Kline, Symbol, Interval
│   ├── exchanges/             # ExchangeSource + binance/, bybit/
│   ├── storage/               # BatchWriter, Parquet layout, миграции DuckDB
│   ├── analytics/             # SQL + типизированные обёртки
│   ├── api/                   # axum
│   └── cli/                   # clap: backfill / stream / query / compact
└── data/
```

### Cargo.toml (основное)

```toml
[workspace.dependencies]
tokio             = { version = "1", features = ["full"] }
tokio-tungstenite = { version = "0.24", features = ["native-tls"] }
reqwest           = { version = "0.12", features = ["json", "gzip"] }
serde             = { version = "1", features = ["derive"] }
serde_json        = "1"
duckdb            = { version = "1", features = ["bundled", "chrono"] }
arrow             = "53"
axum              = "0.7"
chrono            = "0.4"
rust_decimal      = "1"
anyhow            = "1"
thiserror         = "2"
tracing           = "0.1"
tracing-subscriber= "0.3"
clap              = { version = "4", features = ["derive"] }
governor          = "0.7"
async-trait       = "0.1"
```

`features = ["bundled"]` у duckdb собирает DuckDB из исходников — первая сборка
займёт несколько минут, зато никаких системных зависимостей.

---

## 7. Дорожная карта

**Этап 1 — вертикальный срез (1–2 вечера)**
CLI-команда `backfill --symbol BTCUSDT --days 30` → Parquet → один SQL-запрос,
считающий волатильность, вывод в терминал. Всё, скелет работает.

**Этап 2 — аналитика**
Библиотека запросов из раздела 4, покрытие тестами на синтетическом наборе данных.

**Этап 3 — live**
WebSocket-стример, батч-writer, реконнект с добором пропусков.

**Этап 4 — API**
axum-эндпоинты, `/stream` через WS, простой дашборд (можно статикой на ECharts).

**Этап 5 — то, что впечатляет в портфолио**
- Compaction-джоб и бенчмарк «до/после» (размер, время запроса)
- Бенчмарк DuckDB vs pandas на одном датасете
- Мультибиржевость (Bybit) и арбитражный спред между площадками через ASOF JOIN

---

## 8. Подводные камни

1. **Деньги в `f64`** — накопление ошибок округления. Использовать `Decimal` /
   `DECIMAL(18,8)`.
2. **Один писатель на файл DuckDB.** Разводить коллектор и API либо по одному
   процессу, либо через read-only подключение.
3. **Мелкие Parquet-файлы.** Тысячи файлов по 200 КБ убивают скорость чтения —
   нужен compaction.
4. **Блокировка async-рантайма** синхронными DuckDB-вызовами → `spawn_blocking`.
5. **Часовые пояса.** Всё хранить в UTC, конвертировать только на выводе.
6. **Дедупликация.** WebSocket после реконнекта может прислать уже записанные
   сделки — дедуп по `(symbol, trade_id)` перед флашем.
7. **Rate limits.** У Binance весовая модель; при бане IP получишь `418` на
   несколько минут.

---

## 9. Что можно навесить сверху

- **Backtesting**: сигналы считаются тем же SQL, Rust прогоняет позиции и P&L.
- **Алерты**: правило = SQL-запрос по расписанию, срабатывание → Telegram-бот.
- **Feature store для ML**: DuckDB генерирует признаки, `polars`/Python берёт их
  через тот же Parquet.
- **S3**: DuckDB-расширение `httpfs` умеет читать Parquet прямо из объектного
  хранилища — локальный layout меняется на `s3://bucket/...` почти без правок.
