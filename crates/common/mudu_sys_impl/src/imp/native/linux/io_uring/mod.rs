mod completion;
/// File operations on the worker ring.
pub mod file;
/// Low-level io_uring ring wrapper over `rliburing`.
pub mod iouring;
/// Path operations (mkdir/statx/unlink) on the worker ring.
pub mod path;
mod path_io;
mod sock_addr_buf;
/// Socket operations on the worker ring.
pub mod socket;
mod submission_queue_entry;
#[cfg(test)]
pub(crate) mod test_ring;
/// Thread-local worker ring: queues user IO ops and drives the shared ring.
pub mod worker_ring;
