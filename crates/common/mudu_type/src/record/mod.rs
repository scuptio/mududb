use crate::data_type::DataType;
use crate::data_type_impl::data_type_create;

/// Creates the record data type with type name `name` and named, typed
/// `fields`.
pub fn new_record_type(name: String, fields: Vec<(String, DataType)>) -> DataType {
    data_type_create::create_object_type(name, fields)
}
