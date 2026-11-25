use crate::type_family::TypeFamily;
use serde::{Deserialize, Serialize};

/// Serializable descriptor of a data type: the type family plus the
/// JSON-encoded parameter object (empty when the type has no parameter).
#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct DataTypeInfo {
    /// The type family identifier.
    pub id: TypeFamily,
    /// The JSON-encoded type parameter, or empty if the type has none.
    pub param: String,
}

impl DataTypeInfo {
    /// Creates a descriptor from a type family and a JSON parameter string.
    pub fn from_text(data_type_id: TypeFamily, params: String) -> Self {
        Self {
            id: data_type_id,
            param: params,
        }
    }
}
