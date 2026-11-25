//! Versioned slotted pages. Integers are little endian, never native structs.
//! The MBP1 header binds each image to its logical page ID and checksum. Decoding
//! stays in the index layer so buffer implementations need no knowledge of nodes,
//! prefix compression, or overflow keys.
use super::{
    invalid,
    node::{Entry, Key, Link, Node, Page},
    RS,
};
pub(super) const HEADER: usize = 56;
const MAGIC: &[u8; 4] = b"MBP1";

pub(super) fn validate_size(size: usize) -> RS<()> {
    if !(512..=65535).contains(&size) {
        return Err(invalid("index page size must be 512..65535"));
    }
    Ok(())
}

fn key_bytes(key: &Key, prefix: &[u8]) -> Vec<u8> {
    let mut b = Vec::new();
    b.extend_from_slice(&key.overflow.to_le_bytes());
    b.extend_from_slice(&key.len.to_le_bytes());
    b.extend_from_slice(&key.head[prefix.len()..]);
    b
}

fn prefix(n: &Node) -> &[u8] {
    let Some(first) = n.entries.first() else {
        return &[];
    };
    let mut len = first.key.head.len();
    for entry in &n.entries[1..] {
        len = first.key.head[..len]
            .iter()
            .zip(&entry.key.head)
            .take_while(|(a, b)| a == b)
            .count();
    }
    &first.key.head[..len]
}

pub(super) fn encode(page: &Page, size: usize, pid: u64) -> RS<Vec<u8>> {
    let mut image = encode_resident(page, size, pid)?;
    seal(&mut image);
    Ok(image)
}

/// Encodes the resident representation; the index writeback policy seals it later.
pub(super) fn encode_resident(page: &Page, size: usize, pid: u64) -> RS<Vec<u8>> {
    let mut image = vec![0; size];
    image[..4].copy_from_slice(MAGIC);
    image[48..56].copy_from_slice(&pid.to_le_bytes());
    let (records, pre) = match page {
        Page::Overflow { next, bytes } => {
            image[4] = 1;
            image[8..16].copy_from_slice(&next.to_le_bytes());
            (vec![bytes.clone()], vec![])
        }
        Page::Node(n) => {
            image[6..8].copy_from_slice(&n.level.to_le_bytes());
            image[8..16].copy_from_slice(&n.left.pid.to_le_bytes());
            image[16..24].copy_from_slice(&n.right.pid.to_le_bytes());
            image[24..32].copy_from_slice(&n.revision.to_le_bytes());
            let pre = prefix(n).to_vec();
            let mut records = vec![n
                .high
                .as_ref()
                .map(|k| key_bytes(k, &[]))
                .unwrap_or_default()];
            for entry in n.entries.iter() {
                let mut record = entry.target.pid.to_le_bytes().to_vec();
                record.extend_from_slice(&entry.upper.to_le_bytes());
                record.extend(key_bytes(&entry.key, &pre));
                records.push(record);
            }
            (records, pre)
        }
    };
    let slots_end = HEADER + pre.len() + records.len() * 4;
    let payload: usize = records.iter().map(Vec::len).sum();
    if slots_end + payload > size {
        return Err(invalid("index node does not fit page"));
    }
    image[32..34].copy_from_slice(&(records.len() as u16).to_le_bytes());
    image[34..36].copy_from_slice(&(pre.len() as u16).to_le_bytes());
    image[HEADER..HEADER + pre.len()].copy_from_slice(&pre);
    let mut end = size;
    for (i, record) in records.iter().enumerate() {
        end -= record.len();
        image[end..end + record.len()].copy_from_slice(record);
        let slot = HEADER + pre.len() + i * 4;
        image[slot..slot + 2].copy_from_slice(&(end as u16).to_le_bytes());
        image[slot + 2..slot + 4].copy_from_slice(&(record.len() as u16).to_le_bytes());
    }
    Ok(image)
}

