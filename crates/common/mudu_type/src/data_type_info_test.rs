//! Tests for the serializable `DataTypeInfo` descriptor.

use crate::data_type::DataType;
use crate::data_type_codec::{from_opt_object, to_data_type};
use crate::data_type_info::DataTypeInfo;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::type_family::TypeFamily;

#[test]
fn data_type_info_roundtrips_between_object_and_text() {
    let data_type = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
    let from_object = from_opt_object(&data_type);

    let from_text = DataTypeInfo::from_text(from_object.id, from_object.param.clone());
    assert_eq!(from_object, from_text);

    let parsed = to_data_type(&from_text).unwrap();
    assert_eq!(from_opt_object(&parsed), from_object);
}

#[test]
fn data_type_info_rejects_unparseable_params() {
    let bad = DataTypeInfo::from_text(TypeFamily::Numeric, "not-a-json-object".to_string());
    assert!(to_data_type(&bad).is_err());
}
