pub mod object {
    use lazy_static::lazy_static;
    use mududb::common::result::RS;
    use mududb::contract::database::attr_field_access;
    use mududb::contract::database::attr_value::AttrValue;
    use mududb::contract::database::entity::Entity;
    use mududb::contract::database::entity_utils;
    use mududb::contract::database::sql_params::SQLParamMarker;
    use mududb::contract::tuple::datum_desc::DatumDesc;
    use mududb::contract::tuple::tuple_datum::TupleDatumMarker;
    use mududb::contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mududb::types::data_binary::DataBinary;
    use mududb::types::data_textual::DataTextual;
    use mududb::types::data_type::DataType;
    use mududb::types::data_value::DataValue;
    use mududb::types::datum::{Datum, DatumDyn};
    use mududb::types::type_family::TypeFamily;

    // constant definition
    const OPTIONS: &str = "options";

    const OPTION_ID: &str = "option_id";

    const VOTE_ID: &str = "vote_id";

    const OPTION_TEXT: &str = "option_text";

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
            Self {
                option_id: AttrOptionId::from(option_id),

                vote_id: AttrVoteId::from(vote_id),

                option_text: AttrOptionText::from(option_text),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_option_id(&mut self, option_id: String) {
            self.option_id.update(option_id)
        }

        pub fn get_option_id(&self) -> &Option<String> {
            self.option_id.get()
        }

        pub fn set_vote_id(&mut self, vote_id: String) {
            self.vote_id.update(vote_id)
        }

        pub fn get_vote_id(&self) -> &Option<String> {
            self.vote_id.get()
        }

        pub fn set_option_text(&mut self, option_text: String) {
            self.option_text.update(option_text)
        }

        pub fn get_option_text(&self) -> &Option<String> {
            self.option_text.get()
        }
    }

    impl Datum for Options {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_data_type::<Options>)
                .clone()
        }

        fn from_binary(binary: &[u8]) -> RS<Self> {
            entity_utils::entity_from_binary(binary)
        }

        fn from_value(value: &DataValue) -> RS<Self> {
            entity_utils::entity_from_value(value)
        }

        fn from_textual(textual: &str) -> RS<Self> {
            entity_utils::entity_from_textual(textual)
        }
    }

    impl DatumDyn for Options {
        fn type_family(&self) -> RS<TypeFamily> {
            entity_utils::entity_type_family()
        }

        fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
            entity_utils::entity_to_binary(self, data_type)
        }

        fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
            entity_utils::entity_to_textual(self, data_type)
        }

        fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
            entity_utils::entity_to_value(self, data_type)
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
                OPTION_ID => attr_field_access::attr_get_binary::<_>(self.option_id.get()),

                VOTE_ID => attr_field_access::attr_get_binary::<_>(self.vote_id.get()),

                OPTION_TEXT => attr_field_access::attr_get_binary::<_>(self.option_text.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                OPTION_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.option_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                VOTE_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.vote_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OPTION_TEXT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.option_text.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DataValue>> {
            match field {
                OPTION_ID => attr_field_access::attr_get_value::<_>(self.option_id.get()),

                VOTE_ID => attr_field_access::attr_get_value::<_>(self.vote_id.get()),

                OPTION_TEXT => attr_field_access::attr_get_value::<_>(self.option_text.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DataValue>>(&mut self, field: &str, value: B) -> RS<()> {
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

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOptionId {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOptionId {
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

    impl AttrValue<String> for AttrOptionId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
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
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
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
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOptionText {
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

    impl AttrValue<String> for AttrOptionText {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            OPTIONS
        }

        fn attr_name() -> &'static str {
            OPTION_TEXT
        }
    }
}
