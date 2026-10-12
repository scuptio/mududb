//! Timestamp values with a time zone.

use crate::data_type::temporal::{
    format_fractional_micros, parse_fixed_offset_timestamp, validate_precision,
};
use chrono::{DateTime, FixedOffset, Offset, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// A timestamp with time zone stored as microseconds since the Unix epoch in UTC.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimestampTzValue {
    epoch_micros_utc: i64,
}

impl TimestampTzValue {
    /// Creates a value from microseconds since the Unix epoch in UTC.
    pub fn from_epoch_micros_utc(epoch_micros_utc: i64) -> Self {
        Self { epoch_micros_utc }
    }

    /// Returns the microseconds since the Unix epoch in UTC.
    pub fn epoch_micros_utc(&self) -> i64 {
        self.epoch_micros_utc
    }

    /// Parses an RFC 3339 timestamp string with an offset.
    pub fn parse(s: &str) -> Result<Self, String> {
        let timestamp = parse_fixed_offset_timestamp(s)?;
        Ok(Self::from_epoch_micros_utc(timestamp.timestamp_micros()))
    }

    /// Formats the value in UTC with `precision` fractional digits.
    pub fn format(&self, precision: u8) -> Result<String, String> {
        self.format_with_offset(precision, Utc.fix())
    }

    /// Formats the value at the given UTC offset with `precision` fractional digits.
    pub fn format_with_offset(&self, precision: u8, offset: FixedOffset) -> Result<String, String> {
        validate_precision(precision)?;
        let dt = self.to_utc_datetime()?.with_timezone(&offset);
        Ok(format!(
            "{} {}{}{}",
            dt.date_naive().format("%Y-%m-%d"),
            dt.time().format("%H:%M:%S"),
            format_fractional_micros(dt.time().nanosecond() / 1_000, precision),
            dt.format("%:z")
        ))
    }

    /// Truncates the value to the given fractional-second precision.
    pub fn truncate_precision(&self, precision: u8) -> Result<Self, String> {
        validate_precision(precision)?;
        let factor = match precision {
            0 => 1_000_000,
            1 => 100_000,
            2 => 10_000,
            3 => 1_000,
            4 => 100,
            5 => 10,
            _ => 1,
        };
        Ok(Self::from_epoch_micros_utc(
            self.epoch_micros_utc.div_euclid(factor) * factor,
        ))
    }

    /// Converts this value into a UTC [`DateTime`].
    pub fn to_utc_datetime(&self) -> Result<DateTime<Utc>, String> {
        DateTime::<Utc>::from_timestamp_micros(self.epoch_micros_utc)
            .ok_or_else(|| "timestamp with time zone micros out of range".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::{FixedOffset, TimestampTzValue};

    #[test]
    fn timestamptz_normalizes_to_same_utc_instant() {
        let value = TimestampTzValue::parse("2026-05-20 14:30:45.123456+08:00").unwrap();

        assert_eq!(value.format(6).unwrap(), "2026-05-20 06:30:45.123456+00:00");
        assert_eq!(
            value
                .format_with_offset(6, FixedOffset::east_opt(8 * 3600).unwrap())
                .unwrap(),
            "2026-05-20 14:30:45.123456+08:00"
        );
    }

    #[test]
    fn timestamptz_truncates_precision() {
        let value = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();

        assert_eq!(
            value.truncate_precision(3).unwrap().format(6).unwrap(),
            "2026-05-20 06:30:45.123000+00:00"
        );
    }

    #[test]
    fn truncate_precision_covers_all_factors() {
        let value = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();
        let expected = [
            "2026-05-20 06:30:45+00:00",
            "2026-05-20 06:30:45.1+00:00",
            "2026-05-20 06:30:45.12+00:00",
            "2026-05-20 06:30:45.123+00:00",
            "2026-05-20 06:30:45.1234+00:00",
            "2026-05-20 06:30:45.12345+00:00",
            "2026-05-20 06:30:45.123456+00:00",
        ];
        for (precision, &formatted) in expected.iter().enumerate() {
            let truncated = value.truncate_precision(precision as u8).unwrap();
            assert_eq!(truncated.format(precision as u8).unwrap(), formatted);
        }
    }

    #[test]
    fn epoch_micros_utc_accessor_roundtrips() {
        let value = TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap();
        assert_eq!(
            value.epoch_micros_utc(),
            value.to_utc_datetime().unwrap().timestamp_micros()
        );
    }

    #[test]
    fn to_utc_datetime_rejects_out_of_range() {
        let value = TimestampTzValue::from_epoch_micros_utc(i64::MAX);
        assert!(value.to_utc_datetime().is_err());
    }
}
