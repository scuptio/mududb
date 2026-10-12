//! Shared helpers for temporal data types.
//!
//! Temporal values are stored as microsecond offsets from well-known epochs
//! (Unix epoch for timestamps, midnight for times) with up to microsecond
//! precision.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, Utc};

/// Maximum supported fractional-second precision (microseconds).
pub const TEMPORAL_MAX_PRECISION: u8 = 6;
/// Microseconds in one second.
pub const MICROS_PER_SECOND: i64 = 1_000_000;
/// Microseconds in one minute.
pub const MICROS_PER_MINUTE: i64 = 60 * MICROS_PER_SECOND;
/// Microseconds in one hour.
pub const MICROS_PER_HOUR: i64 = 60 * MICROS_PER_MINUTE;
/// Microseconds in one day.
pub const MICROS_PER_DAY: i64 = 24 * MICROS_PER_HOUR;

/// Returns the Unix epoch date (1970-01-01).
pub fn unix_epoch_date() -> NaiveDate {
    NaiveDate::from_ymd_opt(1970, 1, 1).expect("unix epoch date")
}

/// Returns the Unix epoch timestamp (1970-01-01 00:00:00).
pub fn unix_epoch_timestamp() -> NaiveDateTime {
    unix_epoch_date()
        .and_hms_micro_opt(0, 0, 0, 0)
        .expect("unix epoch datetime")
}

/// Checks that `precision` does not exceed [`TEMPORAL_MAX_PRECISION`].
pub fn validate_precision(precision: u8) -> Result<(), String> {
    if precision > TEMPORAL_MAX_PRECISION {
        return Err(format!(
            "temporal precision must be less than or equal to {}",
            TEMPORAL_MAX_PRECISION
        ));
    }
    Ok(())
}

/// Returns the number of microseconds represented by one fractional digit at
/// the given precision (1 for microsecond precision and beyond).
pub fn precision_factor(precision: u8) -> i64 {
    match precision {
        0 => MICROS_PER_SECOND,
        1 => 100_000,
        2 => 10_000,
        3 => 1_000,
        4 => 100,
        5 => 10,
        _ => 1,
    }
}

/// Truncates `micros` to the given fractional-second precision.
pub fn truncate_micros(micros: i64, precision: u8) -> i64 {
    let factor = precision_factor(precision);
    micros.div_euclid(factor) * factor
}

/// Formats `micros` as a fractional-second suffix with `precision` digits,
/// returning an empty string when `precision` is 0.
pub fn format_fractional_micros(micros: u32, precision: u8) -> String {
    if precision == 0 {
        String::new()
    } else {
        let digits = format!("{:06}", micros);
        format!(".{}", &digits[..precision as usize])
    }
}

/// Parses a `HH:MM:SS[.fraction]` time string.
pub fn parse_naive_time(s: &str) -> Result<NaiveTime, String> {
    ["%H:%M:%S%.f", "%H:%M:%S"]
        .iter()
        .find_map(|fmt| NaiveTime::parse_from_str(s, fmt).ok())
        .ok_or_else(|| format!("invalid time {}", s))
}

/// Parses a timestamp string without a time zone.
pub fn parse_naive_timestamp(s: &str) -> Result<NaiveDateTime, String> {
    [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
    ]
    .iter()
    .find_map(|fmt| NaiveDateTime::parse_from_str(s, fmt).ok())
    .ok_or_else(|| format!("invalid timestamp {}", s))
}

/// Parses an RFC 3339 timestamp string with a fixed offset; a space separator
/// between date and time is accepted in place of `T`.
pub fn parse_fixed_offset_timestamp(s: &str) -> Result<DateTime<FixedOffset>, String> {
    if let Ok(value) = DateTime::parse_from_rfc3339(s) {
        return Ok(value);
    }
    let normalized = if s.contains(' ') && !s.contains('T') {
        s.replacen(' ', "T", 1)
    } else {
        s.to_string()
    };
    DateTime::parse_from_rfc3339(&normalized)
        .map_err(|_| format!("invalid timestamp with time zone {}", s))
}

/// Converts a naive timestamp to microseconds since the Unix epoch.
pub fn micros_from_naive_timestamp(value: NaiveDateTime) -> Result<i64, String> {
    value
        .signed_duration_since(unix_epoch_timestamp())
        .num_microseconds()
        .ok_or_else(|| "timestamp micros overflow".to_string())
}

/// Converts microseconds since the Unix epoch to a naive timestamp.
pub fn naive_timestamp_from_micros(micros: i64) -> Result<NaiveDateTime, String> {
    DateTime::<Utc>::from_timestamp_micros(micros)
        .map(|dt| dt.naive_utc())
        .ok_or_else(|| "timestamp micros out of range".to_string())
}

/// Converts microseconds since the Unix epoch to a UTC timestamp.
pub fn utc_timestamp_from_micros(micros: i64) -> Result<DateTime<Utc>, String> {
    DateTime::<Utc>::from_timestamp_micros(micros)
        .ok_or_else(|| "timestamp micros out of range".to_string())
}
