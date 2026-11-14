//! Ordinary leaf edits use byte patches. Structural splits keep the existing
//! prepare-all/publish-without-await protocol and decoded ancestor path.
use super::{
    codec, images, invalid,
    node::{Entry, Key, Link},
    patch,
    tree::cooperate,
    view::NodeView,
    PagedBTree, RS,
};
use crate::index_key::key_tuple::KeyTuple;
use page_buffer::{
    BufferAccess, PageEdit, PageImage, PagePin, PageWriteGuard, PreparedEdit, PreparedWrite,
};
use std::rc::Rc;
use std::sync::Arc;

// Small edits retain no snapshot, allowing unique resident images to be modified
// in place. Structural/recompression changes still publish complete images.
// The full-image variant also carries the decoded node so the write-through
// node cache can be updated at publication without re-decoding.
pub(super) enum LeafImage {
    Edit(PageEdit),
    Image {
        image: PageImage,
        node: Rc<super::node::Node>,
    },
}
// Keep the common edit program inline: boxing it would allocate on every update.
#[allow(clippy::large_enum_variant)]
enum LeafWrite<'a> {
    Edit(PreparedEdit<'a>),
    Image(PreparedWrite<'a>),
}
impl LeafImage {
    fn prepare<'a>(
        self,
        guard: PageWriteGuard<'a>,
        policy: &'a Arc<dyn page_buffer::PageWriteback>,
    ) -> RS<LeafWrite<'a>> {
        match self {
            Self::Edit(edit) => guard
                .prepare_edit_with_shared_writeback(edit, Some(&images::VALIDATOR), policy)
                .map(LeafWrite::Edit),
            Self::Image { image, .. } => guard.prepare(image).map(LeafWrite::Image),
        }
    }
}
impl LeafWrite<'_> {
    fn publish(&mut self) {
        match self {
            Self::Edit(write) => write.publish(),
            Self::Image(write) => write.publish(),
        }
    }
}
struct LeafChange {
    revision: u64,
    target: u64,
    old: Option<u128>,
    replacement: LeafImage,
    cache_edit: Option<super::node_cache::CachedEdit>,
}

/// The logical cache edit mirroring the planned leaf mutation.
fn cache_edit_for(
    equal: bool,
    at: usize,
    stored: Option<&Key>,
    encoded: u128,
) -> Option<super::node_cache::CachedEdit> {
    if equal {
        Some(super::node_cache::CachedEdit::Replace {
            at,
            target: encoded as u64,
            upper: (encoded >> 64) as u64,
        })
    } else {
        stored.map(|key| super::node_cache::CachedEdit::Insert {
            at,
            key_head: key.head.clone(),
            overflow: key.overflow,
            len: key.len,
            target: encoded as u64,
            upper: (encoded >> 64) as u64,
        })
    }
}

impl<V: Send> PagedBTree<V> {
    /// Inserts or replaces a value. Comparator errors leave the mapping unchanged.
    pub async fn insert(&self, key: KeyTuple, value: V) -> RS<Option<V>> {
        let access = &self.access().await;
        self.ensure_open()?;
        self.insert_inner(access, key, value).await
    }

    /// Insert body for callers already holding a traversal lease (batches).
    pub(super) async fn insert_inner(
        &self,
        access: &BufferAccess<'_>,
        key: KeyTuple,
        value: V,
    ) -> RS<Option<V>> {
        if let Some(codec) = &self.inline {
            let replacement = Some((codec.encode)(&value));
            let attempt = self.try_change_resident(access, key.as_slice(), replacement)?;
            let result = match attempt {
                super::resident_write::ChangeAttempt::Done(result) => result,
                waiting => {
                    self.finish_change_resident(access, key.as_slice(), replacement, waiting)
                        .await?
                }
            };
            if let Some(old) = result {
                return Ok(old.map(codec.decode));
            }
        }
        self.insert_slow(access, key, value).await
    }

