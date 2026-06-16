#[cfg(target_os = "linux")]
mod imp {
    pub use crate::imp::native::linux::io_uring::worker_ring::*;
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::sync::Arc;

    pub struct WorkerLocalRing;

    #[allow(dead_code)]
    pub fn set_current_worker_ring(_ring: Arc<WorkerLocalRing>) {}

    #[allow(dead_code)]
    pub fn unset_current_worker_ring() {}

    pub fn has_current_worker_ring() -> bool {
        false
    }

    #[allow(dead_code)]
    pub fn current_ring() -> &'static WorkerLocalRing {
        panic!("worker ring is only available on linux")
    }
}

pub use imp::*;
