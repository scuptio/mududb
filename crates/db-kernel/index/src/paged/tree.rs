use super::node_cache::{CachedKey, CachedNode, NodeCache};
use super::{
    codec, invalid,
    node::{Key, Link, Node, Page},
    BufferOptions, BufferStats, MemoryPageStore, PageStore, RS,
};
use crate::index_key::compare_context::CompareContext;
use page_buffer::LocalRwLock as AMutex;
use page_buffer::{
    AllocatorState, BufferAccess, BufferManager, ClockBufferManager, PageImage, PagePin,
};
use std::cell::RefCell;
use std::{cell::Cell, rc::Rc};
use std::{cmp::Ordering, collections::HashMap, sync::Arc};

/// Partition-local async B-link tree with a bounded page cache.
/// Resident operations use synchronous local borrows; only conflicting tasks or
/// backing I/O suspend. Construct and use the tree on its owning LocalSet.
/// Cross-partition callers must send requests rather than share this object.
///
/// ```compile_fail
/// use index::paged::PagedBTree;
/// fn requires_send<T: Send>() {}
/// requires_send::<PagedBTree<u128>>();
/// ```
/// ```compile_fail
/// use index::paged::PagedBTree;
/// fn requires_sync<T: Sync>() {}
/// requires_sync::<PagedBTree<u128>>();
/// ```
///
/// Generic values remain in memory. File backing bounds key-page residency,
/// not values, overflow-operation scratch space, or returned range results.
/// Raw backing files are rebuildable caches, not committed checkpoints.
pub struct PagedBTree<V> {
    // The tree owns logical links and encoding; the manager owns physical residency.
    pub(super) buffer: Rc<dyn BufferManager>,
    // Explicit comparator capability; never inferred from arbitrary tuple bytes.
    pub(super) lexicographic: bool,
    pub(super) diagnostics: Option<Arc<super::diagnostics::Diagnostics>>,
    // Both handles refer to one allocation. Images retain the erased policy;
    // the builder keeps typed access to its optional diagnostic activation.
    pub(super) writeback: Arc<super::images::IndexWriteback>,
    pub(super) writeback_policy: Arc<dyn page_buffer::PageWriteback>,
    pub(super) root: Cell<u64>,
    pub(super) context: CompareContext,
    // Serializes structural edits while ordinary leaf operations use page latches.
    // Tree-wide quiescence (close, exact count, scans, reclamation) rides on the
    // buffer's exclusive maintenance lease instead of a separate tree gate.
    pub(super) structure: AMutex<()>,
    pub(super) values: Vec<AMutex<HashMap<u64, V>>>,
    pub(super) next_value: Cell<u64>,
    pub(super) count: Cell<usize>,
    pub(super) inline: Option<InlineCodec<V>>,
    pub(super) closed: Cell<bool>,
    // Decoded nodes cached by page identity and validated by write epoch;
    // bounded by the buffer's frame count (zero disables it).
    pub(super) nodes: RefCell<NodeCache>,
    // Last leaf a resident write landed on, or 0. Append/delete streams hit
    // one leaf for a whole burst; the memo is only a hint — every use
    // revalidates type, fence and key envelope on the latched bytes.
    pub(super) memo: Cell<u64>,
    // Lossy per-version visit hints for decode-worthiness: two cell ops per
    // visit, no allocation. A tag match means "this exact page version was
    // visited before, decode it now"; a mismatch remarks it. Collisions and
    // stale tags only lose or waste a decode, never correctness.
    pub(super) seen_hint: Vec<Cell<u64>>,
}

impl<V: Send> PagedBTree<V> {
    /// Creates an empty in-memory tree with default page-cache options.
    pub fn new(context: CompareContext) -> RS<Self> {
        Self::with_store(
            context,
            BufferOptions::default(),
            Arc::new(MemoryPageStore::default()),
        )
    }

    /// Creates an empty tree over a dedicated backing store. Existing cache
    /// contents are not recovered; callers must supply a fresh/private store.
    pub fn with_store(
        context: CompareContext,
        options: BufferOptions,
        store: Arc<dyn PageStore>,
    ) -> RS<Self> {
        codec::validate_size(options.page_size)?;
        let image = PageImage::new(codec::encode(
            &Page::Node(Node::empty(0)),
            options.page_size,
            1,
        )?);
        let buffer = Rc::new(ClockBufferManager::new(
            options,
            store,
            AllocatorState { next_page: 2 },
            Some((1, image)),
        )?);
        Ok(Self::from_buffer(context, buffer, 1))
    }

