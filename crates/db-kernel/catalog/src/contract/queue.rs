//! Queue system-column layout shared by the SQL binder, the command
//! executors, and the queue runtime slices.
//!
//! A queue is stored as a [`crate::contract::schema_class::SchemaClass`]
//! carrying [`crate::contract::schema_class::SchemaClassKind::Queue`] with
//! this fixed leading column layout:
//!
//! - key: `__seq` — per-shard monotonic sequence number assigned at enqueue
//!   (see the shard-bit layout below).
//! - value headers: `__lsn` (commit LSN of the enqueue), `status`
//!   ([`crate::contract::queue::QUEUE_STATUS_READY`] / [`crate::contract::queue::QUEUE_STATUS_CLAIMED`] /
//!   [`crate::contract::queue::QUEUE_STATUS_STATE`]), `receive_count` (how often the message was
//!   claimed), `__state_hi`/`__state_lo` (NULL on message rows; the shard
//!   watermark words on the sentinel row).
//! - payload columns follow the system columns in declaration order.
//!
//! The logical widths from the queue RFC (`u64`/`u8`/`u16`) map onto the
//! available storage type families: `__seq`/`__lsn` use `I64` (sequences are
//! non-negative), `status`/`receive_count` use `I32`.
//!
//! # Sharded queues
//!
//! A queue created with `PARTITION BY GLOBAL RULE ... REFERENCES (<col>)` is
//! sharded by the payload column `<col>`: shard *i* is the partition at index
//! *i* of the rule's partition list, and its rows live on that partition.
//! `__seq` embeds the shard index in its high bits
//! (`shard << QUEUE_SEQ_SHARD_SHIFT | local`), so the shard of any sequence
//! number — and therefore of any RECEIVE result or ACK target — is recovered
//! without extra bookkeeping. An unpartitioned queue uses shard 0, which
//! leaves its sequence numbers bit-identical to the pre-sharding layout.
//!
//! Each shard keeps its transactional watermarks (next sequence, acked
//! frontier, depth, bytes) on a **sentinel row** inside its own partition:
//! the row whose local sequence is [`crate::contract::queue::QUEUE_SEQ_LOCAL_SENTINEL`], with
//! `status = QUEUE_STATUS_STATE` and the four counters packed big-endian into
//! `__state_hi` (`next`, `frontier`) and `__state_lo` (`depth`, `bytes`).
//! Keeping the watermark in relation rows (rather than a worker-KV entry)
//! lets enqueue/receive/ack transactions stay pure relation-write
//! transactions, which compose with the local, handoff, and cross-partition
//! commit paths.

use mudu::common::buf::Buf;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

use crate::contract::schema_class::{SchemaClass, SchemaClassKind};
use crate::contract::schema_column::SchemaColumn;
use crate::contract::table_desc::TableDesc;

/// Name of the queue primary-key column: the per-queue monotonic sequence.
pub const QUEUE_SEQ_COLUMN: &str = "__seq";
/// Name of the value header column carrying the commit LSN of the enqueue.
pub const QUEUE_LSN_COLUMN: &str = "__lsn";
/// Name of the value header column carrying the message status.
pub const QUEUE_STATUS_COLUMN: &str = "status";
/// Name of the value header column counting how often a message was claimed.
pub const QUEUE_RECEIVE_COUNT_COLUMN: &str = "receive_count";
/// Name of the hidden header column carrying the high watermark words
/// (`next`, `frontier`) of the shard sentinel row (NULL on message rows).
pub const QUEUE_STATE_HI_COLUMN: &str = "__state_hi";
/// Name of the hidden header column carrying the low watermark words
/// (`depth`, `bytes`) of the shard sentinel row (NULL on message rows).
pub const QUEUE_STATE_LO_COLUMN: &str = "__state_lo";

/// Attribute index of the `__seq` column (the only key column).
pub const QUEUE_SEQ_ATTR: AttrIndex = 0;
/// Attribute index of the `__lsn` column.
pub const QUEUE_LSN_ATTR: AttrIndex = 1;
/// Attribute index of the `status` column.
pub const QUEUE_STATUS_ATTR: AttrIndex = 2;
/// Attribute index of the `receive_count` column.
pub const QUEUE_RECEIVE_COUNT_ATTR: AttrIndex = 3;
/// Attribute index of the hidden `__state_hi` column.
pub const QUEUE_STATE_HI_ATTR: AttrIndex = 4;
/// Attribute index of the hidden `__state_lo` column.
pub const QUEUE_STATE_LO_ATTR: AttrIndex = 5;
/// Attribute index of the first payload column.
pub const QUEUE_PAYLOAD_ATTR_START: AttrIndex = 6;

