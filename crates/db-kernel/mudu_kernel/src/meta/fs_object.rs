//! The `_fs_object` system table.
//!
//! Every object id stored in an FS-bound table column
//! (`SchemaColumn::fs_binding`) is backed by one row of the `_fs_object`
//! table. The row records the filesystem the object belongs to, its kind,
//! and a lifecycle state. Rows are staged into the transaction by the DML
//! hooks in `command::fs_hook` and become visible on commit like any other
//! relation write.

use std::sync::Arc;

use mudu::common::endian;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::build_tuple::build_tuple;
use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use serde::{Deserialize, Serialize};

use crate::contract::schema_column::SchemaColumn;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::table_info::TableInfo;

/// Fixed table oid of the `_fs_object` system table.
pub const FS_OBJECT_TABLE_ID: OID = 0x6;
/// Name of the `_fs_object` system table.
pub const FS_OBJECT_TABLE_NAME: &str = "_fs_object";
const FS_OBJECT_OID_COLUMN_ID: OID = 0x60001;
const FS_OBJECT_ENTRY_COLUMN_ID: OID = 0x60002;

/// State of a freshly created filesystem object that still accepts writes.
pub const FS_OBJECT_STATE_PENDING: u32 = 0;
/// State of a filesystem object whose content is finalized.
pub const FS_OBJECT_STATE_SEALED: u32 = 1;

/// Top-byte tag marking an oid as a filesystem object id.
const FS_OID_TAG: u128 = 0xF5;

/// Row payload stored in the `_fs_object` table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FsObjectRow {
    /// Filesystem id of the fs type the owning column binds to.
    pub fs_id: u64,
    /// Kind code of the fs type (`FsTypeKind::as_u32`).
    pub kind: u32,
    /// Object generation, incremented every time the object is rebound.
    pub generation: u64,
    /// Object content length in bytes.
    pub length: u64,
    /// Lifecycle state (`FS_OBJECT_STATE_PENDING` / `FS_OBJECT_STATE_SEALED`).
    pub state: u32,
}

/// Build the schema of the `_fs_object` system table.
pub fn fs_object_schema() -> SchemaTable {
    SchemaTable::new_with_oid(
        FS_OBJECT_TABLE_ID,
        FS_OBJECT_TABLE_NAME.to_string(),
        vec![
            SchemaColumn::new_with_oid(
                FS_OBJECT_OID_COLUMN_ID,
                "oid".to_string(),
                TypeFamily::U128,
                DataType::default_for(TypeFamily::U128).to_info(),
            ),
            SchemaColumn::new_with_oid(
                FS_OBJECT_ENTRY_COLUMN_ID,
                "entry".to_string(),
                TypeFamily::Binary,
                DataTypeInfo::from_text(TypeFamily::Binary, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

/// Build the table descriptor of the `_fs_object` system table.
pub fn fs_object_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(fs_object_schema())?.table_desc()
}

/// Generate a fresh filesystem object id.
///
/// The top 8 bits carry the `0xF5` tag so fs object ids are recognizable;
/// the remaining 120 bits are random.
pub fn gen_fs_oid() -> OID {
    (gen_oid() & !(0xFFu128 << 120)) | (FS_OID_TAG << 120)
}

/// Encode an object id into the binary datum stored in a `U128` column.
///
/// This is the same big-endian 16-byte form produced by the `U128` type
/// send function.
pub fn encode_fs_oid_datum(oid: OID) -> Vec<u8> {
    let mut buf = vec![0; size_of::<u128>()];
    endian::write_u128(&mut buf, oid);
    buf
}

/// Decode a binary `U128` datum back into an object id.
pub fn decode_fs_oid_datum(datum: &[u8]) -> RS<OID> {
    if datum.len() != size_of::<u128>() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!("fs object id datum must be 16 bytes, got {}", datum.len())
        ));
    }
    Ok(endian::read_u128(datum))
}

/// Encode an object id into an `_fs_object` key tuple.
pub fn encode_fs_object_key(oid: OID) -> RS<Vec<u8>> {
    let desc = fs_object_desc()?;
    build_tuple(&[encode_fs_oid_datum(oid)], desc.key_desc())
}

/// Decode an `_fs_object` key tuple back into an object id.
pub fn decode_fs_object_key(key: &[u8]) -> RS<OID> {
    let desc = fs_object_desc()?;
    let datum = desc.key_desc().get_field_desc(0).get(key)?;
    decode_fs_oid_datum(datum)
}

/// Encode an `_fs_object` row payload.
pub fn encode_fs_object_row(row: &FsObjectRow) -> RS<Vec<u8>> {
    rmp_serde::to_vec(row)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode fs object row error", e))
}

/// Decode an `_fs_object` row payload.
pub fn decode_fs_object_row(value: &[u8]) -> RS<FsObjectRow> {
    rmp_serde::from_slice(value)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode fs object row error", e))
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

    #[test]
    fn fs_oid_carries_tag_and_random_bits() {
        let oid = gen_fs_oid();
        assert_eq!(oid >> 120, FS_OID_TAG);
        let again = gen_fs_oid();
        assert_eq!(again >> 120, FS_OID_TAG);
        assert_ne!(oid, again);
    }

    #[test]
    fn fs_object_key_codec_roundtrip() {
        for oid in [0, 1, u128::MAX, gen_fs_oid()] {
            let key = encode_fs_object_key(oid).unwrap();
            assert_eq!(decode_fs_object_key(&key).unwrap(), oid);
        }
    }

    #[test]
    fn fs_object_row_codec_roundtrip() {
        let row = FsObjectRow {
            fs_id: 7,
            kind: 1,
            generation: 3,
            length: 42,
            state: FS_OBJECT_STATE_SEALED,
        };
        let value = encode_fs_object_row(&row).unwrap();
        assert_eq!(decode_fs_object_row(&value).unwrap(), row);
    }

    #[test]
    fn fs_oid_datum_codec_roundtrip() {
        let oid = gen_fs_oid();
        let datum = encode_fs_oid_datum(oid);
        assert_eq!(datum.len(), 16);
        assert_eq!(decode_fs_oid_datum(&datum).unwrap(), oid);
        assert!(decode_fs_oid_datum(&[1, 2, 3]).is_err());
    }

    #[test]
    fn fs_object_schema_has_fixed_ids() {
        let desc = fs_object_desc().unwrap();
        assert_eq!(desc.id(), FS_OBJECT_TABLE_ID);
        assert_eq!(desc.name(), FS_OBJECT_TABLE_NAME);
        assert_eq!(desc.key_field_oid(), &vec![FS_OBJECT_OID_COLUMN_ID]);
        assert_eq!(desc.value_field_oid(), &vec![FS_OBJECT_ENTRY_COLUMN_ID]);
    }
}
