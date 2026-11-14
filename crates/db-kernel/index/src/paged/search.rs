//! Point descent keeps just the current page. Ancestor paths and decoded nodes
//! are needed only by structural changes, not by ordinary reads or leaf edits.
use super::{
    invalid,
    node::Link,
    view::{KeyView, NodeView},
    PagedBTree, RS,
};
use page_buffer::{BufferAccess, PageImage, PagePin};
use std::cmp::Ordering;

impl<V: Send> PagedBTree<V> {
    /// Compare complete logical keys using the configured comparator. Arbitrary
    /// tuple comparators cannot be replaced by lexicographic prefix shortcuts.
    pub(super) async fn compare_view(
        &self,
        access: &BufferAccess<'_>,
        stored: KeyView<'_>,
        key: &[u8],
        query_first: bool,
    ) -> RS<Ordering> {
        if let Some(order) = self.compare_inline_order(stored, key, query_first)? {
            return Ok(order);
        }
        if self.lexicographic {
            let order = self.compare_lexical(access, stored, key).await?;
            return Ok(if query_first { order.reverse() } else { order });
        }
        let bytes = self.view_bytes(access, stored).await?;
        if query_first {
            self.compare(key, &bytes)
        } else {
            self.compare(&bytes, key)
        }
    }

    /// Only callers returning an owned key (or comparing an overflow key) allocate.
    pub(super) async fn view_bytes(
        &self,
        access: &BufferAccess<'_>,
        key: KeyView<'_>,
    ) -> RS<Vec<u8>> {
        let owned = key.owned();
        if owned.overflow == 0 {
            Ok(owned.head)
        } else {
            self.bytes(access, &owned).await
        }
    }

    pub(super) async fn position_view(
        &self,
        access: &BufferAccess<'_>,
        node: &NodeView<'_>,
        key: &[u8],
    ) -> RS<(usize, bool)> {
        let (mut lo, mut hi) = (0, node.len());
        while lo < hi {
            let mid = lo + (hi - lo) / 2;
            let stored = node.key(mid)?;
            // Inline comparisons remain ordinary calls inside the search loop.
            let order = match self.compare_inline(stored, key)? {
                Some(order) => order,
                None => self.compare_view(access, stored, key, false).await?,
            };
            match order {
                Ordering::Less => lo = mid + 1,
                Ordering::Equal => return Ok((mid, true)),
                Ordering::Greater => hi = mid,
            }
        }
        Ok((lo, false))
    }

    /// The returned pin is the same one used to descend into the leaf. Its image
    /// is immutable; mutations and successful value reads still recheck revision
    /// under the latch. The access lease prevents page-ID reuse across snapshots.
    ///
    /// The descent runs over the write-epoch-validated decoded-node cache on
    /// repeat visits and over the zero-allocation byte view on first visits
    /// (see `level_node`); only the leaf's image is retained, since callers
    /// may patch its bytes.
    pub(super) async fn view_leaf<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        key: &[u8],
    ) -> RS<(PagePin<'a>, PageImage, usize, bool)> {
        let mut link = self.root_link();
        loop {
            let (page, level) = self.level_node(access, &link).await?;
            match level {
                super::tree::LevelNode::Decoded(node) => {
                    if let Some(high) = &node.high {
                        if self
                            .compare_cached(access, &node, *high, key)
                            .await?
                            .reverse()
                            != Ordering::Less
                        {
                            if node.right == 0 {
                                return Err(invalid("missing B-link sibling"));
                            }
                            link = Link::new(node.right);
                            continue;
                        }
                    }
                    let (at, equal) = self.position_cached(access, &node, key).await?;
                    if node.leaf() {
                        let image = page.snapshot().await;
                        self.validate_page(&image, link.pid)?;
                        return Ok((page, image, at, equal));
                    }
                    link = Link::new(if equal {
                        node.entries[at].target
                    } else if at == 0 {
                        node.left
                    } else {
                        node.entries[at - 1].target
                    });
                }
                super::tree::LevelNode::View(image) => {
                    let node = NodeView::new(&image)?;
                    if let Some(high) = node.high()? {
                        if self.compare_view(access, high, key, true).await? != Ordering::Less {
                            if node.right() == 0 {
                                return Err(invalid("missing B-link sibling"));
                            }
                            link = Link::new(node.right());
                            continue;
                        }
                    }
                    let (at, equal) = self.position_view(access, &node, key).await?;
                    if node.leaf() {
                        return Ok((page, image, at, equal));
                    }
                    link = Link::new(if equal {
                        node.value(at)? as u64
                    } else if at == 0 {
                        node.left()
                    } else {
                        node.value(at - 1)? as u64
                    });
                }
            }
        }
    }

    /// Records the descent's page links without decoding interior nodes; the
    /// leaf link is the last element. Structural edits revalidate every page
    /// by revision at publication, so the descent needs no node copies.
    pub(super) async fn path_links(&self, access: &BufferAccess<'_>, key: &[u8]) -> RS<Vec<Link>> {
        let mut link = self.root_link();
        let mut path = Vec::new();
        loop {
            let (page, level) = self.level_node(access, &link).await?;
            let next = match level {
                super::tree::LevelNode::Decoded(node) => {
                    if let Some(high) = &node.high {
                        if self
                            .compare_cached(access, &node, *high, key)
                            .await?
                            .reverse()
                            != Ordering::Less
                        {
                            if node.right == 0 {
                                return Err(invalid("missing B-link sibling"));
                            }
                            link = Link::new(node.right);
                            continue;
                        }
                    }
                    if node.leaf() {
                        None
                    } else {
                        let (at, equal) = self.position_cached(access, &node, key).await?;
                        Some(Link::new(if equal {
                            node.entries[at].target
                        } else if at == 0 {
                            node.left
                        } else {
                            node.entries[at - 1].target
                        }))
                    }
                }
                super::tree::LevelNode::View(image) => {
                    let node = NodeView::new(&image)?;
                    if let Some(high) = node.high()? {
                        if self.compare_view(access, high, key, true).await? != Ordering::Less {
                            if node.right() == 0 {
                                return Err(invalid("missing B-link sibling"));
                            }
                            link = Link::new(node.right());
                            continue;
                        }
                    }
                    if node.leaf() {
                        None
                    } else {
                        let (at, equal) = self.position_view(access, &node, key).await?;
                        Some(Link::new(if equal {
                            node.value(at)? as u64
                        } else if at == 0 {
                            node.left()
                        } else {
                            node.value(at - 1)? as u64
                        }))
                    }
                }
            };
            drop(page);
            path.push(link);
            match next {
                Some(child) => link = child,
                None => return Ok(path),
            }
        }
    }
}
