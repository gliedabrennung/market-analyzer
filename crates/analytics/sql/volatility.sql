-- FR-3.3: rolling realized volatility from log returns, scaled to a daily
-- horizon by the caller-supplied scale factor (sqrt(bars per day)).
-- Scoped by `exchange` so a future second exchange's rows for the same
-- symbol can never silently interleave into one log-return series
-- (architecture goal Ц4).
-- Params, in order: exchange, symbol, interval, frame_size_minus_one, scale_factor
WITH base AS (
    SELECT
        open_time,
        ln(CAST(close AS DOUBLE) / lag(CAST(close AS DOUBLE)) OVER (ORDER BY open_time)) AS log_ret
    FROM klines
    WHERE exchange = ? AND symbol = ? AND interval = ?
)
SELECT
    open_time,
    stddev_samp(log_ret) OVER (
        ORDER BY open_time
        ROWS BETWEEN ? PRECEDING AND CURRENT ROW
    ) * ? AS realized_volatility
FROM base
WHERE log_ret IS NOT NULL
ORDER BY open_time
