//! Partition-local borrows complete synchronously; conflicts suspend immediately.
//! Spinning cannot advance a lock holder on the same executor thread.
use super::PagedBTree;
use page_buffer::{PagePin, PageReadGuard, PageWriteGuard};

impl<V: Send> PagedBTree<V> {
    /// The caller already failed a nonblocking acquisition, possibly in the
    /// synchronous write traversal. Count it even if the first retry succeeds.
    pub(super) async fn contended_read<'a>(&self, page: &'a PagePin<'_>) -> PageReadGuard<'a> {
        let started = self.latch_wait_started();
        let guard = page.read().await;
        self.latch_wait_finished(started);
        guard
    }

    pub(super) async fn resident_write<'a>(&self, page: &'a PagePin<'_>) -> PageWriteGuard<'a> {
        // Only the synchronous traversal's failed writes reach this continuation.
        let started = self.latch_wait_started();
        let guard = page.write().await;
        self.latch_wait_finished(started);
        guard
    }
}
