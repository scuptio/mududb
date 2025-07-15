use std::any::Any;
use std::fmt;

use crate::data_type::dt_impl::dat_table::get_fn_param;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;

use serde::{Deserialize, Serialize};
use std::hash::Hash;
use std::sync::Arc;

#[derive(Eq, PartialEq, Clone, Debug, Hash, Serialize, Deserialize)]
pub struct ParamInfo {
    pub type_id: DatTypeID,
    pub params: Vec<String>,
}

/// Data type param object
#[derive(Clone, Debug)]
pub struct ParamObj {
    type_id: DatTypeID,
    // A vector of param in text format
    params: Vec<String>,
    // dynamic object for param type
    param_obj: Arc<dyn Any>,
}

unsafe impl Send for ParamObj {}

unsafe impl Sync for ParamObj {}

#[derive(Debug)]
pub enum ErrParam {
    ParamParseError(String),
}
#[derive(Clone, Debug)]
pub struct FnParam {
    pub input: FnParamIn,
}

pub type FnParamIn = fn(params: &Vec<String>) -> Result<ParamObj, ErrParam>;

impl fmt::Display for FnParam {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_fmt(format_args!("{:?}", self))
    }
}

impl ParamInfo {
    pub fn from_opt_object(param: &ParamObj) -> Self {
        param.to_info()
    }

    pub fn from_text(data_type_id: DatTypeID, params: Vec<String>) -> Self {
        Self {
            type_id: data_type_id,
            params,
        }
    }
    pub fn to_object(&self) -> ParamObj {
        if let Ok(p) = ParamObj::from_info(self) {
            p
        } else {
            ParamObj::from(self.type_id.clone(), vec![], ())
        }
    }
}

impl ParamObj {
    pub fn default_for(type_id:DatTypeID) -> ParamObj {
        ParamObj {
            type_id,
            params: vec![],
            param_obj: Arc::new(()),
        }
    }
    
    pub fn data_type_id(&self) -> DatTypeID {
        self.type_id
    }

    pub fn from_no_params(id: DatTypeID) -> ParamObj {
        ParamObj::from(id, vec![], ())
    }

    pub fn from_info(info: &ParamInfo) -> Result<Self, ErrParam> {
        let opt_param = get_fn_param(info.type_id.to_u32());
        if let Some(fn_param) = opt_param {
            (fn_param.input)(&info.params)
        } else {
            Ok(ParamObj::from(info.type_id, vec![], ()))
        }
    }

    pub fn from<P: Clone + Any + 'static>(
        data_type_id: DatTypeID,
        params: Vec<String>,
        any: P,
    ) -> Self {
        Self {
            type_id: data_type_id,
            params,
            param_obj: Arc::new(any),
        }
    }

    pub fn to_info(&self) -> ParamInfo {
        ParamInfo {
            type_id: self.type_id,
            params: self.params.clone(),
        }
    }

    pub fn object<P: Any + Clone + 'static>(&self) -> Option<P> {
        let opt_p = self.param_obj.downcast_ref::<P>();
        opt_p.cloned()
    }
}
