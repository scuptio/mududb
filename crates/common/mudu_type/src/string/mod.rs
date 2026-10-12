use crate::data_type::DataType;
use crate::data_type_impl::data_type_create;

/// Creates the string (`varchar`) data type with an optional maximum length;
/// `None` (or `Some(0)`) means unlimited.
pub fn new_array_type(opt_length: Option<u32>) -> DataType {
    data_type_create::create_string_type(opt_length)
}
