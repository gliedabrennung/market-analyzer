-- FR-3.6: pairwise correlation of log returns across a symbol set on a
-- common time grid. Scoped by `exchange` so a future second exchange's
-- rows for the same symbol can never silently interleave into one series
-- (architecture goal Ц4). Params, in order: exchange, interval, then one
-- `?` per symbol (arity fixed up by ma_analytics::correlation at call time
-- — see rowutil::placeholders; only punctuation is spliced in, never data).
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
