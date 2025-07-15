use crate::common::error::ER;
use crate::common::result::RS;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_param::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use std::fmt::{Debug, Formatter};


#[derive(Clone)]
pub enum Datum {
    Null,
    Binary(DatBinary),
    Printable(DatPrintable),
    Internal(DatInternal),
    Typed(DatTyped),
}

impl AsRef<Datum> for Datum {
    fn as_ref(&self) -> &Datum {
        self
    }
}
impl Debug for Datum {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}
impl Datum {
    pub fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        let type_id = param.data_type_id();
        let typed_val = match self {
            Datum::Binary(binary) => {
                let fn_to_typed = type_id.fn_to_typed();
                let fn_recv = type_id.fn_recv();
                let internal = fn_recv(&binary.buf(), param).map_err(ER::ConvertErr)?;
                let typed = fn_to_typed(&internal, param).map_err(ER::ConvertErr)?;
                typed
            }
            Datum::Printable(printable) => {
                let fn_to_typed = type_id.fn_to_typed();
                let fn_input = type_id.fn_input();
                let internal = fn_input(printable, param).map_err(ER::ConvertErr)?;
                let typed = fn_to_typed(&internal, param).map_err(ER::ConvertErr)?;
                typed
            }
            Datum::Internal(internal) => {
                let fn_to_typed = type_id.fn_to_typed();
                let typed = fn_to_typed(internal, param).map_err(ER::ConvertErr)?;
                typed
            }
            Datum::Typed(typed_val) => typed_val.clone(),
            Datum::Null => { DatTyped::Null }
        };
        Ok(typed_val)
    }
}
