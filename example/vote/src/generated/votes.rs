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
const VOTES:&str = "votes";

const VOTE_ID:&str = "vote_id";

const CREATOR_ID:&str = "creator_id";

const TOPIC:&str = "topic";

const VOTE_TYPE:&str = "vote_type";

const MAX_CHOICES:&str = "max_choices";

const END_TIME:&str = "end_time";

const VISIBILITY_RULE:&str = "visibility_rule";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct Votes {
    
    vote_id: AttrVoteId,
    
    creator_id: AttrCreatorId,
    
    topic: AttrTopic,
    
    vote_type: AttrVoteType,
    
    max_choices: AttrMaxChoices,
    
    end_time: AttrEndTime,
    
    visibility_rule: AttrVisibilityRule,
    
}

impl TupleDatumMarker for Votes {}

impl SQLParamMarker for Votes {}

impl Votes {
    pub fn new(
        vote_id: Option<String>,
        creator_id: Option<String>,
        topic: Option<String>,
        vote_type: Option<String>,
        max_choices: Option<i32>,
        end_time: Option<i32>,
        visibility_rule: Option<String>,
        
    ) -> Self {
        let s = Self {
            
            vote_id : AttrVoteId::from(vote_id),
            
            creator_id : AttrCreatorId::from(creator_id),
            
            topic : AttrTopic::from(topic),
            
            vote_type : AttrVoteType::from(vote_type),
            
            max_choices : AttrMaxChoices::from(max_choices),
            
            end_time : AttrEndTime::from(end_time),
            
            visibility_rule : AttrVisibilityRule::from(visibility_rule),
            
        };
        s
    }

    pub fn new_empty() -> Self {
        Self::default()
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
    
    pub fn set_creator_id(
        &mut self,
        creator_id: String,
    ) {
        self.creator_id.update(creator_id)
    }

    pub fn get_creator_id(
        &self,
    ) -> &Option<String> {
        self.creator_id.get()
    }
    
    pub fn set_topic(
        &mut self,
        topic: String,
    ) {
        self.topic.update(topic)
    }

    pub fn get_topic(
        &self,
    ) -> &Option<String> {
        self.topic.get()
    }
    
    pub fn set_vote_type(
        &mut self,
        vote_type: String,
    ) {
        self.vote_type.update(vote_type)
    }

    pub fn get_vote_type(
        &self,
    ) -> &Option<String> {
        self.vote_type.get()
    }
    
    pub fn set_max_choices(
        &mut self,
        max_choices: i32,
    ) {
        self.max_choices.update(max_choices)
    }

    pub fn get_max_choices(
        &self,
    ) -> &Option<i32> {
        self.max_choices.get()
    }
    
    pub fn set_end_time(
        &mut self,
        end_time: i32,
    ) {
        self.end_time.update(end_time)
    }

    pub fn get_end_time(
        &self,
    ) -> &Option<i32> {
        self.end_time.get()
    }
    
    pub fn set_visibility_rule(
        &mut self,
        visibility_rule: String,
    ) {
        self.visibility_rule.update(visibility_rule)
    }

    pub fn get_visibility_rule(
        &self,
    ) -> &Option<String> {
        self.visibility_rule.get()
    }
    
}

impl Datum for Votes {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Votes>();
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

impl DatumDyn for Votes {
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

impl Entity for Votes {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrVoteId::datum_desc().clone(),
                
                AttrCreatorId::datum_desc().clone(),
                
                AttrTopic::datum_desc().clone(),
                
                AttrVoteType::datum_desc().clone(),
                
                AttrMaxChoices::datum_desc().clone(),
                
                AttrEndTime::datum_desc().clone(),
                
                AttrVisibilityRule::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            VOTE_ID => {
                attr_field_access::attr_get_binary::<_>(self.vote_id.get())
            }
            
            CREATOR_ID => {
                attr_field_access::attr_get_binary::<_>(self.creator_id.get())
            }
            
            TOPIC => {
                attr_field_access::attr_get_binary::<_>(self.topic.get())
            }
            
            VOTE_TYPE => {
                attr_field_access::attr_get_binary::<_>(self.vote_type.get())
            }
            
