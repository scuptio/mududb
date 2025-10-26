use crate::data_type::{dat_type::DatType, dt_impl::dat_type_id::DatTypeID, param_obj::ParamObj};
use serde::{Deserialize, Serialize};

/// Describes a data element with type information and name
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatumDesc {
    dat_type: DatType,
    name: String,
}

impl DatumDesc {
    /// Creates a new DatumDesc with the given name and type declaration
    pub fn new(name: String, type_declare: DatType) -> Self {
        Self {
            dat_type: type_declare,
            name,
        }
    }

    // -- Field accessors --

    /// Returns the name of the data element
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the type declaration
    pub fn dat_type(&self) -> &DatType {
        &self.dat_type
    }

    // -- Type information accessors --

    /// Returns the parameter object for the data type
    pub fn param_obj(&self) -> &ParamObj {
        self.dat_type.param()
    }

    /// Returns the specific type identifier
    pub fn dat_type_id(&self) -> DatTypeID {
        self.dat_type.param().dat_type_id()
    }
}
