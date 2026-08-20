use crate::storage::page::PageId;
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

/// Stable physical file identity used by time-series WAL records.
///
/// The corresponding on-disk relation file is addressed as:
/// `{partition_id}.{table_id}.{file_index}.dat`.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLFileId {
    pub partition_id: OID,
    pub table_id: OID,
    pub file_index: u32,
}

/// A physical log entry for one file object.
///
/// [`PLEntry`] describes physical updates to pages in the corresponding file,
/// rather than a logical SQL-level operation.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLEntry {
    /// Target file object identity.
    pub file: PLFileId,
    /// Ordered physical operations to apply to that file object.
    ///
    /// The operations are replayed in sequence and together describe the
    /// low-level file/page changes captured by this log entry.
    pub ops: Vec<PLOp>,
}

/// Physical operations captured in the log.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum PLOp {
    /// Create the target file object identified by [`PLEntry::file`].
    Create,
    /// Delete the target file object identified by [`PLEntry::file`].
    Delete,
    /// Apply a record-level delta to one page in the file object.
    PageDelta(PageDelta),
}

/// A record-level delta applied to one page of a file.
///
/// This is the record-grained replacement for the old full-page WAL image:
/// instead of logging one 4 KiB page image per touched page, a commit logs
/// only the records that changed plus the page's chain-metadata changes.
/// Recovery replays the delta with read-modify-write against the data file:
///
/// - [`PageDelta::init`] (newly allocated pages only): the page is built
///   empty with the given tuple metadata and chain links, then `upserts`
///   are inserted.
/// - [`PageDelta::links`]: the page header's prev/next chain links change.
/// - [`PageDelta::removes`]: records deleted from the page.
/// - [`PageDelta::upserts`]: records inserted into the page, or replacing
///   the payload of an existing record with the same key.
///
/// Replay is gated on the page header LSN: every page image persisted by
/// the write path carries the WAL LSN of the batch that produced it, and a
/// delta is applied only when the page's stamped LSN is older than the
/// delta's batch LSN. This gives exactly-once application — re-applying a
/// record delta would resurrect records that later batches moved or
/// deleted, which content-based idempotency cannot detect.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PageDelta {
    /// Logical page number inside the target file object.
    pub page_id: PageId,
    /// Initialization metadata for a page allocated by this batch.
    ///
    /// `Some` iff the page did not exist before this batch. Replay builds
    /// the page from scratch when it is absent from the data file; when the
    /// page is already present (flushed before a crash, or a repeated
    /// replay) the init is ignored and the records are applied with the
    /// same idempotent upsert semantics as an existing-page delta.
    pub init: Option<PLPageInit>,
    /// New prev/next chain links for an existing page.
    ///
    /// `None` when the batch did not change the page's links.
    pub links: Option<PLPageLinks>,
    /// Keys of records removed from the page.
    pub removes: Vec<PLRecordKey>,
    /// Records inserted or payload-replaced, in page slot order.
    pub upserts: Vec<PLRecord>,
}

/// Initialization metadata for a newly allocated page.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLPageInit {
    /// Prev-page chain link (`NONE_PAGE_ID` when the page is the head).
    pub prev_page: PageId,
    /// Next-page chain link (`NONE_PAGE_ID` when the page is the tail).
    pub next_page: PageId,
    /// Tuple format version written into the page header.
    pub tuple_format_version: u32,
    /// Tuple schema hash written into the page header.
    pub tuple_schema_hash: u64,
    /// Tuple flags written into the page header.
    pub tuple_flags: u64,
}

/// A prev/next chain-link update for an existing page.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLPageLinks {
    /// New prev-page link (`NONE_PAGE_ID` for none).
    pub prev_page: PageId,
    /// New next-page link (`NONE_PAGE_ID` for none).
    pub next_page: PageId,
}

/// The `(timestamp, tuple_id)` key identifying one time-series record.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLRecordKey {
    pub timestamp: u64,
    pub tuple_id: u64,
}

/// One record carried by a [`PageDelta`].
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLRecord {
    pub timestamp: u64,
    pub tuple_id: u64,
    // Serialized as one msgpack binary blob instead of a per-element
    // sequence; a per-byte encoding would cost one serializer call per
    // payload byte on every WAL append.
    #[serde(with = "payload_serde")]
    pub payload: Vec<u8>,
}

/// Serde helper for [`PLRecord::payload`]: encodes the bytes with
/// `serialize_bytes` (one msgpack `bin` object, one bulk copy).
mod payload_serde {
    use serde::de::Visitor;
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(data: &[u8], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_bytes(data)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<u8>, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct DataVisitor;

        impl Visitor<'_> for DataVisitor {
            type Value = Vec<u8>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a binary blob")
            }

            fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
                Ok(value.to_vec())
            }

            fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
                Ok(value)
            }
        }

        deserializer.deserialize_bytes(DataVisitor)
    }
}
