
WITH rets AS (
    SELECT
        open_time,
        symbol,
        ln(CAST(close AS DOUBLE) / lag(CAST(close AS DOUBLE)) OVER (PARTITION BY symbol ORDER BY open_time)) AS r
    FROM klines
    WHERE exchange = ? AND interval = ? AND symbol IN (__SYMBOL_PLACEHOLDERS__)
)
SELECT
    a.symbol AS symbol_a,
    b.symbol AS symbol_b,
    corr(a.r, b.r) AS correlation
FROM rets a
JOIN rets b ON a.open_time = b.open_time AND a.symbol < b.symbol
WHERE a.r IS NOT NULL AND b.r IS NOT NULL
GROUP BY 1, 2
ORDER BY abs(correlation) DESC
