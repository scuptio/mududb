use std::time::Duration;

pub struct TaskSync;

impl TaskSync {
    pub fn sleep_blocking(_dur: Duration) {
        // sim：空操作，不等待
    }
}
