//! `database::attr_value` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use mudu_type::data_type::DataType;
use mudu_type::datum::Datum;

pub trait AttrValue<T: Datum>: private::Sealed<T> + Sized {
    fn attr_data_type() -> DataType {
        T::data_type().clone()
    }

    fn attr_datum_desc() -> DatumDesc {
        DatumDesc::new(
            Self::attr_name().to_string(),
            Self::attr_data_type().clone(),
        )
    }

    fn data_type() -> &'static DataType;

    fn object_name() -> &'static str;

    fn datum_desc() -> &'static DatumDesc;

    fn attr_name() -> &'static str;
}

mod private {
    use super::Datum;

    pub trait Sealed<T: Datum> {}
}
impl<T: Datum, U: AttrValue<T>> private::Sealed<T> for U {}
