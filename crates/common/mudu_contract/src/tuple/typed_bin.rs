//! `tuple::typed_bin` module.
#![allow(missing_docs)]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_binary::DataBinary;
use mudu_type::data_textual::DataTextual;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use std::fmt::{Debug, Formatter};

#[derive(Clone)]
pub struct TypedBin {
    type_family: TypeFamily,
    bin: Vec<u8>,
}

impl TypedBin {
    pub fn new(type_family: TypeFamily, bin: Vec<u8>) -> Self {
        Self { type_family, bin }
    }
}

impl Debug for TypedBin {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        self.type_family.fmt(f)?;
        self.bin.fmt(f)?;
        Ok(())
    }
}

impl DatumDyn for TypedBin {
    fn type_family(&self) -> RS<TypeFamily> {
        Ok(self.type_family)
    }

    fn to_binary(&self, _: &DataType) -> RS<DataBinary> {
        Ok(DataBinary::from(self.bin.clone()))
    }

    fn to_textual(&self, tyep_obj: &DataType) -> RS<DataTextual> {
        let fn_recv = self.type_family.fn_recv();
        let (internal, _) = fn_recv(&self.bin, tyep_obj)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "to_textual error", e))?;

        let fn_output = self.type_family.fn_output();
        let output = fn_output(&internal, tyep_obj)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "to_textual error", e))?;
        Ok(output)
    }

    fn to_value(&self, type_obj: &DataType) -> RS<DataValue> {
        let fn_recv = self.type_family.fn_recv();
        let (internal, _) = fn_recv(&self.bin, type_obj)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "to_textual error", e))?;
        Ok(internal)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}
