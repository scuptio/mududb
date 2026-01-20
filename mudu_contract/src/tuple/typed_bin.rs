use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_binary::DatBinary;
use mudu_type::dat_textual::DatTextual;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;
use mudu_type::dt_fn_param::DatType;
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
    fn dat_type_id(&self) -> RS<DatTypeID> {
        Ok(self.dat_type_id)
    }


    fn to_binary(&self, _: &DatType) -> RS<DatBinary> {
        Ok(DatBinary::from(self.bin.clone()))
    }

    fn to_textual(&self, tyep_obj: &DatType) -> RS<DatTextual> {
        let fn_recv = self.dat_type_id.fn_recv();
        let (internal, _) = fn_recv(&self.bin, tyep_obj)
            .map_err(|e| m_error!(EC::TypeBaseErr, "to_textual error", e))?;

        let fn_output = self.dat_type_id.fn_output();
        let output = fn_output(&internal, tyep_obj)
            .map_err(|e| m_error!(EC::TypeBaseErr, "to_textual error", e))?;
        Ok(output)
    }

    fn to_value(&self, type_obj: &DatType) -> RS<DatValue> {
        let fn_recv = self.dat_type_id.fn_recv();
        let (internal, _) = fn_recv(&self.bin, type_obj)
            .map_err(|e| m_error!(EC::TypeBaseErr, "to_textual error", e))?;
        Ok(internal)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}
