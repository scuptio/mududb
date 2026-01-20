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
    const VOTE_RESULT: &str = "vote_result";

    const VOTE_ID: &str = "vote_id";

    const TOPIC: &str = "topic";

    const VOTE_ENDED: &str = "vote_ended";

    const TOTAL_VOTES: &str = "total_votes";

    const OPTIONS: &str = "options";


    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct VoteResult {
        vote_id: AttrVoteId,

        topic: AttrTopic,

        vote_ended: AttrVoteEnded,

        total_votes: AttrTotalVotes,

        options: AttrOptions,

    }

    impl TupleDatumMarker for VoteResult {}

    impl SQLParamMarker for VoteResult {}

    impl VoteResult {
        pub fn new(
            vote_id: Option<String>,
            topic: Option<String>,
            vote_ended: Option<i32>,
            total_votes: Option<i32>,
            options: Option<String>,
        ) -> Self {
            let s = Self {
                vote_id: AttrVoteId::from(vote_id),

                topic: AttrTopic::from(topic),

                vote_ended: AttrVoteEnded::from(vote_ended),

                total_votes: AttrTotalVotes::from(total_votes),

                options: AttrOptions::from(options),

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

        pub fn set_vote_ended(
            &mut self,
            vote_ended: i32,
        ) {
            self.vote_ended.update(vote_ended)
        }

        pub fn get_vote_ended(
            &self,
        ) -> &Option<i32> {
            self.vote_ended.get()
        }

        pub fn set_total_votes(
            &mut self,
            total_votes: i32,
        ) {
            self.total_votes.update(total_votes)
        }

        pub fn get_total_votes(
            &self,
        ) -> &Option<i32> {
            self.total_votes.get()
        }

        pub fn set_options(
            &mut self,
            options: String,
        ) {
            self.options.update(options)
        }

        pub fn get_options(
            &self,
        ) -> &Option<String> {
            self.options.get()
        }
    }

    impl Datum for VoteResult {
        fn dat_type() -> &'static DatType {
            lazy_static! {
            static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<VoteResult>();
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

    impl DatumDyn for VoteResult {
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

    impl Entity for VoteResult {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
            static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                
                AttrVoteId::datum_desc().clone(),
                
                AttrTopic::datum_desc().clone(),
                
                AttrVoteEnded::datum_desc().clone(),
                
                AttrTotalVotes::datum_desc().clone(),
                
                AttrOptions::datum_desc().clone(),
                
            ]);
        }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            VOTE_RESULT
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                VOTE_ID => {
                    attr_field_access::attr_get_binary::<_>(self.vote_id.get())
                }

                TOPIC => {
                    attr_field_access::attr_get_binary::<_>(self.topic.get())
            }

                VOTE_ENDED => {
                    attr_field_access::attr_get_binary::<_>(self.vote_ended.get())
                }

                TOTAL_VOTES => {
                    attr_field_access::attr_get_binary::<_>(self.total_votes.get())
                }

                OPTIONS => {
                    attr_field_access::attr_get_binary::<_>(self.options.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                VOTE_ID => {
                    attr_field_access::attr_set_binary::<_, _>(self.vote_id.get_mut(), binary.as_ref())?;
                }

                TOPIC => {
                    attr_field_access::attr_set_binary::<_, _>(self.topic.get_mut(), binary.as_ref())?;
                }

                VOTE_ENDED => {
                    attr_field_access::attr_set_binary::<_, _>(self.vote_ended.get_mut(), binary.as_ref())?;
                }

                TOTAL_VOTES => {
                    attr_field_access::attr_set_binary::<_, _>(self.total_votes.get_mut(), binary.as_ref())?;
                }

                OPTIONS => {
                    attr_field_access::attr_set_binary::<_, _>(self.options.get_mut(), binary.as_ref())?;
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

                TOPIC => {
                    attr_field_access::attr_get_value::<_>(self.topic.get())
                }

                VOTE_ENDED => {
                    attr_field_access::attr_get_value::<_>(self.vote_ended.get())
                }

                TOTAL_VOTES => {
                    attr_field_access::attr_get_value::<_>(self.total_votes.get())
            }

                OPTIONS => {
                    attr_field_access::attr_get_value::<_>(self.options.get())
                }

                _ => { panic!("unknown name"); }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                VOTE_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.vote_id.get_mut(), value)?;
                }

                TOPIC => {
                    attr_field_access::attr_set_value::<_, _>(self.topic.get_mut(), value)?;
                }

                VOTE_ENDED => {
                    attr_field_access::attr_set_value::<_, _>(self.vote_ended.get_mut(), value)?;
                }

                TOTAL_VOTES => {
                    attr_field_access::attr_set_value::<_, _>(self.total_votes.get_mut(), value)?;
                }

                OPTIONS => {
                    attr_field_access::attr_set_value::<_, _>(self.options.get_mut(), value)?;
                }

                _ => { panic!("unknown name"); }
            }
            Ok(())
        }
    }


    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrVoteId {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrVoteId {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
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
            VOTE_RESULT
        }

        fn attr_name() -> &'static str {
            VOTE_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrTopic {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrTopic {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
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
            VOTE_RESULT
        }

        fn attr_name() -> &'static str {
            TOPIC
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrVoteEnded {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrVoteEnded {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrVoteEnded {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            VOTE_RESULT
        }

        fn attr_name() -> &'static str {
            VOTE_ENDED
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrTotalVotes {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrTotalVotes {
        fn from(value: Option<i32>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<i32> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<i32> {
            &mut self.value
        }

        fn set(&mut self, value: Option<i32>) {
            self.value = value
        }

        fn update(&mut self, value: i32) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<i32> for AttrTotalVotes {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            VOTE_RESULT
        }

        fn attr_name() -> &'static str {
            TOTAL_VOTES
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOptions {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOptions {
        fn from(value: Option<String>) -> Self {
            Self {
                is_dirty: false,
                value,
            }
        }

        fn get(&self) -> &Option<String> {
            &self.value
        }

        fn get_mut(&mut self) -> &mut Option<String> {
            &mut self.value
        }

        fn set(&mut self, value: Option<String>) {
            self.value = value
        }

        fn update(&mut self, value: String) {
            self.is_dirty = true;
            self.value = Some(value)
        }
    }

    impl AttrValue<String> for AttrOptions {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            VOTE_RESULT
        }

        fn attr_name() -> &'static str {
            OPTIONS
        }
    }
}