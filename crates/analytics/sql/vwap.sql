-- FR-3.2: rolling VWAP over klines.
--
-- The frame size is inlined at each OVER(...) site rather than shared via a
-- named WINDOW clause: DuckDB's binder counts a parameter once per usage
-- site when a named window with a parameterized frame is referenced by
-- `OVER w` more than once, not once per textual `?` (verified empirically)
-- — inlining keeps the textual `?` count matching what we actually bind.
--
-- Params, in order: frame_size_minus_one (x2, same value), symbol, interval
SELECT
    open_time,
    CAST(close AS VARCHAR) AS close,
    sum(CAST(close AS DOUBLE) * CAST(volume AS DOUBLE)) OVER (
        ORDER BY open_time ROWS BETWEEN ? PRECEDING AND CURRENT ROW
    )
    / nullif(sum(CAST(volume AS DOUBLE)) OVER (
        ORDER BY open_time ROWS BETWEEN ? PRECEDING AND CURRENT ROW
    ), 0) AS vwap
FROM klines
WHERE symbol = ? AND interval = ?
ORDER BY open_time
