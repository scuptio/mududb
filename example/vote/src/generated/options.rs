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
const OPTIONS:&str = "options";

const OPTION_ID:&str = "option_id";

const VOTE_ID:&str = "vote_id";

const OPTION_TEXT:&str = "option_text";


// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct Options {
    
    option_id: AttrOptionId,
    
    vote_id: AttrVoteId,
    
    option_text: AttrOptionText,
    
}

impl TupleDatumMarker for Options {}

impl SQLParamMarker for Options {}

impl Options {
    pub fn new(
        option_id: Option<String>,
        vote_id: Option<String>,
        option_text: Option<String>,
        
    ) -> Self {
        let s = Self {
            
            option_id : AttrOptionId::from(option_id),
            
            vote_id : AttrVoteId::from(vote_id),
            
            option_text : AttrOptionText::from(option_text),
            
        };
        s
    }

    pub fn new_empty() -> Self {
        Self::default()
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
    
    pub fn set_option_text(
        &mut self,
        option_text: String,
    ) {
        self.option_text.update(option_text)
    }

    pub fn get_option_text(
        &self,
    ) -> &Option<String> {
        self.option_text.get()
    }
    
}

impl Datum for Options {
    fn dat_type() -> &'static DatType {
        lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Options>();
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

impl DatumDyn for Options {
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

impl Entity for Options {
    fn new_empty() -> Self {
        Self::new_empty()
    }

    fn tuple_desc() -> &'static TupleFieldDesc {
        lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrOptionId::datum_desc().clone(),
                
                AttrVoteId::datum_desc().clone(),
                
                AttrOptionText::datum_desc().clone(),
                
            ]);
        }
        &TUPLE_DESC
    }

    fn object_name() -> &'static str {
        OPTIONS
    }

    fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
        match field {
            
            OPTION_ID => {
                attr_field_access::attr_get_binary::<_>(self.option_id.get())
            }
            
            VOTE_ID => {
                attr_field_access::attr_get_binary::<_>(self.vote_id.get())
            }
            
            OPTION_TEXT => {
                attr_field_access::attr_get_binary::<_>(self.option_text.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
        match field {
            
            OPTION_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.option_id.get_mut(), binary.as_ref())?;
            }
            
            VOTE_ID => {
                attr_field_access::attr_set_binary::<_, _>(self.vote_id.get_mut(), binary.as_ref())?;
            }
            
            OPTION_TEXT => {
                attr_field_access::attr_set_binary::<_, _>(self.option_text.get_mut(), binary.as_ref())?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
    }

    fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
        match field {
            
            OPTION_ID => {
                attr_field_access::attr_get_value::<_>(self.option_id.get())
            }
            
            VOTE_ID => {
                attr_field_access::attr_get_value::<_>(self.vote_id.get())
            }
            
            OPTION_TEXT => {
                attr_field_access::attr_get_value::<_>(self.option_text.get())
            }
            
            _ => { panic!("unknown name"); }
        }
    }

    fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
        match field {
            
            OPTION_ID => {
                attr_field_access::attr_set_value::<_, _>(self.option_id.get_mut(), value)?;
            }
            
            VOTE_ID => {
                attr_field_access::attr_set_value::<_, _>(self.vote_id.get_mut(), value)?;
            }
            
            OPTION_TEXT => {
                attr_field_access::attr_set_value::<_, _>(self.option_text.get_mut(), value)?;
            }
            
            _ => { panic!("unknown name"); }
        }
        Ok(())
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
        OPTIONS
    }

    fn attr_name() -> &'static str {
        OPTION_ID
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
        OPTIONS
    }

    fn attr_name() -> &'static str {
        VOTE_ID
    }
}

// attribute struct definition
#[derive(Default, Clone, Debug)]
pub struct AttrOptionText {
    is_dirty:bool,
    value: Option<String>
}

impl AttrOptionText {
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

impl AttrValue<String> for AttrOptionText {
    fn dat_type() -> &'static DatType {
        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
    }

    fn datum_desc() -> &'static DatumDesc {
        static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
    }

    fn object_name() -> &'static str {
        OPTIONS
    }

    fn attr_name() -> &'static str {
        OPTION_TEXT
    }
}


}