use chrono::{DateTime, Utc};
use std::time::{Instant, SystemTime};

pub struct Time;

impl Time {
    pub fn instant_now() -> Instant {
        Instant::now()
    }

    pub fn system_time_now() -> SystemTime {
        SystemTime::now()
    }

    pub fn utc_now() -> DateTime<Utc> {
        Utc::now()
    }
}
