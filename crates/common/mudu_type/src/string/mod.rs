use crate::data_type::DataType;
use crate::data_type_impl::data_type_create;

pub fn new_array_type(opt_length: Option<u32>) -> DataType {
    data_type_create::create_string_type(opt_length)
}