    /// Materialization and structural edits stay outside the ordinary resident path.
    async fn insert_slow(
        &self,
        access: &BufferAccess<'_>,
        key: KeyTuple,
        value: V,
    ) -> RS<Option<V>> {
        let (mut stored, mut reserved) = (None, None);
        let mut value = Some(value);
        loop {
            let (page, image, at, equal) = self.view_leaf(access, key.as_slice()).await?;
            let node = NodeView::new(&image)?;
            if equal && self.inline.is_none() {
                if let Some(old) = self.replace_generic(&page, &node, at, &mut value).await? {
                    return Ok(Some(old));
                }
                cooperate().await;
                continue;
            }
            // Value-directory IDs are only needed for new generic mappings.
            let target = if equal {
                node.value(at)? as u64
            } else if self.inline.is_some() {
                0
            } else if let Some(id) = reserved {
                id
            } else {
                let id = self.value_id()?;
                reserved = Some(id);
                id
            };
            let encoded = match &self.inline {
                Some(codec) => (codec.encode)(
                    value
                        .as_ref()
                        .ok_or_else(|| invalid("missing insert value"))?,
                ),
                None => u128::from(target),
            };
            let cache_edit = cache_edit_for(equal, at, stored.as_ref(), encoded);
            let replacement = if equal {
                Some(LeafImage::Edit(patch::replace(&node, at, encoded)?))
            } else {
                if stored.is_none() {
                    if key.as_slice().len() <= self.inline_key_len(key.as_slice().len()) {
                        stored = Some(Key::inline(key.as_slice()));
                    } else {
                        // Overflow initialization may evict other frames; release the leaf.
                        drop(page);
                        stored = Some(self.key(access, key.as_slice()).await?);
                        continue;
                    }
                }
                self.insert_image(
                    &node,
                    at,
                    stored
                        .as_ref()
                        .ok_or_else(|| invalid("missing insert key"))?,
                    encoded,
                )?
            };
            let Some(replacement) = replacement else {
                drop(page);
                if self
                    .split_insert(
                        access,
                        &key,
                        stored
                            .as_ref()
                            .ok_or_else(|| invalid("missing split key"))?,
                        target,
                        &mut value,
                    )
                    .await?
                {
                    return Ok(None);
                }
                continue;
            };
            let change = LeafChange {
                revision: node.revision(),
                target,
                old: if equal { Some(node.value(at)?) } else { None },
                replacement,
                cache_edit,
            };
            // Drop our own snapshot before COW preparation. Concurrent readers
            // and checkpoint snapshots still force a private byte allocation.
            drop(image);
            if let Some(old) = self.publish_leaf(&page, change, &mut value).await? {
                return Ok(old);
            }
            cooperate().await;
        }
    }

    /// Generic replacement changes only the external value directory, not any
    /// page bytes. Keep the directory lock and a shared page latch until the swap
    /// completes; revision validation excludes a removed/reinserted mapping.
    async fn replace_generic(
        &self,
        page: &PagePin<'_>,
        node: &NodeView<'_>,
        at: usize,
        value: &mut Option<V>,
    ) -> RS<Option<V>> {
        let id = node.value(at)? as u64;
        let mut values = self.shard(id).lock().await;
        let guard = page.read().await;
        if codec::node_revision(&guard)? != node.revision() {
            self.retry();
            return Ok(None);
        }
        let old = values
            .get_mut(&id)
            .ok_or_else(|| invalid("missing index value"))?;
        let inserted = value
            .take()
            .ok_or_else(|| invalid("missing insert value"))?;
        Ok(Some(std::mem::replace(old, inserted)))
    }

    /// Recompression is a fallback for prefix changes or a full packed page.
    /// A full decode is never required for fixed-width replacement. The
    /// full-image variant carries its decoded node for the node cache.
    fn insert_image(
        &self,
        node: &NodeView<'_>,
        at: usize,
        key: &Key,
        value: u128,
    ) -> RS<Option<LeafImage>> {
        if let Some(image) = patch::insert(node, at, key, value)? {
            return Ok(Some(LeafImage::Edit(image)));
        }
        let mut updated = codec::decode_validated(node.bytes)?.node()?.clone();
        Arc::make_mut(&mut updated.entries).insert(
            at,
            Entry {
                key: key.clone(),
                target: Link::new(value as u64),
                upper: (value >> 64) as u64,
            },
        );
        if !codec::fits(&updated, self.buffer.page_size()) {
            return Ok(None);
        }
        updated.revision = updated
            .revision
            .checked_add(1)
            .ok_or_else(|| invalid("node revision exhausted"))?;
        let pid = codec::u64_at(node.bytes, 48);
        let node = Rc::new(updated);
        self.image(pid, (*node).clone()).map(|image| {
            Some(LeafImage::Image {
                image,
                node: node.clone(),
            })
        })
    }

