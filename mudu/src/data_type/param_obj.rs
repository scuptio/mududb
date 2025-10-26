use crate::data_type::dt_impl::dat_table::get_fn_param;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_param::ErrParam;
use crate::data_type::param_info::ParamInfo;
use serde::de::Error;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::any::Any;
use std::sync::Arc;

impl<'de> Deserialize<'de> for ParamObj {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let info: ParamInfo = Deserialize::deserialize(deserializer)?;
        Self::from_info(&info)
            .map_err(|e| Error::custom(format!("error deserializing param object: {:?}", e)))
    }
}

impl Serialize for ParamObj {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let info = self.to_info();
        serializer.serialize_some(&info)
    }
}

/// Data type param object
#[derive(Clone, Debug)]
pub struct ParamObj {
    id: DatTypeID,
    // A vector of parameter in text format
    dat_params: Vec<String>,
    // dynamic object for param type, it would be converted to its static type at runtime
    obj: Arc<dyn Any>,
}

unsafe impl Send for ParamObj {}

unsafe impl Sync for ParamObj {}

impl ParamObj {
    pub fn default_for(id: DatTypeID) -> ParamObj {
        let opt = id.opt_fn_param();
        let param_obj = match opt {
            Some(t) => (t.default)(),
            None => ParamObj::new_empty(id),
        };
        param_obj
    }

    pub fn dat_type_id(&self) -> DatTypeID {
        self.id
    }

    pub fn new_empty(id: DatTypeID) -> ParamObj {
        ParamObj::from(id, vec![], ())
    }

    pub fn is_empty(&self) -> bool {
        self.dat_params.is_empty()
    }

    pub fn from_info(info: &ParamInfo) -> Result<Self, ErrParam> {
        let opt_param = get_fn_param(info.id.to_u32());
        if let Some(fn_param) = opt_param {
            (fn_param.input)(&info.type_param)
        } else {
            Ok(ParamObj::from(info.id, vec![], ()))
        }
    }

    pub fn from<P: Clone + Any + 'static>(
        dat_type_id: DatTypeID,
        params: Vec<String>,
        any: P,
    ) -> Self {
        Self {
            id: dat_type_id,
            dat_params: params,
            obj: Arc::new(any),
        }
    }

    pub fn into_info(self) -> ParamInfo {
        ParamInfo {
            id: self.id,
            type_param: self.dat_params,
        }
    }

    pub fn to_info(&self) -> ParamInfo {
        ParamInfo {
            id: self.id,
            type_param: self.dat_params.clone(),
        }
    }

    pub fn object<P: Any + Clone + 'static>(&self) -> Option<P> {
        let opt_p = self.obj.downcast_ref::<P>();
        opt_p.cloned()
    }
}
