use crate::tuple::datum_desc::DatumDesc;
use mudu_type::dat_type::DatType;
use mudu_type::datum::Datum;


pub trait AttrValue<T: Datum>: private::Sealed<T> + Sized {
    fn attr_dat_type() -> DatType {
        T::dat_type().clone()
    }

    fn attr_datum_desc() -> DatumDesc {
        DatumDesc::new(Self::attr_name().to_string(), Self::attr_dat_type().clone())
    }

    fn dat_type() -> &'static DatType;

    fn object_name() -> &'static str;

    fn datum_desc() -> &'static DatumDesc;

    fn attr_name() -> &'static str;
}

mod private {
    use super::Datum;

    pub trait Sealed<T: Datum> {}
}
impl<T: Datum, U: AttrValue<T>> private::Sealed<T> for U {}