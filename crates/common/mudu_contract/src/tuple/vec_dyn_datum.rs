//! `tuple::vec_dyn_datum` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::enumerable_datum::EnumerableDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;

pub trait VecDynDatum: EnumerableDatum {}

impl EnumerableDatum for [&dyn DatumDyn] {
    fn to_value(&self, datum_desc: &[DatumDesc]) -> RS<Vec<DataValue>> {
        if datum_desc.len() != self.len() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "desc length {} and value length {} do not match",
                    datum_desc.len(),
                    self.len()
                )
            ));
        }
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let datum_desc = &datum_desc[i];
            let value = t.to_value(datum_desc.data_type())?;
            vec.push(value)
        }
        Ok(vec)
    }

    fn to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
        if desc.len() != self.len() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "desc length {} and value length {} do not match",
                    desc.len(),
                    self.len()
                )
            ));
        }
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let datum_desc = &desc[i];
            let binary = t.to_binary(datum_desc.data_type())?;
            vec.push(binary.into())
        }
        Ok(vec)
    }

    fn tuple_desc(&self, field_name: &[String]) -> RS<TupleFieldDesc> {
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let id = t.type_family()?;
            let data_type = DataType::default_for(id);
            let name = if self.len() == field_name.len() {
                field_name[i].clone()
            } else {
                format!("v_{}", i)
            };
            let datum_desc = DatumDesc::new(name, data_type);
            vec.push(datum_desc)
        }
        Ok(TupleFieldDesc::new(vec))
    }
}

impl VecDynDatum for [&dyn DatumDyn] {}