    pub(super) fn from_buffer(
        context: CompareContext,
        buffer: Rc<dyn BufferManager>,
        root: u64,
    ) -> Self {
        let nodes_capacity = buffer.frames();
        let writeback = Arc::new(super::images::IndexWriteback::default());
        Self {
            writeback_policy: writeback.clone(),
            writeback,
            lexicographic: false,
            diagnostics: None,
            buffer,
            root: Cell::new(root),
            context,
            structure: AMutex::new(()),
            values: (0..64).map(|_| AMutex::new(HashMap::new())).collect(),
            next_value: Cell::new(1),
            count: Cell::new(0),
            inline: None,
            closed: Cell::new(false),
            nodes: RefCell::new(NodeCache::new(nodes_capacity)),
            memo: Cell::new(0),
            seen_hint: (0..nodes_capacity).map(|_| Cell::new(u64::MAX)).collect(),
        }
    }

    /// Enables prefix/streaming comparisons for a comparator whose ordering is
    /// exactly lexicographic unsigned-byte order, with no extra validation or
    /// side effects. Do not enable for arbitrary SQL tuple encodings. This is a
    /// performance capability, not a change to the stored comparator or format;
    /// callers may enable it again after restore with the same byte-order contract.
    pub fn with_lexicographic_keys(mut self) -> Self {
        self.lexicographic = true;
        self
    }