/// Message status: ready to be received.
pub const QUEUE_STATUS_READY: u8 = 0;
/// Message status: claimed by a RECEIVE, awaiting ACK or abort.
pub const QUEUE_STATUS_CLAIMED: u8 = 1;
/// Sentinel-row status: the row carries the shard watermarks in
/// `__state_hi`/`__state_lo` and is never delivered.
pub const QUEUE_STATUS_STATE: u8 = 2;

/// Number of high `__seq` bits holding the shard index; the remaining low
/// bits hold the per-shard local sequence.
pub const QUEUE_SEQ_SHARD_SHIFT: u32 = 52;
/// Mask extracting the local sequence from a `__seq` value.
pub const QUEUE_SEQ_LOCAL_MASK: u64 = (1 << QUEUE_SEQ_SHARD_SHIFT) - 1;
/// Local sequence value reserved for the shard sentinel row; usable message
/// sequences are strictly below it.
pub const QUEUE_SEQ_LOCAL_SENTINEL: u64 = QUEUE_SEQ_LOCAL_MASK;
/// Highest usable local message sequence of a shard.
pub const QUEUE_SEQ_LOCAL_MAX: u64 = QUEUE_SEQ_LOCAL_SENTINEL - 1;
/// Highest shard index a queue may use. `__seq` is an `I64` column, so the
/// composed value must stay below `2^63`: 52 local bits leave 11 usable
/// shard bits below the sign bit.
pub const QUEUE_SEQ_MAX_SHARD: u64 = (1 << (63 - QUEUE_SEQ_SHARD_SHIFT)) - 1;

/// Composes a `__seq` value from its shard index and local sequence.
pub fn queue_seq_encode(shard: u64, local: u64) -> RS<u64> {
    if shard > QUEUE_SEQ_MAX_SHARD {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("queue shard index {shard} exceeds {QUEUE_SEQ_MAX_SHARD}")
        ));
    }
    if local > QUEUE_SEQ_LOCAL_MAX {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("queue local sequence {local} exceeds {QUEUE_SEQ_LOCAL_MAX}")
        ));
    }
    Ok((shard << QUEUE_SEQ_SHARD_SHIFT) | local)
}

/// Extracts the shard index from a `__seq` value.
pub fn queue_seq_shard(seq: u64) -> u64 {
    seq >> QUEUE_SEQ_SHARD_SHIFT
}

/// Extracts the per-shard local sequence from a `__seq` value.
pub fn queue_seq_local(seq: u64) -> u64 {
    seq & QUEUE_SEQ_LOCAL_MASK
}

/// The `__seq` of the sentinel row of shard `shard`.
pub fn queue_sentinel_seq(shard: u64) -> RS<u64> {
    if shard > QUEUE_SEQ_MAX_SHARD {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("queue shard index {shard} exceeds {QUEUE_SEQ_MAX_SHARD}")
        ));
    }
    Ok((shard << QUEUE_SEQ_SHARD_SHIFT) | QUEUE_SEQ_LOCAL_SENTINEL)
}

/// Default queue depth cap (`queue_max_msgs` of the queue RFC): an enqueue
/// that would push `next_seq - acked_frontier` past this bound fails its
/// transaction with [`ErrorCode::QueueFull`].
// TODO(queue): wire to a `mudud.cfg` setting once the config plumbing reaches
// the worker contract layer; the RFC default is 65536.
pub const QUEUE_MAX_DEPTH_DEFAULT: u64 = 65536;

/// Poison-message threshold: a message whose `receive_count` exceeds this
/// value is moved to the queue's dead-letter queue at ACK time instead of
/// being deleted.
pub const QUEUE_POISON_RECEIVE_COUNT: i32 = 5;

/// Name of the dead-letter queue derived from the queue `name`.
pub fn queue_dlq_name(name: &str) -> String {
    format!("{name}_dlq")
}

/// Wait behavior of a queue `RECEIVE` statement (`RECEIVE ... WAIT n` /
/// `WAIT FOREVER`), carried from the bound statement into the executor.
///
/// WAIT is autocommit-only: the worker SQL layer rejects a RECEIVE carrying
/// it inside an explicit transaction with [`ErrorCode::InvalidState`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueWait {
    /// Wait at most this many milliseconds for the batch to fill up; on
    /// expiry the statement returns the messages claimed so far (possibly
    /// none).
    Timeout(u64),
    /// Wait without a deadline until the batch fills up.
    Forever,
}

