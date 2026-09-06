use std::str::FromStr;

use chrono::{DateTime, Utc};
use duckdb::Row;
use rust_decimal::Decimal;

use crate::error::AnalyticsError;

/// Read a column that was selected as `CAST(col AS VARCHAR)` back into a
/// `Decimal`. Money columns are always round-tripped through text rather
/// than a native decimal binding — see `ma_storage::klines` for why.
pub fn decimal_col(row: &Row<'_>, idx: usize) -> Result<Decimal, AnalyticsError> {
    let text: String = row.get(idx)?;
    Decimal::from_str(&text).map_err(|source| AnalyticsError::Decimal {
        value: text,
        source,
    })
}

/// Same as [`decimal_col`] but for a nullable column.
pub fn decimal_col_opt(row: &Row<'_>, idx: usize) -> Result<Option<Decimal>, AnalyticsError> {
    let text: Option<String> = row.get(idx)?;
    text.map(|t| {
        Decimal::from_str(&t).map_err(|source| AnalyticsError::Decimal { value: t, source })
    })
    .transpose()
}

/// Read a `TIMESTAMP` column as UTC (all timestamps in this project are UTC
/// wall-clock values with no stored offset, per TZ 5.1/5.2).
pub fn timestamp_col(row: &Row<'_>, idx: usize) -> Result<DateTime<Utc>, duckdb::Error> {
    let naive: chrono::NaiveDateTime = row.get(idx)?;
    Ok(DateTime::from_naive_utc_and_offset(naive, Utc))
}

/// A comma-joined list of `n` positional placeholders, e.g. `"?, ?, ?"`, for
/// the one case a prepared statement can't parameterize directly: an `IN`
/// list whose arity is only known at runtime. Only punctuation is spliced
/// into the SQL text here — every value still goes through the driver's
/// normal parameter binding (FR-3.8).
pub fn placeholders(n: usize) -> String {
    vec!["?"; n].join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholders_join_correctly() {
        assert_eq!(placeholders(0), "");
        assert_eq!(placeholders(1), "?");
        assert_eq!(placeholders(3), "?, ?, ?");
    }
}
