use super::{Relation, RelationInner};
use mudu::{common::result::RS, error::ErrorCode, mudu_error};
use std::sync::atomic::Ordering;

impl Relation {
    /// Merges index siblings and reuses unreachable page slots without closing
    /// the relation. Point operations overlap the merge phase; reclamation waits
    /// asynchronously for previous index operations to leave their epoch.
    /// MVCC tombstones remain until a separate visibility-aware vacuum removes them.
    pub async fn maintain_index(&self) -> RS<::index::paged::MaintenanceStats> {
        let closed = self.inner.lifecycle.read().await;
        if *closed {
            return Err(mudu_error!(ErrorCode::Storage, "relation is closed"));
        }
        self.inner.index.maintain().await
    }

    /// Publishes an in-place checkpoint while keeping the relation open.
    /// Writers wait asynchronously for the duration; readers continue. The first
    /// following write invalidates the clean marker durably before modifying data.
    /// A crash after later writes rebuilds from authority/WAL, not this checkpoint.
    pub async fn checkpoint(&self) -> RS<()> {
        let closed = self.inner.lifecycle.write().await;
        if *closed {
            return Err(mudu_error!(ErrorCode::Storage, "relation is closed"));
        }
        self.publish_checkpoint(false).await
    }

    /// Drains writes and publishes a clean index checkpoint. Writes after close
    /// fail. A cancelled or failed close can be retried; Drop performs no I/O.
    pub async fn close(&self) -> RS<()> {
        let mut closed = self.inner.lifecycle.write().await;
        *closed = true;
        self.publish_checkpoint(true).await
    }

    async fn publish_checkpoint(&self, close: bool) -> RS<()> {
        // Invalidate a preceding online checkpoint before any new flush starts.
        self.inner.index.before_write().await?;
        self.flush_wal_async().await?;
        self.inner.key_file.flush().await?;
        self.inner.value_file.flush().await?;
        if self.inner.cache_tainted.load(Ordering::Acquire) {
            return Err(mudu_error!(
                ErrorCode::Storage,
                "index checkpoint disabled after incomplete write; reopen to rebuild"
            ));
        }
        let stamp =
            RelationInner::cache_stamp(&self.inner.key_file, &self.inner.value_file).await?;
        self.inner
            .index
            .checkpoint(
                stamp,
                self.inner.next_tuple_id.load(Ordering::Acquire),
                close,
            )
            .await
    }
}
