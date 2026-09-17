use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};
use std::time::Duration;

/// A wrapper around `std::time::Instant`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Instant(pub(crate) std::time::Instant);

impl Instant {
    pub fn now() -> Self {
        Self(std::time::Instant::now())
    }

    pub fn from_std(instant: std::time::Instant) -> Self {
        Self(instant)
    }

    pub fn into_std(self) -> std::time::Instant {
        self.0
    }

    pub fn duration_since(&self, earlier: Instant) -> Duration {
        self.0.duration_since(earlier.0)
    }

    pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
        self.0.saturating_duration_since(earlier.0)
    }

    pub fn elapsed(&self) -> Duration {
        self.0.elapsed()
    }

    pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
        self.0.checked_add(duration).map(Instant)
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
        self.0.checked_sub(duration).map(Instant)
    }
}

impl Deref for Instant {
    type Target = std::time::Instant;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Add<Duration> for Instant {
    type Output = Instant;

    fn add(self, rhs: Duration) -> Self::Output {
        Instant(self.0 + rhs)
    }
}

impl AddAssign<Duration> for Instant {
    fn add_assign(&mut self, rhs: Duration) {
        self.0 += rhs;
    }
}

impl Sub<Duration> for Instant {
    type Output = Instant;

    fn sub(self, rhs: Duration) -> Self::Output {
        Instant(self.0 - rhs)
    }
}

impl Sub<Instant> for Instant {
    type Output = Duration;

    fn sub(self, rhs: Instant) -> Self::Output {
        self.0 - rhs.0
    }
}

impl SubAssign<Duration> for Instant {
    fn sub_assign(&mut self, rhs: Duration) {
        self.0 -= rhs;
    }
}

impl PartialOrd for Instant {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Instant {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl From<std::time::Instant> for Instant {
    fn from(value: std::time::Instant) -> Self {
        Self(value)
    }
}

impl From<Instant> for std::time::Instant {
    fn from(value: Instant) -> Self {
        value.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::Instant;
    use std::time::Duration;

    #[test]
    #[allow(clippy::disallowed_methods, reason = "testing std instant conversion")]
    fn from_std_into_std_roundtrip() {
        let std_instant = std::time::Instant::now();
        let wrapped = Instant::from_std(std_instant);
        assert_eq!(wrapped.into_std(), std_instant);
    }

    #[test]
    fn checked_add_sub_arithmetic() {
        let now = Instant::now();
        let later = now.checked_add(Duration::from_secs(10)).unwrap();
        let earlier = later.checked_sub(Duration::from_secs(5)).unwrap();
        assert!(later > now);
        assert!(earlier > now);
        assert!(later > earlier);
    }

    #[test]
    fn duration_since_self_is_zero() {
        let now = Instant::now();
        assert_eq!(now.duration_since(now), Duration::ZERO);
    }

    #[test]
    fn ordering_earlier_is_less_than_later() {
        let earlier = Instant::now();
        let later = earlier.checked_add(Duration::from_nanos(1)).unwrap();
        assert!(earlier < later);
        assert!(later > earlier);
        assert_eq!(earlier.cmp(&later), std::cmp::Ordering::Less);
    }
}
