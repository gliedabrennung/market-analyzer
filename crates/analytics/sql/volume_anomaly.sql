
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
