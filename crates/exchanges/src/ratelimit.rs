use std::num::NonZeroU32;
use std::time::Duration;

use governor::{DefaultDirectRateLimiter, Quota};

use crate::error::ExchangeError;

pub type Limiter = DefaultDirectRateLimiter;

pub fn build_limiter(requests_per_second: u32) -> Result<Limiter, ExchangeError> {
    let quota = NonZeroU32::new(requests_per_second)
        .ok_or_else(|| ExchangeError::Config("requests_per_second must be > 0".to_string()))?;
    Ok(governor::RateLimiter::direct(Quota::per_second(quota)))
}

#[derive(Debug, Clone, Copy)]
pub struct BackoffConfig {
    pub base: Duration,
    pub multiplier: u32,
    pub max: Duration,
    pub max_attempts: u32,
}

impl Default for BackoffConfig {
    fn default() -> Self {
        Self {
            base: Duration::from_secs(1),
            multiplier: 2,
            max: Duration::from_secs(60),
            max_attempts: 5,
        }
    }
}

pub fn backoff_delay(cfg: &BackoffConfig, attempt: u32) -> Duration {
    let factor = cfg.multiplier.saturating_pow(attempt.saturating_sub(1));
    let millis = cfg.base.as_millis().saturating_mul(u128::from(factor));
    let capped = millis.min(cfg.max.as_millis());
    Duration::from_millis(capped as u64)
}

pub fn parse_retry_after(headers: &reqwest::header::HeaderMap) -> Option<Duration> {
    headers
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn backoff_grows_geometrically_and_caps_at_max() {
        let cfg = BackoffConfig::default();
        assert_eq!(backoff_delay(&cfg, 1), Duration::from_secs(1));
        assert_eq!(backoff_delay(&cfg, 2), Duration::from_secs(2));
        assert_eq!(backoff_delay(&cfg, 3), Duration::from_secs(4));
        assert_eq!(backoff_delay(&cfg, 4), Duration::from_secs(8));
        assert_eq!(backoff_delay(&cfg, 5), Duration::from_secs(16));
        assert_eq!(backoff_delay(&cfg, 10), Duration::from_secs(60));
    }

    #[test]
    fn build_limiter_rejects_zero_rps() {
        assert!(build_limiter(0).is_err());
    }
}
