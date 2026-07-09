use crate::data_type::DataType;
use crate::type_family::TypeFamily;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

impl DataTypeInfo {
    pub fn from_opt_object(param: &DataType) -> Self {
        param.to_info()
    }

    pub fn from_text(data_type_id: TypeFamily, params: String) -> Self {
        Self {
            id: data_type_id,
            param: params,
        }
    }
    pub fn to_data_type(&self) -> RS<DataType> {
        let ty = DataType::from_info(self)
            .map_err(|_e| mudu_error!(ErrorCode::TypeConversionFailed, "parse parameter error"))?;
        Ok(ty)
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct DataTypeInfo {
    pub id: TypeFamily,
    pub param: String,
}
