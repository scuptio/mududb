pub mod object {
use lazy_static::lazy_static;
use mududb::common::result::RS;
use mududb::contract::database::attr_field_access;
use mududb::contract::database::attr_value::AttrValue;
use mududb::contract::database::entity::Entity;
use mududb::contract::database::entity_utils;
use mududb::contract::database::sql_params::SQLParamMarker;
use mududb::contract::tuple::datum_desc::DatumDesc;
use mududb::contract::tuple::tuple_field_desc::TupleFieldDesc;
use mududb::contract::tuple::tuple_datum::TupleDatumMarker;
use mududb::types::dat_binary::DatBinary;
use mududb::types::dat_textual::DatTextual;
use mududb::types::dat_type::DatType;
use mududb::types::dat_type_id::DatTypeID;
use mududb::types::dat_value::DatValue;
use mududb::types::datum::{Datum, DatumDyn};

// constant definition
const NEW_ORDER:&str = "new_order";

const NO_O_ID:&str = "no_o_id";

const NO_D_ID:&str = "no_d_id";

const NO_W_ID:&str = "no_w_id";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct NewOrder {
    
    no_o_id: AttrNoOId,
    
    no_d_id: AttrNoDId,
    
    no_w_id: AttrNoWId,
    
}

impl TupleDatumMarker for NewOrder {}

impl SQLParamMarker for NewOrder {}

impl NewOrder {
    pub fn new(
        no_o_id: Option<i32>,
        no_d_id: Option<i32>,
        no_w_id: Option<i32>,
        
    ) -> Self {
        
        Self {
            
            no_o_id : AttrNoOId::from(no_o_id),
            
            no_d_id : AttrNoDId::from(no_d_id),
            
            no_w_id : AttrNoWId::from(no_w_id),
            
        }
    }

    pub fn new_empty() -> Self {
        Self::default()
    }

    
    pub fn set_no_o_id(
        &mut self,
        no_o_id: i32,
    ) {
        self.no_o_id.update(no_o_id)
    }

    pub fn get_no_o_id(
        &self,
    ) -> &Option<i32> {
        self.no_o_id.get()
    }
    
    pub fn set_no_d_id(
        &mut self,
        no_d_id: i32,
    ) {
        self.no_d_id.update(no_d_id)
    }

    pub fn get_no_d_id(
        &self,
    ) -> &Option<i32> {
        self.no_d_id.get()
    }
    
    pub fn set_no_w_id(
        &mut self,
        no_w_id: i32,
    ) {
        self.no_w_id.update(no_w_id)
    }

    pub fn get_no_w_id(
        &self,
    ) -> &Option<i32> {
        self.no_w_id.get()
    }
    
}

impl Datum for NewOrder {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<NewOrder>();
        }
        &DAT_TYPE
    }

    fn from_binary(binary: &[u8]) -> RS<Self> {
        entity_utils::entity_from_binary(binary)
    }

    fn from_value(value: &DatValue) -> RS<Self> {
        entity_utils::entity_from_value(value)
    }

    fn from_textual(textual: &str) -> RS<Self> {
        entity_utils::entity_from_textual(textual)
    }
}

impl DatumDyn for NewOrder {
    fn dat_type_id(&self) -> RS<DatTypeID> {
        entity_utils::entity_dat_type_id()
    }

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
        entity_utils::entity_to_binary(self, dat_type)
    }

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
        entity_utils::entity_to_textual(self, dat_type)
    }

    fn to_value(&self, dat_type: &DatType) -> RS<DatValue> {
        entity_utils::entity_to_value(self, dat_type)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        entity_utils::entity_clone_boxed(self)
    }
}

impl Entity for NewOrder {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrNoOId::datum_desc().clone(),
                
                AttrNoDId::datum_desc().clone(),
                
                AttrNoWId::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        NEW_ORDER
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            NO_O_ID => {
                attr_field_access::attr_get_binary::<_>(self.no_o_id.get())
            }
            
            NO_D_ID => {
                attr_field_access::attr_get_binary::<_>(self.no_d_id.get())
            }
            
            NO_W_ID => {
                attr_field_access::attr_get_binary::<_>(self.no_w_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            NO_O_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.no_o_id.get_mut(), binary.as_ref())?;
            }
            
            NO_D_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.no_d_id.get_mut(), binary.as_ref())?;
            }
            
            NO_W_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.no_w_id.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            NO_O_ID => {
                attr_field_access::attr_get_value::<_>(self.no_o_id.get())
            }
            
            NO_D_ID => {
                attr_field_access::attr_get_value::<_>(self.no_d_id.get())
            }
            
            NO_W_ID => {
                attr_field_access::attr_get_value::<_>(self.no_w_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            NO_O_ID => {
                attr_field_access::attr_set_value::<_, _>(self.no_o_id.get_mut(), value)?;
            }
            
            NO_D_ID => {
                attr_field_access::attr_set_value::<_, _>(self.no_d_id.get_mut(), value)?;
            }
            
            NO_W_ID => {
                attr_field_access::attr_set_value::<_, _>(self.no_w_id.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }
}


// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrNoOId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrNoOId {
    fn from(value:Option<i32>) -> Self {
        Self {
            is_dirty: false,
            value
        }
    }

    fn get(&self) -> &Option<i32> {
        &self.value
    }

    fn get_mut(&mut self) -> &mut Option<i32> {
        &mut self.value
    }

    fn set(&mut self, value:Option<i32>) {
        self.value = value
    }

    fn update(&mut self, value: i32) {
        self.is_dirty = true;
        self.value = Some(value)
    }
}

impl AttrValue<i32> for AttrNoOId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        NEW_ORDER
    }

    fn attr_name() -> &'static str {
        NO_O_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrNoDId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrNoDId {
    fn from(value:Option<i32>) -> Self {
        Self {
            is_dirty: false,
            value
        }
    }

    fn get(&self) -> &Option<i32> {
        &self.value
    }

    fn get_mut(&mut self) -> &mut Option<i32> {
        &mut self.value
    }

    fn set(&mut self, value:Option<i32>) {
        self.value = value
    }

    fn update(&mut self, value: i32) {
        self.is_dirty = true;
        self.value = Some(value)
    }
}

impl AttrValue<i32> for AttrNoDId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        NEW_ORDER
    }

    fn attr_name() -> &'static str {
        NO_D_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrNoWId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrNoWId {
    fn from(value:Option<i32>) -> Self {
        Self {
            is_dirty: false,
            value
        }
    }

    fn get(&self) -> &Option<i32> {
        &self.value
    }

    fn get_mut(&mut self) -> &mut Option<i32> {
        &mut self.value
    }

    fn set(&mut self, value:Option<i32>) {
        self.value = value
    }

    fn update(&mut self, value: i32) {
        self.is_dirty = true;
        self.value = Some(value)
    }
}

impl AttrValue<i32> for AttrNoWId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        NEW_ORDER
    }

    fn attr_name() -> &'static str {
        NO_W_ID
    }
}


}
