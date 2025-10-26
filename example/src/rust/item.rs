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

    const TABLE_ITEM: &str = "item";
    const COLUMN_I_ID: &str = "i_id";
    const COLUMN_I_NAME: &str = "i_name";
    const COLUMN_I_PRICE: &str = "i_price";
    const COLUMN_I_DATA: &str = "i_data";
    const COLUMN_I_IM_ID: &str = "i_im_id";

    pub struct Item {
        i_id: Option<AttrIId>,
        i_name: Option<AttrIName>,
        i_price: Option<AttrIPrice>,
        i_data: Option<AttrIData>,
        i_im_id: Option<AttrIImId>,
    }

    impl Item {
        pub fn new(
            i_id: AttrIId,
            i_name: AttrIName,
            i_price: AttrIPrice,
            i_data: AttrIData,
            i_im_id: AttrIImId,
        ) -> Self {
            let s = Self {
                i_id: Some(i_id),
                i_name: Some(i_name),
                i_price: Some(i_price),
                i_data: Some(i_data),
                i_im_id: Some(i_im_id),
            };
            s
        }

        pub fn set_i_id(&mut self, i_id: AttrIId) {
            self.i_id = Some(i_id);
        }

        pub fn get_i_id(&self) -> &Option<AttrIId> {
            &self.i_id
        }

        pub fn set_i_name(&mut self, i_name: AttrIName) {
            self.i_name = Some(i_name);
        }

        pub fn get_i_name(&self) -> &Option<AttrIName> {
            &self.i_name
        }

        pub fn set_i_price(&mut self, i_price: AttrIPrice) {
            self.i_price = Some(i_price);
        }

        pub fn get_i_price(&self) -> &Option<AttrIPrice> {
            &self.i_price
        }

        pub fn set_i_data(&mut self, i_data: AttrIData) {
            self.i_data = Some(i_data);
        }

        pub fn get_i_data(&self) -> &Option<AttrIData> {
            &self.i_data
        }

        pub fn set_i_im_id(&mut self, i_im_id: AttrIImId) {
            self.i_im_id = Some(i_im_id);
        }

        pub fn get_i_im_id(&self) -> &Option<AttrIImId> {
            &self.i_im_id
        }
    }

    impl Record for Item {
        fn new_empty() -> Self {
            let s = Self {
                i_id: None,
                i_name: None,
                i_price: None,
                i_data: None,
                i_im_id: None,
            };
            s
        }
        fn tuple_desc() -> &'static TupleFieldDesc {
            lazy_static! {
                static ref TUPLE_DESC: TupleFieldDesc = TupleFieldDesc::new(vec![
                    AttrIId::datum_desc().clone(),
                    AttrIName::datum_desc().clone(),
                    AttrIPrice::datum_desc().clone(),
                    AttrIData::datum_desc().clone(),
                    AttrIImId::datum_desc().clone(),
                ]);
            }
            &TUPLE_DESC
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn from_tuple<T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(row: T, desc: D) -> RS<Self> {
            record_from_tuple::<Self, T, D>(row, desc)
        }

        fn to_tuple<D: AsRef<TupleFieldDesc>>(&self, desc: D) -> RS<TupleField> {
            record_to_tuple(self, desc)
        }

        fn get_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_I_ID => attr_get_binary(&self.i_id),
                COLUMN_I_NAME => attr_get_binary(&self.i_name),
                COLUMN_I_PRICE => attr_get_binary(&self.i_price),
                COLUMN_I_DATA => attr_get_binary(&self.i_data),
                COLUMN_I_IM_ID => attr_get_binary(&self.i_im_id),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_I_ID => {
                    attr_set_binary(&mut self.i_id, binary.as_ref())?;
                }
                COLUMN_I_NAME => {
                    attr_set_binary(&mut self.i_name, binary.as_ref())?;
                }
                COLUMN_I_PRICE => {
                    attr_set_binary(&mut self.i_price, binary.as_ref())?;
                }
                COLUMN_I_DATA => {
                    attr_set_binary(&mut self.i_data, binary.as_ref())?;
                }
                COLUMN_I_IM_ID => {
                    attr_set_binary(&mut self.i_im_id, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    pub struct AttrIId {
        value: i32,
    }

    impl AttrIId {}

    impl AttrBinary for AttrIId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrIId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }

    pub struct AttrIName {
        value: String,
    }

    impl AttrIName {}

    impl AttrBinary for AttrIName {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrIName {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_NAME
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrIPrice {
        value: f64,
    }

    impl AttrIPrice {}

    impl AttrBinary for AttrIPrice {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: f64 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<f64> for AttrIPrice {
        fn new(datum: f64) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_PRICE
        }

        fn get_value(&self) -> f64 {
            self.value.clone()
        }

        fn set_value(&mut self, value: f64) {
            self.value = value;
        }
    }

    pub struct AttrIData {
        value: String,
    }

    impl AttrIData {}

    impl AttrBinary for AttrIData {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: String = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<String> for AttrIData {
        fn new(datum: String) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_DATA
        }

        fn get_value(&self) -> String {
            self.value.clone()
        }

        fn set_value(&mut self, value: String) {
            self.value = value;
        }
    }

    pub struct AttrIImId {
        value: i32,
    }

    impl AttrIImId {}

    impl AttrBinary for AttrIImId {
        fn get_binary(&self) -> RS<Vec<u8>> {
            datum_to_binary(&self.value)
        }

        fn set_binary<D: AsRef<[u8]>>(&mut self, binary: D) -> RS<()> {
            let value: i32 = datum_from_binary(binary.as_ref())?;
            self.set_value(value);
            Ok(())
        }
    }

    impl AttrValue<i32> for AttrIImId {
        fn new(datum: i32) -> Self {
            Self { value: datum }
        }

        fn from_binary<B: AsRef<[u8]>>(binary: B) -> RS<Self> {
            Ok(Self::new(datum_from_binary(binary)?))
        }

        fn table_name() -> &'static str {
            TABLE_ITEM
        }

        fn column_name() -> &'static str {
            COLUMN_I_IM_ID
        }

        fn get_value(&self) -> i32 {
            self.value.clone()
        }

        fn set_value(&mut self, value: i32) {
            self.value = value;
        }
    }
} // end mod object
