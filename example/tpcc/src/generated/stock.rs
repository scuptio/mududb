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
const STOCK:&str = "stock";

const S_I_ID:&str = "s_i_id";

const S_W_ID:&str = "s_w_id";

const S_QUANTITY:&str = "s_quantity";

const S_YTD:&str = "s_ytd";

const S_ORDER_CNT:&str = "s_order_cnt";

const S_REMOTE_CNT:&str = "s_remote_cnt";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct Stock {
    
    s_i_id: AttrSIId,
    
    s_w_id: AttrSWId,
    
    s_quantity: AttrSQuantity,
    
    s_ytd: AttrSYtd,
    
    s_order_cnt: AttrSOrderCnt,
    
    s_remote_cnt: AttrSRemoteCnt,
    
}

impl TupleDatumMarker for Stock {}

impl SQLParamMarker for Stock {}

impl Stock {
    pub fn new(
        s_i_id: Option<i32>,
        s_w_id: Option<i32>,
        s_quantity: Option<i32>,
        s_ytd: Option<i32>,
        s_order_cnt: Option<i32>,
        s_remote_cnt: Option<i32>,
        
    ) -> Self {
        let s = Self {
            
            s_i_id : AttrSIId::from(s_i_id),
            
            s_w_id : AttrSWId::from(s_w_id),
            
            s_quantity : AttrSQuantity::from(s_quantity),
            
            s_ytd : AttrSYtd::from(s_ytd),
            
            s_order_cnt : AttrSOrderCnt::from(s_order_cnt),
            
            s_remote_cnt : AttrSRemoteCnt::from(s_remote_cnt),
            
        };
        s
    }

    pub fn new_empty() -> Self {
        Self::default()
    }

    
    pub fn set_s_i_id(
        &mut self,
        s_i_id: i32,
    ) {
        self.s_i_id.update(s_i_id)
    }

    pub fn get_s_i_id(
        &self,
    ) -> &Option<i32> {
        self.s_i_id.get()
    }
    
    pub fn set_s_w_id(
        &mut self,
        s_w_id: i32,
    ) {
        self.s_w_id.update(s_w_id)
    }

    pub fn get_s_w_id(
        &self,
    ) -> &Option<i32> {
        self.s_w_id.get()
    }
    
    pub fn set_s_quantity(
        &mut self,
        s_quantity: i32,
    ) {
        self.s_quantity.update(s_quantity)
    }

    pub fn get_s_quantity(
        &self,
    ) -> &Option<i32> {
        self.s_quantity.get()
    }
    
    pub fn set_s_ytd(
        &mut self,
        s_ytd: i32,
    ) {
        self.s_ytd.update(s_ytd)
    }

    pub fn get_s_ytd(
        &self,
    ) -> &Option<i32> {
        self.s_ytd.get()
    }
    
    pub fn set_s_order_cnt(
        &mut self,
        s_order_cnt: i32,
    ) {
        self.s_order_cnt.update(s_order_cnt)
    }

    pub fn get_s_order_cnt(
        &self,
    ) -> &Option<i32> {
        self.s_order_cnt.get()
    }
    
    pub fn set_s_remote_cnt(
        &mut self,
        s_remote_cnt: i32,
    ) {
        self.s_remote_cnt.update(s_remote_cnt)
    }

    pub fn get_s_remote_cnt(
        &self,
    ) -> &Option<i32> {
        self.s_remote_cnt.get()
    }
    
}

impl Datum for Stock {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Stock>();
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

impl DatumDyn for Stock {
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

impl Entity for Stock {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrSIId::datum_desc().clone(),
                
                AttrSWId::datum_desc().clone(),
                
                AttrSQuantity::datum_desc().clone(),
                
                AttrSYtd::datum_desc().clone(),
                
                AttrSOrderCnt::datum_desc().clone(),
                
                AttrSRemoteCnt::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            S_I_ID => {
                attr_field_access::attr_get_binary::<_>(self.s_i_id.get())
            }
            
            S_W_ID => {
                attr_field_access::attr_get_binary::<_>(self.s_w_id.get())
            }
            
            S_QUANTITY => {
                attr_field_access::attr_get_binary::<_>(self.s_quantity.get())
            }
            
            S_YTD => {
                attr_field_access::attr_get_binary::<_>(self.s_ytd.get())
            }
            
            S_ORDER_CNT => {
                attr_field_access::attr_get_binary::<_>(self.s_order_cnt.get())
            }
            
            S_REMOTE_CNT => {
                attr_field_access::attr_get_binary::<_>(self.s_remote_cnt.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            S_I_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.s_i_id.get_mut(), binary.as_ref())?;
            }
            
            S_W_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.s_w_id.get_mut(), binary.as_ref())?;
            }
            
            S_QUANTITY => {
                attr_field_access::attr_set_binary::<_, _>(self.s_quantity.get_mut(), binary.as_ref())?;
            }
            
            S_YTD => {
                attr_field_access::attr_set_binary::<_, _>(self.s_ytd.get_mut(), binary.as_ref())?;
            }
            
            S_ORDER_CNT => {
                attr_field_access::attr_set_binary::<_, _>(self.s_order_cnt.get_mut(), binary.as_ref())?;
            }
            
            S_REMOTE_CNT => {
                attr_field_access::attr_set_binary::<_, _>(self.s_remote_cnt.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            S_I_ID => {
                attr_field_access::attr_get_value::<_>(self.s_i_id.get())
            }
            
            S_W_ID => {
                attr_field_access::attr_get_value::<_>(self.s_w_id.get())
            }
            
            S_QUANTITY => {
                attr_field_access::attr_get_value::<_>(self.s_quantity.get())
            }
            
            S_YTD => {
                attr_field_access::attr_get_value::<_>(self.s_ytd.get())
            }
            
            S_ORDER_CNT => {
                attr_field_access::attr_get_value::<_>(self.s_order_cnt.get())
            }
            
            S_REMOTE_CNT => {
                attr_field_access::attr_get_value::<_>(self.s_remote_cnt.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            S_I_ID => {
                attr_field_access::attr_set_value::<_, _>(self.s_i_id.get_mut(), value)?;
            }
            
            S_W_ID => {
                attr_field_access::attr_set_value::<_, _>(self.s_w_id.get_mut(), value)?;
            }
            
            S_QUANTITY => {
                attr_field_access::attr_set_value::<_, _>(self.s_quantity.get_mut(), value)?;
            }
            
            S_YTD => {
                attr_field_access::attr_set_value::<_, _>(self.s_ytd.get_mut(), value)?;
            }
            
            S_ORDER_CNT => {
                attr_field_access::attr_set_value::<_, _>(self.s_order_cnt.get_mut(), value)?;
            }
            
            S_REMOTE_CNT => {
                attr_field_access::attr_set_value::<_, _>(self.s_remote_cnt.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }
}


// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSIId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSIId {
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

impl AttrValue<i32> for AttrSIId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_I_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSWId {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSWId {
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

impl AttrValue<i32> for AttrSWId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_W_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSQuantity {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSQuantity {
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

impl AttrValue<i32> for AttrSQuantity {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_QUANTITY
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSYtd {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSYtd {
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

impl AttrValue<i32> for AttrSYtd {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_YTD
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSOrderCnt {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSOrderCnt {
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

impl AttrValue<i32> for AttrSOrderCnt {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_ORDER_CNT
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrSRemoteCnt {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrSRemoteCnt {
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

impl AttrValue<i32> for AttrSRemoteCnt {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        STOCK
    }

    fn attr_name() -> &'static str {
        S_REMOTE_CNT
    }
}


}