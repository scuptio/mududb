//! Leaf edits preserve MBP1's packed record ordering. Work builds a private
//! edit program; cancellation, allocation failure, or a revision conflict cannot
//! modify the published image. No decoded Entry/Key collection is needed.
use super::{invalid, node::Key, view::NodeView, RS};
use page_buffer::PageEdit;

fn finish(mut edit: PageEdit, revision: u64, count: usize) -> RS<PageEdit> {
    let next = revision
        .checked_add(1)
        .ok_or_else(|| invalid("node revision exhausted"))?;
    // Revision and slot count are adjacent. One small write keeps insert/remove
    // programs within the four inline changes, avoiding a program-vector allocation.
    let mut header = [0; 10];
    header[..8].copy_from_slice(&next.to_le_bytes());
    header[8..].copy_from_slice(&(count as u16).to_le_bytes());
    edit.write(24, &header);
    Ok(edit)
}
fn slot(bytes: &mut [u8], directory: usize, index: usize, start: usize, len: usize) {
    let at = directory + index * 4;
    bytes[at..at + 2].copy_from_slice(&(start as u16).to_le_bytes());
    bytes[at + 2..at + 4].copy_from_slice(&(len as u16).to_le_bytes());
}

/// Only the fixed-width value and revision change; keys and directory stay intact.
pub(super) fn replace(node: &NodeView<'_>, at: usize, value: u128) -> RS<PageEdit> {
    node.entry(at)?;
    let (start, _) = node.slot(at + 1)?;
    let mut edit = PageEdit::new();
    edit.write(start, &value.to_le_bytes());
    finish(edit, node.revision(), node.count)
}

/// Returns None when the prefix needs rebuilding or the packed page is full.
/// Existing records are moved as bytes; only the inserted record is encoded.
pub(super) fn insert(
    node: &NodeView<'_>,
    at: usize,
    key: &Key,
    value: u128,
) -> RS<Option<PageEdit>> {
    if at > node.len() {
        return Err(invalid("insert slot out of range"));
    }
    if node.len() == 0 || !key.head.starts_with(node.prefix()) {
        return Ok(None);
    }
    let suffix = &key.head[node.prefix().len()..];
    let len = 32 + suffix.len();
    let (bottom, _) = node.slot(node.count - 1)?;
    if node.slots + (node.count + 1) * 4 + len > bottom {
        return Ok(None);
    }
    let index = at + 1;
    let boundary = if index == node.count {
        bottom
    } else {
        let (start, length) = node.slot(index)?;
        start + length
    };
    let start = boundary - len;
    let mut edit = PageEdit::new();
    edit.copy_within(bottom..boundary, bottom - len);
    let mut directory = vec![0; (node.count + 1 - index) * 4];
    slot(&mut directory, 0, 0, start, len);
    for old in index..node.count {
        let (offset, length) = node.slot(old)?;
        slot(&mut directory, 0, old + 1 - index, offset - len, length);
    }
    edit.write_owned(node.slots + index * 4, directory);
    let mut record = Vec::with_capacity(len);
    record.extend_from_slice(&value.to_le_bytes());
    record.extend_from_slice(&key.overflow.to_le_bytes());
    record.extend_from_slice(&key.len.to_le_bytes());
    record.extend_from_slice(suffix);
    edit.write_owned(start, record);
    finish(edit, node.revision(), node.count + 1).map(Some)
}

/// Removal compacts the packed tail and directory while retaining a valid shared
/// prefix. Prefix maximization is optional and can wait until a later rebuild.
pub(super) fn remove(node: &NodeView<'_>, at: usize) -> RS<PageEdit> {
    node.entry(at)?;
    let index = at + 1;
    let (start, len) = node.slot(index)?;
    let (bottom, _) = node.slot(node.count - 1)?;
    let mut edit = PageEdit::new();
    edit.copy_within(bottom..start, bottom + len);
    edit.fill(bottom..bottom + len, 0);
    let mut directory = vec![0; (node.count - index) * 4];
    for old in index + 1..node.count {
        let (offset, length) = node.slot(old)?;
        slot(&mut directory, 0, old - 1 - index, offset + len, length);
    }
    edit.write_owned(node.slots + index * 4, directory);
    finish(edit, node.revision(), node.count - 1)
}
