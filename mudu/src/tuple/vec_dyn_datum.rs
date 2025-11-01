use crate::common::result::RS;
use crate::data_type::dat_type::DatType;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::datum::DatumDyn;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::enumerable_datum::EnumerableDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;

pub trait VecDynDatum: EnumerableDatum {
    fn from_binary(vec_bin: &Vec<Vec<u8>>, desc: &[DatumDesc]) -> RS<Vec<Box<dyn DatumDyn>>>;
}

impl EnumerableDatum for [&dyn DatumDyn] {
    fn to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
        if desc.len() != self.len() {
            panic!("desc and vec length do not match");
        }
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let datum_desc = &desc[i];
            let binary = t.to_binary(datum_desc.param_obj())?;
            vec.push(binary.into())
        }
        Ok(vec)
    }

    fn tuple_desc(&self) -> RS<TupleFieldDesc> {
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let id = t.dat_type_id_self()?;
            let dat_type = DatType::new_with_default_param(id);
            let datum_desc = DatumDesc::new(format!("v_{}", i), dat_type);
            vec.push(datum_desc)
        }
        Ok(TupleFieldDesc::new(vec))
    }
}

impl VecDynDatum for [&dyn DatumDyn] {
    fn from_binary(vec_bin: &Vec<Vec<u8>>, desc: &[DatumDesc]) -> RS<Vec<Box<dyn DatumDyn>>> {
        if vec_bin.len() != desc.len() {
            panic!("vec_bin.len() != desc.len()");
        }
        let mut vec: Vec<Box<dyn DatumDyn>> = Vec::with_capacity(vec_bin.len());
        for (i, bin) in vec_bin.iter().enumerate() {
            let id = desc[i].dat_type_id();
            let param = desc[i].param_obj();
            let internal = id.fn_recv()(bin, param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert fn_recv error", e))?;
            let dat_typed = id.fn_to_typed()(&internal, param)
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert fn_to_typed error", e))?;
            vec.push(Box::new(dat_typed));
        }
        Ok(vec)
    }
}