    pub(super) async fn access(&self) -> BufferAccess<'_> {
        if let Some(access) = self.buffer.try_access() {
            return access;
        }
        self.buffer.access().await
    }

    /// Returns page-cache counters.
    pub fn buffer_stats(&self) -> BufferStats {
        self.buffer.stats()
    }

    /// Flushes cache pages. Does not make generic values durable or create a checkpoint.
    pub async fn flush_pages(&self) -> RS<()> {
        let access = &self.access().await;
        self.buffer.flush(access).await
    }

    /// Returns an exact count after preceding mapping mutations complete.
    pub async fn len(&self) -> RS<usize> {
        let _access = self.buffer.maintenance().await;
        Ok(self.count.get())
    }

    /// Whether the tree has no entries.
    pub async fn is_empty(&self) -> RS<bool> {
        Ok(self.len().await? == 0)
    }

    pub(super) fn root_link(&self) -> Link {
        Link::new(self.root.get())
    }

    pub(super) fn ensure_open(&self) -> RS<()> {
        if self.closed.get() {
            return Err(invalid("index is closed"));
        }
        Ok(())
    }

    pub(super) fn compare(&self, a: &[u8], b: &[u8]) -> RS<Ordering> {
        (self.context.comparator.compare)(a, b, &self.context.desc)
    }

    /// Bound inline keys so even minimum-size pages can split legally.
    pub(super) fn inline_key_len(&self, len: usize) -> usize {
        // Reserve space for split fences and several worst-case heads. Generic
        // comparators need full keys, so avoid unnecessary overflow visits.
        // Lexical comparisons can reject candidates from a short resident head;
        // retaining that head budget preserves fanout under cache pressure.
        let budget = (self.buffer.page_size() - 192) / 4;
        if self.lexicographic {
            budget.min(512).min(len)
        } else {
            budget.min(len)
        }
    }

    pub(super) async fn key(&self, access: &BufferAccess<'_>, bytes: &[u8]) -> RS<Key> {
        let inline = self.inline_key_len(bytes.len());
        if bytes.len() <= inline {
            return Ok(Key::inline(bytes));
        }
        let chunk = self.buffer.page_size() - 64;
        let mut next = 0;
        for bytes in bytes[inline..].rchunks(chunk) {
            next = self
                .allocate(
                    access,
                    Page::Overflow {
                        next,
                        bytes: bytes.to_vec(),
                    },
                )
                .await?
                .pid;
            cooperate().await;
        }
        Ok(Key {
            head: bytes[..inline].to_vec(),
            overflow: next,
            len: bytes.len() as u64,
        })
    }

    pub(super) async fn bytes(&self, access: &BufferAccess<'_>, key: &Key) -> RS<Vec<u8>> {
        let mut bytes = key.head.clone();
        let mut next = key.overflow;
        while next != 0 {
            self.overflow_read();
            let page = self.pin(access, &Link::new(next)).await?;
            let image = page.snapshot().await;
            let decoded = self.decode_image(&image, next).await?;
            let Page::Overflow {
                next: following,
                bytes: chunk,
            } = &decoded
            else {
                return Err(invalid("invalid overflow chain"));
            };
            if chunk.is_empty() || bytes.len() as u64 + chunk.len() as u64 > key.len {
                return Err(invalid("overflow chain exceeds key length"));
            }
            bytes.extend_from_slice(chunk);
            next = *following;
            drop(image);
            drop(page);
            cooperate().await;
        }
        if bytes.len() as u64 != key.len {
            return Err(invalid("truncated overflow key"));
        }
        Ok(bytes)
    }

    pub(super) async fn position(
        &self,
        access: &BufferAccess<'_>,
        node: &Node,
        key: &[u8],
    ) -> RS<(usize, bool)> {
        let (mut lo, mut hi) = (0, node.entries.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self.compare(&self.bytes(access, &node.entries[mid].key).await?, key)? {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok((mid, true)),
                Ordering::Greater => hi = mid,
            }
        }
        Ok((lo, false))
    }

    /// Pins the page and returns the level to descend on: a decoded node
    /// from the write-epoch-validated cache, or the zero-allocation borrowed
    /// image on the first visit of a page version. Decode-worthiness is
    /// tracked in the lossy hint table, so a page rewritten between two
    /// reads (mixed workloads) never pays a decode or map traffic.
    pub(super) async fn level_node<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        link: &Link,
    ) -> RS<(PagePin<'a>, LevelNode)> {
        let page = self.pin(access, link).await?;
        let token = page.cache_token();
        if let Some(node) = self.nodes.borrow().get(link.pid, token) {
            return Ok((page, LevelNode::Decoded(node)));
        }
        if !self.seen_before(link.pid, token) {
            // First visit on this version: stay on the borrowed image.
            let image = page.snapshot().await;
            self.validate_page(&image, link.pid)?;
            return Ok((page, LevelNode::View(image)));
        }
        // Repeat visit on the same version: decode once and cache. The image
        // and its version token are read under the same read guard, so the
        // cached pair is always consistent (a publish needs the write guard
        // and cannot intervene). The contended path keeps the latch-wait
        // accounting.
        let (image, token) = {
            let guard = match page.try_read() {
                Some(guard) => guard,
                None => self.contended_read(&page).await,
            };
            let image = guard.image().clone();
            (image, page.cache_token())
        };
        let decoded = self.decode_image(&image, link.pid).await?;
        let Page::Node(node) = decoded else {
            return Err(invalid("expected an index node"));
        };
        let node = Rc::new(CachedNode::from_node(&node));
        self.nodes
            .borrow_mut()
            .insert(link.pid, token, node.clone());
        Ok((page, LevelNode::Decoded(node)))
    }

    /// Lossy repeat-visit check for one exact page version (page id + cache
    /// token). The hint cell packs a 48-bit tag with a 16-bit visit counter:
    /// a tag match counts one more visit, and the third consecutive visit of
    /// one version makes it decode-worthy. Two cell ops per visit, no
    /// allocation; collisions only lose or occasionally waste a decode.
    fn seen_before(&self, pid: u64, token: super::node_cache::CacheToken) -> bool {
        if self.seen_hint.is_empty() {
            return false;
        }
        let cell = &self.seen_hint[pid as usize % self.seen_hint.len()];
        let tag = super::node_cache::hint_tag(token) & 0xffff_ffff_ffff;
        let packed = cell.get();
        if packed >> 16 == tag {
            let visits = (packed & 0xffff) + 1;
            cell.set((tag << 16) | visits);
            return visits >= 3;
        }
        cell.set((tag << 16) | 1);
        false
    }

    /// Compares a cached stored key with the query key. Inline keys compare
    /// by borrowing their arena head (no materialization); overflow keys
    /// materialize their chain.
    pub(super) async fn compare_cached(
        &self,
        access: &BufferAccess<'_>,
        node: &CachedNode,
        stored: CachedKey,
        query: &[u8],
    ) -> RS<Ordering> {
        if stored.overflow == 0 {
            let head = node.head(&stored);
            if self.lexicographic {
                return Ok(head.cmp(query));
            }
            return self.compare(head, query);
        }
        let key = Key {
            head: node.head(&stored).to_vec(),
            overflow: stored.overflow,
            len: stored.len,
        };
        let bytes = self.bytes(access, &key).await?;
        if self.lexicographic {
            return Ok(bytes.as_slice().cmp(query));
        }
        self.compare(&bytes, query)
    }

    /// `compare_cached` for callers that cannot suspend: `None` asks for the
    /// async path (only overflow keys need it).
    pub(super) fn compare_cached_inline(
        &self,
        node: &CachedNode,
        stored: CachedKey,
        query: &[u8],
    ) -> RS<Option<Ordering>> {
        if stored.overflow != 0 {
            return Ok(None);
        }
        let head = node.head(&stored);
        if self.lexicographic {
            return Ok(Some(head.cmp(query)));
        }
        Ok(Some(self.compare(head, query)?))
    }

    /// Binary search over a cached node's entries.
    pub(super) async fn position_cached(
        &self,
        access: &BufferAccess<'_>,
        node: &CachedNode,
        key: &[u8],
    ) -> RS<(usize, bool)> {
        let (mut lo, mut hi) = (0, node.entries.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            match self
                .compare_cached(access, node, node.entries[mid].key, key)
                .await?
            {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok((mid, true)),
                Ordering::Greater => hi = mid,
            }
        }
        Ok((lo, false))
    }

    /// `position_cached` for callers that cannot suspend; `None` means an
    /// overflow key needed the async path.
    pub(super) fn position_cached_sync(
        &self,
        node: &CachedNode,
        key: &[u8],
    ) -> RS<Option<(usize, bool)>> {
        let (mut lo, mut hi) = (0, node.entries.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let Some(order) = self.compare_cached_inline(node, node.entries[mid].key, key)? else {
                return Ok(None);
            };
            match order {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok(Some((mid, true))),
                Ordering::Greater => hi = mid,
            }
        }
        Ok(Some((lo, false)))
    }

    /// Applies a just-published leaf edit to the cached node, re-keyed to
    /// the post-publication token, so a hot leaf is never re-decoded after a
    /// write. Pages not currently cached stay uncached.
    pub(super) fn cache_apply_edit(
        &self,
        page: &PagePin<'_>,
        before: super::node_cache::CacheToken,
        edit: super::node_cache::CachedEdit,
    ) {
        let after = page.cache_token();
        self.nodes
            .borrow_mut()
            .apply_edit(page.id(), before, after, edit);
    }

    /// Replaces the cached node of a page republished wholesale (splits and
    /// merges publish nodes built off the cache).
    pub(super) fn cache_replace_node(&self, page: &PagePin<'_>, node: Rc<CachedNode>) {
        let token = page.cache_token();
        self.nodes.borrow_mut().insert(page.id(), token, node);
    }

    pub(super) async fn snapshot<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        link: &Link,
    ) -> RS<(PagePin<'a>, Node)> {
        let page = self.pin(access, link).await?;
        let image = page.snapshot().await;
        let node = self.decode_image(&image, link.pid).await?.node()?.clone();
        Ok((page, node))
    }

    #[cfg(test)]
    pub(super) async fn path(
        &self,
        access: &BufferAccess<'_>,
        key: &[u8],
    ) -> RS<Vec<(Link, Node)>> {
        let mut link = self.root_link();
        let mut path = Vec::new();
        loop {
            let (page, node) = self.snapshot(access, &link).await?;
            drop(page);
            if let Some(high) = &node.high {
                if self.compare(key, &self.bytes(access, high).await?)? != Ordering::Less {
                    if node.right.pid == 0 {
                        return Err(invalid("missing B-link sibling"));
                    }
                    link = node.right.clone();
                    continue;
                }
            }
            let next = if node.leaf() {
                None
            } else {
                let (at, equal) = self.position(access, &node, key).await?;
                Some(if equal {
                    node.entries[at].target.clone()
                } else if at == 0 {
                    node.left.clone()
                } else {
                    node.entries[at - 1].target.clone()
                })
            };
            path.push((link, node));
            match next {
                Some(child) => link = child,
                None => return Ok(path),
            }
        }
    }

    pub(super) fn shard(&self, id: u64) -> &AMutex<HashMap<u64, V>> {
        &self.values[id as usize % self.values.len()]
    }
    pub(super) fn value_id(&self) -> RS<u64> {
        let id = self.next_value.get();
        self.next_value.set(
            id.checked_add(1)
                .ok_or_else(|| invalid("index value IDs exhausted"))?,
        );
        Ok(id)
    }
}

pub(super) async fn cooperate() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            std::task::Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            std::task::Poll::Pending
        }
    })
    .await
}

/// What a descent level is visited through: a cached decoded node, or the
/// zero-allocation borrowed image on the first visit of a page version.
pub(super) enum LevelNode {
    Decoded(Rc<CachedNode>),
    View(PageImage),
}

/// Codec for values that fit entirely inside one leaf slot.
pub(super) struct InlineCodec<V> {
    pub encode: fn(&V) -> u128,
    pub decode: fn(u128) -> V,
}

impl PagedBTree<u128> {
    /// Creates a tree whose complete values live in leaf pages, without a
    /// per-row in-memory value directory. Suitable for durable row identifiers.
    pub fn with_row_ids(
        context: CompareContext,
        options: BufferOptions,
        store: Arc<dyn PageStore>,
    ) -> RS<Self> {
        let mut tree = Self::with_store(context, options, store)?;
        tree.inline = Some(InlineCodec {
            encode: |id| *id,
            decode: |id| id,
        });
        Ok(tree)
    }
}
