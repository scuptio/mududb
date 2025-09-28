use crate::data_type::dat_type::DatType;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::param_obj::ParamObj;
use serde::{Deserialize, Serialize};

#[derive(
    Clone,
    Debug,
    Serialize, Deserialize,
)]
pub struct DatumDesc {
    dat_type: DatType,
    name: String,
}


impl DatumDesc {
    pub fn new(name: String, type_declare: DatType) -> Self {
        Self {
            dat_type: type_declare,
            name,
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn type_declare(&self) -> &DatType {
        &self.dat_type
    }

    pub fn dat_type_param(&self) -> &ParamObj {
        self.dat_type.param()
    }

    pub fn dat_type_id(&self) -> DatTypeID {
        self.dat_type.param().dat_type_id()
    }
}