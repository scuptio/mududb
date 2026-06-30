use std::ops::{Add, AddAssign, Deref, Sub, SubAssign};
use std::time::Duration;

/// A wrapper around `std::time::SystemTime`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SystemTime(pub(crate) std::time::SystemTime);

impl SystemTime {
    pub fn now() -> Self {
        Self(std::time::SystemTime::now())
    }

    pub fn from_std(st: std::time::SystemTime) -> Self {
        Self(st)
    }

    pub fn into_std(self) -> std::time::SystemTime {
        self.0
    }

    pub fn checked_add(&self, duration: Duration) -> Option<SystemTime> {
        self.0.checked_add(duration).map(SystemTime)
    }

    pub fn checked_sub(&self, duration: Duration) -> Option<SystemTime> {
        self.0.checked_sub(duration).map(SystemTime)
    }
}

impl Deref for SystemTime {
    type Target = std::time::SystemTime;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Add<Duration> for SystemTime {
    type Output = SystemTime;

    fn add(self, rhs: Duration) -> Self::Output {
        SystemTime(self.0 + rhs)
    }
}

impl AddAssign<Duration> for SystemTime {
    fn add_assign(&mut self, rhs: Duration) {
        self.0 += rhs;
    }
}

impl Sub<Duration> for SystemTime {
    type Output = SystemTime;

    fn sub(self, rhs: Duration) -> Self::Output {
        SystemTime(self.0 - rhs)
    }
}

impl SubAssign<Duration> for SystemTime {
    fn sub_assign(&mut self, rhs: Duration) {
        self.0 -= rhs;
    }
}

impl PartialOrd for SystemTime {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for SystemTime {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl From<std::time::SystemTime> for SystemTime {
    fn from(value: std::time::SystemTime) -> Self {
        Self(value)
    }
}

impl From<SystemTime> for std::time::SystemTime {
    fn from(value: SystemTime) -> Self {
        value.0
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn system_time_now() {
        let now = SystemTime::now();
        assert_ne!(now, SystemTime::from_std(std::time::UNIX_EPOCH));
    }

    #[test]
    fn system_time_from_std_into_std_roundtrip() {
        let std = std::time::SystemTime::now();
        let st = SystemTime::from_std(std);
        assert_eq!(st.into_std(), std);
    }

    #[test]
    fn system_time_checked_add_sub() {
        let st = SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH);
        let added = st.checked_add(Duration::from_secs(1)).unwrap();
        let subbed = added.checked_sub(Duration::from_secs(1)).unwrap();
        assert_eq!(subbed, st);
    }

    #[test]
    fn system_time_checked_sub_overflow() {
        let st = SystemTime::now();
        assert!(st.checked_sub(Duration::from_secs(u64::MAX)).is_none());
    }

    #[test]
    fn system_time_add_sub_traits() {
        let st = SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH);
        let result = st + Duration::from_secs(5) - Duration::from_secs(5);
        assert_eq!(result, st);
    }

    #[test]
    fn system_time_add_assign_sub_assign() {
        let mut st = SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH);
        st += Duration::from_secs(10);
        assert!(st > SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH));
        st -= Duration::from_secs(10);
        assert_eq!(st, SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH));
    }

    #[test]
    fn system_time_ord() {
        let earlier = SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH);
        let later = earlier + Duration::from_secs(1);
        assert!(earlier < later);

        let same = SystemTime::from_std(std::time::SystemTime::UNIX_EPOCH);
        assert_eq!(earlier, same);
    }

    #[test]
    fn system_time_from_std() {
        let std = std::time::SystemTime::now();
        let from = SystemTime::from(std);
        assert_eq!(from, SystemTime::from_std(std));
    }
}
