#[cfg(any(test, feature = "test"))]
use arbitrary::{Arbitrary, Unstructured};
use mudu::common::id::{gen_oid, OID};
use mudu::data_type::dt_impl::dat_type_id::DatTypeID as TypeID;
use mudu::data_type::param_info::ParamInfo;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaColumn {
    oid: OID,
    name: String,
    type_id: TypeID,
    type_param: ParamInfo,
    index: u32,
    is_primary: bool,
}

impl SchemaColumn {
    pub fn new(name: String, data_type: TypeID, type_param: ParamInfo) -> Self {
        Self {
            oid: gen_oid(),
            name,
            type_id: data_type,
            type_param: type_param.clone(),

            index: 0,
            is_primary: false,
        }
    }

    pub fn get_oid(&self) -> OID {
        self.oid
    }

    pub fn get_name(&self) -> &String {
        &self.name
    }

    pub fn is_primary(&self) -> bool {
        self.is_primary
    }

    pub fn set_primary(&mut self, is_primary: bool) {
        self.is_primary = is_primary;
    }

    pub fn get_index(&self) -> u32 {
        self.index
    }

    pub fn set_index(&mut self, index: u32) {
        self.index = index;
    }

    pub fn type_id(&self) -> TypeID {
        self.type_id
    }

    pub fn is_fixed_length(&self) -> bool {
        self.type_id().is_fixed_len()
    }

    pub fn type_param(&self) -> &ParamInfo {
        &self.type_param
    }
}


#[cfg(any(test, feature = "test"))]
impl<'a> Arbitrary<'a> for SchemaColumn {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let name = String::arbitrary(u)?;
        let data_type = TypeID::arbitrary(u)?;
        let fn_arbitrary = data_type.fn_arb_param();
        let param = fn_arbitrary(u)?;
        let schema = Self::new(name, data_type, ParamInfo::from_opt_object(&param));
        Ok(schema)
    }
}
