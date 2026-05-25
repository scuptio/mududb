use crate::api::task_sync::SysTaskSync;
use std::time::Duration;

pub struct LinuxTaskSync;

impl SysTaskSync for LinuxTaskSync {
    fn sleep_blocking(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}
