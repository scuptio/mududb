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
    const ORDERS: &str = "orders";

    const O_ID: &str = "o_id";

    const O_D_ID: &str = "o_d_id";

    const O_W_ID: &str = "o_w_id";

    const O_C_ID: &str = "o_c_id";

    const O_ENTRY_D: &str = "o_entry_d";

    const O_CARRIER_ID: &str = "o_carrier_id";

    const O_OL_CNT: &str = "o_ol_cnt";

    const O_ALL_LOCAL: &str = "o_all_local";

    const O_STATUS: &str = "o_status";

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Orders {
        o_id: AttrOId,

        o_d_id: AttrODId,

        o_w_id: AttrOWId,

        o_c_id: AttrOCId,

        o_entry_d: AttrOEntryD,

        o_carrier_id: AttrOCarrierId,

        o_ol_cnt: AttrOOlCnt,

        o_all_local: AttrOAllLocal,

        o_status: AttrOStatus,
    }

    pub struct OrdersParams {
        pub o_id: Option<i32>,
        pub o_d_id: Option<i32>,
        pub o_w_id: Option<i32>,
        pub o_c_id: Option<i32>,
        pub o_entry_d: Option<String>,
        pub o_carrier_id: Option<i32>,
        pub o_ol_cnt: Option<i32>,
        pub o_all_local: Option<i32>,
        pub o_status: Option<String>,
    }

    impl TupleDatumMarker for Orders {}

    impl SQLParamMarker for Orders {}

    impl Orders {
        pub fn new(params: OrdersParams) -> Self {
            Self {
                o_id: AttrOId::from(params.o_id),

                o_d_id: AttrODId::from(params.o_d_id),

                o_w_id: AttrOWId::from(params.o_w_id),

                o_c_id: AttrOCId::from(params.o_c_id),

                o_entry_d: AttrOEntryD::from(params.o_entry_d),

                o_carrier_id: AttrOCarrierId::from(params.o_carrier_id),

                o_ol_cnt: AttrOOlCnt::from(params.o_ol_cnt),

                o_all_local: AttrOAllLocal::from(params.o_all_local),

                o_status: AttrOStatus::from(params.o_status),
            }
        }

        pub fn new_empty() -> Self {
            Self::default()
        }

        pub fn set_o_id(&mut self, o_id: i32) {
            self.o_id.update(o_id)
        }

        pub fn get_o_id(&self) -> &Option<i32> {
            self.o_id.get()
        }

        pub fn set_o_d_id(&mut self, o_d_id: i32) {
            self.o_d_id.update(o_d_id)
        }

        pub fn get_o_d_id(&self) -> &Option<i32> {
            self.o_d_id.get()
        }

        pub fn set_o_w_id(&mut self, o_w_id: i32) {
            self.o_w_id.update(o_w_id)
        }

        pub fn get_o_w_id(&self) -> &Option<i32> {
            self.o_w_id.get()
        }

        pub fn set_o_c_id(&mut self, o_c_id: i32) {
            self.o_c_id.update(o_c_id)
        }

        pub fn get_o_c_id(&self) -> &Option<i32> {
            self.o_c_id.get()
        }

        pub fn set_o_entry_d(&mut self, o_entry_d: String) {
            self.o_entry_d.update(o_entry_d)
        }

        pub fn get_o_entry_d(&self) -> &Option<String> {
            self.o_entry_d.get()
        }

        pub fn set_o_carrier_id(&mut self, o_carrier_id: i32) {
            self.o_carrier_id.update(o_carrier_id)
        }

        pub fn get_o_carrier_id(&self) -> &Option<i32> {
            self.o_carrier_id.get()
        }

        pub fn set_o_ol_cnt(&mut self, o_ol_cnt: i32) {
            self.o_ol_cnt.update(o_ol_cnt)
        }

        pub fn get_o_ol_cnt(&self) -> &Option<i32> {
            self.o_ol_cnt.get()
        }

        pub fn set_o_all_local(&mut self, o_all_local: i32) {
            self.o_all_local.update(o_all_local)
        }

        pub fn get_o_all_local(&self) -> &Option<i32> {
            self.o_all_local.get()
        }

        pub fn set_o_status(&mut self, o_status: String) {
            self.o_status.update(o_status)
        }

        pub fn get_o_status(&self) -> &Option<String> {
            self.o_status.get()
        }
    }

    impl Datum for Orders {
        fn dat_type() -> DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_dat_type::<Orders>)
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

    impl DatumDyn for Orders {
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

    impl Entity for Orders {
        fn new_empty() -> Self {
            Self::new_empty()
        }

        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrOId::datum_desc().clone(),
                    AttrODId::datum_desc().clone(),
                    AttrOWId::datum_desc().clone(),
                    AttrOCId::datum_desc().clone(),
                    AttrOEntryD::datum_desc().clone(),
                    AttrOCarrierId::datum_desc().clone(),
                    AttrOOlCnt::datum_desc().clone(),
                    AttrOAllLocal::datum_desc().clone(),
                    AttrOStatus::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn get_field_binary(&self, field: &str) -> RS<Option<Vec<u8>>> {
            match field {
                O_ID => attr_field_access::attr_get_binary::<_>(self.o_id.get()),

                O_D_ID => attr_field_access::attr_get_binary::<_>(self.o_d_id.get()),

                O_W_ID => attr_field_access::attr_get_binary::<_>(self.o_w_id.get()),

                O_C_ID => attr_field_access::attr_get_binary::<_>(self.o_c_id.get()),

                O_ENTRY_D => attr_field_access::attr_get_binary::<_>(self.o_entry_d.get()),

                O_CARRIER_ID => attr_field_access::attr_get_binary::<_>(self.o_carrier_id.get()),

                O_OL_CNT => attr_field_access::attr_get_binary::<_>(self.o_ol_cnt.get()),

                O_ALL_LOCAL => attr_field_access::attr_get_binary::<_>(self.o_all_local.get()),

                O_STATUS => attr_field_access::attr_get_binary::<_>(self.o_status.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, field: &str, binary: B) -> RS<()> {
            match field {
                O_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_D_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_d_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_w_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_C_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_c_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_ENTRY_D => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_entry_d.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_CARRIER_ID => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_carrier_id.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_OL_CNT => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_ol_cnt.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_ALL_LOCAL => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_all_local.get_mut(),
                        binary.as_ref(),
                    )?;
                }

                O_STATUS => {
                    attr_field_access::attr_set_binary::<_, _>(
                        self.o_status.get_mut(),
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
                O_ID => attr_field_access::attr_get_value::<_>(self.o_id.get()),

                O_D_ID => attr_field_access::attr_get_value::<_>(self.o_d_id.get()),

                O_W_ID => attr_field_access::attr_get_value::<_>(self.o_w_id.get()),

                O_C_ID => attr_field_access::attr_get_value::<_>(self.o_c_id.get()),

                O_ENTRY_D => attr_field_access::attr_get_value::<_>(self.o_entry_d.get()),

                O_CARRIER_ID => attr_field_access::attr_get_value::<_>(self.o_carrier_id.get()),

                O_OL_CNT => attr_field_access::attr_get_value::<_>(self.o_ol_cnt.get()),

                O_ALL_LOCAL => attr_field_access::attr_get_value::<_>(self.o_all_local.get()),

                O_STATUS => attr_field_access::attr_get_value::<_>(self.o_status.get()),

                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, field: &str, value: B) -> RS<()> {
            match field {
                O_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.o_id.get_mut(), value)?;
                }

                O_D_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.o_d_id.get_mut(), value)?;
                }

                O_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.o_w_id.get_mut(), value)?;
                }

                O_C_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.o_c_id.get_mut(), value)?;
                }

                O_ENTRY_D => {
                    attr_field_access::attr_set_value::<_, _>(self.o_entry_d.get_mut(), value)?;
                }

                O_CARRIER_ID => {
                    attr_field_access::attr_set_value::<_, _>(self.o_carrier_id.get_mut(), value)?;
                }

                O_OL_CNT => {
                    attr_field_access::attr_set_value::<_, _>(self.o_ol_cnt.get_mut(), value)?;
                }

                O_ALL_LOCAL => {
                    attr_field_access::attr_set_value::<_, _>(self.o_all_local.get_mut(), value)?;
                }

                O_STATUS => {
                    attr_field_access::attr_set_value::<_, _>(self.o_status.get_mut(), value)?;
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
    pub struct AttrOId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOId {
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

    impl AttrValue<i32> for AttrOId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrODId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrODId {
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

    impl AttrValue<i32> for AttrODId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_D_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOWId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOWId {
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

    impl AttrValue<i32> for AttrOWId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_W_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOCId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOCId {
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

    impl AttrValue<i32> for AttrOCId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_C_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOEntryD {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOEntryD {
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

    impl AttrValue<String> for AttrOEntryD {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_ENTRY_D
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOCarrierId {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOCarrierId {
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

    impl AttrValue<i32> for AttrOCarrierId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_CARRIER_ID
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOOlCnt {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOOlCnt {
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

    impl AttrValue<i32> for AttrOOlCnt {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_OL_CNT
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOAllLocal {
        is_dirty: bool,
        value: Option<i32>,
    }

    impl AttrOAllLocal {
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

    impl AttrValue<i32> for AttrOAllLocal {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_ALL_LOCAL
        }
    }

    // attribute struct definition
    #[derive(Default, Clone, Debug)]
    pub struct AttrOStatus {
        is_dirty: bool,
        value: Option<String>,
    }

    impl AttrOStatus {
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

    impl AttrValue<String> for AttrOStatus {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_dat_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            ORDERS
        }

        fn attr_name() -> &'static str {
            O_STATUS
        }
    }
}
