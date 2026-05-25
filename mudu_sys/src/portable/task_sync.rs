use crate::api::task_sync::SysTaskSync;
use std::time::Duration;

pub struct PortableTaskSync;

impl SysTaskSync for PortableTaskSync {
    fn sleep_blocking(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}
