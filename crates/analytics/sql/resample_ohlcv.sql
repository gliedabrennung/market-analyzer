-- FR-3.1: resample raw trades into OHLCV bars of an arbitrary width.
-- Scoped by `exchange` so a future second exchange's rows for the same
-- symbol can never silently interleave into one bar (architecture goal
-- Ц4). Params, in order: bucket_seconds, exchange, symbol,
-- from_ts (inclusive), to_ts (exclusive)
SELECT
    time_bucket((? || ' seconds')::INTERVAL, ts) AS bucket,
    symbol,
    CAST(first(price ORDER BY ts) AS VARCHAR) AS open,
    CAST(max(price) AS VARCHAR) AS high,
    CAST(min(price) AS VARCHAR) AS low,
    CAST(last(price ORDER BY ts) AS VARCHAR) AS close,
    CAST(sum(qty) AS VARCHAR) AS volume,
    count(*)::BIGINT AS trades_count
FROM trades
WHERE exchange = ? AND symbol = ? AND ts >= ? AND ts < ?
GROUP BY 1, 2
ORDER BY 1
