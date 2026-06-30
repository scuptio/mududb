//! `tuple::tuple_field_desc` module.
#![allow(missing_docs)]

use crate::tuple::{datum_desc::DatumDesc, tuple_binary_desc::TupleBinaryDesc};
use mudu::common::result::RS;
use mudu::common::serde_utils;
use mudu_type::dat_type::DatType;
use mudu_type::dtp_object::DTPRecord;
use serde::{Deserialize, Serialize};

type FieldMappingInfo = (usize, bool, Option<u16>);

/// Describes the structure and types of a tuple's elements
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleFieldDesc {
    fields: Vec<DatumDesc>,
}

impl TupleFieldDesc {
    /// Creates a new TupleItemDesc from a collection of field descriptions
    pub fn new(fields: Vec<DatumDesc>) -> Self {
        Self { fields }
    }

    pub fn into(self) -> Vec<DatumDesc> {
        self.fields
    }
    /// Returns a reference to the field descriptions
    pub fn fields(&self) -> &[DatumDesc] {
        &self.fields
    }

    pub fn into_fields(self) -> Vec<DatumDesc> {
        self.fields
    }

    /// Converts to a binary tuple description with index mapping
    /// Returns a tuple of (binary_descriptor, original_to_normalized_index_mapping)
    pub fn to_tuple_binary_desc(&self) -> RS<(TupleBinaryDesc, Vec<usize>)> {
        let mut nullable_count = 0usize;
        let mut null_bit_indices = Vec::with_capacity(self.fields.len());
        for field in &self.fields {
            if field.nullable() {
                let bit_idx = u16::try_from(nullable_count).map_err(|_| {
                    mudu::mudu_error!(
                        mudu::error::ErrorCode::Parse,
                        "nullable column count exceeds u16::MAX"
                    )
                })?;
                null_bit_indices.push(Some(bit_idx));
                nullable_count += 1;
            } else {
                null_bit_indices.push(None);
            }
        }

        let type_descs_with_indices: Vec<(DatType, FieldMappingInfo)> = self
            .fields
            .iter()
            .enumerate()
            .map(|(original_index, field_desc)| {
                let type_desc = field_desc.dat_type();
                (
                    type_desc.clone(),
                    (
                        original_index,
                        field_desc.nullable(),
                        null_bit_indices[original_index],
                    ),
                )
            })
            .collect();

        let (normalized_type_descs, normalized_payload) =
            TupleBinaryDesc::normalized_type_desc_vec(type_descs_with_indices)?;

        let index_mapping = normalized_payload
            .iter()
            .map(|(original_index, _, _)| *original_index)
            .collect::<Vec<_>>();
        let typed_fields = normalized_type_descs
            .into_iter()
            .zip(normalized_payload)
            .map(|(ty, (_, nullable, null_bit_idx))| (ty, nullable, null_bit_idx))
            .collect::<Vec<_>>();
        let binary_desc = TupleBinaryDesc::from_typed_fields(typed_fields, 1)?;
        Ok((binary_desc, index_mapping))
    }

    pub fn serialize_to(&self) -> RS<Vec<u8>> {
        let vec = serde_utils::serialize_sized_to_vec(self)?;
        Ok(vec)
    }

    pub fn deserialize_from(slice: &[u8]) -> RS<Self> {
        let (d, _) = serde_utils::deserialize_sized_from::<Self>(slice)?;
        Ok(d)
    }

    pub fn to_record_type(&self, name: String) -> RS<DatType> {
        let mut vec = Vec::with_capacity(self.fields.len());
        for d in self.fields.iter() {
            vec.push((d.name().to_string(), d.dat_type().clone()));
        }
        Ok(DatType::from_record(DTPRecord::new(name, vec)))
    }
}

impl AsRef<TupleFieldDesc> for TupleFieldDesc {
    fn as_ref(&self) -> &TupleFieldDesc {
        self
    }
}
