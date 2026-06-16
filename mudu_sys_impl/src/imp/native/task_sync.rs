use std::time::Duration;

pub struct TaskSync;

impl TaskSync {
    pub fn sleep_blocking(dur: Duration) {
        std::thread::sleep(dur);
    }
}
