-- FR-3.5: Order Flow Imbalance from trades, bucketed by time.
-- buy_volume/sell_volume are stored quantities (qty, DECIMAL(18,8)) and stay
-- DECIMAL end to end; only `ofi` itself is a genuinely-derived ratio in
-- [-1, 1], so that one stays DOUBLE.
-- Scoped by `exchange` so a future second exchange's rows for the same
-- symbol can never silently double up one bucket (architecture goal Ц4).
-- Params, in order: bucket_seconds, exchange, symbol, from_ts (inclusive), to_ts (exclusive)
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
