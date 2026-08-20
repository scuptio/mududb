#[cfg(target_os = "linux")]
mod imp {
    pub use crate::imp::native::linux::io_uring::worker_ring::*;
}

#[cfg(not(target_os = "linux"))]
mod imp {
    use std::sync::Arc;
    use std::task::Waker;

    use crate::time::Instant;
    use mudu::common::result::RS;

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

    #[allow(dead_code)]
    pub fn with_current_ring<F, R>(_f: F) -> RS<R>
    where
        F: FnOnce(&Arc<WorkerLocalRing>) -> RS<R>,
    {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            "current worker ring is not set"
        ))
    }

    #[allow(dead_code)]
    impl WorkerLocalRing {
        pub fn register_timeout(&self, _deadline: Instant, _waker: Waker) -> RS<u64> {
            Ok(0)
        }

        pub fn remove_timeout(&self, _deadline: Instant, _id: u64) -> RS<()> {
            Ok(())
        }

        pub fn take_expired_timeouts(&self, _now: Instant) -> RS<Vec<Waker>> {
            Ok(Vec::new())
        }

        pub fn next_timeout_deadline(&self) -> RS<Option<Instant>> {
            Ok(None)
        }
    }
}

pub use imp::*;
