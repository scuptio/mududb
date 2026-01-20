use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dt_info::DTInfo;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Clone, Debug)]
pub struct DatPrim {
    dat_type: DatType,
}

impl DatPrim {
    pub fn new_without_param(id: DatTypeID) -> Self {
        if !id.is_primitive_type() {
            panic!("DatPrim id must be primitive type, but got {}", id.name());
        }
        Self {
            dat_type: DatType::new_no_param(id),
        }
    }

    pub fn new_default(id: DatTypeID) -> Self {
        if !id.is_primitive_type() {
            panic!("DatPrim id must be primitive type, but got {}", id.name());
        }
        Self::new(DatType::default_for(id))
    }

    pub fn new(type_obj: DatType) -> Self {
        if !type_obj.dat_type_id().is_primitive_type() {
            panic!(
                "DatPrim id must be primitive type, but got {}",
                type_obj.dat_type_id().name()
            );
        }
        Self { dat_type: type_obj }
    }

    pub fn id(&self) -> DatTypeID {
        self.dat_type.dat_type_id()
    }

    pub fn type_obj(&self) -> &DatType {
        &self.dat_type
    }

    pub fn dat_param(&self) -> &DatType {
        &self.dat_type
    }

    pub fn has_param(&self) -> bool {
        !self.dat_type.has_no_param()
    }

    pub fn param_info(&self) -> DTInfo {
        self.dat_type.to_info()
    }
}

impl<'de> Deserialize<'de> for DatPrim {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let obj: DatType = Deserialize::deserialize(deserializer)?;
        Ok(Self { dat_type: obj })
    }
}

impl Serialize for DatPrim {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_some(&self.dat_type)
    }
}
