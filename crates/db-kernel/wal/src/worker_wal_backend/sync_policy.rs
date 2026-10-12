use std::time::Duration;

/// Durability policy applied by the worker WAL flush driver.
///
/// WAL frames carry CRC checksums and recovery truncates each chunk at the
/// first invalid frame (see `worker_log::scan_valid_frame_prefix`), so an
/// un-fsynced tail lost in a power failure is dropped cleanly at startup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WalSyncPolicy {
    /// Fsync every flush round before reporting its LSNs durable. A
    /// committed transaction survives any power loss; this is the current
    /// behavior and the default.
    #[default]
    Commit,
    /// Flush rounds only write() into the page cache and report LSNs
    /// durable immediately; a background driver fsyncs the dirty chunks at
    /// most once per `interval`. A power loss may lose acknowledged commits
    /// from the last `interval` (like PostgreSQL `synchronous_commit=off`).
    Periodic {
        /// Minimum time between two fsyncs of the WAL chunks.
        interval: Duration,
    },
}
