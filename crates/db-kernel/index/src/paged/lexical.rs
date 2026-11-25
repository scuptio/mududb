//! Opt-in unsigned-byte comparisons over compressed heads and overflow chunks.
use super::{codec, invalid, node::Link, tree::cooperate, view::KeyView, PagedBTree, RS};
use page_buffer::BufferAccess;
use std::cmp::Ordering;

/// Compare an available segment, deferring length ordering until the logical end.
fn segment(bytes: &[u8], query: &[u8], offset: &mut usize) -> Option<Ordering> {
    let remaining = &query[*offset..];
    let shared = bytes.len().min(remaining.len());
    let order = bytes[..shared].cmp(&remaining[..shared]);
    if order != Ordering::Equal {
        return Some(order);
    }
    if bytes.len() > remaining.len() {
        return Some(Ordering::Greater);
    }
    *offset += bytes.len();
    None
}
/// Returns a complete comparison when resident segments suffice, without allocation.
pub(super) fn head_order(stored: KeyView<'_>, query: &[u8]) -> Option<Ordering> {
    let mut offset = 0;
    for part in [stored.prefix, stored.suffix] {
        if let Some(order) = segment(part, query, &mut offset) {
            return Some(order);
        }
    }
    (stored.overflow == 0).then(|| offset.cmp(&query.len()))
}
impl<V: Send> PagedBTree<V> {
    /// A differing resident head decides ordering without touching overflow pages.
    /// Equal prefixes stream validated chunks; no complete key allocation is needed.
    pub(super) async fn compare_lexical(
        &self,
        access: &BufferAccess<'_>,
        stored: KeyView<'_>,
        query: &[u8],
    ) -> RS<Ordering> {
        let mut offset = 0;
        for part in [stored.prefix, stored.suffix] {
            if let Some(order) = segment(part, query, &mut offset) {
                return Ok(order);
            }
        }
        let mut next = stored.overflow;
        let mut consumed = offset as u64;
        while next != 0 {
            self.overflow_read();
            let (_pin, image) = self.page_image(access, &Link::new(next)).await?;
            if image[4] != 1 {
                return Err(invalid("expected overflow page"));
            }
            let directory = codec::HEADER + codec::u16_at(&image, 34);
            let start = codec::u16_at(&image, directory);
            let len = codec::u16_at(&image, directory + 2);
            consumed = consumed
                .checked_add(len as u64)
                .ok_or_else(|| invalid("overflow key length"))?;
            if len == 0 || consumed > stored.len {
                return Err(invalid("invalid overflow key length"));
            }
            next = codec::u64_at(&image, 8);
            if let Some(order) = segment(&image[start..start + len], query, &mut offset) {
                return Ok(order);
            }
            cooperate().await;
        }
        if consumed != stored.len {
            return Err(invalid("truncated overflow key"));
        }
        Ok(offset.cmp(&query.len()))
    }
}
