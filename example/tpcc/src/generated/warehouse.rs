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
    const WAREHOUSE: &str = "warehouse";

    const W_ID: &str = "w_id";

    const W_NAME: &str = "w_name";

    const W_TAX: &str = "w_tax";

    const W_YTD: &str = "w_ytd";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Warehouse {
        w_id: AttrWId,

        w_name: AttrWName,

        w_tax: AttrWTax,

        w_ytd: AttrWYtd,
    }

    impl TupleDatumMarker for Warehouse {}

    impl SQLParamMarker for Warehouse {}

    impl Warehouse {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            w_id: Option<i32>,
            w_name: Option<String>,
            w_tax: Option<i32>,
            w_ytd: Option<i32>,
        ) -> Self {
            Self {
                w_id: AttrWId::from(w_id),

                w_name: AttrWName::from(w_name),

                w_tax: AttrWTax::from(w_tax),

                w_ytd: AttrWYtd::from(w_ytd),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_w_id(&mut self, w_id: i32) {
            self.w_id.update(w_id)
        }

        pub fn get_w_id(&self) -> &Option<i32> {
            self.w_id.get()
        }

        pub fn set_w_name(&mut self, w_name: String) {
            self.w_name.update(w_name)
        }

        pub fn get_w_name(&self) -> &Option<String> {
            self.w_name.get()
        }

        pub fn set_w_tax(&mut self, w_tax: i32) {
            self.w_tax.update(w_tax)
        }

        pub fn get_w_tax(&self) -> &Option<i32> {
            self.w_tax.get()
        }

        pub fn set_w_ytd(&mut self, w_ytd: i32) {
            self.w_ytd.update(w_ytd)
        }

        pub fn get_w_ytd(&self) -> &Option<i32> {
            self.w_ytd.get()
        }
    }

    impl Datum for Warehouse {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_data_type::<Warehouse>)
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

    impl DatumDyn for Warehouse {
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

    impl Entity for Warehouse {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrWId::datum_desc().clone(),
                    AttrWName::datum_desc().clone(),
                    AttrWTax::datum_desc().clone(),
                    AttrWYtd::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            WAREHOUSE
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                W_ID => attr_field_access::attr_get_binary::<_>(self.w_id.get()),

                W_NAME => attr_field_access::attr_get_binary::<_>(self.w_name.get()),

                W_TAX => attr_field_access::attr_get_binary::<_>(self.w_tax.get()),

                W_YTD => attr_field_access::attr_get_binary::<_>(self.w_ytd.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                W_NAME => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.w_name.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                W_TAX => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.w_tax.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                W_YTD => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.w_ytd.get_mut(),
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
                W_ID => attr_field_access::attr_get_value::<_>(self.w_id.get()),

                W_NAME => attr_field_access::attr_get_value::<_>(self.w_name.get()),

                W_TAX => attr_field_access::attr_get_value::<_>(self.w_tax.get()),

                W_YTD => attr_field_access::attr_get_value::<_>(self.w_ytd.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DataValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.w_id.get_mut(), value)?;
                }

                W_NAME => {
                    attr_field_access::attr_set_value::<_, _>(self.w_name.get_mut(), value)?;
                }

                W_TAX => {
                    attr_field_access::attr_set_value::<_, _>(self.w_tax.get_mut(), value)?;
                }

                W_YTD => {
                    attr_field_access::attr_set_value::<_, _>(self.w_ytd.get_mut(), value)?;
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
    pub struct AttrWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrWId {
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

    impl AttrValue<i32> for AttrWId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            WAREHOUSE
        }

        fn attr_name() -> &'static str {
            W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrWName {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrWName {
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

    impl AttrValue<String> for AttrWName {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            WAREHOUSE
        }

        fn attr_name() -> &'static str {
            W_NAME
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrWTax {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrWTax {
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

    impl AttrValue<i32> for AttrWTax {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            WAREHOUSE
        }

        fn attr_name() -> &'static str {
            W_TAX
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrWYtd {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrWYtd {
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

    impl AttrValue<i32> for AttrWYtd {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            WAREHOUSE
        }

        fn attr_name() -> &'static str {
            W_YTD
        }
    }
}
