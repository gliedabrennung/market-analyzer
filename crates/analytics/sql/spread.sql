
SELECT
    a.open_time AS ts,
    CAST(a.close AS VARCHAR) AS price_a,
    CAST(b.close AS VARCHAR) AS price_b,
    CAST(a.close - b.close AS VARCHAR) AS spread_abs,
    CAST(a.close - b.close AS DOUBLE) / nullif(CAST(b.close AS DOUBLE), 0) * 10000 AS spread_bps
FROM (SELECT * FROM klines WHERE exchange = ? AND symbol = ? AND interval = ?) a
ASOF JOIN (SELECT * FROM klines WHERE exchange = ? AND symbol = ? AND interval = ?) b
    ON b.open_time <= a.open_time
ORDER BY a.open_time
