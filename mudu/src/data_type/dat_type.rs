use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::param_info::ParamInfo;
use crate::data_type::param_obj::ParamObj;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug)]
pub struct DatType {
    dat_type_obj: ParamObj,
}

impl DatType {
    pub fn new_with_no_param(id: DatTypeID) -> Self {
        Self {
            dat_type_obj: ParamObj::new_empty(id),
        }
    }

    pub fn new_with_default_param(id: DatTypeID) -> Self {
        Self::new_with_obj(ParamObj::default_for(id))
    }

    pub fn new_with_obj(param: ParamObj) -> Self {
        Self {
            dat_type_obj: param,
        }
    }

    pub fn id(&self) -> DatTypeID {
        self.dat_type_obj.dat_type_id()
    }

    pub fn param(&self) -> &ParamObj {
        &self.dat_type_obj
    }

    pub fn has_param(&self) -> bool {
        !self.dat_type_obj.is_empty()
    }

    pub fn param_info(&self) -> ParamInfo {
        self.dat_type_obj.to_info()
    }
}

impl<'de> Deserialize<'de> for DatType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let obj: ParamObj = Deserialize::deserialize(deserializer)?;
        Ok(Self { dat_type_obj: obj })
    }
}

impl Serialize for DatType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_some(&self.dat_type_obj)
    }
}
