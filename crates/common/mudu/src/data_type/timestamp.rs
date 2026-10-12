//! Timestamp values without a time zone.

use crate::data_type::temporal::{
    format_fractional_micros, micros_from_naive_timestamp, parse_naive_timestamp,
    validate_precision,
};
use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};

/// A timestamp without time zone stored as microseconds since the Unix epoch.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimestampValue {
    epoch_micros: i64,
}

impl TimestampValue {
    /// Creates a timestamp from microseconds since the Unix epoch.
    pub fn from_epoch_micros(epoch_micros: i64) -> Self {
        Self { epoch_micros }
    }

    /// Returns the microseconds since the Unix epoch.
    pub fn epoch_micros(&self) -> i64 {
        self.epoch_micros
    }

    /// Parses a timestamp string without a time zone.
    pub fn parse(s: &str) -> Result<Self, String> {
        let timestamp = parse_naive_timestamp(s)?;
        Self::from_naive_datetime(timestamp)
    }

    /// Formats the timestamp as `YYYY-MM-DD HH:MM:SS` with `precision`
    /// fractional digits.
    pub fn format(&self, precision: u8) -> Result<String, String> {
        validate_precision(precision)?;
        let dt = self.to_naive_datetime()?;
        Ok(format!(
            "{} {}{}",
            dt.date().format("%Y-%m-%d"),
            dt.time().format("%H:%M:%S"),
            format_fractional_micros(dt.time().nanosecond() / 1_000, precision)
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
        Ok(Self::from_epoch_micros(
            self.epoch_micros.div_euclid(factor) * factor,
        ))
    }

    /// Converts a [`NaiveDateTime`] into a `TimestampValue`.
    pub fn from_naive_datetime(timestamp: NaiveDateTime) -> Result<Self, String> {
        Ok(Self::from_epoch_micros(micros_from_naive_timestamp(
            timestamp,
        )?))
    }

    /// Converts this value into a [`NaiveDateTime`].
    pub fn to_naive_datetime(&self) -> Result<NaiveDateTime, String> {
        DateTime::<Utc>::from_timestamp_micros(self.epoch_micros)
            .map(|dt| dt.naive_utc())
            .ok_or_else(|| "timestamp micros out of range".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::TimestampValue;

    #[test]
    fn timestamp_parse_formats_and_truncates() {
        let value = TimestampValue::parse("2026-05-20T14:30:45.123456").unwrap();

        assert_eq!(value.format(6).unwrap(), "2026-05-20 14:30:45.123456");
        assert_eq!(
            value.truncate_precision(4).unwrap().format(6).unwrap(),
            "2026-05-20 14:30:45.123400"
        );
        assert_eq!(value.format(4).unwrap(), "2026-05-20 14:30:45.1234");
    }

    #[test]
    fn timestamp_out_of_range_is_reported() {
        let value = TimestampValue::from_epoch_micros(i64::MAX);
        assert!(value.to_naive_datetime().is_err());
    }

    #[test]
    fn truncate_precision_covers_all_factors() {
        let value = TimestampValue::parse("2026-05-20T14:30:45.123456").unwrap();
        let expected = [
            "2026-05-20 14:30:45",
            "2026-05-20 14:30:45.1",
            "2026-05-20 14:30:45.12",
            "2026-05-20 14:30:45.123",
            "2026-05-20 14:30:45.1234",
            "2026-05-20 14:30:45.12345",
            "2026-05-20 14:30:45.123456",
        ];
        for (precision, &formatted) in expected.iter().enumerate() {
            let truncated = value.truncate_precision(precision as u8).unwrap();
            assert_eq!(truncated.format(precision as u8).unwrap(), formatted);
        }
    }

    #[test]
    fn epoch_micros_accessor_roundtrips() {
        let value = TimestampValue::from_epoch_micros(1234567890123456);
        assert_eq!(value.epoch_micros(), 1234567890123456);
    }
}
