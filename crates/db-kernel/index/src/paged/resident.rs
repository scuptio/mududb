//! Resident inline-key lookup. Uncontended latches complete synchronously;
//! contention waits on the pinned page without creating a search snapshot.
use super::{
    invalid,
    node::Link,
    view::{KeyView, NodeView},
    PagedBTree, RS,
};
use page_buffer::BufferAccess;
use std::cmp::Ordering;

impl<V: Send> PagedBTree<V> {
    /// None requests materialization in the async path. Comparator errors remain
    /// errors, never a reason to silently switch ordering or ignore a failure.
    pub(super) fn compare_inline(&self, stored: KeyView<'_>, key: &[u8]) -> RS<Option<Ordering>> {
        self.compare_inline_order(stored, key, false)
    }
    /// Preserve comparator operand order on fence checks, including error behavior.
    pub(super) fn compare_inline_order(
        &self,
        stored: KeyView<'_>,
        key: &[u8],
        query_first: bool,
    ) -> RS<Option<Ordering>> {
        if self.lexicographic {
            return Ok(super::lexical::head_order(stored, key).map(|order| {
                if query_first {
                    order.reverse()
                } else {
                    order
                }
            }));
        }
        if stored.overflow != 0 {
            return Ok(None);
        }
        let compare = |bytes: &[u8]| {
            if query_first {
                self.compare(key, bytes)
            } else {
                self.compare(bytes, key)
            }
        };
        if stored.prefix.is_empty() {
            return compare(stored.suffix).map(Some);
        }
        let len = stored.prefix.len() + stored.suffix.len();
        if len > 512 {
            return Ok(None);
        }
        let mut scratch = [0; 512];
        scratch[..stored.prefix.len()].copy_from_slice(stored.prefix);
        scratch[stored.prefix.len()..len].copy_from_slice(stored.suffix);
        compare(&scratch[..len]).map(Some)
    }
    pub(super) fn position_inline(
        &self,
        node: &NodeView<'_>,
        key: &[u8],
    ) -> RS<Option<(usize, bool)>> {
        // A shared-prefix mismatch orders the query against every record at once.
        if self.lexicographic && !node.prefix().is_empty() {
            let prefix = node.prefix();
            let shared = prefix.len().min(key.len());
            match prefix[..shared].cmp(&key[..shared]) {
                Ordering::Greater => return Ok(Some((0, false))),
                Ordering::Less => return Ok(Some((node.len(), false))),
                Ordering::Equal if key.len() < prefix.len() => return Ok(Some((0, false))),
                _ => {}
            }
        }
        let (mut lo, mut hi) = (0, node.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let Some(order) = self.compare_inline(node.key(mid)?, key)? else {
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
    /// Outer None means async fallback; Some(None) is a completed lookup miss.
    ///
    /// Descends over the write-epoch-validated decoded-node cache: a hit
    /// avoids both the page latch and the slot-directory reinterpretation.
    /// A pin miss, cache miss or overflow key falls back to the async path.
    pub(super) async fn lookup_resident(
        &self,
        access: &BufferAccess<'_>,
        key: &[u8],
    ) -> RS<Option<Option<u128>>> {
        let mut link = self.root_link();
        loop {
            let Some(page) = self
                .buffer
                .try_pin(access, &self.buffer.page_ref(link.pid))?
            else {
                return Ok(None);
            };
            let Some(node) = self.nodes.borrow().get(link.pid, page.cache_token()) else {
                return Ok(None);
            };
            if let Some(high) = &node.high {
                let Some(order) = self.compare_cached_inline(&node, *high, key)? else {
                    return Ok(None);
                };
                if order.reverse() != Ordering::Less {
                    if node.right == 0 {
                        return Err(invalid("missing B-link sibling"));
                    }
                    link = Link::new(node.right);
                    continue;
                }
            }
            let Some((at, equal)) = self.position_cached_sync(&node, key)? else {
                return Ok(None);
            };
            if node.leaf() {
                return Ok(Some(if equal {
                    let entry = &node.entries[at];
                    Some((entry.target as u128) | ((entry.upper as u128) << 64))
                } else {
                    None
                }));
            }
            link = Link::new(if equal {
                node.entries[at].target
            } else if at == 0 {
                node.left
            } else {
                node.entries[at - 1].target
            });
        }
    }
}
