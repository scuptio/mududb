#![allow(missing_docs)]
pub mod datetime;
pub mod instant;
pub mod system_time;

pub use datetime::{DateTime, Utc};
pub use instant::Instant;
pub use system_time::SystemTime;

use mudu::common::result::RS;
use std::future::Future;
use std::time::Duration;

pub struct SysTime;

impl Default for SysTime {
    fn default() -> Self {
        Self::new()
    }
}

impl SysTime {
    pub fn new() -> Self {
        Self
    }

    pub fn instant_now(&self) -> Instant {
        Instant::now()
    }

    pub fn system_time_now(&self) -> SystemTime {
        SystemTime::now()
    }

    pub fn utc_now(&self) -> DateTime<Utc> {
        DateTime::now()
    }

    pub fn sleep_blocking(&self, dur: Duration) {
        super::task::TaskSync::sleep_blocking(dur)
    }

    pub async fn sleep(&self, dur: Duration) -> RS<()> {
        super::task::TaskAsync::sleep(dur).await
    }

    pub async fn timeout<F>(&self, dur: Duration, fut: F) -> Option<F::Output>
    where
        F: Future,
    {
        super::task::TaskAsync::timeout(dur, fut).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn new_and_default_produce_usable_instance() {
        let a = SysTime::new();
        let _ = a.instant_now();
        let b = SysTime;
        let _ = b.system_time_now();
    }

    #[test]
    fn instant_now_is_monotonic() {
        let time = SysTime::new();
        let earlier = time.instant_now();
        let later = time.instant_now();
        assert!(later >= earlier);
    }

    #[test]
    fn system_time_now_after_epoch() {
        let time = SysTime::new();
        let now = time.system_time_now();
        assert!(now > super::SystemTime::from_std(std::time::UNIX_EPOCH));
    }

    #[test]
    fn utc_now_timestamp_positive() {
        let time = SysTime::new();
        assert!(time.utc_now().timestamp() > 0);
    }

    #[test]
    fn sleep_blocking_waits_at_least_duration() {
        let time = SysTime::new();
        let before = std::time::Instant::now();
        time.sleep_blocking(Duration::from_millis(50));
        let elapsed = before.elapsed();
        assert!(elapsed >= Duration::from_millis(50));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn sleep_waits_at_least_duration() {
        let time = SysTime::new();
        let before = std::time::Instant::now();
        assert!(time.sleep(Duration::from_millis(50)).await.is_ok());
        let elapsed = before.elapsed();
        assert!(elapsed >= Duration::from_millis(50));
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timeout_returns_some_for_quick_future() {
        let time = SysTime::new();
        let result = time
            .timeout(
                Duration::from_millis(100),
                time.sleep(Duration::from_millis(1)),
            )
            .await;
        assert!(result.is_some());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn timeout_returns_none_for_slow_future() {
        let time = SysTime::new();
        let result = time
            .timeout(
                Duration::from_millis(1),
                time.sleep(Duration::from_millis(100)),
            )
            .await;
        assert!(result.is_none());
    }

    #[test]
    fn two_instances_agree_within_small_window() {
        let a = SysTime::new();
        let b = SysTime::new();
        let diff = (a.utc_now().timestamp() - b.utc_now().timestamp()).abs();
        assert!(diff <= 1);
    }
}
