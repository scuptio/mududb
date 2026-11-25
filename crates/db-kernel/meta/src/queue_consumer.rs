//! The `_queue_consumer` system table.
//!
//! One row per named queue consumer cursor per shard
//! (`CREATE CONSUMER cg ON q`): the key is `(queue_oid, shard, consumer)`,
//! the value is the cursor frontier in "every local `__seq` below it is
//! acknowledged" units — the next local sequence the cursor will receive on
//! that shard (a fresh cursor starts at 0, so it sees the whole shard
//! including local `__seq` 0). `ACK q FOR cg TO seq` advances the frontier of
//! the shard embedded in `seq` to `max(frontier, local(seq) + 1)`. Rows are
//! ordinary transactional DML writes, so cursor identity and frontier are
//! durable across restarts and ride the XL replication stream like any other
//! relation write (unlike catalog objects, which are create/drop-only and do
//! not replicate). A queue is in fan-out mode exactly while it has at least
//! one row here; with no rows the default competing cursor applies.

use std::sync::Arc;

use mudu::common::buf::Buf;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

use ::catalog::contract::schema_class::SchemaClass;
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::table_info::TableInfo;

use crate::fs_object::{decode_fs_oid_datum, encode_fs_oid_datum};

/// Fixed table oid of the `_queue_consumer` system table.
pub const QUEUE_CONSUMER_TABLE_ID: OID = 0x7;
/// Name of the `_queue_consumer` system table.
pub(crate) const QUEUE_CONSUMER_TABLE_NAME: &str = "_queue_consumer";
const QUEUE_CONSUMER_OID_COLUMN_ID: OID = 0x70001;
const QUEUE_CONSUMER_NAME_COLUMN_ID: OID = 0x70002;
const QUEUE_CONSUMER_FRONTIER_COLUMN_ID: OID = 0x70003;
const QUEUE_CONSUMER_SHARD_COLUMN_ID: OID = 0x70004;

/// Attribute index of the queue oid key column.
pub const QUEUE_CONSUMER_OID_ATTR: AttrIndex = 0;
/// Attribute index of the shard index key column.
pub const QUEUE_CONSUMER_SHARD_ATTR: AttrIndex = 1;
/// Attribute index of the consumer name key column.
pub const QUEUE_CONSUMER_NAME_ATTR: AttrIndex = 2;
/// Attribute index of the frontier value column.
pub const QUEUE_CONSUMER_FRONTIER_ATTR: AttrIndex = 3;

/// Build the schema of the `_queue_consumer` system table.
///
/// Public so test fixtures (e.g. `catalog-testing`'s `TestMetaMgr`) can
/// register the built-in table the way `MetaMgrImpl::initialize_inner` does.
pub fn queue_consumer_schema() -> SchemaClass {
    let columns = [
        (QUEUE_CONSUMER_OID_COLUMN_ID, "queue_oid", TypeFamily::U128),
        (QUEUE_CONSUMER_SHARD_COLUMN_ID, "shard", TypeFamily::I64),
        (
            QUEUE_CONSUMER_NAME_COLUMN_ID,
            "consumer",
            TypeFamily::String,
        ),
        (
            QUEUE_CONSUMER_FRONTIER_COLUMN_ID,
            "frontier",
            TypeFamily::I64,
        ),
    ]
    .into_iter()
    .map(|(oid, name, family)| {
        let ty = DataType::default_for(family);
        let mut column = SchemaColumn::new_with_oid(
            oid,
            name.to_string(),
            family,
            mudu_type::data_type_codec::from_opt_object(&ty),
        );
        column.set_nullable(false);
        column
    })
    .collect();
    SchemaClass::new_with_oid(
        QUEUE_CONSUMER_TABLE_ID,
        QUEUE_CONSUMER_TABLE_NAME.to_string(),
        columns,
        vec![0, 1, 2],
        vec![3],
    )
}