/// Builds the schema of the dead-letter queue derived from the queue
/// `schema`: the same queue system columns and payload layout under the name
/// `<name>_dlq` in the same schema (namespace). The DLQ is a plain queue
/// object; it is created through the storage chain directly, so it never
/// derives a DLQ of its own.
pub fn queue_dlq_schema(schema: &SchemaClass) -> SchemaClass {
    let mut columns = queue_system_columns();
    columns.extend(schema.columns()[QUEUE_PAYLOAD_ATTR_START..].iter().cloned());
    // This schema is derived while applying a replicated command. No fresh
    // identity may escape into the durable catalog on replay or another node.
    let name = queue_dlq_name(schema.class_name());
    let columns: Vec<_> = columns
        .into_iter()
        .map(|column| {
            let mut derived = SchemaColumn::new_with_oid(
                crate::contract::derive_oid::derive_column_oid(
                    schema.schema(),
                    &name,
                    column.get_name(),
                ),
                column.get_name().clone(),
                column.type_id(),
                column.type_param().clone(),
            );
            derived.set_nullable(column.nullable());
            derived.set_fs_binding(column.fs_binding());
            derived
        })
        .collect();
    let value_indices = (1..columns.len()).collect();
    let mut dlq = SchemaClass::new_with_oid(
        crate::contract::derive_oid::derive_table_oid(schema.schema(), &name),
        name,
        columns,
        vec![QUEUE_SEQ_ATTR],
        value_indices,
    );
    dlq.set_schema(schema.schema().clone());
    dlq.set_kind(SchemaClassKind::Queue);
    dlq
}

/// Whether `name` is a queue system column name and therefore reserved
/// (rejected in queue payload column definitions).
pub fn is_queue_system_column(name: &str) -> bool {
    matches!(
        name,
        QUEUE_SEQ_COLUMN
            | QUEUE_LSN_COLUMN
            | QUEUE_STATUS_COLUMN
            | QUEUE_RECEIVE_COUNT_COLUMN
            | QUEUE_STATE_HI_COLUMN
            | QUEUE_STATE_LO_COLUMN
    )
}

/// Builds the queue system columns in layout order: the `__seq` key column,
/// then the `__lsn`, `status`, `receive_count` value header columns, then the
/// hidden nullable `__state_hi`/`__state_lo` watermark words (NULL on message
/// rows; the shard watermarks on the sentinel row).
///
/// The returned columns are in original column order; `__seq` is marked
/// non-nullable here and [`crate::contract::schema_class::SchemaClass::new`]
/// normalizes its key membership when the queue schema is assembled.
pub fn queue_system_columns() -> Vec<SchemaColumn> {
    let system = [
        (QUEUE_SEQ_COLUMN, TypeFamily::I64),
        (QUEUE_LSN_COLUMN, TypeFamily::I64),
        (QUEUE_STATUS_COLUMN, TypeFamily::I32),
        (QUEUE_RECEIVE_COUNT_COLUMN, TypeFamily::I32),
    ];
    let mut columns: Vec<SchemaColumn> = system
        .into_iter()
        .map(|(name, family)| {
            let ty = DataType::default_for(family);
            let mut column = SchemaColumn::new(
                name.to_string(),
                family,
                mudu_type::data_type_codec::from_opt_object(&ty),
            );
            column.set_nullable(false);
            column
        })
        .collect();
    for name in [QUEUE_STATE_HI_COLUMN, QUEUE_STATE_LO_COLUMN] {
        let ty = DataType::default_for(TypeFamily::U128);
        columns.push(SchemaColumn::new(
            name.to_string(),
            TypeFamily::U128,
            mudu_type::data_type_codec::from_opt_object(&ty),
        ));
    }
    columns
}

