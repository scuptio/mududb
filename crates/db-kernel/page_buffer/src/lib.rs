//! Thread-confined asynchronous byte-page buffers. No tree format lives here.
#![warn(missing_docs)]
mod access;
mod local_lock;
pub use local_lock::{LocalReadGuard, LocalRwLock, LocalWriteGuard};
mod clock;
mod edit;
mod handles;
mod image;
mod ordered_file;
mod store;

pub use access::{BufferAccess, BufferDomain};
use async_trait::async_trait;
pub use clock::{BufferOptions, BufferStats, ClockBufferManager};
pub use edit::{PageEdit, PreparedEdit};
pub use handles::{
    PagePin, PageReadGuard, PageRef, PageReservation, PageWriteGuard, PreparedWrite, ResidentPage,
};
pub use image::{ImageIdobject, ImageValidator, PageImage, PageWriteback};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
pub use ordered_file::OrderedFile;
use std::collections::HashSet;
pub use store::{FilePageStore, MemoryPageStore, PageStore};

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::Storage, message)
}

/// Allocation high-water mark. Free slots are rediscovered by reachability sweep.
#[derive(Clone, Copy, Debug)]
pub struct AllocatorState {
    /// First never-reserved page number. Zero is never a valid page number.
    pub next_page: u64,
}

/// Object-safe, partition-local service for fixed-size byte pages. Methods require a manager-bound
/// access lease; retain it across the entire traversal, including copied page IDs.
/// This is a cache contract, not a transaction or a database checkpoint protocol.
/// Backends must call PageImage::writeback_bytes before persisting an image,
/// including initialization, and keep failed writebacks dirty for retry.
#[async_trait(?Send)]
pub trait BufferManager {
    /// Optional immediate traversal lease; None uses the asynchronous slow path.
    fn try_access(&self) -> Option<BufferAccess<'_>> {
        None
    }
    /// Enters a shared traversal epoch.
    async fn access(&self) -> BufferAccess<'_>;
    /// Waits asynchronously for all earlier epochs before maintenance.
    async fn maintenance(&self) -> BufferAccess<'_>;
    /// Binds a logical page number to this manager (without performing I/O).
    fn page_ref(&self, id: u64) -> PageRef;
    /// Immediately pins a resident page, or returns None on a miss/contention.
    fn try_pin<'a>(&self, access: &'a BufferAccess<'_>, page: &PageRef) -> RS<Option<PagePin<'a>>>;
    /// Pins a page, loading asynchronously when necessary.
    async fn pin<'a>(&self, access: &'a BufferAccess<'_>, page: &PageRef) -> RS<PagePin<'a>>;
    /// Reserves an unpublished page number; cancellation leaves a reclaimable orphan.
    async fn reserve_page<'a>(&self, access: &'a BufferAccess<'_>) -> RS<PageReservation<'a>>;
    /// Initializes a reservation. Only the returned reference may be published.
    async fn initialize(
        &self,
        access: &BufferAccess<'_>,
        page: PageReservation<'_>,
        image: PageImage,
    ) -> RS<PageRef>;
    /// Initializes a reservation as a dirty resident page without any backing
    /// write. For callers that already made the image durable themselves (for
    /// example a WAL append): the backing store is written later by eviction
    /// or flush. The default delegates to [`BufferManager::initialize`], a
    /// durable superset of this contract.
    async fn initialize_resident(
        &self,
        access: &BufferAccess<'_>,
        page: PageReservation<'_>,
        image: PageImage,
    ) -> RS<PageRef> {
        self.initialize(access, page, image).await
    }
    /// Best-effort, bounded read-ahead.
    async fn prefetch(&self, access: &BufferAccess<'_>, page: &PageRef);
    /// Writes dirty images without forcing durability; the default is
    /// [`BufferManager::flush`]. Callers whose writes are redo-protected
    /// (WAL) use this for background writeback and keep `flush` for points
    /// that must fsync.
    async fn writeback_dirty(&self, access: &BufferAccess<'_>) -> RS<()> {
        self.flush(access).await
    }
    /// Writes dirty images and makes prior writes durable; not an atomic snapshot.
    async fn flush(&self, access: &BufferAccess<'_>) -> RS<()>;
    /// Number of resident pages whose newest image was not written back yet.
    fn dirty_count(&self) -> usize {
        0
    }
    /// Returns allocation metadata, stable under an exclusive maintenance lease.
    fn allocator_state(&self, access: &BufferAccess<'_>) -> RS<AllocatorState>;
    /// Reuses unreachable slots. Requires an exclusive lease and no outstanding pins.
    async fn reclaim(&self, access: &BufferAccess<'_>, live: &HashSet<u64>) -> RS<usize>;
    /// Required size of every image managed by this instance.
    fn page_size(&self) -> usize;
    /// Number of resident frames this manager holds; zero for managers that
    /// do not publish a residency bound (consumers size derived caches by it).
    fn frames(&self) -> usize {
        0
    }
    /// Cumulative cache counters.
    fn stats(&self) -> BufferStats;
}

#[cfg(test)]
mod io_tests;
#[cfg(test)]
mod local_tests;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod edit_tests;
