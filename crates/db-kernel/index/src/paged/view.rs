//! Borrowed MBP1 views. Validation walks every slot once per immutable image;
//! lookup then visits only the slots selected by binary search, without decoding
//! a Vec of entries or allocating individual keys. All slicing remains checked.
use super::{
    codec::{u16_at, u64_at, HEADER},
    invalid,
    node::Key,
    RS,
};

/// A compressed key borrows its shared prefix and record-local suffix separately.
#[derive(Clone, Copy)]
pub(super) struct KeyView<'a> {
    pub prefix: &'a [u8],
    pub suffix: &'a [u8],
    pub overflow: u64,
    pub len: u64,
}
impl<'a> KeyView<'a> {
    fn new(record: &'a [u8], prefix: &'a [u8]) -> RS<Self> {
        if record.len() < 16 {
            return Err(invalid("truncated index key"));
        }
        let key = Self {
            prefix,
            suffix: &record[16..],
            overflow: u64_at(record, 0),
            len: u64_at(record, 8),
        };
        let head = (key.prefix.len() + key.suffix.len()) as u64;
        if key.len < head || (key.overflow == 0 && key.len != head) {
            return Err(invalid("invalid index key length"));
        }
        Ok(key)
    }
    pub fn owned(self) -> Key {
        let mut head = Vec::with_capacity(self.prefix.len() + self.suffix.len());
        head.extend_from_slice(self.prefix);
        head.extend_from_slice(self.suffix);
        Key {
            head,
            overflow: self.overflow,
            len: self.len,
        }
    }
}

/// Read-only node interpretation; ownership and latching stay with PageImage/PagePin.
pub(super) struct NodeView<'a> {
    pub bytes: &'a [u8],
    pub slots: usize,
    pub count: usize,
}
impl<'a> NodeView<'a> {
    /// Checks the small header even on memo hits. The caller validates the entire
    /// immutable image before using it; this constructor never scans all entries.
    pub fn new(bytes: &'a [u8]) -> RS<Self> {
        if bytes.len() < HEADER || bytes[4] != 0 {
            return Err(invalid("expected an index node"));
        }
        let count = u16_at(bytes, 32);
        let slots = HEADER + u16_at(bytes, 34);
        if count == 0 || slots + count * 4 > bytes.len() {
            return Err(invalid("invalid slot directory"));
        }
        Ok(Self {
            bytes,
            slots,
            count,
        })
    }
    pub fn len(&self) -> usize {
        self.count - 1
    }
    /// Child levels guide latch selection; the child is still validated after pinning.
    pub fn level(&self) -> usize {
        u16_at(self.bytes, 6)
    }
    pub fn leaf(&self) -> bool {
        self.level() == 0
    }
    pub fn revision(&self) -> u64 {
        u64_at(self.bytes, 24)
    }
    pub fn left(&self) -> u64 {
        u64_at(self.bytes, 8)
    }
    pub fn right(&self) -> u64 {
        u64_at(self.bytes, 16)
    }
    pub fn prefix(&self) -> &'a [u8] {
        &self.bytes[HEADER..self.slots]
    }
    pub fn slot(&self, slot: usize) -> RS<(usize, usize)> {
        if slot >= self.count {
            return Err(invalid("index slot out of range"));
        }
        let start = u16_at(self.bytes, self.slots + slot * 4);
        let len = u16_at(self.bytes, self.slots + slot * 4 + 2);
        if start < self.slots + self.count * 4 || start + len > self.bytes.len() {
            return Err(invalid("invalid index slot"));
        }
        Ok((start, len))
    }
    pub fn record(&self, slot: usize) -> RS<&'a [u8]> {
        let (start, len) = self.slot(slot)?;
        Ok(&self.bytes[start..start + len])
    }
    pub fn high(&self) -> RS<Option<KeyView<'a>>> {
        let record = self.record(0)?;
        if record.is_empty() {
            Ok(None)
        } else {
            KeyView::new(record, &[]).map(Some)
        }
    }
    /// Bounds-check a record once, without decoding either its key or value.
    #[inline]
    fn entry_record(&self, at: usize) -> RS<&'a [u8]> {
        let record = self.record(at.checked_add(1).ok_or_else(|| invalid("slot overflow"))?)?;
        if record.len() < 32 {
            return Err(invalid("truncated index entry"));
        }
        Ok(record)
    }
    /// Search probes need only the key; decode the value after choosing a slot.
    #[inline]
    pub fn key(&self, at: usize) -> RS<KeyView<'a>> {
        KeyView::new(&self.entry_record(at)?[16..], self.prefix())
    }
    /// Read a selected value without reconstructing its already-compared key.
    #[inline]
    pub fn value(&self, at: usize) -> RS<u128> {
        let record = self.entry_record(at)?;
        Ok(u128::from_le_bytes(
            record[..16]
                .try_into()
                .map_err(|_| invalid("truncated value"))?,
        ))
    }
    /// Scans consume both fields, sharing the same checked record lookup.
    #[inline]
    pub fn entry(&self, at: usize) -> RS<(KeyView<'a>, u128)> {
        let record = self.entry_record(at)?;
        Ok((
            KeyView::new(&record[16..], self.prefix())?,
            u128::from_le_bytes(
                record[..16]
                    .try_into()
                    .map_err(|_| invalid("truncated value"))?,
            ),
        ))
    }
}

/// Validate all payload bounds and key descriptors, including slots not visited
/// by the current query. No owned keys or decoded nodes are constructed here.
pub(super) fn validate_layout(bytes: &[u8]) -> RS<()> {
    if bytes.len() < HEADER {
        return Err(invalid("truncated index header"));
    }
    let count = u16_at(bytes, 32);
    let slots = HEADER + u16_at(bytes, 34);
    let lower = slots + count * 4;
    if count == 0 || lower > bytes.len() {
        return Err(invalid("invalid slot directory"));
    }
    let mut previous = bytes.len();
    for slot in 0..count {
        let at = u16_at(bytes, slots + slot * 4);
        let len = u16_at(bytes, slots + slot * 4 + 2);
        if at < lower || at + len != previous {
            return Err(invalid("invalid index slot"));
        }
        previous = at;
    }
    if bytes[4] == 1 {
        if count != 1 {
            return Err(invalid("invalid overflow page"));
        }
        return Ok(());
    }
    let node = NodeView::new(bytes)?;
    node.high()?;
    for at in 0..node.len() {
        node.entry(at)?;
    }
    Ok(())
}
