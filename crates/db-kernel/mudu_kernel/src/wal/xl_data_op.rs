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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;

    fn sample_insert() -> XLInsert {
        XLInsert {
            table_id: 1,
            partition_id: 2,
            tuple_id: 3,
            key: vec![4],
            value: vec![5, 6],
        }
    }

    fn sample_update() -> XLUpdate {
        XLUpdate {
            table_id: 11,
            partition_id: 12,
            tuple_id: 13,
            key: vec![14],
            delta: vec![15, 16],
        }
    }

    fn sample_delete() -> XLDelete {
        XLDelete {
            table_id: 21,
            partition_id: 22,
            tuple_id: 23,
            key: vec![24],
        }
    }

    #[test]
    fn xl_insert_serializes_and_deserializes() {
        let orig = sample_insert();
        let encoded = rmp_serde::to_vec(&orig).unwrap();
        let decoded: XLInsert = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(orig, decoded);
    }

    #[test]
    fn xl_update_serializes_and_deserializes() {
        let orig = sample_update();
        let encoded = rmp_serde::to_vec(&orig).unwrap();
        let decoded: XLUpdate = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(orig, decoded);
    }

    #[test]
    fn xl_delete_serializes_and_deserializes() {
        let orig = sample_delete();
        let encoded = rmp_serde::to_vec(&orig).unwrap();
        let decoded: XLDelete = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(orig, decoded);
    }

    #[test]
    fn xl_write_table_id_returns_embedded_table_id() {
        assert_eq!(XLWrite::Insert(sample_insert()).table_id(), 1);
        assert_eq!(XLWrite::Update(sample_update()).table_id(), 11);
        assert_eq!(XLWrite::Delete(sample_delete()).table_id(), 21);
    }

    #[test]
    fn xl_write_partition_id_returns_embedded_partition_id() {
        assert_eq!(XLWrite::Insert(sample_insert()).partition_id(), 2);
        assert_eq!(XLWrite::Update(sample_update()).partition_id(), 12);
        assert_eq!(XLWrite::Delete(sample_delete()).partition_id(), 22);
    }
}
