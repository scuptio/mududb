use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::param_info::ParamInfo;
use mudu::data_type::param_obj::ParamObj;

#[derive(Clone, Debug)]
pub struct TypeDeclare {
    id: DatTypeID,
    param: ParamObj,
}

impl TypeDeclare {
    pub fn new(param: ParamObj) -> Self {
        Self {
            id: param.dat_type_id(),
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
