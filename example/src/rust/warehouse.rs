pub mod object {
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu::database::attr_binary::AttrBinary;
    use mudu::database::attr_set_get::{attr_get_binary, attr_set_binary};
    use mudu::database::attr_value::AttrValue;
    use mudu::database::record::Record;
    use mudu::database::record_convert_tuple::{record_from_tuple, record_to_tuple};
    use mudu::tuple::datum_convert::{datum_from_binary, datum_to_binary};
    use mudu::tuple::tuple_field::TupleField;
    use mudu::tuple::tuple_field_desc::TupleFieldDesc;

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

    pub struct Warehouse {
        w_id: Option<AttrWId>,
        w_ytd: Option<AttrWYtd>,
        w_tax: Option<AttrWTax>,
        w_name: Option<AttrWName>,
        w_street_1: Option<AttrWStreet1>,
        w_street_2: Option<AttrWStreet2>,
        w_city: Option<AttrWCity>,
        w_state: Option<AttrWState>,
        w_zip: Option<AttrWZip>,
    }

    impl Warehouse {
        pub fn new(
            w_id: AttrWId,
            w_ytd: AttrWYtd,
            w_tax: AttrWTax,
            w_name: AttrWName,
            w_street_1: AttrWStreet1,
            w_street_2: AttrWStreet2,
            w_city: AttrWCity,
            w_state: AttrWState,
            w_zip: AttrWZip,
        ) -> Self {
            let s = Self {
                w_id: Some(w_id),
                w_ytd: Some(w_ytd),
                w_tax: Some(w_tax),
                w_name: Some(w_name),
                w_street_1: Some(w_street_1),
                w_street_2: Some(w_street_2),
                w_city: Some(w_city),
                w_state: Some(w_state),
                w_zip: Some(w_zip),
            };
            s
        }

        pub fn set_w_id(&mut self, w_id: AttrWId) {
            self.w_id = Some(w_id);
        }

        pub fn get_w_id(&self) -> &Option<AttrWId> {
            &self.w_id
        }

        pub fn set_w_ytd(&mut self, w_ytd: AttrWYtd) {
            self.w_ytd = Some(w_ytd);
        }

        pub fn get_w_ytd(&self) -> &Option<AttrWYtd> {
            &self.w_ytd
        }

        pub fn set_w_tax(&mut self, w_tax: AttrWTax) {
            self.w_tax = Some(w_tax);
        }

        pub fn get_w_tax(&self) -> &Option<AttrWTax> {
            &self.w_tax
        }

        pub fn set_w_name(&mut self, w_name: AttrWName) {
            self.w_name = Some(w_name);
        }

        pub fn get_w_name(&self) -> &Option<AttrWName> {
            &self.w_name
        }

        pub fn set_w_street_1(&mut self, w_street_1: AttrWStreet1) {
            self.w_street_1 = Some(w_street_1);
        }

        pub fn get_w_street_1(&self) -> &Option<AttrWStreet1> {
            &self.w_street_1
        }

        pub fn set_w_street_2(&mut self, w_street_2: AttrWStreet2) {
            self.w_street_2 = Some(w_street_2);
        }

        pub fn get_w_street_2(&self) -> &Option<AttrWStreet2> {
            &self.w_street_2
        }

        pub fn set_w_city(&mut self, w_city: AttrWCity) {
            self.w_city = Some(w_city);
        }

        pub fn get_w_city(&self) -> &Option<AttrWCity> {
            &self.w_city
        }

        pub fn set_w_state(&mut self, w_state: AttrWState) {
            self.w_state = Some(w_state);
        }

        pub fn get_w_state(&self) -> &Option<AttrWState> {
            &self.w_state
        }

        pub fn set_w_zip(&mut self, w_zip: AttrWZip) {
            self.w_zip = Some(w_zip);
        }

        pub fn get_w_zip(&self) -> &Option<AttrWZip> {
            &self.w_zip
        }
    }

    impl Record for Warehouse {
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

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(row: T, desc: D) -> RS<Self> {
            record_from_tuple::<Self, T, D>(row, desc)
        }

        fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, desc: D) -> RS<TupleField> {
            record_to_tuple(self, desc)
        }

        fn get_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_W_ID => attr_get_binary(&self.w_id),
                COLUMN_W_YTD => attr_get_binary(&self.w_ytd),
                COLUMN_W_TAX => attr_get_binary(&self.w_tax),
                COLUMN_W_NAME => attr_get_binary(&self.w_name),
                COLUMN_W_STREET_1 => attr_get_binary(&self.w_street_1),
                COLUMN_W_STREET_2 => attr_get_binary(&self.w_street_2),
                COLUMN_W_CITY => attr_get_binary(&self.w_city),
                COLUMN_W_STATE => attr_get_binary(&self.w_state),
                COLUMN_W_ZIP => attr_get_binary(&self.w_zip),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_W_ID => {
                    attr_set_binary(&mut self.w_id, binary.as_ref())?;
                }
                COLUMN_W_YTD => {
                    attr_set_binary(&mut self.w_ytd, binary.as_ref())?;
                }
                COLUMN_W_TAX => {
                    attr_set_binary(&mut self.w_tax, binary.as_ref())?;
                }
                COLUMN_W_NAME => {
                    attr_set_binary(&mut self.w_name, binary.as_ref())?;
                }
                COLUMN_W_STREET_1 => {
                    attr_set_binary(&mut self.w_street_1, binary.as_ref())?;
                }
                COLUMN_W_STREET_2 => {
                    attr_set_binary(&mut self.w_street_2, binary.as_ref())?;
                }
                COLUMN_W_CITY => {
                    attr_set_binary(&mut self.w_city, binary.as_ref())?;
                }
                COLUMN_W_STATE => {
                    attr_set_binary(&mut self.w_state, binary.as_ref())?;
                }
                COLUMN_W_ZIP => {
                    attr_set_binary(&mut self.w_zip, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrWId {
        value: i32,
    }

    impl AttrWId {}

    impl AttrBinary for AttrWId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrWId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrWYtd {
        value: f64,
    }

    impl AttrWYtd {}

    impl AttrBinary for AttrWYtd {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: f64 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<f64> for AttrWYtd {
        fn new(datum: f64) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_YTD
        }

        fn get_value(&self) -> f64 {
            self.value.clone()
        }

        fn set_value(&mut self, value: f64) {
            self.value = value;
        }
    }

    pub struct AttrWTax {
        value: f64,
    }

    impl AttrWTax {}

    impl AttrBinary for AttrWTax {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: f64 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<f64> for AttrWTax {
        fn new(datum: f64) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_TAX
        }

        fn get_value(&self) -> f64 {
            self.value.clone()
        }

        fn set_value(&mut self, value: f64) {
            self.value = value;
        }
    }

    pub struct AttrWName {
        value: String,
    }

    impl AttrWName {}

    impl AttrBinary for AttrWName {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWName {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_NAME
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrWStreet1 {
        value: String,
    }

    impl AttrWStreet1 {}

    impl AttrBinary for AttrWStreet1 {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWStreet1 {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_STREET_1
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrWStreet2 {
        value: String,
    }

    impl AttrWStreet2 {}

    impl AttrBinary for AttrWStreet2 {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWStreet2 {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_STREET_2
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrWCity {
        value: String,
    }

    impl AttrWCity {}

    impl AttrBinary for AttrWCity {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWCity {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_CITY
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrWState {
        value: String,
    }

    impl AttrWState {}

    impl AttrBinary for AttrWState {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWState {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_STATE
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrWZip {
        value: String,
    }

    impl AttrWZip {}

    impl AttrBinary for AttrWZip {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrWZip {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_WAREHOUSE
        }

        fn column_name() -> &'static str {
            COLUMN_W_ZIP
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }
} // end mod object
