use chrono::{DateTime, Utc};
use std::time::{Instant, SystemTime};

pub struct Time;

impl Time {
    pub fn instant_now() -> Instant {
        panic!("[sim] Time::instant_now not implemented")
    }

    pub fn system_time_now() -> SystemTime {
        panic!("[sim] Time::system_time_now not implemented")
    }

    pub fn utc_now() -> DateTime<Utc> {
        panic!("[sim] Time::utc_now not implemented")
    }
}
