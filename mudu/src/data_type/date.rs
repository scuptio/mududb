use crate::data_type::temporal::unix_epoch_date;
use chrono::{NaiveDate, TimeDelta};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DateValue {
    days_since_epoch: i32,
}

impl DateValue {
    pub fn from_days_since_epoch(days_since_epoch: i32) -> Self {
        Self { days_since_epoch }
    }

    pub fn days_since_epoch(&self) -> i32 {
        self.days_since_epoch
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        let date =
            NaiveDate::parse_from_str(s, "%Y-%m-%d").map_err(|_| format!("invalid date {}", s))?;
        Ok(Self::from_naive_date(date))
    }

    pub fn format(&self) -> String {
        self.to_naive_date().format("%Y-%m-%d").to_string()
    }

    pub fn from_naive_date(date: NaiveDate) -> Self {
        let days = date.signed_duration_since(unix_epoch_date()).num_days() as i32;
        Self::from_days_since_epoch(days)
    }

    pub fn to_naive_date(&self) -> NaiveDate {
        unix_epoch_date()
            .checked_add_signed(TimeDelta::days(self.days_since_epoch as i64))
            .expect("date in range")
    }
}

#[cfg(test)]
mod tests {
    use super::DateValue;

    #[test]
    fn date_roundtrip_preserves_epoch_offsets() {
        let before_epoch = DateValue::parse("1969-12-31").unwrap();
        let epoch = DateValue::parse("1970-01-01").unwrap();
        let after_epoch = DateValue::parse("2026-05-20").unwrap();

        assert_eq!(before_epoch.days_since_epoch(), -1);
        assert_eq!(epoch.days_since_epoch(), 0);
        assert_eq!(after_epoch.days_since_epoch(), 20_593);
        assert_eq!(after_epoch.format(), "2026-05-20");
    }

    #[test]
    fn invalid_date_is_rejected() {
        assert!(DateValue::parse("2026-02-30").is_err());
    }
}
