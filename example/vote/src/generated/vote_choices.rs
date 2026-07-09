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
    const VOTE_CHOICES: &str = "vote_choices";

    const CHOICE_ID: &str = "choice_id";

    const ACTION_ID: &str = "action_id";

    const OPTION_ID: &str = "option_id";

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
            Self {
                choice_id: AttrChoiceId::from(choice_id),

                action_id: AttrActionId::from(action_id),

                option_id: AttrOptionId::from(option_id),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_choice_id(&mut self, choice_id: String) {
            self.choice_id.update(choice_id)
        }

        pub fn get_choice_id(&self) -> &Option<String> {
            self.choice_id.get()
        }

        pub fn set_action_id(&mut self, action_id: String) {
            self.action_id.update(action_id)
        }

        pub fn get_action_id(&self) -> &Option<String> {
            self.action_id.get()
        }

        pub fn set_option_id(&mut self, option_id: String) {
            self.option_id.update(option_id)
        }

        pub fn get_option_id(&self) -> &Option<String> {
            self.option_id.get()
        }
    }

    impl Datum for VoteChoices {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_data_type::<VoteChoices>)
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

    impl DatumDyn for VoteChoices {
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
                CHOICE_ID => attr_field_access::attr_get_binary::<_>(self.choice_id.get()),

                ACTION_ID => attr_field_access::attr_get_binary::<_>(self.action_id.get()),

                OPTION_ID => attr_field_access::attr_get_binary::<_>(self.option_id.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                CHOICE_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.choice_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                ACTION_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.action_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                OPTION_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.option_id.get_mut(),
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
                CHOICE_ID => attr_field_access::attr_get_value::<_>(self.choice_id.get()),

                ACTION_ID => attr_field_access::attr_get_value::<_>(self.action_id.get()),

                OPTION_ID => attr_field_access::attr_get_value::<_>(self.option_id.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DataValue>>(&mut self, field: &str, value: B) -> RS<()> {
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

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrChoiceId {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrChoiceId {
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

    impl AttrValue<String> for AttrChoiceId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
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
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrActionId {
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

    impl AttrValue<String> for AttrActionId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
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
            VOTE_CHOICES
        }

        fn attr_name() -> &'static str {
            OPTION_ID
        }
    }
}
