use crate::common::result::RS;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use crate::tuple::datum::DatumDyn;
use serde::{Deserialize, Serialize};

/// Enum variant with single field tuple,
/// The field is a static data type which implements `Datum` trait
#[derive(Deserialize, Serialize, Clone, Debug)]
pub enum DatTyped {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String),
}

macro_rules! impl_datum_for_typed {
    ($($variant:ident($ty:ty)),*) => {
        impl DatumDyn for DatTyped {
            fn dat_type_id_self(&self) -> RS<DatTypeID> {
                match self {
                    $(DatTyped::$variant(v) => v.dat_type_id_self(),)*
                }
            }

            fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
                Ok(self.clone())
            }

            fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
                match self {
                    $(DatTyped::$variant(v) => v.to_binary(param),)*
                }
            }

            fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
                match self {
                    $(DatTyped::$variant(v) => v.to_printable(param),)*
                }
            }

            fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
                match self {
                    $(DatTyped::$variant(v) => v.to_internal(param),)*
                }
            }

            fn clone_boxed(&self) -> Box<dyn DatumDyn> {
                Box::new(self.clone())
            }
        }
    };
}

impl_datum_for_typed! {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String)
}
