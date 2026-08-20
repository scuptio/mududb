//! Public time, instant, and datetime helpers.
#![allow(missing_docs)]
pub use crate::imp::time::{DateTime, Instant, SystemTime, Utc};

use std::time::Duration;

pub fn instant_now() -> Instant {
    crate::default_env().time().instant_now()
}

pub fn system_time_now() -> SystemTime {
    crate::default_env().time().system_time_now()
}

pub fn utc_now() -> DateTime<Utc> {
    crate::default_env().time().utc_now()
}

/// CPU time consumed by the calling thread (Linux
/// `CLOCK_THREAD_CPUTIME_ID`).
///
/// Advances only while the thread runs on a CPU, not while it is blocked on
/// sleep, locks, IO, or scheduler queueing. Returns `None` on platforms
/// without a per-thread CPU clock; callers must treat it as an optional
/// metric.
pub fn thread_cpu_time_now() -> Option<Duration> {
    crate::imp::time::thread_cpu_time_now()
}
