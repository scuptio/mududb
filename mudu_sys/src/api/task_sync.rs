use std::time::Duration;

pub trait SysTaskSync: Send + Sync {
    fn sleep_blocking(&self, dur: Duration);
}
