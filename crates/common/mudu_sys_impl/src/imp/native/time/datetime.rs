use std::ops::{Deref, Sub};
use std::time::Duration;

/// Marker type representing UTC, analogous to `chrono::Utc`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Utc;

/// A wrapper around `chrono::DateTime<chrono::Utc>`.
///
/// The generic `Tz` parameter is preserved for API compatibility with
/// `chrono::DateTime<Utc>`, but the current implementation always stores a
/// UTC-backed datetime.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DateTime<Tz = Utc>(chrono::DateTime<chrono::Utc>, std::marker::PhantomData<Tz>);

impl DateTime<Utc> {
    pub fn now() -> Self {
        Self(chrono::Utc::now(), std::marker::PhantomData)
    }
}

impl<Tz> DateTime<Tz> {
    pub fn from_utc(dt: chrono::DateTime<chrono::Utc>) -> Self {
        Self(dt, std::marker::PhantomData)
    }

    pub fn into_utc(self) -> chrono::DateTime<chrono::Utc> {
        self.0
    }

    pub fn timestamp(&self) -> i64 {
        self.0.timestamp()
    }

    pub fn timestamp_millis(&self) -> i64 {
        self.0.timestamp_millis()
    }

    pub fn timestamp_micros(&self) -> i64 {
        self.0.timestamp_micros()
    }

    pub fn timestamp_nanos(&self) -> i64 {
        self.0.timestamp_nanos_opt().unwrap_or(i64::MAX)
    }

    pub fn checked_add_signed(&self, duration: Duration) -> Option<Self> {
        self.0
            .checked_add_signed(chrono::Duration::from_std(duration).ok()?)
            .map(Self::from_utc)
    }

    pub fn checked_sub_signed(&self, duration: Duration) -> Option<Self> {
        self.0
            .checked_sub_signed(chrono::Duration::from_std(duration).ok()?)
            .map(Self::from_utc)
    }

    pub fn signed_duration_since<Tz2>(&self, rhs: DateTime<Tz2>) -> chrono::Duration {
        self.0.signed_duration_since(rhs.0)
    }
}

impl<Tz, Tz2> Sub<DateTime<Tz2>> for DateTime<Tz> {
    type Output = chrono::Duration;

    fn sub(self, rhs: DateTime<Tz2>) -> Self::Output {
        self.0.signed_duration_since(rhs.0)
    }
}

impl<Tz> Deref for DateTime<Tz> {
    type Target = chrono::DateTime<chrono::Utc>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<Tz> From<chrono::DateTime<chrono::Utc>> for DateTime<Tz> {
    fn from(value: chrono::DateTime<chrono::Utc>) -> Self {
        Self(value, std::marker::PhantomData)
    }
}

impl<Tz> From<DateTime<Tz>> for chrono::DateTime<chrono::Utc> {
    fn from(value: DateTime<Tz>) -> Self {
        value.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn datetime_now_timestamp_is_non_zero() {
        let dt = DateTime::<Utc>::now();
        assert!(dt.timestamp() > 0);
    }

    #[test]
    fn datetime_from_utc_into_utc_roundtrip() {
        let utc = chrono::Utc::now();
        let dt = DateTime::<Utc>::from_utc(utc);
        assert_eq!(dt.into_utc(), utc);
    }

    #[test]
    fn datetime_timestamp_variants() {
        let utc = chrono::DateTime::from_timestamp(1_700_000_000, 123_456_789).unwrap();
        let dt = DateTime::<Utc>::from_utc(utc);
        assert_eq!(dt.timestamp(), 1_700_000_000);
        assert_eq!(dt.timestamp_millis(), 1_700_000_000_123);
        assert_eq!(dt.timestamp_micros(), 1_700_000_000_123_456);
    }

    #[test]
    fn datetime_checked_add_then_sub_returns_original() {
        let dt = DateTime::<Utc>::from_utc(chrono::Utc::now());
        let plus = dt.checked_add_signed(Duration::from_secs(60)).unwrap();
        let minus = plus.checked_sub_signed(Duration::from_secs(60)).unwrap();
        assert_eq!(minus.timestamp(), dt.timestamp());
    }

    #[test]
    fn datetime_checked_add_signed_overflow() {
        let dt = DateTime::<Utc>::from_utc(chrono::Utc::now());
        let huge = Duration::from_secs(u64::MAX);
        assert!(dt.checked_add_signed(huge).is_none());
    }

    #[test]
    fn datetime_signed_duration_since() {
        let earlier = DateTime::<Utc>::from_utc(chrono::Utc::now());
        let later = DateTime::<Utc>::from_utc(earlier.0 + chrono::Duration::seconds(10));
        let diff = later.signed_duration_since(earlier);
        assert_eq!(diff.num_seconds(), 10);
    }

    #[test]
    fn datetime_sub_trait() {
        let earlier = DateTime::<Utc>::from_utc(chrono::Utc::now());
        let later = DateTime::<Utc>::from_utc(earlier.0 + chrono::Duration::seconds(10));
        let diff: chrono::Duration = later - earlier;
        assert_eq!(diff.num_seconds(), 10);
    }

    #[test]
    fn datetime_from_chrono() {
        let utc = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
        let dt: DateTime<Utc> = DateTime::from(utc);
        assert_eq!(dt.timestamp(), utc.timestamp());
        assert_eq!((*dt).timestamp(), utc.timestamp());
    }

    #[test]
    fn datetime_deref_to_chrono() {
        let dt = DateTime::<Utc>::from_utc(chrono::Utc::now());
        assert_eq!((*dt).timestamp(), dt.timestamp());
    }
}
