-- FR-3.4: volume z-score anomalies. Baseline window excludes the current
-- row so a spike can't inflate its own baseline.
--
-- The frame is inlined at each OVER(...) site rather than shared via a
-- named WINDOW clause — see vwap.sql for why (DuckDB counts a parameter
-- once per usage site of a reused named window, not once per textual `?`).
--
-- Params, in order: window (x2, same value), symbol, interval, threshold
WITH stats AS (
    SELECT
        open_time,
        CAST(volume AS DOUBLE) AS volume,
        avg(CAST(volume AS DOUBLE)) OVER (
            ORDER BY open_time ROWS BETWEEN ? PRECEDING AND 1 PRECEDING
        ) AS avg_vol,
        stddev_samp(CAST(volume AS DOUBLE)) OVER (
            ORDER BY open_time ROWS BETWEEN ? PRECEDING AND 1 PRECEDING
        ) AS sd_vol
    FROM klines
    WHERE symbol = ? AND interval = ?
)
SELECT
    open_time,
    volume,
    (volume - avg_vol) / nullif(sd_vol, 0) AS z_score
FROM stats
WHERE (volume - avg_vol) / nullif(sd_vol, 0) > ?
ORDER BY open_time
