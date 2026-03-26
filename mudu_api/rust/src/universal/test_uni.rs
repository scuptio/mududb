#[cfg(test)]
mod tests {
    use crate::universal::uni_dat_type::UniDatType;
    use crate::universal::uni_primitive::UniPrimitive;
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use mudu::common::serde_utils::{deserialize_from_json, serialize_to_json, serialize_to_vec};

    #[test]
    fn test_uni_dat_type() {
        let uni_dat_ty = UniDatType::Record(UniRecordType {
            record_name: "record".to_string(),
            record_fields: vec![UniRecordField {
                field_name: "field1".to_string(),
                field_type: UniDatType::Primitive(UniPrimitive::I16),
            }],
        });
        let json = serialize_to_json(&uni_dat_ty).unwrap();
        let binary = serialize_to_vec(&uni_dat_ty).unwrap();
        println!("{}", json);
        println!("{}", hex::encode(&binary));
        let uni_dat_ty2: UniDatType = deserialize_from_json(json.as_str()).unwrap();
        println!("{:?}", uni_dat_ty2);
    }
}
