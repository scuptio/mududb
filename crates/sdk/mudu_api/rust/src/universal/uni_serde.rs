//! Legacy serde implementations for the universal types with **JSON-facing**
//! consumers.
//!
//! The syscall wire (MSSP) no longer uses serde: generated records and
//! variants convert through [`crate::universal::mp_wire`] (`to_value` /
//! `from_value`) and the canonical MessagePack runtime. The impls in this
//! file exist only for the remaining serde consumers, which are all JSON:
//!
//! - the `syscall_schema` descriptor ([`UniSchemaDesc`], serialized by
//!   `mgen -t`), embedding [`UniDataType`];
//! - [`UniTypeDesc`] (type-description JSON documents);
//! - `mudu_client`'s JSON client, which stores [`UniDataValue`] as JSON
//!   bytes and embeds [`UniOid`] in its HTTP request/response JSON;
//! - the management API, which embeds [`UniOid`] in topology JSON;
//! - the procedure HTTP API, which renders [`UniProcedureResult`] as JSON.
//!
//! They reproduce the pre-refactor shapes exactly: records use
//! `serialize_struct` (a named-field object in JSON, a positional array in
//! binary formats), variants use the `[tag, payload]` 2-element array, and
//! enums keep their generated `serde_repr` impls. Do NOT use these impls for
//! the syscall wire: the wire record shape is the integer-keyed map produced
//! by `to_value`.

use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_data_value::{UniDataValue, UniDataValueField};
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_procedure_result::UniProcedureResult;
use crate::universal::uni_record_type::{UniFieldAttr, UniRecordField, UniRecordType};
use crate::universal::uni_result_type::UniResultType;
use crate::universal::uni_scalar::UniScalar;
use crate::universal::uni_scalar_value::UniScalarValue;
use serde::de::{MapAccess, SeqAccess, Visitor};
use serde::ser::{SerializeSeq, SerializeStruct};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;

// ---------------------------------------------------------------------------
// Records
// ---------------------------------------------------------------------------

macro_rules! impl_record_serde {
    (
        $name:ident, $len:expr,
        $(($field:ident, $fname:literal, $ty:ty, $index:expr)),+
    ) => {
        impl Serialize for $name {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                let mut serialize_struct =
                    serializer.serialize_struct(stringify!($name), $len)?;
                $(
                    serialize_struct.serialize_field($fname, &self.$field)?;
                )+
                serialize_struct.end()
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                struct RecordVisitor;

                impl<'de> Visitor<'de> for RecordVisitor {
                    type Value = $name;

                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        formatter.write_str(concat!("a ", stringify!($name), " (sequence or map)"))
                    }

                    fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                        let mut seq = seq;
                        $(
                            let $field = seq
                                .next_element::<$ty>()?
                                .ok_or_else(|| serde::de::Error::invalid_length($index, &self))?;
                        )+
                        Ok(Self::Value { $($field),+ })
                    }

                    fn visit_map<A: MapAccess<'de>>(self, map: A) -> Result<Self::Value, A::Error> {
                        let mut map = map;
                        $(
                            let mut $field = None;
                        )+
                        while let Some(key) = map.next_key::<String>()? {
                            match key.as_str() {
                                $(
                                    $fname => {
                                        $field = Some(map.next_value::<$ty>()?);
                                    }
                                )+
                                _ => {
                                    let _ = map.next_value::<serde::de::IgnoredAny>()?;
                                }
                            }
                        }
                        Ok(Self::Value {
                            $(
                                $field: $field.unwrap_or_default(),
                            )+
                        })
                    }
                }

                const FIELDS: &[&str] = &[$($fname),+];
                deserializer.deserialize_struct(stringify!($name), FIELDS, RecordVisitor)
            }
        }
    };
}

impl_record_serde!(UniOid, 2, (h, "h", u64, 0), (l, "l", u64, 1));

impl_record_serde!(
    UniFieldAttr,
    2,
    (attr_name, "attr_name", String, 0),
    (attr_value, "attr_value", String, 1)
);

impl_record_serde!(
    UniRecordField,
    3,
    (field_name, "field_name", String, 0),
    (field_type, "field_type", UniDataType, 1),
    (field_attrs, "field_attrs", Vec<UniFieldAttr>, 2)
);

impl_record_serde!(
    UniRecordType,
    2,
    (record_name, "record_name", String, 0),
    (record_fields, "record_fields", Vec<UniRecordField>, 1)
);

impl_record_serde!(
    UniResultType,
    2,
    (ok, "ok", Option<Box<UniDataType>>, 0),
    (err, "err", Option<Box<UniDataType>>, 1)
);

impl_record_serde!(
    UniDataValueField,
    2,
    (field_name, "field_name", String, 0),
    (field_value, "field_value", UniDataValue, 1)
);