/// Build the table descriptor of the `_queue_consumer` system table.
pub fn queue_consumer_desc() -> RS<Arc<TableDesc>> {
    TableInfo::new(queue_consumer_schema())?.table_desc()
}

/// Encode a queue oid into the binary datum of the `queue_oid` column.
pub fn queue_consumer_oid_datum(queue_oid: OID) -> Buf {
    encode_fs_oid_datum(queue_oid)
}

/// Decode a `queue_oid` column datum.
pub fn decode_queue_consumer_oid(bin: &[u8]) -> RS<OID> {
    decode_fs_oid_datum(bin)
}

/// Encode a consumer name into the binary datum of the `consumer` column.
pub fn queue_consumer_name_datum(name: &str) -> RS<Buf> {
    mudu_type::data_type_function::send_binary(
        &DataValue::from_string(name.to_string()),
        &DataType::default_for(TypeFamily::String),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode queue consumer name error", e))
}

/// Decode a `consumer` column datum.
pub fn decode_queue_consumer_name(bin: &[u8]) -> RS<String> {
    let value = TypedBin::new(TypeFamily::String, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::String))?;
    Ok(value.expect_string().clone())
}

/// Encode a cursor frontier into the binary datum of the `frontier` column.
pub fn queue_consumer_frontier_datum(frontier: u64) -> RS<Buf> {
    let value = i64::try_from(frontier)
        .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, "consumer frontier overflow", e))?;
    mudu_type::data_type_function::send_binary(
        &DataValue::from_i64(value),
        &DataType::default_for(TypeFamily::I64),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode consumer frontier error", e))
}

/// Decode a `frontier` column datum.
pub fn decode_queue_consumer_frontier(bin: &[u8]) -> RS<u64> {
    let value = TypedBin::new(TypeFamily::I64, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::I64))?;
    u64::try_from(value.to_i64())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "negative consumer frontier", e))
}

/// Encode a shard index into the binary datum of the `shard` column.
pub fn queue_consumer_shard_datum(shard: u64) -> RS<Buf> {
    let value = i64::try_from(shard)
        .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, "consumer shard overflow", e))?;
    mudu_type::data_type_function::send_binary(
        &DataValue::from_i64(value),
        &DataType::default_for(TypeFamily::I64),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode consumer shard error", e))
}

/// Decode a `shard` column datum.
pub fn decode_queue_consumer_shard(bin: &[u8]) -> RS<u64> {
    let value = TypedBin::new(TypeFamily::I64, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::I64))?;
    u64::try_from(value.to_i64())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "negative consumer shard", e))
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
    fn queue_consumer_schema_has_fixed_ids() {
        let desc = queue_consumer_desc().unwrap();
        assert_eq!(desc.id(), QUEUE_CONSUMER_TABLE_ID);
        assert_eq!(desc.name(), QUEUE_CONSUMER_TABLE_NAME);
        assert_eq!(
            desc.key_field_oid(),
            &vec![
                QUEUE_CONSUMER_SHARD_COLUMN_ID,
                QUEUE_CONSUMER_OID_COLUMN_ID,
                QUEUE_CONSUMER_NAME_COLUMN_ID
            ],
            "key field oids in canonical tuple order"
        );
        assert_eq!(
            desc.value_field_oid(),
            &vec![QUEUE_CONSUMER_FRONTIER_COLUMN_ID]
        );
    }

    #[test]
    fn queue_consumer_datum_codecs_roundtrip() {
        let oid = queue_consumer_oid_datum(0xABCD);
        assert_eq!(decode_queue_consumer_oid(&oid).unwrap(), 0xABCD);

        let name = queue_consumer_name_datum("cg1").unwrap();
        assert_eq!(decode_queue_consumer_name(&name).unwrap(), "cg1");

        let frontier = queue_consumer_frontier_datum(42).unwrap();
        assert_eq!(decode_queue_consumer_frontier(&frontier).unwrap(), 42);
        assert!(decode_queue_consumer_frontier(&[1, 2, 3]).is_err());
    }
}