/// Result-set descriptor of a `RECEIVE` statement over the queue `desc`:
/// the `__seq` and `__lsn` envelope columns (both NOT NULL) followed by the
/// payload columns in declaration order (`status`/`receive_count` stay
/// internal and are not returned).
pub fn queue_envelope_tuple_desc(desc: &TableDesc) -> TupleFieldDesc {
    let mut fields = Vec::with_capacity(2 + desc.fields().len());
    for attr in [QUEUE_SEQ_ATTR, QUEUE_LSN_ATTR] {
        let field = desc.get_attr(attr);
        fields.push(DatumDesc::new_nullable(
            field.name().clone(),
            field.type_desc().clone(),
            false,
        ));
    }
    for attr in QUEUE_PAYLOAD_ATTR_START..desc.fields().len() {
        let field = desc.get_attr(attr);
        fields.push(DatumDesc::new_nullable(
            field.name().clone(),
            field.type_desc().clone(),
            field.nullable(),
        ));
    }
    TupleFieldDesc::new(fields)
}

fn encode_i64_datum(value: i64) -> RS<Buf> {
    mudu_type::data_type_function::send_binary(
        &DataValue::from_i64(value),
        &DataType::default_for(TypeFamily::I64),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode queue i64 datum error", e))
}

fn encode_i32_datum(value: i32) -> RS<Buf> {
    mudu_type::data_type_function::send_binary(
        &DataValue::from_i32(value),
        &DataType::default_for(TypeFamily::I32),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode queue i32 datum error", e))
}

fn decode_i64_datum(bin: &[u8]) -> RS<i64> {
    let value = TypedBin::new(TypeFamily::I64, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::I64))?;
    Ok(value.to_i64())
}

fn decode_i32_datum(bin: &[u8]) -> RS<i32> {
    let value = TypedBin::new(TypeFamily::I32, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::I32))?;
    Ok(value.to_i32())
}

/// Encodes a `__seq` value into the key column's binary layout.
pub fn queue_seq_datum(seq: u64) -> RS<Buf> {
    let value = i64::try_from(seq)
        .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, "queue seq overflow", e))?;
    encode_i64_datum(value)
}

/// Decodes a `__seq` key column datum.
pub fn decode_queue_seq(bin: &[u8]) -> RS<u64> {
    u64::try_from(decode_i64_datum(bin)?)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "negative queue seq", e))
}

/// Encodes a `__lsn` value into its column's binary layout.
pub fn queue_lsn_datum(lsn: u64) -> RS<Buf> {
    let value = i64::try_from(lsn)
        .map_err(|e| mudu_error!(ErrorCode::InvalidArgument, "queue lsn overflow", e))?;
    encode_i64_datum(value)
}

/// Decodes a `__lsn` column datum.
pub fn decode_queue_lsn(bin: &[u8]) -> RS<u64> {
    u64::try_from(decode_i64_datum(bin)?)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "negative queue lsn", e))
}

/// Encodes a `status` value into its column's binary layout.
pub fn queue_status_datum(status: u8) -> RS<Buf> {
    encode_i32_datum(i32::from(status))
}

/// Decodes a `status` column datum.
pub fn decode_queue_status(bin: &[u8]) -> RS<u8> {
    u8::try_from(decode_i32_datum(bin)?)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "invalid queue status", e))
}

/// Encodes a `receive_count` value into its column's binary layout.
pub fn queue_receive_count_datum(count: i32) -> RS<Buf> {
    encode_i32_datum(count)
}

/// Decodes a `receive_count` column datum.
pub fn decode_queue_receive_count(bin: &[u8]) -> RS<i32> {
    decode_i32_datum(bin)
}

/// Packs two watermark words into one `U128` datum (`hi` in the high half).
pub fn queue_state_word_datum(hi: u64, lo: u64) -> RS<Buf> {
    mudu_type::data_type_function::send_binary(
        &DataValue::from_u128(((hi as u128) << 64) | lo as u128),
        &DataType::default_for(TypeFamily::U128),
    )
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode queue state word error", e))
}

