-- FR-5.1 GET /ohlcv/{symbol}: bounded, paginated raw candles.
-- Params, in order: symbol, interval, from_ts (inclusive), to_ts (exclusive), limit, offset
SELECT
    open_time, close_time,
    CAST(open AS VARCHAR) AS open,
    CAST(high AS VARCHAR) AS high,
    CAST(low AS VARCHAR) AS low,
    CAST(close AS VARCHAR) AS close,
    CAST(volume AS VARCHAR) AS volume,
    CAST(quote_volume AS VARCHAR) AS quote_volume,
    trades_count,
    is_closed
FROM klines
WHERE symbol = ? AND interval = ? AND open_time >= ? AND open_time < ?
ORDER BY open_time
LIMIT ? OFFSET ?
