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

/// Physical page-level operations captured in the log.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum PLOp {
    /// Create the target file object identified by [`PLEntry::file`].
    Create,
    /// Delete the target file object identified by [`PLEntry::file`].
    Delete,
    /// Apply an in-place byte-range update to one page in the file object.
    PageUpdate(PageUpdate),
}

/// A physical delta applied to a page in a file.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PageUpdate {
    /// Logical page number inside the target file object.
    ///
    /// Recovery uses this to locate which page should receive the byte patch.
    pub page_id: PageId,
    /// Byte offset from the start of the page where the patch begins.
    ///
    /// The update writes `data.len()` bytes starting at this offset.
    pub offset: u32,
    /// Replacement bytes to copy into the page at [`PageUpdate::offset`].
    ///
    /// This is a partial page image, not necessarily a full page. Callers must
    /// ensure `offset + data.len()` does not exceed the page size.
    pub data: Vec<u8>,
}
