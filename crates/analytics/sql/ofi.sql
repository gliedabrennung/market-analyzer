
SELECT
    time_bucket((? || ' seconds')::INTERVAL, ts) AS bucket,
    CAST(sum(CASE WHEN NOT is_buyer_maker THEN qty ELSE 0 END) AS VARCHAR) AS buy_volume,
    CAST(sum(CASE WHEN is_buyer_maker THEN qty ELSE 0 END) AS VARCHAR) AS sell_volume,
    (sum(CASE WHEN NOT is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END)
        - sum(CASE WHEN is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END))
        / nullif(sum(CAST(qty AS DOUBLE)), 0) AS ofi
FROM trades
WHERE exchange = ? AND symbol = ? AND ts >= ? AND ts < ?
GROUP BY 1
ORDER BY 1
