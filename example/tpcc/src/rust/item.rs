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
    use mududb::types::dat_binary::DatBinary;
    use mududb::types::dat_textual::DatTextual;
    use mududb::types::dat_type::DatType;
    use mududb::types::dat_type_id::DatTypeID;
    use mududb::types::dat_value::DatValue;
    use mududb::types::datum::{Datum, DatumDyn};

    // constant definition
    const ITEM: &str = "item";

    const I_ID: &str = "i_id";

    const I_NAME: &str = "i_name";

    const I_PRICE: &str = "i_price";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Item {
        i_id: AttrIId,

        i_name: AttrIName,

        i_price: AttrIPrice,
    }

    impl TupleDatumMarker for Item {}

    impl SQLParamMarker for Item {}

    impl Item {
        pub fn new(i_id: Option<i32>, i_name: Option<String>, i_price: Option<i32>) -> Self {
            Self {
                i_id: AttrIId::from(i_id),

                i_name: AttrIName::from(i_name),

                i_price: AttrIPrice::from(i_price),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_i_id(&mut self, i_id: i32) {
            self.i_id.update(i_id)
        }

        pub fn get_i_id(&self) -> &Option<i32> {
            self.i_id.get()
        }

        pub fn set_i_name(&mut self, i_name: String) {
            self.i_name.update(i_name)
        }

        pub fn get_i_name(&self) -> &Option<String> {
            self.i_name.get()
        }

        pub fn set_i_price(&mut self, i_price: i32) {
            self.i_price.update(i_price)
        }

        pub fn get_i_price(&self) -> &Option<i32> {
            self.i_price.get()
        }
    }

    impl Datum for Item {
        fn dat_type() -> DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_dat_type::<Item>)
                .clone()
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

    impl DatumDyn for Item {
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

    impl Entity for Item {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrIId::datum_desc().clone(),
                    AttrIName::datum_desc().clone(),
                    AttrIPrice::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            ITEM
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                I_ID => attr_field_access::attr_get_binary::<_>(self.i_id.get()),

                I_NAME => attr_field_access::attr_get_binary::<_>(self.i_name.get()),

                I_PRICE => attr_field_access::attr_get_binary::<_>(self.i_price.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                I_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.i_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                I_NAME => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.i_name.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                I_PRICE => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.i_price.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }

        fn get_field_value(&self, field: &str) -> RS<Option<DatValue>> {
            match field {
                I_ID => attr_field_access::attr_get_value::<_>(self.i_id.get()),

                I_NAME => attr_field_access::attr_get_value::<_>(self.i_name.get()),

                I_PRICE => attr_field_access::attr_get_value::<_>(self.i_price.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                I_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.i_id.get_mut(), value)?;
                }

                I_NAME => {
                    attr_field_access::attr_set_value::<_, _>(self.i_name.get_mut(), value)?;
                }

                I_PRICE => {
                    attr_field_access::attr_set_value::<_, _>(self.i_price.get_mut(), value)?;
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
    pub struct AttrIId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrIId {
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

    impl AttrValue<i32> for AttrIId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ITEM
        }

        fn attr_name() -> &'static str {
            I_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrIName {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrIName {
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

    impl AttrValue<String> for AttrIName {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ITEM
        }

        fn attr_name() -> &'static str {
            I_NAME
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrIPrice {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrIPrice {
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

    impl AttrValue<i32> for AttrIPrice {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ITEM
        }

        fn attr_name() -> &'static str {
            I_PRICE
        }
    }
}
