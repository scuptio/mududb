//! Public time, instant, and datetime helpers.
#![allow(missing_docs)]
pub use crate::imp::time::{DateTime, Instant, SystemTime, Utc};

pub fn instant_now() -> Instant {
    crate::default_env().time().instant_now()
}

pub fn system_time_now() -> SystemTime {
    crate::default_env().time().system_time_now()
}

pub fn utc_now() -> DateTime<Utc> {
    crate::default_env().time().utc_now()
}