            MAX_CHOICES => {
                attr_field_access::attr_get_binary::<_>(self.max_choices.get())
            }
            
            END_TIME => {
                attr_field_access::attr_get_binary::<_>(self.end_time.get())
            }
            
            VISIBILITY_RULE => {
                attr_field_access::attr_get_binary::<_>(self.visibility_rule.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            VOTE_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.vote_id.get_mut(), binary.as_ref())?;
            }
            
            CREATOR_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.creator_id.get_mut(), binary.as_ref())?;
            }
            
            TOPIC => {
                attr_field_access::attr_set_binary::<_, _>(self.topic.get_mut(), binary.as_ref())?;
            }
            
            VOTE_TYPE => {
                attr_field_access::attr_set_binary::<_, _>(self.vote_type.get_mut(), binary.as_ref())?;
            }
            
            MAX_CHOICES => {
                attr_field_access::attr_set_binary::<_, _>(self.max_choices.get_mut(), binary.as_ref())?;
            }
            
            END_TIME => {
                attr_field_access::attr_set_binary::<_, _>(self.end_time.get_mut(), binary.as_ref())?;
            }
            
            VISIBILITY_RULE => {
                attr_field_access::attr_set_binary::<_, _>(self.visibility_rule.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            VOTE_ID => {
                attr_field_access::attr_get_value::<_>(self.vote_id.get())
            }
            
            CREATOR_ID => {
                attr_field_access::attr_get_value::<_>(self.creator_id.get())
            }
            
            TOPIC => {
                attr_field_access::attr_get_value::<_>(self.topic.get())
            }
            
            VOTE_TYPE => {
                attr_field_access::attr_get_value::<_>(self.vote_type.get())
            }
            
            MAX_CHOICES => {
                attr_field_access::attr_get_value::<_>(self.max_choices.get())
            }
            
            END_TIME => {
                attr_field_access::attr_get_value::<_>(self.end_time.get())
            }
            
            VISIBILITY_RULE => {
                attr_field_access::attr_get_value::<_>(self.visibility_rule.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            VOTE_ID => {
                attr_field_access::attr_set_value::<_, _>(self.vote_id.get_mut(), value)?;
            }
            
            CREATOR_ID => {
                attr_field_access::attr_set_value::<_, _>(self.creator_id.get_mut(), value)?;
            }
            
            TOPIC => {
                attr_field_access::attr_set_value::<_, _>(self.topic.get_mut(), value)?;
            }
            
            VOTE_TYPE => {
                attr_field_access::attr_set_value::<_, _>(self.vote_type.get_mut(), value)?;
            }
            
            MAX_CHOICES => {
                attr_field_access::attr_set_value::<_, _>(self.max_choices.get_mut(), value)?;
            }
            
            END_TIME => {
                attr_field_access::attr_set_value::<_, _>(self.end_time.get_mut(), value)?;
            }
            
            VISIBILITY_RULE => {
                attr_field_access::attr_set_value::<_, _>(self.visibility_rule.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
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
        VOTES
    }

    fn attr_name() -> &'static str {
        VOTE_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrCreatorId {
    is_dirty:bool,
    value: Option<String>
}

impl AttrCreatorId {
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

impl AttrValue<String> for AttrCreatorId {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        CREATOR_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrTopic {
    is_dirty:bool,
    value: Option<String>
}

impl AttrTopic {
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

impl AttrValue<String> for AttrTopic {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        TOPIC
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrVoteType {
    is_dirty:bool,
    value: Option<String>
}

impl AttrVoteType {
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

impl AttrValue<String> for AttrVoteType {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        VOTE_TYPE
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrMaxChoices {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrMaxChoices {
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

impl AttrValue<i32> for AttrMaxChoices {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        MAX_CHOICES
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrEndTime {
    is_dirty:bool,
    value: Option<i32>
}

impl AttrEndTime {
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

impl AttrValue<i32> for AttrEndTime {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        END_TIME
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrVisibilityRule {
    is_dirty:bool,
    value: Option<String>
}

impl AttrVisibilityRule {
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

impl AttrValue<String> for AttrVisibilityRule {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        VOTES
    }

    fn attr_name() -> &'static str {
        VISIBILITY_RULE
    }
}


}