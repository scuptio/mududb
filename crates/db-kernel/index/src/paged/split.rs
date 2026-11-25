//! Split preparation may allocate pages and fail before any reachable node changes.
//! Replacements are revision-checked and prepared under page latches, then published
//! without awaiting; the latches remain held through root and count updates.
//! Descent, node division, and page allocation run outside the structure lock;
//! only the atomic revision-checked publication serializes with other structural
//! changes, so concurrent splits never queue behind another split's I/O.
//!
//! Interior ancestors that can absorb the separator are patched, not decoded:
//! the descent records only page links, and a parent with room gets a prebuilt
//! byte edit instead of a decode/divide/re-encode round. Only a packed parent
//! falls back to full-node division.
use super::{
    codec, invalid,
    node::{Entry, Key, Link, Node, Page},
    node_cache::{CacheToken, CachedEdit},
    patch,
    split_point::divide,
    tree::cooperate,
    view::NodeView,
    PagedBTree, RS,
};
use crate::index_key::key_tuple::KeyTuple;
use page_buffer::{BufferAccess, PageEdit, PreparedEdit, PreparedWrite};
use std::rc::Rc;
use std::sync::Arc;

/// Right-edge append: when the just-inserted entry extends the rightmost
/// leaf, move only it to the fresh sibling instead of dividing the full
/// leaf. Ascending inserts then fill pages completely and never re-serialize
/// the appended leaf. Mirrors `divide`'s fence/link bookkeeping exactly.
/// Returns `None` when the left page would not fit with the fence key added
/// (the fence costs 16+head bytes on a previously fence-free rightmost page).
fn right_edge_divide(node: &Node, size: usize) -> RS<Option<(Node, Key, Node)>> {
    let mut left = node.clone();
    let entry = Arc::make_mut(&mut left.entries)
        .pop()
        .ok_or_else(|| invalid("empty right-edge leaf"))?;
    let separator = entry.key.clone();
    left.high = Some(separator.clone());
    if !codec::fits(&left, size) {
        return Ok(None);
    }
    let mut right = Node::empty(node.level);
    right.high = node.high.clone();
    right.right = node.right.clone();
    right.entries = Arc::new(vec![entry]);
    Ok(Some((left, separator, right)))
}

/// Divides a full node, or appends a fresh right sibling holding only the
/// just-inserted entry when it extends the rightmost leaf (ascending
/// inserts then fill pages completely and never re-serialize the appended
/// leaf). Parent splits always take the divide path.
fn divide_or_append(node: &Node, at: usize, size: usize) -> RS<(Node, Key, Node)> {
    if node.leaf() && node.high.is_none() && at == node.entries.len() - 1 {
        if let Some(parts) = right_edge_divide(node, size)? {
            return Ok(parts);
        }
    }
    divide(node, size)
}

/// One page to replace at publication: a full re-encoded image for divided
/// nodes, or a prebuilt byte edit for a parent that absorbs the separator.
enum Replacement {
    Image {
        link: Link,
        revision: u64,
        node: Node,
    },
    /// The edit program is deterministic in the page bytes, so the revision
    /// check at publication proves it still applies to unchanged content.
    Patch {
        link: Link,
        revision: u64,
        edit: PageEdit,
        cache_edit: CachedEdit,
    },
}
impl Replacement {
    fn link(&self) -> &Link {
        match self {
            Self::Image { link, .. } | Self::Patch { link, .. } => link,
        }
    }
    fn revision(&self) -> u64 {
        match self {
            Self::Image { revision, .. } | Self::Patch { revision, .. } => *revision,
        }
    }
}

/// A prepared publication over mixed replacement kinds. Splits are rare
/// enough that boxing the larger edit variant beats a 264-byte enum.
enum PreparedReplacement<'a> {
    Edit(Box<PreparedEdit<'a>>),
    Image(PreparedWrite<'a>),
}
impl PreparedReplacement<'_> {
    fn publish(&mut self) {
        match self {
            Self::Edit(write) => write.publish(),
            Self::Image(write) => write.publish(),
        }
    }
}

/// Post-publication cache action for one replaced page.
enum After {
    Image(Node),
    Patch(CacheToken, CachedEdit),
}

