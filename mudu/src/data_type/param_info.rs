use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::param_obj::ParamObj;
use serde::{Deserialize, Serialize};

impl ParamInfo {
    pub fn from_opt_object(param: &ParamObj) -> Self {
        param.to_info()
    }

    pub fn from_text(data_type_id: DatTypeID, params: Vec<String>) -> Self {
        Self {
            id: data_type_id,
            type_param: params,
        }
    }
    pub fn to_object(&self) -> ParamObj {
        if let Ok(p) = ParamObj::from_info(self) {
            p
        } else {
            ParamObj::from(self.id.clone(), vec![], ())
        }
    }
}

#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct ParamInfo {
    pub id: DatTypeID,
    pub type_param: Vec<String>,
}
