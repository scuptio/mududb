//! Merges publish with ordinary writers running. Reuse waits for an operation
//! epoch boundary, so stale paths and overflow snapshots cannot observe ABA.
use super::{
    codec, invalid,
    node::{Entry, Link, Node, Page},
    tree::cooperate,
    PagedBTree, RS,
};
use page_buffer::BufferAccess;
use std::{collections::HashSet, rc::Rc, sync::Arc};

/// Result of an online maintenance pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct MaintenanceStats {
    /// Sibling pairs merged, including internal nodes.
    pub merged: usize,
    /// Unreachable page slots now available for reuse (including prior free slots).
    pub reusable: usize,
}

impl<V: Send> PagedBTree<V> {
    /// Combines adjacent siblings that fit in one page. Ordinary point reads
    /// and writes continue; structural changes serialize with splits. Readers
    /// holding an old path follow a forwarding node until the next grace period.
    pub async fn merge_pages(&self) -> RS<usize> {
        let access = &self.access().await;
        self.ensure_open()?;
        let _structure = self.structure.lock().await;
        let mut merged = 0;
        let mut pending = vec![self.root_link()];
        while let Some(parent) = pending.pop() {
            let (_, mut node) = self.snapshot(access, &parent).await?;
            if node.leaf() {
                continue;
            }
            let mut at = 0;
            while at < node.entries.len() {
                if self.merge_pair(access, &parent, &node, at).await? {
                    merged += 1;
                    node = self.snapshot(access, &parent).await?.1;
                } else {
                    at += 1;
                }
                cooperate().await;
            }
            if parent.pid == self.root.get() && node.entries.is_empty() {
                self.root.set(node.left.pid);
            }
            pending.push(node.left.clone());
            pending.extend(node.entries.iter().map(|e| e.target.clone()));
        }
        Ok(merged)
    }

    async fn merge_pair(
        &self,
        access: &BufferAccess<'_>,
        parent: &Link,
        node: &Node,
        at: usize,
    ) -> RS<bool> {
        let left = if at == 0 {
            &node.left
        } else {
            &node.entries[at - 1].target
        };
        let right = &node.entries[at].target;
        let (_, mut combined) = self.snapshot(access, left).await?;
        let (_, sibling) = self.snapshot(access, right).await?;
        if combined.level != sibling.level || combined.right.pid != right.pid {
            return Err(invalid("merge siblings are not adjacent"));
        }
        let left_revision = combined.revision;
        if !combined.leaf() {
            Arc::make_mut(&mut combined.entries).push(Entry {
                key: node.entries[at].key.clone(),
                target: sibling.left.clone(),
                upper: 0,
            });
        }
        Arc::make_mut(&mut combined.entries).extend(sibling.entries.iter().cloned());
        combined.high = sibling.high.clone();
        combined.right = sibling.right.clone();
        if !codec::fits(&combined, self.buffer.page_size()) {
            return Ok(false);
        }
        combined.revision += 1;
        let mut updated_parent = node.clone();
        Arc::make_mut(&mut updated_parent.entries).remove(at);
        updated_parent.revision += 1;
        // A stale path to the retired right page descends through this forwarding
        // node. A writer that already snapped its old leaf fails revision validation.
        let mut forward = Node::empty(
            sibling
                .level
                .checked_add(1)
                .ok_or_else(|| invalid("merge forwarding level exhausted"))?,
        );
        forward.left = left.clone();
        forward.revision = sibling.revision + 1;
        let mut changes = vec![
            (parent.clone(), node.revision, updated_parent),
            (left.clone(), left_revision, combined),
            (right.clone(), sibling.revision, forward),
        ];
        changes.sort_by_key(|c| c.0.pid);
        let images = changes
            .iter()
            .map(|(link, _, node)| self.image(link.pid, node.clone()))
            .collect::<RS<Vec<_>>>()?;
        let mut pins = Vec::new();
        for (link, _, _) in &changes {
            pins.push(self.pin(access, link).await?);
        }
        let mut guards = Vec::new();
        for pin in &pins {
            let Some(guard) = pin.try_write() else {
                return Ok(false);
            };
            guards.push(guard);
        }
        for (guard, (_, revision, _)) in guards.iter().zip(&changes) {
            if codec::node_revision(guard)? != *revision {
                return Ok(false);
            }
        }
        let mut writes = guards
            .into_iter()
            .zip(images)
            .map(|(guard, image)| guard.prepare(image))
            .collect::<RS<Vec<_>>>()?;
        // No fallible operation, I/O, await or latch release during publication.
        for write in &mut writes {
            write.publish();
        }
        // Publish the merged nodes into the decoded-node cache as well, so
        // readers never re-decode after a merge.
        for (pin, (_, _, node)) in pins.iter().zip(&changes) {
            self.cache_replace_node(pin, Rc::new(super::node_cache::CachedNode::from_node(node)));
        }
        Ok(true)
    }

    /// Reclaims unreachable node and overflow slots without closing the tree.
    /// The exclusive maintenance lease is a grace period for all previous
    /// operations; page IDs are reusable only after those operations have
    /// dropped their pins.
    /// Cancellation before completion may defer reclamation, never free live pages.
    pub async fn reclaim_pages(&self) -> RS<usize> {
        let access = &self.buffer.maintenance().await;
        self.ensure_open()?;
        let mut live = HashSet::new();
        let mut pending = vec![self.root.get()];
        while let Some(pid) = pending.pop() {
            if pid == 0 || !live.insert(pid) {
                continue;
            }
            if pid >= self.buffer.allocator_state(access)?.next_page {
                return Err(invalid("reachable page outside allocator"));
            }
            let pin = self.pin(access, &Link::new(pid)).await?;
            let image = pin.snapshot().await;
            let page = self.decode_image(&image, pid).await?;
            match &page {
                Page::Overflow { next, .. } => pending.push(*next),
                Page::Node(node) => {
                    pending.push(node.right.pid);
                    if let Some(high) = &node.high {
                        pending.push(high.overflow);
                    }
                    if !node.leaf() {
                        pending.push(node.left.pid);
                    }
                    for entry in node.entries.iter() {
                        pending.push(entry.key.overflow);
                        if !node.leaf() {
                            pending.push(entry.target.pid);
                        }
                    }
                }
            }
            drop(page);
            drop(pin);
            cooperate().await;
        }
        self.buffer.reclaim(access, &live).await
    }

    /// Runs concurrent sibling merging followed by an asynchronous reclamation
    /// grace period. File extents are reused; the backing file is not truncated.
    pub async fn maintain(&self) -> RS<MaintenanceStats> {
        let merged = self.merge_pages().await?;
        let reusable = self.reclaim_pages().await?;
        Ok(MaintenanceStats { merged, reusable })
    }
}
