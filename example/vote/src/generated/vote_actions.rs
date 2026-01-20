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
const VOTE_ACTIONS:&str = "vote_actions";

const ACTION_ID:&str = "action_id";

const USER_ID:&str = "user_id";

const VOTE_ID:&str = "vote_id";

const ACTION_TIME:&str = "action_time";

const IS_WITHDRAWN:&str = "is_withdrawn";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct VoteActions {
    
    action_id: AttrActionId,
    
    user_id: AttrUserId,
    
    vote_id: AttrVoteId,
    
    action_time: AttrActionTime,
    
    is_withdrawn: AttrIsWithdrawn,
    
}

impl TupleDatumMarker for VoteActions {}

impl SQLParamMarker for VoteActions {}

impl VoteActions {
    pub fn new(
        action_id: Option<String>,
        user_id: Option<String>,
        vote_id: Option<String>,
        action_time: Option<i32>,
        is_withdrawn: Option<i32>,
        
    ) -> Self {
        let s = Self {
            
            action_id : AttrActionId::from(action_id),
            
            user_id : AttrUserId::from(user_id),
            
            vote_id : AttrVoteId::from(vote_id),
            
            action_time : AttrActionTime::from(action_time),
            
            is_withdrawn : AttrIsWithdrawn::from(is_withdrawn),
            
        };
        s
    }

    pub fn new_empty() -> Self {
        Self::default()
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
    
    pub fn set_user_id(
        &mut self,
        user_id: String,
    ) {
        self.user_id.update(user_id)
    }

    pub fn get_user_id(
        &self,
    ) -> &Option<String> {
        self.user_id.get()
    }
    
    pub fn set_vote_id(
        &mut self,
        vote_id: String,
    ) {
        self.vote_id.update(vote_id)
    }

    pub fn get_vote_id(
        &self,
    ) -> &Option<String> {
        self.vote_id.get()
    }
    
    pub fn set_action_time(
        &mut self,
        action_time: i32,
    ) {
        self.action_time.update(action_time)
    }

    pub fn get_action_time(
        &self,
    ) -> &Option<i32> {
        self.action_time.get()
    }
    
    pub fn set_is_withdrawn(
        &mut self,
        is_withdrawn: i32,
    ) {
        self.is_withdrawn.update(is_withdrawn)
    }

    pub fn get_is_withdrawn(
        &self,
    ) -> &Option<i32> {
        self.is_withdrawn.get()
    }
    
}

impl Datum for VoteActions {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<VoteActions>();
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

impl DatumDyn for VoteActions {
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

impl Entity for VoteActions {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrActionId::datum_desc().clone(),
                
                AttrUserId::datum_desc().clone(),
                
                AttrVoteId::datum_desc().clone(),
                
                AttrActionTime::datum_desc().clone(),
                
                AttrIsWithdrawn::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        VOTE_ACTIONS
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            ACTION_ID => {
                attr_field_access::attr_get_binary::<_>(self.action_id.get())
            }
            
            USER_ID => {
                attr_field_access::attr_get_binary::<_>(self.user_id.get())
            }
            
            VOTE_ID => {
                attr_field_access::attr_get_binary::<_>(self.vote_id.get())
            }
            
            ACTION_TIME => {
                attr_field_access::attr_get_binary::<_>(self.action_time.get())
            }
            
            IS_WITHDRAWN => {
                attr_field_access::attr_get_binary::<_>(self.is_withdrawn.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            ACTION_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.action_id.get_mut(), binary.as_ref())?;
            }
            
            USER_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.user_id.get_mut(), binary.as_ref())?;
            }
            
            VOTE_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.vote_id.get_mut(), binary.as_ref())?;
            }
            
            ACTION_TIME => {
                attr_field_access::attr_set_binary::<_, _>(self.action_time.get_mut(), binary.as_ref())?;
            }
            
            IS_WITHDRAWN => {
                attr_field_access::attr_set_binary::<_, _>(self.is_withdrawn.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            ACTION_ID => {
                attr_field_access::attr_get_value::<_>(self.action_id.get())
            }
            
            USER_ID => {
                attr_field_access::attr_get_value::<_>(self.user_id.get())
            }
            
            VOTE_ID => {
                attr_field_access::attr_get_value::<_>(self.vote_id.get())
            }
            
            ACTION_TIME => {
                attr_field_access::attr_get_value::<_>(self.action_time.get())
            }
            
            IS_WITHDRAWN => {
                attr_field_access::attr_get_value::<_>(self.is_withdrawn.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            ACTION_ID => {
                attr_field_access::attr_set_value::<_, _>(self.action_id.get_mut(), value)?;
            }
            
            USER_ID => {
                attr_field_access::attr_set_value::<_, _>(self.user_id.get_mut(), value)?;
            }
            
            VOTE_ID => {
                attr_field_access::attr_set_value::<_, _>(self.vote_id.get_mut(), value)?;
            }
            
            ACTION_TIME => {
                attr_field_access::attr_set_value::<_, _>(self.action_time.get_mut(), value)?;
            }
            
            IS_WITHDRAWN => {
                attr_field_access::attr_set_value::<_, _>(self.is_withdrawn.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
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
        VOTE_ACTIONS
    }

    fn attr_name() -> &'static str {
        ACTION_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrUserId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrUserId {
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

impl AttrValue<String> for AttrUserId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_ACTIONS
    }

    fn attr_name() -> &'static str {
        USER_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrVoteId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrVoteId {
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

impl AttrValue<String> for AttrVoteId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_ACTIONS
    }

    fn attr_name() -> &'static str {
        VOTE_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrActionTime {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrActionTime {
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

impl AttrValue<i32> for AttrActionTime {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_ACTIONS
    }

    fn attr_name() -> &'static str {
        ACTION_TIME
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrIsWithdrawn {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrIsWithdrawn {
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

impl AttrValue<i32> for AttrIsWithdrawn {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTE_ACTIONS
    }

    fn attr_name() -> &'static str {
        IS_WITHDRAWN
    }
}


}