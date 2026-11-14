//! Decoded-node cache for the paged tree: avoids re-decoding a page's slot
//! directory and prefix-compressed keys on every visit.
//!
//! The cached form is arena-allocated (one bytes arena plus one entries
//! vector per node, no per-key allocations), which makes both the fill
//! decode and the write-through edits cheap. Entries are keyed by page id
//! and validated against the frame's cache token (frame identity, residency
//! generation, write epoch): any publication bumps the epoch and any frame
//! reuse bumps the generation, so a stale entry can never be observed
//! (worst case is a spurious miss). Publish sites re-key and patch the
//! cached node in place, so a hot leaf is never re-decoded after a write.
//!
//! Decode-worthiness is tracked in a lossy hint table on the tree (two cell
//! ops per visit, no allocation), so pages visited once per version
//! (one-shot scans, churn-heavy leaves) stay on the zero-allocation byte
//! view. Capacity is bounded by the buffer's frame count, keeping cached
//! memory proportional to the resident budget.
use super::node::Node;
use std::collections::{HashMap, VecDeque};
use std::hash::{BuildHasherDefault, Hasher};
use std::rc::Rc;

/// (frame identity, residency generation, write epoch) from
/// `PagePin::cache_token`.
pub(super) type CacheToken = (usize, u64, u64);

/// Packs a cache token into one u64 hint tag. Collisions only lose or
/// occasionally waste a decode, never correctness.
pub(super) fn hint_tag(token: CacheToken) -> u64 {
    (token.0 as u64)
        .wrapping_mul(0x9e37_79b9_7f4a_7c15)
        .rotate_left(17)
        ^ token.1.rotate_left(32)
        ^ token.2
}

/// A key in the arena: its head bytes plus the overflow descriptor.
#[derive(Clone, Copy)]
pub(super) struct CachedKey {
    pub head_off: u32,
    pub head_len: u32,
    pub overflow: u64,
    pub len: u64,
}

/// One decoded entry: key location plus the value/child slot.
#[derive(Clone, Copy)]
pub(super) struct CachedEntry {
    pub key: CachedKey,
    pub target: u64,
    pub upper: u64,
}

/// The cached decoded form of one page version: two allocations total.
pub(super) struct CachedNode {
    pub level: u16,
    pub revision: u64,
    pub left: u64,
    pub right: u64,
    pub high: Option<CachedKey>,
    pub entries: Vec<CachedEntry>,
    pub arena: Vec<u8>,
}

impl CachedNode {
    pub(super) fn leaf(&self) -> bool {
        self.level == 0
    }
    /// Head bytes of a cached key.
    pub(super) fn head(&self, key: &CachedKey) -> &[u8] {
        &self.arena[key.head_off as usize..(key.head_off + key.head_len) as usize]
    }
    /// Flattens a decoded node into the arena form.
    pub(super) fn from_node(node: &Node) -> Self {
        let mut arena = Vec::with_capacity(
            node.entries.iter().map(|e| e.key.head.len()).sum::<usize>()
                + node.high.as_ref().map_or(0, |k| k.head.len()),
        );
        let mut pack = |key: &super::node::Key| {
            let head_off = arena.len() as u32;
            arena.extend_from_slice(&key.head);
            CachedKey {
                head_off,
                head_len: key.head.len() as u32,
                overflow: key.overflow,
                len: key.len,
            }
        };
        let entries = node
            .entries
            .iter()
            .map(|e| CachedEntry {
                key: pack(&e.key),
                target: e.target.pid,
                upper: e.upper,
            })
            .collect();
        let high = node.high.as_ref().map(&mut pack);
        Self {
            level: node.level,
            revision: node.revision,
            left: node.left.pid,
            right: node.right.pid,
            high,
            entries,
            arena,
        }
    }
}

/// A logical leaf edit published to a page, applied in place to the cached
/// decoded node (copy-on-write when readers still hold the old version).
pub(super) enum CachedEdit {
    Insert {
        at: usize,
        key_head: Vec<u8>,
        overflow: u64,
        len: u64,
        target: u64,
        upper: u64,
    },
    Remove {
        at: usize,
    },
    Replace {
        at: usize,
        target: u64,
        upper: u64,
    },
}

