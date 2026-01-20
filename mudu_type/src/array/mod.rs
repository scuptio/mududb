pub mod dat_val_array;

use crate::dat_type::DatType;
use crate::dt_impl::dt_create;

pub fn new_array_type(inner_type: DatType) -> DatType {
    dt_create::create_array_type(inner_type)
}