/// Decodes one `U128` state-word datum back into its `(hi, lo)` words.
pub fn decode_queue_state_word(bin: &[u8]) -> RS<(u64, u64)> {
    let value = TypedBin::new(TypeFamily::U128, bin.to_vec())
        .to_value(&DataType::default_for(TypeFamily::U128))?;
    let word = value.to_oid();
    Ok(((word >> 64) as u64, word as u64))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn seq_codec_roundtrip_and_layout() {
        assert_eq!(queue_seq_encode(0, 7).unwrap(), 7);
        let seq = queue_seq_encode(3, 42).unwrap();
        assert_eq!(queue_seq_shard(seq), 3);
        assert_eq!(queue_seq_local(seq), 42);
        // Unpartitioned queues (shard 0) keep the pre-sharding numbering.
        assert_eq!(queue_seq_encode(0, 0).unwrap(), 0);
        // Per-shard order is the order of the local part.
        assert!(queue_seq_encode(3, 1).unwrap() < queue_seq_encode(3, 2).unwrap());
    }

    #[test]
    fn seq_codec_rejects_out_of_range_parts() {
        assert!(queue_seq_encode(QUEUE_SEQ_MAX_SHARD + 1, 0).is_err());
        assert!(queue_seq_encode(0, QUEUE_SEQ_LOCAL_MAX + 1).is_err());
        assert!(queue_seq_encode(0, QUEUE_SEQ_LOCAL_SENTINEL).is_err());
        assert!(queue_sentinel_seq(QUEUE_SEQ_MAX_SHARD + 1).is_err());
    }

    #[test]
    fn sentinel_seq_sorts_past_every_message_seq_of_its_shard() {
        let sentinel = queue_sentinel_seq(2).unwrap();
        assert_eq!(queue_seq_shard(sentinel), 2);
        assert!(sentinel > queue_seq_encode(2, QUEUE_SEQ_LOCAL_MAX).unwrap());
        // The largest encodable seq still fits the I64 key column.
        assert!(i64::try_from(queue_sentinel_seq(QUEUE_SEQ_MAX_SHARD).unwrap()).is_ok());
        assert!(queue_seq_datum(queue_sentinel_seq(0).unwrap()).is_ok());
    }

    #[test]
    fn state_word_codec_roundtrip() {
        let datum = queue_state_word_datum(9, 4).unwrap();
        assert_eq!(decode_queue_state_word(&datum).unwrap(), (9, 4));
        let max = queue_state_word_datum(u64::MAX, u64::MAX).unwrap();
        assert_eq!(decode_queue_state_word(&max).unwrap(), (u64::MAX, u64::MAX));
    }

    #[test]
    fn lsn_codec_roundtrip() {
        let datum = queue_lsn_datum(42).unwrap();
        assert_eq!(decode_queue_lsn(&datum).unwrap(), 42);
        let max = queue_lsn_datum(i64::MAX as u64).unwrap();
        assert_eq!(decode_queue_lsn(&max).unwrap(), i64::MAX as u64);
        // A negative encoded value is rejected by the decode guard.
        assert!(decode_queue_lsn(&encode_i64_datum(-1).unwrap()).is_err());
    }

    #[test]
    fn seq_datum_roundtrip() {
        let datum = queue_seq_datum(7).unwrap();
        assert_eq!(decode_queue_seq(&datum).unwrap(), 7);
        assert!(decode_queue_seq(&encode_i64_datum(-1).unwrap()).is_err());
    }

    #[test]
    fn status_codec_roundtrip() {
        for status in [QUEUE_STATUS_READY, QUEUE_STATUS_CLAIMED, QUEUE_STATUS_STATE] {
            let datum = queue_status_datum(status).unwrap();
            assert_eq!(decode_queue_status(&datum).unwrap(), status);
        }
        // Values outside the u8 range are rejected by the decode guard.
        assert!(decode_queue_status(&encode_i32_datum(256).unwrap()).is_err());
        assert!(decode_queue_status(&encode_i32_datum(-1).unwrap()).is_err());
    }

    #[test]
    fn receive_count_codec_roundtrip() {
        let datum = queue_receive_count_datum(5).unwrap();
        assert_eq!(decode_queue_receive_count(&datum).unwrap(), 5);
        let datum = queue_receive_count_datum(-1).unwrap();
        assert_eq!(decode_queue_receive_count(&datum).unwrap(), -1);
    }

    #[test]
    fn datum_encode_rejects_overflowing_seq_and_lsn() {
        assert!(queue_seq_datum(u64::MAX).is_err());
        assert!(queue_lsn_datum(u64::MAX).is_err());
    }

    #[test]
    fn queue_system_column_names_are_reserved() {
        for name in [
            QUEUE_SEQ_COLUMN,
            QUEUE_LSN_COLUMN,
            QUEUE_STATUS_COLUMN,
            QUEUE_RECEIVE_COUNT_COLUMN,
            QUEUE_STATE_HI_COLUMN,
            QUEUE_STATE_LO_COLUMN,
        ] {
            assert!(is_queue_system_column(name), "{name}");
        }
        assert!(!is_queue_system_column("payload"));
        assert!(!is_queue_system_column(""));
    }
}

#[cfg(test)]
mod dlq_tests;

mod drop;
pub use drop::resolve_queue_dlq_for_drop;