    /// None means a revision conflict; Some(None) means a successful new mapping.
    /// Generic value locks precede page latches. Inline values never take those locks.
    async fn publish_leaf(
        &self,
        page: &PagePin<'_>,
        change: LeafChange,
        value: &mut Option<V>,
    ) -> RS<Option<Option<V>>> {
        let LeafChange {
            revision,
            target,
            old: old_inline,
            replacement,
            cache_edit,
        } = change;
        // Full-image replacements carry their decoded node; extract it
        // before preparation consumes the image.
        let cache_node = match &replacement {
            LeafImage::Image { node, .. } => Some(node.clone()),
            _ => None,
        };
        let mut values = if self.inline.is_none() {
            Some(self.shard(target).lock().await)
        } else {
            None
        };
        let guard = page.write().await;
        if codec::node_revision(&guard)? != revision {
            self.retry();
            return Ok(None);
        }
        let before = page.cache_token();
        let mut write = replacement.prepare(guard, &self.writeback_policy)?;
        let inserted = value
            .take()
            .ok_or_else(|| invalid("missing insert value"))?;
        let old = if let Some(codec) = &self.inline {
            old_inline.map(codec.decode)
        } else {
            values
                .as_mut()
                .ok_or_else(|| invalid("missing value guard"))?
                .insert(target, inserted)
        };
        // No failure or suspension after the value directory has been updated.
        if let LeafWrite::Edit(edit) = &write {
            self.record_edit(edit);
        }
        write.publish();
        // The published image is current now; bring the cached decoded node
        // along so later readers never re-decode the page.
        if let Some(node) = cache_node {
            self.cache_replace_node(
                page,
                Rc::new(super::node_cache::CachedNode::from_node(&node)),
            );
        } else if let Some(edit) = cache_edit {
            self.cache_apply_edit(page, before, edit);
        }
        if old_inline.is_none() {
            self.count.set(self.count.get() + 1);
        }
        Ok(Some(old))
    }

    /// Removes a key, returning its previous value.
    pub async fn remove(&self, key: &KeyTuple) -> RS<Option<V>> {
        let access = &self.access().await;
        self.ensure_open()?;
        self.remove_inner(access, key).await
    }
    pub(super) async fn remove_inner(
        &self,
        access: &BufferAccess<'_>,
        key: &KeyTuple,
    ) -> RS<Option<V>> {
        if let Some(codec) = &self.inline {
            let attempt = self.try_change_resident(access, key.as_slice(), None)?;
            let result = match attempt {
                super::resident_write::ChangeAttempt::Done(result) => result,
                waiting => {
                    self.finish_change_resident(access, key.as_slice(), None, waiting)
                        .await?
                }
            };
            if let Some(old) = result {
                return Ok(old.map(codec.decode));
            }
        }
        loop {
            let (page, image, at, equal) = self.view_leaf(access, key.as_slice()).await?;
            if !equal {
                return Ok(None);
            }
            let node = NodeView::new(&image)?;
            let id = node.value(at)?;
            let replacement = patch::remove(&node, at)?;
            let revision = node.revision();
            drop(image);
            let mut values = if self.inline.is_none() {
                Some(self.shard(id as u64).lock().await)
            } else {
                None
            };
            let guard = page.write().await;
            if codec::node_revision(&guard)? != revision {
                self.retry();
                continue;
            }
            let before = page.cache_token();
            let mut write = guard.prepare_edit_with_shared_writeback(
                replacement,
                Some(&images::VALIDATOR),
                &self.writeback_policy,
            )?;
            let old = if let Some(codec) = &self.inline {
                (codec.decode)(id)
            } else {
                values
                    .as_mut()
                    .ok_or_else(|| invalid("missing value guard"))?
                    .remove(&(id as u64))
                    .ok_or_else(|| invalid("missing index value"))?
            };
            self.record_edit(&write);
            write.publish();
            self.cache_apply_edit(&page, before, super::node_cache::CachedEdit::Remove { at });
            self.count.set(self.count.get() - 1);
            return Ok(Some(old));
        }
    }
}

impl PagedBTree<u128> {
    /// Batch point lookups under one traversal lease, in input order.
    /// Clustered key streams share the decoded-node cache across rows.
    pub async fn get_batch(&self, keys: &[KeyTuple]) -> RS<Vec<Option<u128>>> {
        let access = &self.access().await;
        self.ensure_open()?;
        let mut found = Vec::with_capacity(keys.len());
        for key in keys {
            found.push(
                self.lookup_inner(access, key, false)
                    .await?
                    .map(|(_, id)| id),
            );
        }
        Ok(found)
    }

    /// Batch inserts under one traversal lease; old values in input order.
    /// Ordered runs ride the write memo, so a clustered batch descends once
    /// per leaf instead of once per row.
    pub async fn insert_batch(&self, pairs: &[(KeyTuple, u128)]) -> RS<Vec<Option<u128>>> {
        let access = &self.access().await;
        self.ensure_open()?;
        let mut old = Vec::with_capacity(pairs.len());
        for (key, value) in pairs {
            old.push(self.insert_inner(access, key.clone(), *value).await?);
        }
        Ok(old)
    }
}
