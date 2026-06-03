use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

/// Logical WAL payload for inserting one tuple into a table.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLInsert {
    /// Target table object identifier.
    ///
    /// Recovery uses this to locate which table should receive the inserted
    /// tuple.
    pub table_id: OID,
    /// Physical partition identifier for relation rows.
    ///
    /// `0` is reserved for worker-local KV WAL records.
    pub partition_id: OID,
    /// Tuple identifier assigned to the inserted row version.
    ///
    /// This is the logical tuple id within the target table, not a physical
    /// page/slot address.
    pub tuple_id: u64,
    /// Primary lookup key or record key bytes for the tuple.
    ///
    /// This key is recorded in WAL so recovery can rebuild the same logical
    /// insert operation.
    pub key: Vec<u8>,
    /// Full value bytes of the tuple to insert.
    ///
    /// Unlike updates, inserts persist the complete row payload here.
    pub value: Vec<u8>,
}

/// Logical WAL payload for deleting one tuple from a table.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLDelete {
    /// Target table object identifier.
    pub table_id: OID,
    /// Physical partition identifier for relation rows.
    ///
    /// `0` is reserved for worker-local KV WAL records.
    pub partition_id: OID,
    /// Tuple identifier of the row version being deleted.
    pub tuple_id: u64,
    /// Key bytes of the tuple to delete.
    ///
    /// This allows recovery to identify the same logical record that was
    /// removed.
    pub key: Vec<u8>,
}

/// Logical WAL payload for updating one tuple in a table.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLUpdate {
    /// Target table object identifier.
    pub table_id: OID,
    /// Physical partition identifier for relation rows.
    pub partition_id: OID,
    /// Tuple identifier of the row version being updated.
    pub tuple_id: u64,
    /// Key bytes of the tuple to update.
    pub key: Vec<u8>,
    /// Encoded logical delta for the new tuple contents.
    ///
    /// This is not necessarily the full row image. It stores the change set
    /// needed to transform the previous value into the new value.
    pub delta: Vec<u8>,
}

#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum XLWrite {
    Insert(XLInsert),
    Update(XLUpdate),
    Delete(XLDelete),
}

impl XLWrite {
    pub fn table_id(&self) -> OID {
        match self {
            Self::Insert(write) => write.table_id,
            Self::Update(write) => write.table_id,
            Self::Delete(write) => write.table_id,
        }
    }

    pub fn partition_id(&self) -> OID {
        match self {
            Self::Insert(write) => write.partition_id,
            Self::Update(write) => write.partition_id,
            Self::Delete(write) => write.partition_id,
        }
    }
}
