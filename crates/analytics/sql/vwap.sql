-- FR-3.2: rolling VWAP over klines.
--
-- The frame size is inlined at each OVER(...) site rather than shared via a
-- named WINDOW clause: DuckDB's binder counts a parameter once per usage
-- site when a named window with a parameterized frame is referenced by
-- `OVER w` more than once, not once per textual `?` (verified empirically)
-- — inlining keeps the textual `?` count matching what we actually bind.
--
-- Scoped by `exchange` so a future second exchange's rows for the same
-- symbol can never silently interleave into one rolling window
-- (architecture goal Ц4).
-- Params, in order: frame_size_minus_one (x2, same value), exchange, symbol, interval
-- VWAP is a price (same unit as `close`), so it stays DECIMAL end to end —
-- never DOUBLE. DuckDB's decimal `/` always yields DOUBLE (verified
-- empirically), so the division result is cast back to DECIMAL(18,8)
-- (close's own scale) before the usual VARCHAR round-trip.
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
