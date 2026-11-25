use std::sync::Arc;

/// Logical tree reference; physical residency belongs entirely to the buffer.
#[derive(Clone, Debug)]
pub(super) struct Link {
    pub pid: u64,
}
impl Link {
    pub fn new(pid: u64) -> Self {
        Self { pid }
    }
}

/// Inline key bytes plus an optional overflow chain and the full encoded length.
#[derive(Clone, Debug)]
pub(super) struct Key {
    pub head: Vec<u8>,
    pub overflow: u64,
    pub len: u64,
}

impl Key {
    pub fn inline(bytes: &[u8]) -> Self {
        Self {
            head: bytes.to_vec(),
            overflow: 0,
            len: bytes.len() as u64,
        }
    }
}

#[derive(Clone, Debug)]
pub(super) struct Entry {
    pub key: Key,
    // Value slot for a leaf, right child for an internal node.
    pub target: Link,
    // Upper half of an inline 128-bit value; target carries the lower half in leaves.
    pub upper: u64,
}

/// Decoded traversal snapshot; mutations build a replacement rather than edit frames.
#[derive(Clone, Debug)]
pub(super) struct Node {
    pub level: u16,
    // Writers recheck this under the page latch before publishing snapshot-based edits.
    pub revision: u64,
    pub left: Link,
    pub right: Link,
    pub high: Option<Key>,
    pub entries: Arc<Vec<Entry>>,
}

impl Node {
    pub fn empty(level: u16) -> Self {
        Self {
            level,
            revision: 0,
            left: Link::new(0),
            right: Link::new(0),
            high: None,
            entries: Arc::new(vec![]),
        }
    }

    pub fn leaf(&self) -> bool {
        self.level == 0
    }
}

#[derive(Clone, Debug)]
pub(super) enum Page {
    Node(Node),
    Overflow { next: u64, bytes: Vec<u8> },
}

impl Page {
    pub fn node(&self) -> super::RS<&Node> {
        match self {
            Self::Node(n) => Ok(n),
            _ => Err(super::invalid("expected an index node")),
        }
    }
}
