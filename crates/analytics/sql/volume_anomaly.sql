-- FR-3.4: volume z-score anomalies. Baseline window excludes the current
-- row so a spike can't inflate its own baseline.
--
-- The frame is inlined at each OVER(...) site rather than shared via a
-- named WINDOW clause — see vwap.sql for why (DuckDB counts a parameter
-- once per usage site of a reused named window, not once per textual `?`).
--
-- `volume` (a stored quantity, DECIMAL(28,8)) stays DECIMAL in the output;
-- `volume_f` is a DOUBLE-cast copy used only for the stats math (avg/stddev
-- are inherently float, like realized_volatility's log returns).
--
-- Scoped by `exchange` so a future second exchange's rows for the same
-- symbol can never silently interleave into one baseline (architecture
-- goal Ц4).
-- Params, in order: window (x2, same value), exchange, symbol, interval, threshold
WITH stats AS (
    SELECT
        open_time,
        volume,
        CAST(volume AS DOUBLE) AS volume_f,
        avg(CAST(volume AS DOUBLE)) OVER (
            ORDER BY open_time ROWS BETWEEN ? PRECEDING AND 1 PRECEDING
        ) AS avg_vol,
        stddev_samp(CAST(volume AS DOUBLE)) OVER (
            ORDER BY open_time ROWS BETWEEN ? PRECEDING AND 1 PRECEDING
        ) AS sd_vol
    FROM klines
    WHERE exchange = ? AND symbol = ? AND interval = ?
)
SELECT
    open_time,
    CAST(volume AS VARCHAR) AS volume,
    (volume_f - avg_vol) / nullif(sd_vol, 0) AS z_score
FROM stats
WHERE (volume_f - avg_vol) / nullif(sd_vol, 0) > ?
ORDER BY open_time