/// Recompute the persisted checksum after an index-owned byte edit.
pub(super) fn seal(bytes: &mut [u8]) {
    seal_observed(bytes, None);
}
/// Optional per-owner accounting leaves the persisted checksum unchanged.
pub(super) fn seal_observed(
    bytes: &mut [u8],
    diagnostics: Option<&super::diagnostics::Diagnostics>,
) {
    let checksum = checksum(bytes, diagnostics);
    bytes[40..48].copy_from_slice(&checksum.to_le_bytes());
}
fn checksum(bytes: &[u8], diagnostics: Option<&super::diagnostics::Diagnostics>) -> u64 {
    let started = diagnostics.map(|_| mudu_sys::time::instant_now());
    // Preserve the eight-byte field and every existing page offset. CRC32C is
    // zero-extended, so validation also rejects nonzero upper checksum bytes.
    // Append the second segment without allocating or copying a temporary page.
    let head = crc32c::crc32c(&bytes[..40]);
    let value = u64::from(crc32c::crc32c_append(head, &bytes[48..]));
    if let (Some(diagnostics), Some(started)) = (diagnostics, started) {
        let nanos = started.elapsed().as_nanos().min(u128::from(u64::MAX)) as u64;
        diagnostics.record_checksum(bytes.len() - 8, nanos);
    }
    value
}
pub(super) fn u16_at(b: &[u8], at: usize) -> usize {
    usize::from(u16::from_le_bytes([b[at], b[at + 1]]))
}
pub(super) fn u64_at(b: &[u8], at: usize) -> u64 {
    let mut bytes = [0; 8];
    bytes.copy_from_slice(&b[at..at + 8]);
    u64::from_le_bytes(bytes)
}
fn decode_key(b: &[u8], prefix: &[u8]) -> RS<Key> {
    if b.len() < 16 {
        return Err(invalid("truncated index key"));
    }
    let mut head = prefix.to_vec();
    head.extend_from_slice(&b[16..]);
    let key = Key {
        head,
        overflow: u64_at(b, 0),
        len: u64_at(b, 8),
    };
    if key.len < key.head.len() as u64 || (key.overflow == 0 && key.len != key.head.len() as u64) {
        return Err(invalid("invalid index key length"));
    }
    Ok(key)
}

#[cfg(test)]
pub(super) fn validate_image(b: &[u8], size: usize, pid: u64) -> RS<()> {
    validate_image_observed(b, size, pid, None)
}
/// Counts actual hash work, including a checksum mismatch, but not header rejects.
pub(super) fn validate_image_observed(
    b: &[u8],
    size: usize,
    pid: u64,
    diagnostics: Option<&super::diagnostics::Diagnostics>,
) -> RS<()> {
    if b.len() != size
        || size < HEADER
        || &b[..4] != MAGIC
        || checksum(b, diagnostics) != u64_at(b, 40)
        || u64_at(b, 48) != pid
    {
        return Err(invalid("invalid index page header or checksum"));
    }
    Ok(())
}

#[cfg(test)]
pub(super) fn decode(b: &[u8], size: usize, pid: u64) -> RS<Page> {
    validate_image(b, size, pid)?;
    decode_validated(b)
}

// The immutable image identity must have passed validate_image first. Payload
// bounds and slot ordering remain checked on every decode.
pub(super) fn decode_validated(b: &[u8]) -> RS<Page> {
    let size = b.len();
    let count = u16_at(b, 32);
    let prefix_len = u16_at(b, 34);
    let slots = HEADER + prefix_len;
    let lower = slots + count * 4;
    if count == 0 || lower > size {
        return Err(invalid("invalid slot directory"));
    }
    let mut records = Vec::with_capacity(count);
    let mut previous = size;
    for i in 0..count {
        let at = u16_at(b, slots + i * 4);
        let len = u16_at(b, slots + i * 4 + 2);
        if at < lower || at + len != previous {
            return Err(invalid("invalid index slot"));
        }
        records.push(&b[at..at + len]);
        previous = at;
    }
    if b[4] == 1 {
        if count != 1 {
            return Err(invalid("invalid overflow page"));
        }
        return Ok(Page::Overflow {
            next: u64_at(b, 8),
            bytes: records[0].to_vec(),
        });
    }
    if b[4] != 0 {
        return Err(invalid("unknown index page kind"));
    }
    let high = if records[0].is_empty() {
        None
    } else {
        Some(decode_key(records[0], &[])?)
    };
    let mut entries = Vec::new();
    for record in &records[1..] {
        if record.len() < 32 {
            return Err(invalid("truncated index entry"));
        }
        entries.push(Entry {
            key: decode_key(&record[16..], &b[HEADER..slots])?,
            target: Link::new(u64_at(record, 0)),
            upper: u64_at(record, 8),
        });
    }
    Ok(Page::Node(Node {
        level: u16_at(b, 6) as u16,
        revision: u64_at(b, 24),
        left: Link::new(u64_at(b, 8)),
        right: Link::new(u64_at(b, 16)),
        high,
        entries: entries.into(),
    }))
}

pub(super) fn fits(node: &Node, size: usize) -> bool {
    encoded_len(node) <= size
}

pub(super) fn encoded_len(node: &Node) -> usize {
    let pre = prefix(node).len();
    HEADER
        + pre
        + (node.entries.len() + 1) * 4
        + node.high.as_ref().map(|k| 16 + k.head.len()).unwrap_or(0)
        + node
            .entries
            .iter()
            .map(|e| 32 + e.key.head.len() - pre)
            .sum::<usize>()
}

// Only used on images created by this index, after a validated snapshot. All
// replacements pass encode before publication. Buffer write epochs are separate.
pub(super) fn node_revision(bytes: &[u8]) -> RS<u64> {
    if bytes.len() < HEADER || &bytes[..4] != MAGIC || bytes[4] != 0 {
        return Err(invalid("expected an encoded node"));
    }
    Ok(u64_at(bytes, 24))
}
