use crate::dat_type::DatType;
use crate::dt_impl::dt_create;

pub fn new_record_type(name: String, fields: Vec<(String, DatType)>) -> DatType {
    dt_create::create_object_type(name, fields)
}
