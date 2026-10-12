use crate::data_type::DataType;
use crate::type_family::TypeFamily;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

impl DataTypeInfo {
    /// Returns the serializable descriptor of a type object.
    pub fn from_opt_object(param: &DataType) -> Self {
        param.to_info()
    }

    /// Creates a descriptor from a type family and a JSON parameter string.
    pub fn from_text(data_type_id: TypeFamily, params: String) -> Self {
        Self {
            id: data_type_id,
            param: params,
        }
    }
    /// Parses this descriptor back into a concrete type object.
    pub fn to_data_type(&self) -> RS<DataType> {
        let ty = DataType::from_info(self)
            .map_err(|_e| mudu_error!(ErrorCode::TypeConversionFailed, "parse parameter error"))?;
        Ok(ty)
    }
}

/// Serializable descriptor of a data type: the type family plus the
/// JSON-encoded parameter object (empty when the type has no parameter).
#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct DataTypeInfo {
    /// The type family identifier.
    pub id: TypeFamily,
    /// The JSON-encoded type parameter, or empty if the type has none.
    pub param: String,
}
