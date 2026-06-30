use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde::{Deserialize, Serialize};

impl DTInfo {
    pub fn from_opt_object(param: &DatType) -> Self {
        param.to_info()
    }

    pub fn from_text(data_type_id: DatTypeID, params: String) -> Self {
        Self {
            id: data_type_id,
            param: params,
        }
    }
    pub fn to_dat_type(&self) -> RS<DatType> {
        let ty = DatType::from_info(self)
            .map_err(|_e| mudu_error!(ErrorCode::TypeConversionFailed, "parse parameter error"))?;
        Ok(ty)
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct DTInfo {
    pub id: DatTypeID,
    pub param: String,
}
