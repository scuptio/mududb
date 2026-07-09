//! `database::test_object` module.
#![allow(missing_docs)]

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
pub mod object {
    use crate::database::attr_field_access;
    use crate::database::attr_value::AttrValue;
    use crate::database::entity::Entity;
    use crate::database::entity_utils;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_type::data_binary::DataBinary;
    use mudu_type::data_textual::DataTextual;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::{Datum, DatumDyn};
    use mudu_type::type_family::TypeFamily;

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
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity_utils::entity_data_type::<Item>)
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

    impl DatumDyn for Item {
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

    impl Entity for Item {
        fn new_empty() -> Self {
            Self {
                i_id: None,
                i_name: None,
                i_price: None,
                i_data: None,
                i_im_id: None,
            }
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
        fn get_field_value(&self, column: &str) -> RS<Option<DataValue>> {
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

        fn set_field_value<B: AsRef<DataValue>>(&mut self, column: &str, value: B) -> RS<()> {
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

    // Marker type used only at the type level via AttrValue<T>; never constructed directly.
    #[allow(dead_code)]
    pub struct AttrIId {}

    impl AttrValue<i32> for AttrIId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_ID
        }
    }

    // Marker type used only at the type level via AttrValue<T>; never constructed directly.
    #[allow(dead_code)]
    pub struct AttrIName {}

    impl AttrValue<String> for AttrIName {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_NAME
        }
    }
    // Marker type used only at the type level via AttrValue<T>; never constructed directly.
    #[allow(dead_code)]
    pub struct AttrIPrice {}

    impl AttrValue<f64> for AttrIPrice {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_PRICE
        }
    }

    // Marker type used only at the type level via AttrValue<T>; never constructed directly.
    #[allow(dead_code)]
    pub struct AttrIData {}

    impl AttrValue<String> for AttrIData {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_DATA
        }
    }

    // Marker type used only at the type level via AttrValue<T>; never constructed directly.
    #[allow(dead_code)]
    pub struct AttrIImId {}

    impl AttrValue<i32> for AttrIImId {
        fn data_type() -> &'static DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_data_type)
        }

        fn datum_desc() -> &'static DatumDesc {
            static ONCE_LOCK: std::sync::OnceLock<DatumDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(Self::attr_datum_desc)
        }

        fn object_name() -> &'static str {
            TABLE_ITEM
        }

        fn attr_name() -> &'static str {
            COLUMN_I_IM_ID
        }
    }
} // end mod object
