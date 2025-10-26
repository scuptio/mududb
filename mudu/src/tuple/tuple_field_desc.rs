use crate::{
    data_type::type_desc::TypeDesc,
    tuple::{datum_desc::DatumDesc, tuple_binary_desc::TupleBinaryDesc},
};
use serde::{Deserialize, Serialize};

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

    /// Returns a reference to the field descriptions
    pub fn fields(&self) -> &[DatumDesc] {
        &self.fields
    }

    /// Converts to a binary tuple description with index mapping
    /// Returns a tuple of (binary_descriptor, original_to_normalized_index_mapping)
    pub fn to_tuple_binary_desc(&self) -> (TupleBinaryDesc, Vec<usize>) {
        let type_descs_with_indices: Vec<(TypeDesc, usize)> = self
            .fields
            .iter()
            .enumerate()
            .map(|(original_index, field_desc)| {
                let type_desc =
                    TypeDesc::new(field_desc.dat_type_id(), field_desc.dat_type().param_info());
                (type_desc, original_index)
            })
            .collect();

        let (normalized_type_descs, index_mapping) =
            TupleBinaryDesc::normalized_type_desc_vec(type_descs_with_indices);

        let binary_desc = TupleBinaryDesc::from(normalized_type_descs);
        (binary_desc, index_mapping)
    }
}

impl AsRef<TupleFieldDesc> for TupleFieldDesc {
    fn as_ref(&self) -> &TupleFieldDesc {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        common::serde_utils::{deserialize_from_json, serialize_to_json},
        data_type::{dat_type::DatType, dt_impl::dat_type_id::DatTypeID},
        tuple::datum_desc::DatumDesc,
    };

    #[test]
    fn test_serialization_round_trip() {
        let fields = vec![
            DatumDesc::new("c1".to_string(), DatType::new_with_no_param(DatTypeID::I32)),
            DatumDesc::new("c2".to_string(), DatType::new_with_no_param(DatTypeID::I64)),
            DatumDesc::new("c3".to_string(), DatType::new_with_no_param(DatTypeID::I32)),
        ];

        let original_desc = TupleFieldDesc::new(fields);
        let json = serialize_to_json(&original_desc).unwrap();
        println!("Serialized JSON:\n{}", json);

        let deserialized_desc: TupleFieldDesc = deserialize_from_json(&json).unwrap();
        assert_eq!(
            original_desc.fields().len(),
            deserialized_desc.fields().len()
        );
    }
}
