use std::str::FromStr;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, NaiveDateTime, Utc};
use serde_json::json;

use ma_core::{Interval, Symbol};

use crate::error::ApiError;
use crate::state::{ApiLimits, AppState};

pub async fn validate_symbol(state: &AppState, raw: &str) -> Result<Symbol, ApiError> {
    let symbol = Symbol::new(raw).map_err(|e| ApiError::bad_request("symbol", e.to_string()))?;
    let checked = symbol.clone();
    let exists = state
        .pool
        .with_meta(move |meta| meta.symbol_exists(checked.as_str()).map_err(ApiError::from))
        .await?;
    if !exists {
        return Err(ApiError::UnknownSymbol {
            symbol: raw.to_string(),
        });
    }
    Ok(symbol)
}

pub fn parse_interval(raw: Option<&str>) -> Result<Interval, ApiError> {
    let raw = raw.unwrap_or("1m");
    Interval::from_str(raw).map_err(|e| ApiError::bad_request("interval", e.to_string()))
}

pub fn parse_date_range(
    from: Option<&str>,
    to: Option<&str>,
    limits: &ApiLimits,
) -> Result<(NaiveDateTime, NaiveDateTime), ApiError> {
    let now = Utc::now().naive_utc();
    let to_dt = match to {
        Some(s) => parse_flexible_datetime(s, "to")?,
        None => now,
    };
    let from_dt = match from {
        Some(s) => parse_flexible_datetime(s, "from")?,
        None => to_dt - ChronoDuration::days(limits.max_date_range_days),
    };

    if from_dt >= to_dt {
        return Err(ApiError::bad_request(
            "from",
            "'from' must be strictly before 'to'",
        ));
    }
    if to_dt - from_dt > ChronoDuration::days(limits.max_date_range_days) {
        return Err(ApiError::BadRequest {
            message: format!(
                "date range exceeds the maximum of {} days",
                limits.max_date_range_days
            ),
            details: json!({ "max_days": limits.max_date_range_days }),
        });
    }
    Ok((from_dt, to_dt))
}

fn parse_flexible_datetime(s: &str, field: &str) -> Result<NaiveDateTime, ApiError> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Ok(dt.naive_utc());
    }
    if let Ok(date) = NaiveDate::parse_from_str(s, "%Y-%m-%d") {
        return Ok(date.and_hms_opt(0, 0, 0).unwrap_or_default());
    }
    Err(ApiError::bad_request(
        field,
        format!("'{field}' must be RFC3339 or YYYY-MM-DD, got '{s}'"),
    ))
}

pub fn parse_window(raw: Option<u32>, default: u32) -> Result<u32, ApiError> {
    let window = raw.unwrap_or(default);
    if window == 0 || window > 100_000 {
        return Err(ApiError::bad_request(
            "window",
            format!("window must be between 1 and 100000, got {window}"),
        ));
    }
    Ok(window)
}

pub fn parse_threshold(raw: Option<f64>, default: f64) -> Result<f64, ApiError> {
    let threshold = raw.unwrap_or(default);
    if !threshold.is_finite() || threshold <= 0.0 {
        return Err(ApiError::bad_request(
            "threshold",
            format!("threshold must be a positive finite number, got {threshold}"),
        ));
    }
    Ok(threshold)
}

pub fn parse_bucket_seconds(raw: Option<i64>, default: i64) -> Result<i64, ApiError> {
    let bucket = raw.unwrap_or(default);
    const MAX_BUCKET_SECONDS: i64 = 7 * 24 * 3600;
    if bucket <= 0 || bucket > MAX_BUCKET_SECONDS {
        return Err(ApiError::bad_request(
            "bucket",
            format!("bucket must be a positive number of seconds up to {MAX_BUCKET_SECONDS}, got {bucket}"),
        ));
    }
    Ok(bucket)
}

pub fn parse_pagination(
    limit: Option<i64>,
    offset: Option<i64>,
    limits: &ApiLimits,
) -> Result<(i64, i64), ApiError> {
    let limit = limit.unwrap_or(limits.default_pagination_limit);
    if limit < 1 || limit > limits.max_pagination_limit {
        return Err(ApiError::bad_request(
            "limit",
            format!(
                "limit must be between 1 and {}, got {limit}",
                limits.max_pagination_limit
            ),
        ));
    }
    let offset = offset.unwrap_or(0);
    if offset < 0 {
        return Err(ApiError::bad_request("offset", "offset must be >= 0"));
    }
    Ok((limit, offset))
}

pub fn paginate<T>(rows: Vec<T>, limit: i64, offset: i64) -> Vec<T> {
    rows.into_iter()
        .skip(offset.max(0) as usize)
        .take(limit.max(0) as usize)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits() -> ApiLimits {
        ApiLimits {
            max_date_range_days: 30,
            default_pagination_limit: 1000,
            max_pagination_limit: 10_000,
            max_correlation_symbols: 10,
            max_ws_connections: 64,
        }
    }

    #[test]
    fn date_range_rejects_inverted_range() {
        let result = parse_date_range(Some("2026-08-10"), Some("2026-08-01"), &limits());
        assert!(result.is_err());
    }

    #[test]
    fn date_range_rejects_too_wide() {
        let result = parse_date_range(Some("2026-01-01"), Some("2026-12-31"), &limits());
        assert!(result.is_err());
    }

    #[test]
    fn date_range_accepts_valid_range() {
        let (from, to) =
            parse_date_range(Some("2026-08-01"), Some("2026-08-10"), &limits()).unwrap();
        assert!(from < to);
    }

    #[test]
    fn pagination_defaults_and_caps() {
        let limits = limits();
        assert_eq!(parse_pagination(None, None, &limits).unwrap(), (1000, 0));
        assert!(parse_pagination(Some(10_001), None, &limits).is_err());
        assert!(parse_pagination(Some(0), None, &limits).is_err());
        assert!(parse_pagination(None, Some(-1), &limits).is_err());
    }

    #[test]
    fn window_rejects_zero_and_absurdly_large() {
        assert!(parse_window(Some(0), 20).is_err());
        assert!(parse_window(Some(1_000_000), 20).is_err());
        assert_eq!(parse_window(None, 20).unwrap(), 20);
    }

    #[test]
    fn threshold_rejects_non_finite_and_non_positive() {
        assert!(parse_threshold(Some(f64::NAN), 3.0).is_err());
        assert!(parse_threshold(Some(-1.0), 3.0).is_err());
        assert_eq!(parse_threshold(None, 3.0).unwrap(), 3.0);
    }
}