impl_record_serde!(
    UniProcedureResult,
    1,
    (return_list, "return_list", Vec<UniDataValue>, 0)
);

// ---------------------------------------------------------------------------
// Variants
// ---------------------------------------------------------------------------

impl Serialize for UniDataType {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDataType::Scalar(inner) => {
                seq.serialize_element(&0u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Array(inner) => {
                seq.serialize_element(&1u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Record(inner) => {
                seq.serialize_element(&2u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Option(inner) => {
                seq.serialize_element(&3u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Tuple(inner) => {
                seq.serialize_element(&4u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Result(inner) => {
                seq.serialize_element(&5u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Identifier(inner) => {
                seq.serialize_element(&6u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataType::Binary => {
                seq.serialize_element(&7u32)?;
                seq.serialize_element(&0u8)?;
            }
            UniDataType::Box(inner) => {
                seq.serialize_element(&8u32)?;
                seq.serialize_element(inner)?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UniDataType {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniDataTypeVisitor;

        impl<'de> Visitor<'de> for UniDataTypeVisitor {
            type Value = UniDataType;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a [tag, payload] uni-data-type array")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                let mut seq = seq;
                let tag = variant_tag(&mut seq)?;
                match tag {
                    0 => Ok(UniDataType::Scalar(variant_payload(&mut seq)?)),
                    1 => Ok(UniDataType::Array(variant_payload(&mut seq)?)),
                    2 => Ok(UniDataType::Record(variant_payload(&mut seq)?)),
                    3 => Ok(UniDataType::Option(variant_payload(&mut seq)?)),
                    4 => Ok(UniDataType::Tuple(variant_payload(&mut seq)?)),
                    5 => Ok(UniDataType::Result(variant_payload(&mut seq)?)),
                    6 => Ok(UniDataType::Identifier(variant_payload(&mut seq)?)),
                    7 => {
                        variant_unit_payload(&mut seq)?;
                        Ok(UniDataType::Binary)
                    }
                    8 => Ok(UniDataType::Box(variant_payload(&mut seq)?)),
                    _ => Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(tag as u64),
                        &"a uni-data-type tag in 0..=8",
                    )),
                }
            }
        }

        deserializer.deserialize_seq(UniDataTypeVisitor)
    }
}

impl Serialize for UniScalarValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniScalarValue::Bool(inner) => {
                seq.serialize_element(&0u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::U8(inner) => {
                seq.serialize_element(&1u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::I8(inner) => {
                seq.serialize_element(&2u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::U16(inner) => {
                seq.serialize_element(&3u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::I16(inner) => {
                seq.serialize_element(&4u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::U32(inner) => {
                seq.serialize_element(&5u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::I32(inner) => {
                seq.serialize_element(&6u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::U64(inner) => {
                seq.serialize_element(&7u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::U128(inner) => {
                seq.serialize_element(&8u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::I64(inner) => {
                seq.serialize_element(&9u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::I128(inner) => {
                seq.serialize_element(&10u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::F32(inner) => {
                seq.serialize_element(&11u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::F64(inner) => {
                seq.serialize_element(&12u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Char(inner) => {
                seq.serialize_element(&13u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::String(inner) => {
                seq.serialize_element(&14u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Blob(inner) => {
                seq.serialize_element(&15u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Numeric(inner) => {
                seq.serialize_element(&16u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Date(inner) => {
                seq.serialize_element(&17u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Time(inner) => {
                seq.serialize_element(&18u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Timestamp(inner) => {
                seq.serialize_element(&19u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::TimestampTz(inner) => {
                seq.serialize_element(&20u32)?;
                seq.serialize_element(inner)?;
            }
            UniScalarValue::Null => {
                seq.serialize_element(&21u32)?;
                // Payload-less case: the wire shape is `[tag, 0u8]`.
                seq.serialize_element(&0u8)?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UniScalarValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniScalarValueVisitor;

        impl<'de> Visitor<'de> for UniScalarValueVisitor {
            type Value = UniScalarValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a [tag, payload] uni-scalar-value array")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                let mut seq = seq;
                let tag = variant_tag(&mut seq)?;
                match tag {
                    0 => Ok(UniScalarValue::Bool(variant_payload(&mut seq)?)),
                    1 => Ok(UniScalarValue::U8(variant_payload(&mut seq)?)),
                    2 => Ok(UniScalarValue::I8(variant_payload(&mut seq)?)),
                    3 => Ok(UniScalarValue::U16(variant_payload(&mut seq)?)),
                    4 => Ok(UniScalarValue::I16(variant_payload(&mut seq)?)),
                    5 => Ok(UniScalarValue::U32(variant_payload(&mut seq)?)),
                    6 => Ok(UniScalarValue::I32(variant_payload(&mut seq)?)),
                    7 => Ok(UniScalarValue::U64(variant_payload(&mut seq)?)),
                    8 => Ok(UniScalarValue::U128(variant_payload(&mut seq)?)),
                    9 => Ok(UniScalarValue::I64(variant_payload(&mut seq)?)),
                    10 => Ok(UniScalarValue::I128(variant_payload(&mut seq)?)),
                    11 => Ok(UniScalarValue::F32(variant_payload(&mut seq)?)),
                    12 => Ok(UniScalarValue::F64(variant_payload(&mut seq)?)),
                    13 => Ok(UniScalarValue::Char(variant_payload(&mut seq)?)),
                    14 => Ok(UniScalarValue::String(variant_payload(&mut seq)?)),
                    15 => Ok(UniScalarValue::Blob(variant_payload(&mut seq)?)),
                    16 => Ok(UniScalarValue::Numeric(variant_payload(&mut seq)?)),
                    17 => Ok(UniScalarValue::Date(variant_payload(&mut seq)?)),
                    18 => Ok(UniScalarValue::Time(variant_payload(&mut seq)?)),
                    19 => Ok(UniScalarValue::Timestamp(variant_payload(&mut seq)?)),
                    20 => Ok(UniScalarValue::TimestampTz(variant_payload(&mut seq)?)),
                    21 => {
                        variant_unit_payload(&mut seq)?;
                        Ok(UniScalarValue::Null)
                    }
                    _ => Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(tag as u64),
                        &"a uni-scalar-value tag in 0..=21",
                    )),
                }
            }
        }

        deserializer.deserialize_seq(UniScalarValueVisitor)
    }
}

impl Serialize for UniDataValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDataValue::Scalar(inner) => {
                seq.serialize_element(&0u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataValue::Array(inner) => {
                seq.serialize_element(&1u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataValue::Record(inner) => {
                seq.serialize_element(&2u32)?;
                seq.serialize_element(inner)?;
            }
            UniDataValue::Binary(inner) => {
                seq.serialize_element(&3u32)?;
                seq.serialize_element(inner)?;
            }
        }
        seq.end()
    }
}

impl<'de> Deserialize<'de> for UniDataValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct UniDataValueVisitor;

        impl<'de> Visitor<'de> for UniDataValueVisitor {
            type Value = UniDataValue;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a [tag, payload] uni-data-value array")
            }

            fn visit_seq<A: SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
                let mut seq = seq;
                let tag = variant_tag(&mut seq)?;
                match tag {
                    0 => Ok(UniDataValue::Scalar(variant_payload(&mut seq)?)),
                    1 => Ok(UniDataValue::Array(variant_payload(&mut seq)?)),
                    2 => Ok(UniDataValue::Record(variant_payload(&mut seq)?)),
                    3 => Ok(UniDataValue::Binary(variant_payload(&mut seq)?)),
                    _ => Err(serde::de::Error::invalid_value(
                        serde::de::Unexpected::Unsigned(tag as u64),
                        &"a uni-data-value tag in 0..=3",
                    )),
                }
            }
        }

        deserializer.deserialize_seq(UniDataValueVisitor)
    }
}

/// Reads the variant tag element of a `[tag, payload]` sequence.
fn variant_tag<'de, A: SeqAccess<'de>>(seq: &mut A) -> Result<u32, A::Error> {
    seq.next_element::<u32>()?
        .ok_or_else(|| serde::de::Error::invalid_length(0, &"a variant tag"))
}

/// Reads the payload element of a `[tag, payload]` sequence.
fn variant_payload<'de, A: SeqAccess<'de>, T: Deserialize<'de>>(
    seq: &mut A,
) -> Result<T, A::Error> {
    seq.next_element::<T>()?
        .ok_or_else(|| serde::de::Error::invalid_length(1, &"a variant payload"))
}

/// Reads and discards the `0u8` placeholder of a payload-less variant case.
fn variant_unit_payload<'de, A: SeqAccess<'de>>(seq: &mut A) -> Result<(), A::Error> {
    let _ = seq
        .next_element::<u8>()?
        .ok_or_else(|| serde::de::Error::invalid_length(1, &"a 0u8 placeholder"))?;
    Ok(())
}

// Keep the compile-time reference to the UniScalar serde_repr impl honest:
// the enum's Serialize/Deserialize impls come from the generated code.
const _: fn() = || {
    fn assert_serialize<T: Serialize>() {}
    fn assert_deserialize<'de, T: Deserialize<'de>>() {}
    assert_serialize::<UniScalar>();
    assert_deserialize::<UniScalar>();
};
