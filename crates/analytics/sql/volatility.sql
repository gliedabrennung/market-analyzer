
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
