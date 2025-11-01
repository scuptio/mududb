use crate::common::result::RS;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub enum EnumDat {
    Null,
    Binary(DatBinary),
    Printable(DatPrintable),
    Internal(DatInternal),
    Typed(DatTyped),
}

impl AsRef<EnumDat> for EnumDat {
    fn as_ref(&self) -> &EnumDat {
        self
    }
}
impl Debug for EnumDat {
    fn fmt(&self, _f: &mut Formatter<'_>) -> std::fmt::Result {
        Ok(())
    }
}
impl EnumDat {
    pub fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        let type_id = param.dat_type_id();
        let typed_val = match self {
            EnumDat::Binary(binary) => {
                let fn_to_typed = type_id.fn_to_typed();
                let fn_recv = type_id.fn_recv();
                let internal = fn_recv(&binary.buf(), param)
                    .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
                let typed = fn_to_typed(&internal, param)
                    .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
                typed
            }
            EnumDat::Printable(printable) => {
                let fn_to_typed = type_id.fn_to_typed();
                let fn_input = type_id.fn_input();
                let internal = fn_input(printable, param)
                    .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
                let typed = fn_to_typed(&internal, param)
                    .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
                typed
            }
            EnumDat::Internal(internal) => {
                let fn_to_typed = type_id.fn_to_typed();
                let typed = fn_to_typed(internal, param)
                    .map_err(|e| m_error!(EC::TypeBaseErr, "convert data format error", e))?;
                typed
            }
            EnumDat::Typed(typed_val) => typed_val.clone(),
            EnumDat::Null => {
                panic!("null data")
            }
        };
        Ok(typed_val)
    }
}
