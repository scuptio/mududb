use crate::data_type_fn_param::DataType;
use crate::data_type_param_array::DataTypeParamArray;
use crate::data_type_param_record::DataTypeParamRecord;
use crate::data_type_param_string::DataTypeParamString;

pub fn create_string_type(opt_length: Option<u32>) -> DataType {
    DataType::from_string(DataTypeParamString::new(opt_length.unwrap_or(0)))
}
pub fn create_array_type(inner_type: DataType) -> DataType {
    DataType::from_array(DataTypeParamArray::new(inner_type))
}

pub fn create_object_type(name: String, fields: Vec<(String, DataType)>) -> DataType {
    DataType::from_record(DataTypeParamRecord::new(name, fields))
}
