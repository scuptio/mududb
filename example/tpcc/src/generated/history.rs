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
    const HISTORY: &str = "history";

    const H_ID: &str = "h_id";

    const H_C_ID: &str = "h_c_id";

    const H_C_D_ID: &str = "h_c_d_id";

    const H_C_W_ID: &str = "h_c_w_id";

    const H_D_ID: &str = "h_d_id";

    const H_W_ID: &str = "h_w_id";

    const H_AMOUNT: &str = "h_amount";

    const H_DATA: &str = "h_data";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct History {
        h_id: AttrHId,

        h_c_id: AttrHCId,

        h_c_d_id: AttrHCDId,

        h_c_w_id: AttrHCWId,

        h_d_id: AttrHDId,

        h_w_id: AttrHWId,

        h_amount: AttrHAmount,

        h_data: AttrHData,
    }

    impl TupleDatumMarker for History {}

    impl SQLParamMarker for History {}

    impl History {
        #[allow(clippy::too_many_arguments)]
        pub fn new(
            h_id: Option<String>,
            h_c_id: Option<i32>,
            h_c_d_id: Option<i32>,
            h_c_w_id: Option<i32>,
            h_d_id: Option<i32>,
            h_w_id: Option<i32>,
            h_amount: Option<i32>,
            h_data: Option<String>,
        ) -> Self {
            Self {
                h_id: AttrHId::from(h_id),

                h_c_id: AttrHCId::from(h_c_id),

                h_c_d_id: AttrHCDId::from(h_c_d_id),

                h_c_w_id: AttrHCWId::from(h_c_w_id),

                h_d_id: AttrHDId::from(h_d_id),

                h_w_id: AttrHWId::from(h_w_id),

                h_amount: AttrHAmount::from(h_amount),

                h_data: AttrHData::from(h_data),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_h_id(&mut self, h_id: String) {
            self.h_id.update(h_id)
        }

        pub fn get_h_id(&self) -> &Option<String> {
            self.h_id.get()
        }

        pub fn set_h_c_id(&mut self, h_c_id: i32) {
            self.h_c_id.update(h_c_id)
        }

        pub fn get_h_c_id(&self) -> &Option<i32> {
            self.h_c_id.get()
        }

        pub fn set_h_c_d_id(&mut self, h_c_d_id: i32) {
            self.h_c_d_id.update(h_c_d_id)
        }

        pub fn get_h_c_d_id(&self) -> &Option<i32> {
            self.h_c_d_id.get()
        }

        pub fn set_h_c_w_id(&mut self, h_c_w_id: i32) {
            self.h_c_w_id.update(h_c_w_id)
        }

        pub fn get_h_c_w_id(&self) -> &Option<i32> {
            self.h_c_w_id.get()
        }

        pub fn set_h_d_id(&mut self, h_d_id: i32) {
            self.h_d_id.update(h_d_id)
        }

        pub fn get_h_d_id(&self) -> &Option<i32> {
            self.h_d_id.get()
        }

        pub fn set_h_w_id(&mut self, h_w_id: i32) {
            self.h_w_id.update(h_w_id)
        }

        pub fn get_h_w_id(&self) -> &Option<i32> {
            self.h_w_id.get()
        }

        pub fn set_h_amount(&mut self, h_amount: i32) {
            self.h_amount.update(h_amount)
        }

        pub fn get_h_amount(&self) -> &Option<i32> {
            self.h_amount.get()
        }

        pub fn set_h_data(&mut self, h_data: String) {
            self.h_data.update(h_data)
        }

        pub fn get_h_data(&self) -> &Option<String> {
            self.h_data.get()
        }
    }

    impl Datum for History {
        fn dat_type() -> DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_dat_type::<History>)
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

    impl DatumDyn for History {
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

    impl Entity for History {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrHId::datum_desc().clone(),
                    AttrHCId::datum_desc().clone(),
                    AttrHCDId::datum_desc().clone(),
                    AttrHCWId::datum_desc().clone(),
                    AttrHDId::datum_desc().clone(),
                    AttrHWId::datum_desc().clone(),
                    AttrHAmount::datum_desc().clone(),
                    AttrHData::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                H_ID => attr_field_access::attr_get_binary::<_>(self.h_id.get()),

                H_C_ID => attr_field_access::attr_get_binary::<_>(self.h_c_id.get()),

                H_C_D_ID => attr_field_access::attr_get_binary::<_>(self.h_c_d_id.get()),

                H_C_W_ID => attr_field_access::attr_get_binary::<_>(self.h_c_w_id.get()),

                H_D_ID => attr_field_access::attr_get_binary::<_>(self.h_d_id.get()),

                H_W_ID => attr_field_access::attr_get_binary::<_>(self.h_w_id.get()),

                H_AMOUNT => attr_field_access::attr_get_binary::<_>(self.h_amount.get()),

                H_DATA => attr_field_access::attr_get_binary::<_>(self.h_data.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                H_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_C_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_c_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_C_D_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_c_d_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_C_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_c_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_D_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_d_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_AMOUNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_amount.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                H_DATA => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.h_data.get_mut(),
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
                H_ID => attr_field_access::attr_get_value::<_>(self.h_id.get()),

                H_C_ID => attr_field_access::attr_get_value::<_>(self.h_c_id.get()),

                H_C_D_ID => attr_field_access::attr_get_value::<_>(self.h_c_d_id.get()),

                H_C_W_ID => attr_field_access::attr_get_value::<_>(self.h_c_w_id.get()),

                H_D_ID => attr_field_access::attr_get_value::<_>(self.h_d_id.get()),

                H_W_ID => attr_field_access::attr_get_value::<_>(self.h_w_id.get()),

                H_AMOUNT => attr_field_access::attr_get_value::<_>(self.h_amount.get()),

                H_DATA => attr_field_access::attr_get_value::<_>(self.h_data.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                H_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_id.get_mut(), value)?;
                }

                H_C_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_c_id.get_mut(), value)?;
                }

                H_C_D_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_c_d_id.get_mut(), value)?;
                }

                H_C_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_c_w_id.get_mut(), value)?;
                }

                H_D_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_d_id.get_mut(), value)?;
                }

                H_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.h_w_id.get_mut(), value)?;
                }

                H_AMOUNT => {
                    attr_field_access::attr_set_value::<_, _>(self.h_amount.get_mut(), value)?;
                }

                H_DATA => {
                    attr_field_access::attr_set_value::<_, _>(self.h_data.get_mut(), value)?;
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
    pub struct AttrHId {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrHId {
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

    impl AttrValue<String> for AttrHId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHCId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHCId {
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

    impl AttrValue<i32> for AttrHCId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_C_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHCDId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHCDId {
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

    impl AttrValue<i32> for AttrHCDId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_C_D_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHCWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHCWId {
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

    impl AttrValue<i32> for AttrHCWId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_C_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHDId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHDId {
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

    impl AttrValue<i32> for AttrHDId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_D_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHWId {
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

    impl AttrValue<i32> for AttrHWId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHAmount {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrHAmount {
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

    impl AttrValue<i32> for AttrHAmount {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_AMOUNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrHData {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrHData {
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

    impl AttrValue<String> for AttrHData {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            HISTORY
        }

        fn attr_name() -> &'static str {
            H_DATA
        }
    }
}
