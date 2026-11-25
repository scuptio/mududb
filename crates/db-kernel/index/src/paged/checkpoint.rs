//! Checkpoint metadata belongs to the index; flushing bytes and allocator state
//! belongs to the buffer. The relation layer publishes the durable clean marker
//! only after the tree, row versions, and authoritative files have been flushed.
use super::{invalid, node::Link, BufferOptions, PageStore, PagedBTree, RS};
use crate::index_key::compare_context::CompareContext;
use page_buffer::{AllocatorState, BufferManager, ClockBufferManager};
use std::rc::Rc;
use std::sync::Arc;

/// Root and allocator state of a quiescent row-ID tree. Persist this only after
/// all associated row versions and authoritative relation files are durable.
#[derive(Clone, Debug)]
pub struct TreeCheckpoint {
    /// Root page ID.
    pub root: u64,
    /// First never-allocated page ID.
    pub next_page: u64,
    /// Number of mappings.
    pub count: u64,
    /// Next generic value reservation ID.
    pub next_value: u64,
    /// Encoded page size.
    pub page_size: usize,
}

impl PagedBTree<u128> {
    /// Flushes a consistent checkpoint without closing the tree. Mapping
    /// operations wait asynchronously during the flush. The owner must durably
    /// invalidate its manifest BEFORE any subsequent mutation of this store;
    /// this is an in-place checkpoint, not an immutable historical snapshot.
    pub async fn checkpoint(&self) -> RS<TreeCheckpoint> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        self.checkpoint_locked(access).await
    }

    /// Stops mapping operations and durably flushes the tree's pages. On
    /// cancellation/error the tree remains closed; this method may be retried.
    /// Publishing and invalidating the external checkpoint is the owner's job.
    pub async fn close_checkpoint(&self) -> RS<TreeCheckpoint> {
        // The exclusive lease drains in-flight operations first; marking the
        // tree closed afterwards keeps their completion guarantee intact
        // while every later operation fails its open check.
        let access = &self.buffer.maintenance().await;
        self.closed.set(true);
        self.checkpoint_locked(access).await
    }

    async fn checkpoint_locked(
        &self,
        access: &page_buffer::BufferAccess<'_>,
    ) -> RS<TreeCheckpoint> {
        if self.inline.is_none() {
            return Err(invalid("checkpoint requires inline row IDs"));
        }
        self.buffer.flush(access).await?;
        Ok(TreeCheckpoint {
            root: self.root.get(),
            next_page: self.buffer.allocator_state(access)?.next_page,
            count: self.count.get() as u64,
            next_value: self.next_value.get(),
            page_size: self.buffer.page_size(),
        })
    }

    /// Opens a validated, previously closed checkpoint. The owner must first
    /// durably invalidate its clean marker before allowing this tree to mutate.
    pub async fn restore_row_ids(
        context: CompareContext,
        options: BufferOptions,
        store: Arc<dyn PageStore>,
        state: &TreeCheckpoint,
    ) -> RS<Self> {
        if state.root == 0
            || state.root >= state.next_page
            || state.next_value == 0
            || state.page_size != options.page_size
            || state.count > usize::MAX as u64
        {
            return Err(invalid("invalid tree checkpoint"));
        }
        let buffer = Rc::new(ClockBufferManager::new(
            options,
            store,
            AllocatorState {
                next_page: state.next_page,
            },
            None,
        )?);
        Self::restore_with_buffer(context, buffer, state).await
    }

    /// Restores row IDs using an injected buffer opened with the saved allocator
    /// state. The manager's page space must be exclusively owned by this tree.
    pub async fn restore_with_buffer(
        context: CompareContext,
        buffer: Rc<dyn BufferManager>,
        state: &TreeCheckpoint,
    ) -> RS<Self> {
        super::codec::validate_size(buffer.page_size())?;
        let access = buffer.maintenance().await;
        if state.root == 0
            || state.root >= state.next_page
            || state.next_value == 0
            || state.page_size != buffer.page_size()
            || state.count > usize::MAX as u64
            || buffer.allocator_state(&access)?.next_page != state.next_page
        {
            return Err(invalid("invalid tree checkpoint"));
        }
        drop(access);
        let mut tree = Self::from_buffer(context, buffer, state.root);
        tree.inline = Some(super::tree::InlineCodec {
            encode: |id| *id,
            decode: |id| id,
        });
        tree.next_value.set(state.next_value);
        tree.count.set(state.count as usize);
        {
            let access = tree.buffer.access().await;
            tree.snapshot(&access, &Link::new(state.root)).await?;
        }
        Ok(tree)
    }
}