impl<V: Send> PagedBTree<V> {
    /// Mirrors every published replacement into the decoded-node cache so
    /// readers never re-decode after a split: full images replace the cached
    /// node, patches apply their logical edit and re-key it to the
    /// post-publication token.
    fn cache_after_publish(&self, pins: &[page_buffer::PagePin<'_>], afters: Vec<After>) {
        for (pin, after) in pins.iter().zip(afters) {
            match after {
                After::Image(node) => self.cache_replace_node(
                    pin,
                    Rc::new(super::node_cache::CachedNode::from_node(&node)),
                ),
                After::Patch(before, cache_edit) => self.cache_apply_edit(pin, before, cache_edit),
            }
        }
    }
    pub(super) async fn split_insert(
        &self,
        access: &BufferAccess<'_>,
        key: &KeyTuple,
        stored: &Key,
        id: u64,
        value: &mut Option<V>,
    ) -> RS<bool> {
        // Descent, division and allocation run without the structure lock;
        // every page reached is revalidated by revision at publication, and a
        // root moved by a concurrent split or merge fails the root recheck.
        let root_at_build = self.root.get();
        let path = self.path_links(access, key.as_slice()).await?;
        let leaf = path.last().ok_or_else(|| invalid("empty split path"))?;
        let (_, mut node) = self.snapshot(access, leaf).await?;
        let (at, equal) = self.position(access, &node, key.as_slice()).await?;
        if equal {
            return Ok(false);
        }
        let encoded = match &self.inline {
            Some(codec) => (codec.encode)(
                value
                    .as_ref()
                    .ok_or_else(|| invalid("missing split value"))?,
            ),
            None => u128::from(id),
        };
        Arc::make_mut(&mut node.entries).insert(
            at,
            Entry {
                key: stored.clone(),
                target: Link::new(encoded as u64),
                upper: (encoded >> 64) as u64,
            },
        );
        let (replacements, root) = self.build_replacements(access, &path, node, at).await?;
        let _smo = self.structure.lock().await;
        self.publish(access, replacements, root, root_at_build, id, value)
            .await
    }

    /// Builds the replacement chain bottom-up: divides full nodes, allocates
    /// right siblings, and patches separators into parents that have room.
    /// Returns the replacements (leaf first) and the root to publish.
    async fn build_replacements(
        &self,
        access: &BufferAccess<'_>,
        path: &[Link],
        mut node: Node,
        at: usize,
    ) -> RS<(Vec<Replacement>, u64)> {
        let mut replacements = Vec::new();
        let mut current = path
            .last()
            .ok_or_else(|| invalid("empty split path"))?
            .clone();
        let mut ancestors = path[..path.len() - 1].iter().rev();
        let mut root = self.root.get();
        loop {
            let revision = node.revision;
            node.revision += 1;
            if codec::fits(&node, self.buffer.page_size()) {
                replacements.push(Replacement::Image {
                    link: current,
                    revision,
                    node,
                });
                break;
            }
            let (left, separator, right) = divide_or_append(&node, at, self.buffer.page_size())?;
            let right_link = self.allocate(access, Page::Node(right)).await?;
            let mut left = left;
            left.right = right_link.clone();
            let level = left.level;
            replacements.push(Replacement::Image {
                link: current.clone(),
                revision,
                node: left,
            });
            match ancestors.next() {
                Some(parent) => {
                    // A parent with room absorbs the separator as a byte patch;
                    // only a packed parent is decoded and divided further.
                    if let Some(patch) = self
                        .separator_patch(access, parent, &current, &separator, &right_link)
                        .await?
                    {
                        replacements.push(patch);
                        break;
                    }
                    let (_, mut parent_node) = self.snapshot(access, parent).await?;
                    let at = if parent_node.left.pid == current.pid {
                        0
                    } else {
                        parent_node
                            .entries
                            .iter()
                            .position(|e| e.target.pid == current.pid)
                            .ok_or_else(|| invalid("split child absent from parent"))?
                            + 1
                    };
                    Arc::make_mut(&mut parent_node.entries).insert(
                        at,
                        Entry {
                            key: separator,
                            target: right_link,
                            upper: 0,
                        },
                    );
                    current = parent.clone();
                    node = parent_node;
                }
                None => {
                    let mut new_root = Node::empty(
                        level
                            .checked_add(1)
                            .ok_or_else(|| invalid("tree height exhausted"))?,
                    );
                    new_root.left = current;
                    Arc::make_mut(&mut new_root.entries).push(Entry {
                        key: separator,
                        target: right_link,
                        upper: 0,
                    });
                    root = self.allocate(access, Page::Node(new_root)).await?.pid;
                    break;
                }
            }
        }
        Ok((replacements, root))
    }

    /// A prebuilt byte edit inserting `separator -> right` after `child` in
    /// the parent, or None when the parent needs recompression or division.
    async fn separator_patch(
        &self,
        access: &BufferAccess<'_>,
        link: &Link,
        child: &Link,
        separator: &Key,
        right: &Link,
    ) -> RS<Option<Replacement>> {
        let page = self.pin(access, link).await?;
        let guard = page.read().await;
        let node = NodeView::new(&guard)?;
        if node.leaf() {
            return Ok(None);
        }
        let at = if node.left() == child.pid {
            0
        } else {
            let mut at = None;
            for i in 0..node.len() {
                if node.value(i)? as u64 == child.pid {
                    at = Some(i + 1);
                    break;
                }
            }
            at.ok_or_else(|| invalid("split child absent from parent"))?
        };
        let Some(edit) = patch::insert(&node, at, separator, right.pid as u128)? else {
            return Ok(None);
        };
        Ok(Some(Replacement::Patch {
            link: link.clone(),
            revision: node.revision(),
            edit,
            cache_edit: CachedEdit::Insert {
                at,
                key_head: separator.head.clone(),
                overflow: separator.overflow,
                len: separator.len,
                target: right.pid,
                upper: 0,
            },
        }))
    }

    async fn publish(
        &self,
        access: &BufferAccess<'_>,
        mut replacements: Vec<Replacement>,
        root: u64,
        root_at_build: u64,
        id: u64,
        value: &mut Option<V>,
    ) -> RS<bool> {
        // A root moved by a concurrent split or merge invalidates the whole
        // descent, even when every touched page kept its revision.
        if self.root.get() != root_at_build {
            self.retry();
            return Ok(false);
        }
        replacements.sort_by_key(|r| r.link().pid);
        let mut pins = Vec::new();
        for replacement in &replacements {
            pins.push(self.pin(access, replacement.link()).await?);
        }
        let mut values = if self.inline.is_none() {
            Some(self.shard(id).lock().await)
        } else {
            None
        };
        let mut guards = Vec::new();
        for pin in &pins {
            let Some(guard) = pin.try_write() else {
                drop(guards);
                drop(values);
                drop(pin.write().await);
                self.retry();
                cooperate().await;
                return Ok(false);
            };
            guards.push(guard);
        }
        for (guard, replacement) in guards.iter().zip(&replacements) {
            if codec::node_revision(guard)? != replacement.revision() {
                self.retry();
                return Ok(false);
            }
        }
        // Prepare every replacement before any publication. Patch programs
        // are consumed into their prepared form; image nodes are kept for the
        // post-publication cache fill.
        let mut writes = Vec::new();
        let mut afters = Vec::new();
        for (guard, replacement) in guards.into_iter().zip(replacements) {
            match replacement {
                Replacement::Image { link, node, .. } => {
                    let image = self.image(link.pid, node.clone())?;
                    writes.push(PreparedReplacement::Image(guard.prepare(image)?));
                    afters.push(After::Image(node));
                }
                Replacement::Patch {
                    edit, cache_edit, ..
                } => {
                    writes.push(PreparedReplacement::Edit(Box::new(
                        guard.prepare_edit_with_shared_writeback(
                            edit,
                            Some(&super::images::VALIDATOR),
                            &self.writeback_policy,
                        )?,
                    )));
                    afters.push(After::Patch(CacheToken::default(), cache_edit));
                }
            }
        }
        // Pre-publication tokens let patch replacements re-key the cached
        // decoded node to the post-publication version.
        for (pin, after) in pins.iter().zip(afters.iter_mut()) {
            if let After::Patch(token, _) = after {
                *token = pin.cache_token();
            }
        }
        let inserted = value.take().ok_or_else(|| invalid("missing split value"))?;
        if let Some(values) = &mut values {
            values.insert(id, inserted);
        }
        // All old pages latched, all new pages initialized. No suspension or
        // fallible work after publication begins.
        // No fallible operation, I/O, await or latch release during publication.
        for write in &mut writes {
            write.publish();
        }
        self.cache_after_publish(&pins, afters);
        self.root.set(root);
        self.count.set(self.count.get() + 1);
        Ok(true)
    }
}
