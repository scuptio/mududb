/// Blocking eventfd and raw file-descriptor helpers.
pub mod blocking;
/// Bounded blocking MPSC channel.
pub mod channel;
/// Blocking mutex and condition variable over `std::sync`.
pub mod std_mutex;
/// Blocking read-write lock over `std::sync`.
pub mod std_rwlock;
/// Unbounded blocking MPSC channel.
pub mod unbounded_channel;
pub(crate) mod unique_inner;