impl CachedNode {
    /// Applies the edit in place. The arena only ever grows: removed keys
    /// leave unreachable bytes, reclaimed by the next full decode.
    fn apply(&mut self, edit: &CachedEdit) {
        self.revision += 1;
        match edit {
            CachedEdit::Insert {
                at,
                key_head,
                overflow,
                len,
                target,
                upper,
            } => {
                let head_off = self.arena.len() as u32;
                self.arena.extend_from_slice(key_head);
                self.entries.insert(
                    *at,
                    CachedEntry {
                        key: CachedKey {
                            head_off,
                            head_len: key_head.len() as u32,
                            overflow: *overflow,
                            len: *len,
                        },
                        target: *target,
                        upper: *upper,
                    },
                );
            }
            CachedEdit::Remove { at } => {
                self.entries.remove(*at);
            }
            CachedEdit::Replace { at, target, upper } => {
                self.entries[*at].target = *target;
                self.entries[*at].upper = *upper;
            }
        }
    }
}

/// Multiplicative-xor mixer for the small fixed-shape cache key.
#[derive(Default)]
struct KeyHasher(u64);
impl Hasher for KeyHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = 0u64;
            for (i, byte) in chunk.iter().enumerate() {
                word |= (*byte as u64) << (i * 8);
            }
            self.write_u64(word);
        }
    }
    fn write_u64(&mut self, value: u64) {
        self.0 = (self.0 ^ value).wrapping_mul(0x9e37_79b9_7f4a_7c15);
    }
    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
    fn finish(&self) -> u64 {
        self.0
    }
}
type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<KeyHasher>>;

pub(super) struct NodeCache {
    // One entry per page: the version token travels in the value, so a
    // publish updates in place without re-keying the map.
    map: FastMap<u64, (CacheToken, Rc<CachedNode>)>,
    // FIFO eviction order over page ids.
    fifo: VecDeque<u64>,
    capacity: usize,
}

impl NodeCache {
    pub(super) fn new(capacity: usize) -> Self {
        Self {
            map: FastMap::default(),
            fifo: VecDeque::new(),
            capacity,
        }
    }

    /// Returns the cached node when its entry exactly matches this content
    /// version token.
    pub(super) fn get(&self, pid: u64, token: CacheToken) -> Option<Rc<CachedNode>> {
        match self.map.get(&pid) {
            Some((cached, node)) if *cached == token => Some(node.clone()),
            _ => None,
        }
    }

    /// Publishes a decoded node (first fill) after the hint table confirmed
    /// a repeat visit on this version.
    pub(super) fn insert(&mut self, pid: u64, token: CacheToken, node: Rc<CachedNode>) {
        if self.capacity == 0 {
            return;
        }
        if let Some(entry) = self.map.get_mut(&pid) {
            *entry = (token, node);
            return;
        }
        self.evict_to_fit();
        self.map.insert(pid, (token, node));
        self.fifo.push_back(pid);
    }

    /// Applies a just-published edit to the cached node in place (copy-on-
    /// write when a reader still holds the old version) and moves its token
    /// to the post-publication one. Pages not currently cached at the
    /// pre-publication version stay uncached.
    pub(super) fn apply_edit(
        &mut self,
        pid: u64,
        before: CacheToken,
        after: CacheToken,
        edit: CachedEdit,
    ) {
        let Some((token, slot)) = self.map.get_mut(&pid) else {
            return;
        };
        if *token != before {
            return;
        }
        if let Some(node) = Rc::get_mut(slot) {
            node.apply(&edit);
        } else {
            let mut node = slot.clone_cow();
            node.apply(&edit);
            *slot = Rc::new(node);
        }
        *token = after;
    }

    fn evict_to_fit(&mut self) {
        while self.map.len() >= self.capacity {
            let Some(victim) = self.fifo.pop_front() else {
                break;
            };
            if self.map.remove(&victim).is_some() {
                break;
            }
        }
    }
}

impl CachedNode {
    /// Clone-on-write for a shared cached node: two flat copies, no per-key
    /// allocations.
    fn clone_cow(&self) -> Self {
        Self {
            level: self.level,
            revision: self.revision,
            left: self.left,
            right: self.right,
            high: self.high,
            entries: self.entries.clone(),
            arena: self.arena.clone(),
        }
    }
}
