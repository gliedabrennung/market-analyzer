use rust_decimal::Decimal;

use crate::error::CoreError;

/// Rescale a decimal up to `target_scale`, refusing to silently drop precision.
///
/// Money values must never be truncated implicitly (see FR-2.4 / NFR money
/// requirements) — if the source value already carries more fractional
/// digits than the target column scale allows, this is an error, not a
/// rounding.
pub fn rescale_checked(value: Decimal, target_scale: u32) -> Result<Decimal, CoreError> {
    if value.scale() > target_scale {
        return Err(CoreError::DecimalScale {
            value: value.to_string(),
            actual: value.scale(),
            target: target_scale,
        });
    }
    let mut v = value;
    v.rescale(target_scale);
    Ok(v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn rescales_up_without_change_of_value() {
        let v = Decimal::from_str("1.5").unwrap();
        let r = rescale_checked(v, 8).unwrap();
        assert_eq!(r.to_string(), "1.50000000");
    }

    #[test]
    fn rejects_precision_loss() {
        let v = Decimal::from_str("1.123456789").unwrap();
        assert!(rescale_checked(v, 8).is_err());
    }
}
