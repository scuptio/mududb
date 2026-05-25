use std::sync::Arc;

pub(crate) struct WorkerLocalRing;

#[allow(dead_code)]
pub(crate) fn set_current_worker_ring(_ring: Arc<WorkerLocalRing>) {}

#[allow(dead_code)]
pub(crate) fn unset_current_worker_ring() {}

pub(crate) fn has_current_worker_ring() -> bool {
    false
}

#[allow(dead_code)]
pub(crate) fn current_ring() -> &'static WorkerLocalRing {
    panic!("worker ring is only available on linux")
}
