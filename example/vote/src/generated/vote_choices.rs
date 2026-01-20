pub mod object {
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_contract::database::attr_field_access;
    use mudu_contract::database::attr_value::AttrValue;
    use mudu_contract::database::entity::Entity;
    use mudu_contract::database::entity_utils;
    use mudu_contract::database::sql_params::SQLParamMarker;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_datum::TupleDatumMarker;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::dat_binary::DatBinary;
    use mudu_type::dat_textual::DatTextual;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;
    use mudu_type::datum::{Datum, DatumDyn};

    // constant definition
const VOTE_CHOICES:&str = "vote_choices";

const CHOICE_ID:&str = "choice_id";

const ACTION_ID:&str = "action_id";

const OPTION_ID:&str = "option_id";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct VoteChoices {
    
    choice_id: AttrChoiceId,
    
    action_id: AttrActionId,
    
    option_id: AttrOptionId,
    
}

impl TupleDatumMarker for VoteChoices {}

impl SQLParamMarker for VoteChoices {}

impl VoteChoices {
    pub fn new(
        choice_id: Option<String>,
        action_id: Option<String>,
        option_id: Option<String>,
        
    ) -> Self {
        let s = Self {
            
            choice_id : AttrChoiceId::from(choice_id),
            
            action_id : AttrActionId::from(action_id),
            
            option_id : AttrOptionId::from(option_id),
            
        };
        s
    }

    pub fn new_empty() -> Self {
        Self::default()
    }

    
    pub fn set_choice_id(
        &mut self,
        choice_id: String,
    ) {
        self.choice_id.update(choice_id)
    }

    pub fn get_choice_id(
        &self,
    ) -> &Option<String> {
        self.choice_id.get()
    }
    
    pub fn set_action_id(
        &mut self,
        action_id: String,
    ) {
        self.action_id.update(action_id)
    }

    pub fn get_action_id(
        &self,
    ) -> &Option<String> {
        self.action_id.get()
    }
    
    pub fn set_option_id(
        &mut self,
        option_id: String,
    ) {
        self.option_id.update(option_id)
    }

    pub fn get_option_id(
        &self,
    ) -> &Option<String> {
        self.option_id.get()
    }
    
}

impl Datum for VoteChoices {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<VoteChoices>();
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

impl DatumDyn for VoteChoices {
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

impl Entity for VoteChoices {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrChoiceId::datum_desc().clone(),
                
                AttrActionId::datum_desc().clone(),
                
                AttrOptionId::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        VOTE_CHOICES
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            CHOICE_ID => {
                attr_field_access::attr_get_binary::<_>(self.choice_id.get())
            }
            
            ACTION_ID => {
                attr_field_access::attr_get_binary::<_>(self.action_id.get())
            }
            
            OPTION_ID => {
                attr_field_access::attr_get_binary::<_>(self.option_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            CHOICE_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.choice_id.get_mut(), binary.as_ref())?;
            }
            
            ACTION_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.action_id.get_mut(), binary.as_ref())?;
            }
            
            OPTION_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.option_id.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            CHOICE_ID => {
                attr_field_access::attr_get_value::<_>(self.choice_id.get())
            }
            
            ACTION_ID => {
                attr_field_access::attr_get_value::<_>(self.action_id.get())
            }
            
            OPTION_ID => {
                attr_field_access::attr_get_value::<_>(self.option_id.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            CHOICE_ID => {
                attr_field_access::attr_set_value::<_, _>(self.choice_id.get_mut(), value)?;
            }
            
            ACTION_ID => {
                attr_field_access::attr_set_value::<_, _>(self.action_id.get_mut(), value)?;
            }
            
            OPTION_ID => {
                attr_field_access::attr_set_value::<_, _>(self.option_id.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }
}


// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrChoiceId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrChoiceId {
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

impl AttrValue<String> for AttrChoiceId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_CHOICES
    }

    fn attr_name() -> &'static str {
        CHOICE_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrActionId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrActionId {
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

impl AttrValue<String> for AttrActionId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_CHOICES
    }

    fn attr_name() -> &'static str {
        ACTION_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrOptionId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrOptionId {
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

    impl AttrValue<String> for AttrOptionId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_CHOICES
    }

    fn attr_name() -> &'static str {
        OPTION_ID
    }
}


}