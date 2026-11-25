use super::{
    codec, invalid,
    node::{Link, Node, Page},
    tree::cooperate,
    view::NodeView,
    PagedBTree, RS,
};
use crate::index_key::key_tuple::KeyTuple;
use page_buffer::{BufferAccess, PagePin};
use std::{cmp::Ordering, ops::Bound};

impl<V: Send> PagedBTree<V> {
    /// Whether a key is present.
    pub async fn contains_key(&self, key: &KeyTuple) -> RS<bool> {
        let access = &self.access().await;
        self.ensure_open()?;
        let found = self.view_leaf(access, key.as_slice()).await?.3;
        Ok(found)
    }

    /// Atomically removes all mappings; cancellation before publication is harmless.
    pub async fn clear(&self) -> RS<()> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        let root = self.allocate(access, Page::Node(Node::empty(0))).await?;
        let mut guards = Vec::new();
        if self.inline.is_none() {
            for shard in &self.values {
                guards.push(shard.lock().await);
            }
        }
        // All fallible/suspending work precedes the atomic mapping change.
        for values in &mut guards {
            values.clear();
        }
        self.root.set(root.pid);
        // The previous leaf pages stay allocated-but-unreachable with intact
        // bytes until reclamation; the write memo must not validate against
        // an orphaned leaf.
        self.memo.set(0);
        self.count.set(0);
        Ok(())
    }

    /// Removes the smallest mapping atomically.
    pub async fn pop_first(&self) -> RS<Option<(KeyTuple, V)>> {
        self.pop(false).await
    }
    /// Removes the largest mapping atomically.
    pub async fn pop_last(&self) -> RS<Option<(KeyTuple, V)>> {
        self.pop(true).await
    }
    async fn pop(&self, last: bool) -> RS<Option<(KeyTuple, V)>> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        let Some((key, _)) = self.extreme(access, last).await? else {
            return Ok(None);
        };
        Ok(self
            .remove_inner(access, &key)
            .await?
            .map(|value| (key, value)))
    }

    pub(super) async fn first_leaf(&self, access: &BufferAccess<'_>) -> RS<Link> {
        let mut link = self.root_link();
        loop {
            let (_, image) = self.page_image(access, &link).await?;
            let node = NodeView::new(&image)?;
            if node.leaf() {
                return Ok(link);
            }
            link = Link::new(node.left());
        }
    }

    async fn extreme(&self, access: &BufferAccess<'_>, last: bool) -> RS<Option<(KeyTuple, u128)>> {
        let mut link = self.first_leaf(access).await?;
        let mut result = None;
        loop {
            let (_, image) = self.page_image(access, &link).await?;
            let node = NodeView::new(&image)?;
            if node.len() != 0 {
                let (key, value) = node.entry(if last { node.len() - 1 } else { 0 })?;
                result = Some((KeyTuple::from(self.view_bytes(access, key).await?), value));
                if !last {
                    return Ok(result);
                }
            }
            if node.right() == 0 {
                return Ok(result);
            }
            link = Link::new(node.right());
            cooperate().await;
        }
    }
}

impl<V: Clone + Send> PagedBTree<V> {
    /// Returns a cloned value without reconstructing the stored key.
    pub async fn get(&self, key: &KeyTuple) -> RS<Option<V>> {
        Ok(self.lookup(key, false).await?.map(|(_, value)| value))
    }
    /// Returns the stored key and a cloned value, including when the comparator
    /// considers a differently encoded query key equal to the stored key.
    pub async fn get_key_value(&self, key: &KeyTuple) -> RS<Option<(KeyTuple, V)>> {
        Ok(self
            .lookup(key, true)
            .await?
            .map(|(key, value)| (KeyTuple::from(key), value)))
    }
    async fn lookup(&self, key: &KeyTuple, return_key: bool) -> RS<Option<(Vec<u8>, V)>> {
        let access = &self.access().await;
        self.ensure_open()?;
        self.lookup_inner(access, key, return_key).await
    }

