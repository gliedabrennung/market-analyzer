
SELECT
    open_time,
    CAST(close AS VARCHAR) AS close,
    CAST(
        CAST(
            sum(close * volume) OVER (
                ORDER BY open_time ROWS BETWEEN ? PRECEDING AND CURRENT ROW
            )
            / nullif(sum(volume) OVER (
                ORDER BY open_time ROWS BETWEEN ? PRECEDING AND CURRENT ROW
            ), 0)
        AS DECIMAL(18,8))
    AS VARCHAR) AS vwap
FROM klines
WHERE exchange = ? AND symbol = ? AND interval = ?
ORDER BY open_time
