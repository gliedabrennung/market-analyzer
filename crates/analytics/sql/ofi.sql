-- FR-3.5: Order Flow Imbalance from trades, bucketed by time.
-- Params, in order: bucket_seconds, symbol, from_ts (inclusive), to_ts (exclusive)
SELECT
    time_bucket((? || ' seconds')::INTERVAL, ts) AS bucket,
    sum(CASE WHEN NOT is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END) AS buy_volume,
    sum(CASE WHEN is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END) AS sell_volume,
    (sum(CASE WHEN NOT is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END)
        - sum(CASE WHEN is_buyer_maker THEN CAST(qty AS DOUBLE) ELSE 0 END))
        / nullif(sum(CAST(qty AS DOUBLE)), 0) AS ofi
FROM trades
WHERE symbol = ? AND ts >= ? AND ts < ?
GROUP BY 1
ORDER BY 1
