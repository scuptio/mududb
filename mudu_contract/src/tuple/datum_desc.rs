use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use serde::{Deserialize, Serialize};

/// Describes a data element with type information and name
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatumDesc {
    dat_type: DatType,
    name: String,
}

impl DatumDesc {
    /// Creates a new DatumDesc with the given name and type declaration
    pub fn new(name: String, dat_type: DatType) -> Self {
        Self {
            dat_type,
            name,
        }
    }

    // -- Field accessors --

    /// Returns the name of the data element
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the data type
    pub fn dat_type(&self) -> &DatType {
        &self.dat_type
    }

    /// Returns the specific type identifier
    pub fn dat_type_id(&self) -> DatTypeID {
        self.dat_type.dat_type_id()
    }

    pub fn into(self) -> (String, DatType) {
        (self.name, self.dat_type)
    }
}
