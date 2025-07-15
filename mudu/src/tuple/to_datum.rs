use std::fmt;

use crate::common::error::ER;
use crate::common::result::RS;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_param::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;

pub trait ToDatum : fmt::Debug + Sync {
    fn to_type_id(&self) -> DatTypeID;

    fn to_typed(&self, param:&ParamObj) -> RS<DatTyped>;

    fn to_binary(&self, param:&ParamObj) -> RS<DatBinary>;

    fn to_printable(&self, param:&ParamObj) -> RS<DatPrintable>;

    fn to_internal(&self, param:&ParamObj) -> RS<DatInternal>;
}

impl ToDatum for i32 {
    fn to_type_id(&self) -> DatTypeID {
        DatTypeID::I32
    }

    fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
        Ok(DatTyped::I32(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        Ok((param
            .data_type_id()
            .fn_base()
            .send
            )(&DatInternal::from_i32(*self), param)
            .map_err(|e| { ER::ConvertErr(e)})?
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        Ok((param
            .data_type_id()
            .fn_base()
            .output
        )(&DatInternal::from_i32(*self), param)
            .map_err(|e| { ER::ConvertErr(e)})?
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_i32(*self))
    }
}


impl ToDatum for i64 {
    fn to_type_id(&self) -> DatTypeID {
        DatTypeID::I64
    }

    fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
        Ok(DatTyped::I64(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        Ok((param
            .data_type_id()
            .fn_base()
            .send
        )(&DatInternal::from_i64(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        Ok((param
            .data_type_id()
            .fn_base()
            .output
        )(&DatInternal::from_i64(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_i64(*self))
    }
}

impl ToDatum for f32 {
    fn to_type_id(&self) -> DatTypeID {
        DatTypeID::F32
    }

    fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
        Ok(DatTyped::F32(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        Ok((param
            .data_type_id()
            .fn_base()
            .send
        )(&DatInternal::from_f32(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        Ok((param
            .data_type_id()
            .fn_base()
            .output
        )(&DatInternal::from_f32(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_f32(*self))
    }
}

impl ToDatum for f64 {
    fn to_type_id(&self) -> DatTypeID {
        DatTypeID::F64
    }
    
    fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
        Ok(DatTyped::F64(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        Ok((param
            .data_type_id()
            .fn_base()
            .send
        )(&DatInternal::from_f64(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        Ok((param
            .data_type_id()
            .fn_base()
            .output
        )(&DatInternal::from_f64(*self), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_f64(*self))
    }
}


impl ToDatum for String {
    fn to_type_id(&self) -> DatTypeID {
        DatTypeID::VarLenString
    }
    
    fn to_typed(&self, _: &ParamObj) -> RS<DatTyped> {
        Ok(DatTyped::String(self.clone()))
    }

    fn to_binary(&self, param: &ParamObj) -> RS<DatBinary> {
        Ok((param
            .data_type_id()
            .fn_base()
            .send
        )(&DatInternal::from_any_type(self.clone()), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_printable(&self, param: &ParamObj) -> RS<DatPrintable> {
        Ok((param
            .data_type_id()
            .fn_base()
            .output
        )(&DatInternal::from_any_type(self.clone()), param)
            .map_err(|e| { ER::ConvertErr(e) })?
        )
    }

    fn to_internal(&self, _: &ParamObj) -> RS<DatInternal> {
        Ok(DatInternal::from_any_type(self.clone()))
    }
}