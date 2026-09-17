//! `database::test_command_in` module.
#![allow(missing_docs)]

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_desc::DatumDesc;
    use mudu::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
    use mudu::utils::json::{from_json_str, to_json_str};
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    #[test]
    fn test_datum_desc() {
        let desc = DatumDesc::new("id".to_string(), DataType::default_for(TypeFamily::String));
        let json = to_json_str(&desc).expect("Serializing into string failed");
        let command_in = from_json_str::<DatumDesc>(&json).expect("json deserialization failed");
        let vec = serialize_sized_to_vec::<_>(&command_in).unwrap();
        let (command_in_1, _n) = deserialize_sized_from::<DatumDesc>(vec.as_slice()).unwrap();
        println!("{:?}", serde_json::to_string_pretty(&command_in_1).unwrap());
    }
}
