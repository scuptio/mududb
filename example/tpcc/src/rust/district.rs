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
const DISTRICT:&str = "district";

const D_ID:&str = "d_id";

const D_W_ID:&str = "d_w_id";

const D_NAME:&str = "d_name";

const D_TAX:&str = "d_tax";

const D_YTD:&str = "d_ytd";

const D_NEXT_O_ID:&str = "d_next_o_id";

const D_LAST_DELIVERY_O_ID:&str = "d_last_delivery_o_id";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct District {
    
    d_id: AttrDId,
    
    d_w_id: AttrDWId,
    
    d_name: AttrDName,
    
    d_tax: AttrDTax,
    
    d_ytd: AttrDYtd,
    
    d_next_o_id: AttrDNextOId,
    
    d_last_delivery_o_id: AttrDLastDeliveryOId,
    
}

impl TupleDatumMarker for District {}

impl SQLParamMarker for District {}

impl District {
    pub fn new(
        d_id: Option<i32>,
        d_w_id: Option<i32>,
        d_name: Option<String>,
        d_tax: Option<i32>,
        d_ytd: Option<i32>,
        d_next_o_id: Option<i32>,
        d_last_delivery_o_id: Option<i32>,
        
    ) -> Self {
        
        Self {
            
            d_id : AttrDId::from(d_id),
            
            d_w_id : AttrDWId::from(d_w_id),
            
            d_name : AttrDName::from(d_name),
            
            d_tax : AttrDTax::from(d_tax),
            
            d_ytd : AttrDYtd::from(d_ytd),
            
            d_next_o_id : AttrDNextOId::from(d_next_o_id),
            
            d_last_delivery_o_id : AttrDLastDeliveryOId::from(d_last_delivery_o_id),
            
        }
    }

    pub fn new_empty() -> Self {
        Self::default()
    }

    
    pub fn set_d_id(
        &mut self,
        d_id: i32,
    ) {
        self.d_id.update(d_id)
    }

    pub fn get_d_id(
        &self,
    ) -> &Option<i32> {
        self.d_id.get()
    }
    
    pub fn set_d_w_id(
        &mut self,
        d_w_id: i32,
    ) {
        self.d_w_id.update(d_w_id)
    }

    pub fn get_d_w_id(
        &self,
    ) -> &Option<i32> {
        self.d_w_id.get()
    }
    
    pub fn set_d_name(
        &mut self,
        d_name: String,
    ) {
        self.d_name.update(d_name)
    }

    pub fn get_d_name(
        &self,
    ) -> &Option<String> {
        self.d_name.get()
    }
    
    pub fn set_d_tax(
        &mut self,
        d_tax: i32,
    ) {
        self.d_tax.update(d_tax)
    }

    pub fn get_d_tax(
        &self,
    ) -> &Option<i32> {
        self.d_tax.get()
    }
    
    pub fn set_d_ytd(
        &mut self,
        d_ytd: i32,
    ) {
        self.d_ytd.update(d_ytd)
    }

    pub fn get_d_ytd(
        &self,
    ) -> &Option<i32> {
        self.d_ytd.get()
    }
    
    pub fn set_d_next_o_id(
        &mut self,
        d_next_o_id: i32,
    ) {
        self.d_next_o_id.update(d_next_o_id)
    }

    pub fn get_d_next_o_id(
        &self,
    ) -> &Option<i32> {
        self.d_next_o_id.get()
    }
    
    pub fn set_d_last_delivery_o_id(
        &mut self,
        d_last_delivery_o_id: i32,
    ) {
        self.d_last_delivery_o_id.update(d_last_delivery_o_id)
    }

    pub fn get_d_last_delivery_o_id(
        &self,
    ) -> &Option<i32> {
        self.d_last_delivery_o_id.get()
    }
    
}

impl Datum for District {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<District>();
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

impl DatumDyn for District {
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

impl Entity for District {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrDId::datum_desc().clone(),
                
                AttrDWId::datum_desc().clone(),
                
                AttrDName::datum_desc().clone(),
                
                AttrDTax::datum_desc().clone(),
                
                AttrDYtd::datum_desc().clone(),
                
                AttrDNextOId::datum_desc().clone(),
                
                AttrDLastDeliveryOId::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            D_ID => {
                attr_field_access::attr_get_binary::<_>(self.d_id.get())
            }
            
            D_W_ID => {
                attr_field_access::attr_get_binary::<_>(self.d_w_id.get())
            }
            
            D_NAME => {
                attr_field_access::attr_get_binary::<_>(self.d_name.get())
            }
            
