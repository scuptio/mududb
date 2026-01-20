pub mod object {
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_contract::database::attr_field_access;
    use mudu_contract::database::attr_value::AttrValue;
    use mudu_contract::database::entity::Entity;
    use mudu_contract::database::entity_utils;
    use mudu_contract::tuple::datum_desc::DatumDesc;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_type::dat_binary::DatBinary;
    use mudu_type::dat_textual::DatTextual;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;
    use mudu_type::datum::{Datum, DatumDyn};

    const TABLE_WAREHOUSE: &str = "warehouse";
    const COLUMN_W_ID: &str = "w_id";
    const COLUMN_W_YTD: &str = "w_ytd";
    const COLUMN_W_TAX: &str = "w_tax";
    const COLUMN_W_NAME: &str = "w_name";
    const COLUMN_W_STREET_1: &str = "w_street_1";
    const COLUMN_W_STREET_2: &str = "w_street_2";
    const COLUMN_W_CITY: &str = "w_city";
    const COLUMN_W_STATE: &str = "w_state";
    const COLUMN_W_ZIP: &str = "w_zip";

    #[derive(Debug, Clone)]
    pub struct Warehouse {
        w_id: Option<i32>,
        w_ytd: Option<f64>,
        w_tax: Option<f64>,
        w_name: Option<String>,
        w_street_1: Option<String>,
        w_street_2: Option<String>,
        w_city: Option<String>,
        w_state: Option<String>,
        w_zip: Option<String>,
    }

    impl Warehouse {
        pub fn new(
            w_id: Option<i32>,
            w_ytd: Option<f64>,
            w_tax: Option<f64>,
            w_name: Option<String>,
            w_street_1: Option<String>,
            w_street_2: Option<String>,
            w_city: Option<String>,
            w_state: Option<String>,
            w_zip: Option<String>,
        ) -> Self {
            let s = Self {
                w_id,
                w_ytd,
                w_tax,
                w_name,
                w_street_1,
                w_street_2,
                w_city,
                w_state,
                w_zip,
            };
            s
        }

        pub fn set_w_id(&mut self, w_id: i32) {
            self.w_id = Some(w_id);
        }

        pub fn get_w_id(&self) -> &Option<i32> {
            &self.w_id
        }

        pub fn set_w_ytd(&mut self, w_ytd: f64) {
            self.w_ytd = Some(w_ytd);
        }

        pub fn get_w_ytd(&self) -> &Option<f64> {
            &self.w_ytd
        }

        pub fn set_w_tax(&mut self, w_tax: f64) {
            self.w_tax = Some(w_tax);
        }

        pub fn get_w_tax(&self) -> &Option<f64> {
            &self.w_tax
        }

        pub fn set_w_name(&mut self, w_name: String) {
            self.w_name = Some(w_name);
        }

        pub fn get_w_name(&self) -> &Option<String> {
            &self.w_name
        }

        pub fn set_w_street_1(&mut self, w_street_1: String) {
            self.w_street_1 = Some(w_street_1);
        }

        pub fn get_w_street_1(&self) -> &Option<String> {
            &self.w_street_1
        }

        pub fn set_w_street_2(&mut self, w_street_2: String) {
            self.w_street_2 = Some(w_street_2);
        }

        pub fn get_w_street_2(&self) -> &Option<String> {
            &self.w_street_2
        }

        pub fn set_w_city(&mut self, w_city: String) {
            self.w_city = Some(w_city);
        }

        pub fn get_w_city(&self) -> &Option<String> {
            &self.w_city
        }

        pub fn set_w_state(&mut self, w_state: String) {
            self.w_state = Some(w_state);
        }

        pub fn get_w_state(&self) -> &Option<String> {
            &self.w_state
        }

        pub fn set_w_zip(&mut self, w_zip: String) {
            self.w_zip = Some(w_zip);
        }

        pub fn get_w_zip(&self) -> &Option<String> {
            &self.w_zip
        }
    }

    impl Datum for Warehouse {
        fn dat_type() -> &'static DatType {
            lazy_static! {
                static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Warehouse>();
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

    impl DatumDyn for Warehouse {
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

    impl Entity for Warehouse {
        fn new_empty() -> Self {
            let s = Self {
                w_id: None,
                w_ytd: None,
                w_tax: None,
                w_name: None,
                w_street_1: None,
                w_street_2: None,
                w_city: None,
                w_state: None,
                w_zip: None,
            };
            s
        }
        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrWId::datum_desc().clone(),
                    AttrWYtd::datum_desc().clone(),
                    AttrWTax::datum_desc().clone(),
                    AttrWName::datum_desc().clone(),
                    AttrWStreet1::datum_desc().clone(),
                    AttrWStreet2::datum_desc().clone(),
                    AttrWCity::datum_desc().clone(),
                    AttrWState::datum_desc().clone(),
                    AttrWZip::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn get_field_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_W_ID => attr_field_access::attr_get_binary::<_>(&self.w_id),
                COLUMN_W_YTD => attr_field_access::attr_get_binary::<_>(&self.w_ytd),
                COLUMN_W_TAX => attr_field_access::attr_get_binary::<_>(&self.w_tax),
                COLUMN_W_NAME => attr_field_access::attr_get_binary::<_>(&self.w_name),
                COLUMN_W_STREET_1 => attr_field_access::attr_get_binary::<_>(&self.w_street_1),
                COLUMN_W_STREET_2 => attr_field_access::attr_get_binary::<_>(&self.w_street_2),
                COLUMN_W_CITY => attr_field_access::attr_get_binary::<_>(&self.w_city),
                COLUMN_W_STATE => attr_field_access::attr_get_binary::<_>(&self.w_state),
                COLUMN_W_ZIP => attr_field_access::attr_get_binary::<_>(&self.w_zip),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_W_ID => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_id, binary.as_ref())?;
                }
                COLUMN_W_YTD => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_ytd, binary.as_ref())?;
                }
                COLUMN_W_TAX => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_tax, binary.as_ref())?;
                }
                COLUMN_W_NAME => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_name, binary.as_ref())?;
                }
                COLUMN_W_STREET_1 => {
                    attr_field_access::attr_set_binary::<_, _>(
                        &mut self.w_street_1,
                        binary.as_ref(),
                    )?;
                }
                COLUMN_W_STREET_2 => {
                    attr_field_access::attr_set_binary::<_, _>(
                        &mut self.w_street_2,
                        binary.as_ref(),
                    )?;
                }
                COLUMN_W_CITY => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_city, binary.as_ref())?;
                }
                COLUMN_W_STATE => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_state, binary.as_ref())?;
                }
                COLUMN_W_ZIP => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.w_zip, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
        fn get_field_value(&self, column: &str) -> RS<Option<DatValue>> {
            match column {
                COLUMN_W_ID => attr_field_access::attr_get_value::<_>(&self.w_id),
                COLUMN_W_YTD => attr_field_access::attr_get_value::<_>(&self.w_ytd),
                COLUMN_W_TAX => attr_field_access::attr_get_value::<_>(&self.w_tax),
                COLUMN_W_NAME => attr_field_access::attr_get_value::<_>(&self.w_name),
                COLUMN_W_STREET_1 => attr_field_access::attr_get_value::<_>(&self.w_street_1),
                COLUMN_W_STREET_2 => attr_field_access::attr_get_value::<_>(&self.w_street_2),
                COLUMN_W_CITY => attr_field_access::attr_get_value::<_>(&self.w_city),
                COLUMN_W_STATE => attr_field_access::attr_get_value::<_>(&self.w_state),
                COLUMN_W_ZIP => attr_field_access::attr_get_value::<_>(&self.w_zip),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, column: &str, value: B) -> RS<()> {
            match column {
                COLUMN_W_ID => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_id, value)?;
                }
                COLUMN_W_YTD => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_ytd, value)?;
                }
                COLUMN_W_TAX => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_tax, value)?;
                }
                COLUMN_W_NAME => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_name, value)?;
                }
                COLUMN_W_STREET_1 => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_street_1, value)?;
                }
                COLUMN_W_STREET_2 => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_street_2, value)?;
                }
                COLUMN_W_CITY => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_city, value)?;
                }
                COLUMN_W_STATE => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_state, value)?;
                }
                COLUMN_W_ZIP => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.w_zip, value)?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrWId {}

    impl AttrValue<i32> for AttrWId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_ID
        }
    }

    pub struct AttrWYtd {}

    impl AttrValue<f64> for AttrWYtd {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_YTD
        }
    }

    pub struct AttrWTax {}

    impl AttrValue<f64> for AttrWTax {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_TAX
        }
    }

    pub struct AttrWName {}

    impl AttrValue<String> for AttrWName {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_NAME
        }
    }

    pub struct AttrWStreet1 {}

    impl AttrValue<String> for AttrWStreet1 {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_STREET_1
        }
    }

    pub struct AttrWStreet2 {}

    impl AttrValue<String> for AttrWStreet2 {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_STREET_2
        }
    }

    pub struct AttrWCity {}

    impl AttrValue<String> for AttrWCity {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_CITY
        }
    }

    pub struct AttrWState {}

    impl AttrValue<String> for AttrWState {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_STATE
        }
    }

    pub struct AttrWZip {}

    impl AttrValue<String> for AttrWZip {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn attr_name() -> &'static str {
            COLUMN_W_ZIP
        }
    }
} // end mod object