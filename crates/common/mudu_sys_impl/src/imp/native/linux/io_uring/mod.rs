mod completion;
/// File operations on the worker ring.
pub mod file;
/// Low-level io_uring ring wrapper over `rliburing`.
pub mod iouring;
/// Path operations (mkdir/statx/unlink) on the worker ring.
pub mod path;
/// Socket operations on the worker ring.
pub mod socket;
mod submission_queue_entry;
/// Thread-local worker ring: queues user IO ops and drives the shared ring.
pub mod worker_ring;