            D_TAX => {
                attr_field_access::attr_get_binary::<_>(self.d_tax.get())
            }
            
            D_YTD => {
                attr_field_access::attr_get_binary::<_>(self.d_ytd.get())
            }
            
            D_NEXT_O_ID => {
                attr_field_access::attr_get_binary::<_>(self.d_next_o_id.get())
            }
            
            D_LAST_DELIVERY_O_ID => {
                attr_field_access::attr_get_binary::<_>(self.d_last_delivery_o_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            D_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.d_id.get_mut(), binary.as_ref())?;
            }
            
            D_W_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.d_w_id.get_mut(), binary.as_ref())?;
            }
            
            D_NAME => {
                attr_field_access::attr_set_binary::<_, _>(self.d_name.get_mut(), binary.as_ref())?;
            }
            
            D_TAX => {
                attr_field_access::attr_set_binary::<_, _>(self.d_tax.get_mut(), binary.as_ref())?;
            }
            
            D_YTD => {
                attr_field_access::attr_set_binary::<_, _>(self.d_ytd.get_mut(), binary.as_ref())?;
            }
            
            D_NEXT_O_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.d_next_o_id.get_mut(), binary.as_ref())?;
            }
            
            D_LAST_DELIVERY_O_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.d_last_delivery_o_id.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            D_ID => {
                attr_field_access::attr_get_value::<_>(self.d_id.get())
            }
            
            D_W_ID => {
                attr_field_access::attr_get_value::<_>(self.d_w_id.get())
            }
            
            D_NAME => {
                attr_field_access::attr_get_value::<_>(self.d_name.get())
            }
            
            D_TAX => {
                attr_field_access::attr_get_value::<_>(self.d_tax.get())
            }
            
            D_YTD => {
                attr_field_access::attr_get_value::<_>(self.d_ytd.get())
            }
            
            D_NEXT_O_ID => {
                attr_field_access::attr_get_value::<_>(self.d_next_o_id.get())
            }
            
            D_LAST_DELIVERY_O_ID => {
                attr_field_access::attr_get_value::<_>(self.d_last_delivery_o_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            D_ID => {
                attr_field_access::attr_set_value::<_, _>(self.d_id.get_mut(), value)?;
            }
            
            D_W_ID => {
                attr_field_access::attr_set_value::<_, _>(self.d_w_id.get_mut(), value)?;
            }
            
            D_NAME => {
                attr_field_access::attr_set_value::<_, _>(self.d_name.get_mut(), value)?;
            }
            
            D_TAX => {
                attr_field_access::attr_set_value::<_, _>(self.d_tax.get_mut(), value)?;
            }
            
            D_YTD => {
                attr_field_access::attr_set_value::<_, _>(self.d_ytd.get_mut(), value)?;
            }
            
            D_NEXT_O_ID => {
                attr_field_access::attr_set_value::<_, _>(self.d_next_o_id.get_mut(), value)?;
            }
            
            D_LAST_DELIVERY_O_ID => {
                attr_field_access::attr_set_value::<_, _>(self.d_last_delivery_o_id.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }
}


// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDId {
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

impl AttrValue<i32> for AttrDId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDWId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDWId {
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

impl AttrValue<i32> for AttrDWId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_W_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDName {
    is_dirty:bool,
    value: Option<String>
}

impl AttrDName {
    fn from(value:Option<String>) -> Self {
        Self {
            is_dirty: false,
            value
        }
    }

    fn get(&self) -> &Option<String> {
        &self.value
    }

    fn get_mut(&mut self) -> &mut Option<String> {
        &mut self.value
    }

    fn set(&mut self, value:Option<String>) {
        self.value = value
    }

    fn update(&mut self, value: String) {
        self.is_dirty = true;
        self.value = Some(value)
    }
}

impl AttrValue<String> for AttrDName {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_NAME
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDTax {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDTax {
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

impl AttrValue<i32> for AttrDTax {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_TAX
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDYtd {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDYtd {
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

impl AttrValue<i32> for AttrDYtd {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_YTD
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDNextOId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDNextOId {
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

impl AttrValue<i32> for AttrDNextOId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_NEXT_O_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrDLastDeliveryOId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrDLastDeliveryOId {
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

impl AttrValue<i32> for AttrDLastDeliveryOId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_dat_type)
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(Self::attr_datum_desc)
    }

    fn object_name() -> &'static str {
        DISTRICT
    }

    fn attr_name() -> &'static str {
        D_LAST_DELIVERY_O_ID
    }
}


}
