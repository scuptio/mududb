/// The `DataValueArray` typed homogeneous array value.
pub mod data_value_array;

use crate::data_type::DataType;
use crate::data_type_impl::data_type_create;

/// Creates the array data type with element type `inner_type`.
pub fn new_array_type(inner_type: DataType) -> DataType {
    data_type_create::create_array_type(inner_type)
}
