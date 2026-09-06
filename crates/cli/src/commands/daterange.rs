use anyhow::{bail, Context, Result};
use chrono::{DateTime, NaiveDate, TimeZone, Utc};

/// `from` at `00:00:00.000`, one day past `to` (defaulting to today) at
/// `00:00:00.000` — i.e. `[from, to_exclusive)`.
fn day_bounds_utc(
    from: NaiveDate,
    to: Option<NaiveDate>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let from_utc = Utc.from_utc_datetime(
        &from
            .and_hms_opt(0, 0, 0)
            .context("--from is not a valid date")?,
    );

    let to_date = to.unwrap_or_else(|| Utc::now().date_naive());
    let to_exclusive_utc = Utc.from_utc_datetime(
        &to_date
            .succ_opt()
            .context("--to is out of range")?
            .and_hms_opt(0, 0, 0)
            .context("--to is not a valid date")?,
    );

    if from_utc >= to_exclusive_utc {
        bail!("--from must be strictly before --to");
    }

    Ok((from_utc, to_exclusive_utc))
}

/// Turns `--from`/`--to` day arguments into an inclusive UTC instant range:
/// `from` at `00:00:00.000`, `to` at `23:59:59.999`. For historical REST
/// backfill windows, which are inclusive on both ends.
pub fn day_range_utc(
    from: NaiveDate,
    to: Option<NaiveDate>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let (from_utc, to_exclusive_utc) = day_bounds_utc(from, to)?;
    Ok((
        from_utc,
        to_exclusive_utc - chrono::Duration::milliseconds(1),
    ))
}

/// Same day range, but with an exclusive upper bound (`ts < to`) — for
/// analytics queries whose SQL filters that way.
pub fn day_range_utc_exclusive_end(
    from: NaiveDate,
    to: Option<NaiveDate>,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    day_bounds_utc(from, to)
}
