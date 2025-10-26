use crate::common::result::RS;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_param::ParamObj;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use crate::tuple::datum::DatumDyn;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct TypedBin {
    dat_type_id: DatTypeID,
    bin: Vec<u8>,
}

impl TypedBin {
    pub fn new(dat_type_id: DatTypeID, bin: Vec<u8>) -> Self {
        Self { dat_type_id, bin }
    }
}

impl Debug for TypedBin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.dat_type_id.fmt(f)?;
        self.bin.fmt(f)?;
        Ok(())
    }
}

impl DatumDyn for TypedBin {
    fn dat_type_id_self(&self) -> RS<DatTypeID> {
        Ok(self.dat_type_id)
    }

    fn to_typed(&self, param: &ParamObj) -> RS<DatTyped> {
        let fn_recv = self.dat_type_id.fn_recv();
        let internal =
            fn_recv(&self.bin, param).map_err(|e| m_error!(EC::ConvertErr, "to_typed error", e))?;
        let fn_to_typed = self.dat_type_id.fn_to_typed();
        let typed = fn_to_typed(&internal, param)
            .map_err(|e| m_error!(EC::ConvertErr, "to_typed error", e))?;
        Ok(typed)
    }

    fn to_binary(&self, _: &ParamObj) -> RS<DatBinary> {
        Ok(DatBinary::from(self.bin.clone()))
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        let fn_recv = self.dat_type_id.fn_recv();
        let internal = fn_recv(&self.bin, param)
            .map_err(|e| m_error!(EC::ConvertErr, "to_printable error", e))?;

        let fn_output = self.dat_type_id.fn_output();
        let output = fn_output(&internal, param)
            .map_err(|e| m_error!(EC::ConvertErr, "to_printable error", e))?;
        Ok(output)
    }

    fn to_internal(&self, param: &ParamObj) -> RS<DatInternal> {
        let fn_recv = self.dat_type_id.fn_recv();
        let internal = fn_recv(&self.bin, param)
            .map_err(|e| m_error!(EC::ConvertErr, "to_printable error", e))?;
        Ok(internal)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}
