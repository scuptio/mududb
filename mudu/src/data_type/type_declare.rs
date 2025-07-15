use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_param::{ParamInfo, ParamObj};

#[derive(Clone, Debug)]
pub struct TypeDeclare {
    id: DatTypeID,
    param: ParamObj,
}

impl TypeDeclare {
    pub fn new(param: ParamObj) -> Self {
        Self {
            id: param.data_type_id(),
            param,
        }
    }

    pub fn id(&self) -> DatTypeID {
        self.id
    }

    pub fn param(&self) -> &ParamObj {
        &self.param
    }

    pub fn param_info(&self) -> ParamInfo {
        self.param.to_info()
    }
}