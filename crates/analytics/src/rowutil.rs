use std::str::FromStr;

use chrono::{DateTime, Utc};
use duckdb::Row;
use rust_decimal::Decimal;

use crate::error::AnalyticsError;

pub fn decimal_col(row: &Row<'_>, idx: usize) -> Result<Decimal, AnalyticsError> {
    let text: String = row.get(idx)?;
    Decimal::from_str(&text).map_err(|source| AnalyticsError::Decimal {
        value: text,
        source,
    })
}

pub fn decimal_col_opt(row: &Row<'_>, idx: usize) -> Result<Option<Decimal>, AnalyticsError> {
    let text: Option<String> = row.get(idx)?;
    text.map(|t| {
        Decimal::from_str(&t).map_err(|source| AnalyticsError::Decimal { value: t, source })
    })
    .transpose()
}

pub fn timestamp_col(row: &Row<'_>, idx: usize) -> Result<DateTime<Utc>, duckdb::Error> {
    let naive: chrono::NaiveDateTime = row.get(idx)?;
    Ok(DateTime::from_naive_utc_and_offset(naive, Utc))
}

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
