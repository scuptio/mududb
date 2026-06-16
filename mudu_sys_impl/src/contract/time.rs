use crate::imp::env::Sys;
use chrono::{DateTime, Utc};
use std::time::{Instant, SystemTime};

pub fn instant_now() -> Instant {
    Sys::instant_now()
}

pub fn system_time_now() -> SystemTime {
    Sys::system_time_now()
}

pub fn utc_now() -> DateTime<Utc> {
    Sys::utc_now()
}
