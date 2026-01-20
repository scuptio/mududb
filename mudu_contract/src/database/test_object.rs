#[cfg(test)]
pub mod object {
    use crate::database::attr_field_access;
    use crate::database::attr_value::AttrValue;
    use crate::database::entity::Entity;
    use crate::database::entity_utils;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_type::dat_binary::DatBinary;
    use mudu_type::dat_textual::DatTextual;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;
    use mudu_type::datum::{Datum, DatumDyn};

    const TABLE_ITEM: &str = "item";
    const COLUMN_I_ID: &str = "i_id";
    const COLUMN_I_NAME: &str = "i_name";
    const COLUMN_I_PRICE: &str = "i_price";
    const COLUMN_I_DATA: &str = "i_data";
    const COLUMN_I_IM_ID: &str = "i_im_id";

    #[derive(Debug, Clone)]
    pub struct Item {
        i_id: Option<i32>,
        i_name: Option<String>,
        i_price: Option<f64>,
        i_data: Option<String>,
        i_im_id: Option<i32>,
    }

    impl Item {
        #[allow(unused)]
        pub fn new(
            i_id: Option<i32>,
            i_name: Option<String>,
            i_price: Option<f64>,
            i_data: Option<String>,
            i_im_id: Option<i32>,
        ) -> Self {
            let s = Self {
                i_id,
                i_name,
                i_price,
                i_data,
                i_im_id,
            };
            s
        }

        pub fn set_i_id(&mut self, i_id: i32) {
            self.i_id = Some(i_id);
        }

        pub fn get_i_id(&self) -> &Option<i32> {
            &self.i_id
        }

        pub fn set_i_name(&mut self, i_name: String) {
            self.i_name = Some(i_name);
        }

        pub fn get_i_name(&self) -> &Option<String> {
            &self.i_name
        }

        pub fn set_i_price(&mut self, i_price: f64) {
            self.i_price = Some(i_price);
        }

        pub fn get_i_price(&self) -> &Option<f64> {
            &self.i_price
        }

        pub fn set_i_data(&mut self, i_data: String) {
            self.i_data = Some(i_data);
        }

        pub fn get_i_data(&self) -> &Option<String> {
            &self.i_data
        }

        pub fn set_i_im_id(&mut self, i_im_id: i32) {
            self.i_im_id = Some(i_im_id);
        }

        pub fn get_i_im_id(&self) -> &Option<i32> {
            &self.i_im_id
        }
    }

    impl Datum for Item {
        fn dat_type() -> &'static DatType {
            lazy_static! {
                static ref DAT_TYPE: DatType = entity_utils::entity_dat_type::<Item>();
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

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn get_field_binary(&self, column: &str) -> RS<Option<Vec<u8>>> {
            match column {
                COLUMN_I_ID => attr_field_access::attr_get_binary::<_>(&self.i_id),
                COLUMN_I_NAME => attr_field_access::attr_get_binary::<_>(&self.i_name),
                COLUMN_I_PRICE => attr_field_access::attr_get_binary::<_>(&self.i_price),
                COLUMN_I_DATA => attr_field_access::attr_get_binary::<_>(&self.i_data),
                COLUMN_I_IM_ID => attr_field_access::attr_get_binary::<_>(&self.i_im_id),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_binary<B: AsRef<[u8]>>(&mut self, column: &str, binary: B) -> RS<()> {
            match column {
                COLUMN_I_ID => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.i_id, binary.as_ref())?;
                }
                COLUMN_I_NAME => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.i_name, binary.as_ref())?;
                }
                COLUMN_I_PRICE => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.i_price, binary.as_ref())?;
                }
                COLUMN_I_DATA => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.i_data, binary.as_ref())?;
                }
                COLUMN_I_IM_ID => {
                    attr_field_access::attr_set_binary::<_, _>(&mut self.i_im_id, binary.as_ref())?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
        fn get_field_value(&self, column: &str) -> RS<Option<DatValue>> {
            match column {
                COLUMN_I_ID => attr_field_access::attr_get_value::<_>(&self.i_id),
                COLUMN_I_NAME => attr_field_access::attr_get_value::<_>(&self.i_name),
                COLUMN_I_PRICE => attr_field_access::attr_get_value::<_>(&self.i_price),
                COLUMN_I_DATA => attr_field_access::attr_get_value::<_>(&self.i_data),
                COLUMN_I_IM_ID => attr_field_access::attr_get_value::<_>(&self.i_im_id),
                _ => {
                    panic!("unknown name");
                }
            }
        }

        fn set_field_value<B: AsRef<DatValue>>(&mut self, column: &str, value: B) -> RS<()> {
            match column {
                COLUMN_I_ID => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.i_id, value)?;
                }
                COLUMN_I_NAME => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.i_name, value)?;
                }
                COLUMN_I_PRICE => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.i_price, value)?;
                }
                COLUMN_I_DATA => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.i_data, value)?;
                }
                COLUMN_I_IM_ID => {
                    attr_field_access::attr_set_value::<_, _>(&mut self.i_im_id, value)?;
                }
                _ => {
                    panic!("unknown name");
                }
            }
            Ok(())
        }
    }

    #[allow(unused)]
    pub struct AttrIId {}

    impl AttrValue<i32> for AttrIId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_ID
        }
    }

    #[allow(unused)]
    pub struct AttrIName {}

    impl AttrValue<String> for AttrIName {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_NAME
        }
    }
    #[allow(unused)]
    pub struct AttrIPrice {}

    impl AttrValue<f64> for AttrIPrice {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_PRICE
        }
    }

    #[allow(unused)]
    pub struct AttrIData {}

    impl AttrValue<String> for AttrIData {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_DATA
        }
    }

    #[allow(unused)]
    pub struct AttrIImId {}

    impl AttrValue<i32> for AttrIImId {
        fn dat_type() -> &'static DatType {
            static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_dat_type())
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| Self::attr_datum_desc())
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_IM_ID
        }
    }
} // end mod object
