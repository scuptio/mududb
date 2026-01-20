use crate::dt_fn_param::DatType;
use crate::dtp_array::DTPArray;
use crate::dtp_object::DTPRecord;
use crate::dtp_string::DTPString;

pub fn create_string_type(opt_length: Option<u32>) -> DatType {
    DatType::from_string(DTPString::new(opt_length.unwrap_or(0)))
}
pub fn create_array_type(inner_type: DatType) -> DatType {
    DatType::from_array(DTPArray::new(inner_type))
}

pub fn create_object_type(name: String, fields: Vec<(String, DatType)>) -> DatType {
    DatType::from_record(DTPRecord::new(name, fields))
}
