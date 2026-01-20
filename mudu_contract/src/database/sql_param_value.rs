use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

use crate::database::sql_params::SQLParams;

pub struct SQLParamValue {
    param:Vec<DatValue>
}


impl SQLParams for SQLParamValue {
    fn size(&self) -> u64 {
        self.param.len() as u64
    }

    fn get_idx(&self, n: u64) -> Option<&dyn DatumDyn> {
        let dat_value = self.param.get(n as usize)?;
        Some(dat_value as _)
    }
}

impl SQLParamValue {
    pub fn into(self) -> Vec<DatValue> {
        self.param
    }

    pub fn params(&self) -> &[DatValue] {
        &self.param
    }
    pub fn from_vec(vec:Vec<DatValue>) -> Self {
        Self { param: vec }
    }
}