    /// Lookup body for callers already holding a traversal lease (batches).
    pub(super) async fn lookup_inner(
        &self,
        access: &BufferAccess<'_>,
        key: &KeyTuple,
        return_key: bool,
    ) -> RS<Option<(Vec<u8>, V)>> {
        if !return_key {
            if let Some(codec) = &self.inline {
                if let Some(found) = self.lookup_resident(access, key.as_slice()).await? {
                    return Ok(found.map(|id| (Vec::new(), (codec.decode)(id))));
                }
            }
        }
        loop {
            let (page, image, at, equal) = self.view_leaf(access, key.as_slice()).await?;
            if !equal {
                return Ok(None);
            }
            let node = NodeView::new(&image)?;
            let (stored, id) = node.entry(at)?;
            let bytes = if return_key {
                self.view_bytes(access, stored).await?
            } else {
                Vec::new()
            };
            // Only generic values need the external directory lock, always before
            // the page latch so the mapping and value form one consistent read.
            let values = if self.inline.is_none() {
                Some(self.shard(id as u64).lock().await)
            } else {
                None
            };
            let guard = page.read().await;
            if codec::node_revision(&guard)? != node.revision() {
                self.retry();
                continue;
            }
            let value = if let Some(codec) = &self.inline {
                (codec.decode)(id)
            } else {
                values
                    .as_ref()
                    .ok_or_else(|| invalid("missing value guard"))?
                    .get(&(id as u64))
                    .cloned()
                    .ok_or_else(|| invalid("missing index value"))?
            };
            return Ok(Some((bytes, value)));
        }
    }
    /// Returns the smallest mapping.
    pub async fn first_key_value(&self) -> RS<Option<(KeyTuple, V)>> {
        self.extreme_value(false).await
    }
    /// Returns the largest mapping.
    pub async fn last_key_value(&self) -> RS<Option<(KeyTuple, V)>> {
        self.extreme_value(true).await
    }
    async fn extreme_value(&self, last: bool) -> RS<Option<(KeyTuple, V)>> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        let Some((key, id)) = self.extreme(access, last).await? else {
            return Ok(None);
        };
        let value = if let Some(codec) = &self.inline {
            (codec.decode)(id)
        } else {
            self.shard(id as u64)
                .lock()
                .await
                .get(&(id as u64))
                .cloned()
                .ok_or_else(|| invalid("missing index value"))?
        };
        Ok(Some((key, value)))
    }
    /// Returns a consistent, sorted mapping collection; mapping writers wait
    /// asynchronously for this scan. Interior mutations of V are independent.
    pub async fn range(
        &self,
        bounds: (Bound<&KeyTuple>, Bound<&KeyTuple>),
    ) -> RS<Vec<(KeyTuple, V)>> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        self.validate_bounds(&bounds)?;
        // Keep the descent's pin, image and exact lower slot. The exclusive scan
        // lease keeps this mapping stable while each immutable image is consumed.
        let (mut page, mut image, mut start) = match bounds.0 {
            Bound::Included(key) | Bound::Excluded(key) => {
                let (page, image, at, equal) = self.view_leaf(access, key.as_slice()).await?;
                (
                    page,
                    image,
                    at + usize::from(equal && matches!(bounds.0, Bound::Excluded(_))),
                )
            }
            Bound::Unbounded => {
                let link = self.first_leaf(access).await?;
                let (page, image) = self.page_image(access, &link).await?;
                (page, image, 0)
            }
        };
        let mut result = Vec::new();
        // Cooperate every few pages instead of at every page transition: a
        // page of scan work is a bounded, small CPU slice, and a per-page
        // reschedule prices intervening tasks into the scan's latency.
        let mut since_cooperate = 0usize;
        loop {
            let node = NodeView::new(&image)?;
            let (end, finished) = match bounds.1 {
                Bound::Unbounded => (node.len(), false),
                Bound::Included(key) | Bound::Excluded(key) => {
                    let (at, equal) = self.position_view(access, &node, key.as_slice()).await?;
                    (
                        at + usize::from(equal && matches!(bounds.1, Bound::Included(_))),
                        at < node.len(),
                    )
                }
            };
            self.scan_node(access, &page, &node, start..end, &mut result)
                .await?;
            if finished || node.right() == 0 {
                return Ok(result);
            }
            // Prefetch only after proving that the scan needs the next page.
            let sibling = self.buffer.page_ref(node.right());
            self.buffer.prefetch(access, &sibling).await;
            let (next_page, next_image) = self.page_image(access, &Link::new(sibling.id())).await?;
            page = next_page;
            image = next_image;
            start = 0;
            since_cooperate += 1;
            if since_cooperate >= 8 {
                since_cooperate = 0;
                cooperate().await;
            }
        }
    }
    async fn scan_node(
        &self,
        access: &BufferAccess<'_>,
        page: &PagePin<'_>,
        node: &NodeView<'_>,
        slots: std::ops::Range<usize>,
        result: &mut Vec<(KeyTuple, V)>,
    ) -> RS<()> {
        result.reserve(slots.end.saturating_sub(slots.start));
        // A token-valid cached leaf materializes rows straight from the arena:
        // one allocation per key, no slot-directory machinery. The exclusive
        // scan lease keeps the pinned image and the cached node on the same
        // page version for the whole scan. The Rc is bound before any await so
        // no RefCell borrow crosses a suspension.
        let cached = self.nodes.borrow().get(page.id(), page.cache_token());
        if let Some(cached) = cached {
            return self.scan_cached(access, &cached, node, slots, result).await;
        }
        if let Some(codec) = &self.inline {
            // Consume each resident run in one synchronous loop. Only overflow
            // keys suspend; result ownership and the exclusive scan lease are unchanged.
            let mut at = slots.start;
            while at < slots.end {
                at = Self::scan_inline(node, at..slots.end, codec, result)?;
                if at < slots.end {
                    let (stored, id) = node.entry(at)?;
                    let key = KeyTuple::from(self.view_bytes(access, stored).await?);
                    result.push((key, (codec.decode)(id)));
                    at += 1;
                }
            }
            return Ok(());
        }
        for at in slots {
            let (stored, id) = node.entry(at)?;
            let key = KeyTuple::from(self.view_bytes(access, stored).await?);
            // The inline-value branch already returned above.
            let value = self
                .shard(id as u64)
                .lock()
                .await
                .get(&(id as u64))
                .cloned()
                .ok_or_else(|| invalid("missing index value"))?;
            result.push((key, value));
        }
        Ok(())
    }
    /// Cached-leaf scan: rows come straight from the arena. Only overflow keys
    /// take the byte-view row so their chain materialization stays shared.
    async fn scan_cached(
        &self,
        access: &BufferAccess<'_>,
        cached: &super::node_cache::CachedNode,
        node: &NodeView<'_>,
        slots: std::ops::Range<usize>,
        result: &mut Vec<(KeyTuple, V)>,
    ) -> RS<()> {
        for at in slots {
            let entry = &cached.entries[at];
            let key = if entry.key.overflow == 0 {
                KeyTuple::from(cached.head(&entry.key).to_vec())
            } else {
                let (stored, _) = node.entry(at)?;
                KeyTuple::from(self.view_bytes(access, stored).await?)
            };
            let id = (entry.target as u128) | ((entry.upper as u128) << 64);
            let value = if let Some(codec) = &self.inline {
                (codec.decode)(id)
            } else {
                self.shard(entry.target)
                    .lock()
                    .await
                    .get(&entry.target)
                    .cloned()
                    .ok_or_else(|| invalid("missing index value"))?
            };
            result.push((key, value));
        }
        Ok(())
    }
    /// Appends inline keys until the first overflow descriptor, preserving order.
    fn scan_inline(
        node: &NodeView<'_>,
        slots: std::ops::Range<usize>,
        codec: &super::tree::InlineCodec<V>,
        result: &mut Vec<(KeyTuple, V)>,
    ) -> RS<usize> {
        let end = slots.end;
        for at in slots {
            let (stored, id) = node.entry(at)?;
            if stored.overflow != 0 {
                return Ok(at);
            }
            result.push((KeyTuple::from(stored.owned().head), (codec.decode)(id)));
        }
        Ok(end)
    }
    fn validate_bounds(&self, bounds: &(Bound<&KeyTuple>, Bound<&KeyTuple>)) -> RS<()> {
        if let (Bound::Included(a) | Bound::Excluded(a), Bound::Included(b) | Bound::Excluded(b)) =
            bounds
        {
            let order = self.compare(a.as_slice(), b.as_slice())?;
            if order == Ordering::Greater
                || (order == Ordering::Equal
                    && matches!(bounds, (Bound::Excluded(_), Bound::Excluded(_))))
            {
                return Err(invalid("invalid index range bounds"));
            }
        }
        Ok(())
    }
}
