use crate::data_type::temporal::{
    MICROS_PER_DAY, MICROS_PER_HOUR, MICROS_PER_MINUTE, MICROS_PER_SECOND,
    format_fractional_micros, parse_naive_time, truncate_micros, validate_precision,
};
use chrono::{NaiveTime, Timelike};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct TimeValue {
    micros_since_midnight: i64,
}

impl TimeValue {
    pub fn from_micros_since_midnight(micros_since_midnight: i64) -> Result<Self, String> {
        if !(0..MICROS_PER_DAY).contains(&micros_since_midnight) {
            return Err("time micros must be within a single day".to_string());
        }
        Ok(Self {
            micros_since_midnight,
        })
    }

    pub fn micros_since_midnight(&self) -> i64 {
        self.micros_since_midnight
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let time = parse_naive_time(s)?;
        Self::from_naive_time(time)
    }

    pub fn format(&self, precision: u8) -> String {
        let precision = precision.min(6);
        let micros = truncate_micros(self.micros_since_midnight, precision);
        let hour = micros / MICROS_PER_HOUR;
        let minute = (micros % MICROS_PER_HOUR) / MICROS_PER_MINUTE;
        let second = (micros % MICROS_PER_MINUTE) / MICROS_PER_SECOND;
        let micros_part = (micros % MICROS_PER_SECOND) as u32;
        format!(
            "{:02}:{:02}:{:02}{}",
            hour,
            minute,
            second,
            format_fractional_micros(micros_part, precision)
        )
    }

    pub fn truncate_precision(&self, precision: u8) -> Result<Self, String> {
        validate_precision(precision)?;
        Self::from_micros_since_midnight(truncate_micros(self.micros_since_midnight, precision))
    }

    pub fn from_naive_time(time: NaiveTime) -> Result<Self, String> {
        let micros = time.num_seconds_from_midnight() as i64 * MICROS_PER_SECOND
            + (time.nanosecond() / 1_000) as i64;
        Self::from_micros_since_midnight(micros)
    }
}

#[cfg(test)]
mod tests {
    use super::{MICROS_PER_DAY, MICROS_PER_HOUR, MICROS_PER_MINUTE, MICROS_PER_SECOND, TimeValue};

    #[test]
    fn time_parse_and_truncate_precision() {
        let value = TimeValue::parse("12:34:56.123456").unwrap();

        assert_eq!(value.format(6), "12:34:56.123456");
        assert_eq!(
            value.truncate_precision(3).unwrap().format(6),
            "12:34:56.123000"
        );
        assert_eq!(value.format(3), "12:34:56.123");
    }

    #[test]
    fn time_micros_must_stay_within_day() {
        assert!(TimeValue::from_micros_since_midnight(-1).is_err());
        assert!(TimeValue::from_micros_since_midnight(MICROS_PER_DAY).is_err());
    }

    #[test]
    fn micros_since_midnight_accessor_roundtrips() {
        let value = TimeValue::parse("12:34:56.123456").unwrap();
        assert_eq!(
            value.micros_since_midnight(),
            12 * MICROS_PER_HOUR + 34 * MICROS_PER_MINUTE + 56 * MICROS_PER_SECOND + 123456
        );
    }
}
