use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dt_info::DTInfo;

#[derive(Clone, Debug)]
pub struct TypeDeclare {
    id: DatTypeID,
    param: DatType,
}

impl TypeDeclare {
    pub fn new(param: DatType) -> Self {
        Self {
            id: param.dat_type_id(),
            param,
        }
    }

    pub fn id(&self) -> DatTypeID {
        self.id
    }

    pub fn param(&self) -> &DatType {
        &self.param
    }

    pub fn param_info(&self) -> DTInfo {
        self.param.to_info()
    }
}
