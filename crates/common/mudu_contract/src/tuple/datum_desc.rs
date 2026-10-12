//! Description of a named, typed datum.

use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use serde::{Deserialize, Serialize};

/// Describes a data element with type information and name
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DatumDesc {
    data_type: DataType,
    name: String,
    #[serde(default)]
    nullable: bool,
}

impl DatumDesc {
    /// Creates a new DatumDesc with the given name and type declaration
    pub fn new(name: String, data_type: DataType) -> Self {
        Self {
            data_type,
            name,
            nullable: false,
        }
    }

    /// Creates a new DatumDesc with explicit nullability.
    pub fn new_nullable(name: String, data_type: DataType, nullable: bool) -> Self {
        Self {
            data_type,
            name,
            nullable,
        }
    }

    // -- Field accessors --

    /// Returns the name of the data element
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the data type
    pub fn data_type(&self) -> &DataType {
        &self.data_type
    }

    /// Returns the specific type identifier
    pub fn type_family(&self) -> TypeFamily {
        self.data_type.type_family()
    }

    /// Returns whether the datum may hold SQL NULL.
    pub fn nullable(&self) -> bool {
        self.nullable
    }

    /// Consumes `self` and returns the name and data type.
    pub fn into(self) -> (String, DataType) {
        (self.name, self.data_type)
    }
}
