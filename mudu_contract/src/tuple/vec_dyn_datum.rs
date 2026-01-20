use mudu::common::result::RS;
use mudu_type::dat_type::DatType;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::enumerable_datum::EnumerableDatum;
use crate::tuple::tuple_field_desc::TupleFieldDesc;

pub trait VecDynDatum: EnumerableDatum {}

impl EnumerableDatum for [&dyn DatumDyn] {
    fn to_value(&self, datum_desc: &[DatumDesc]) -> RS<Vec<DatValue>> {
        if datum_desc.len() != self.len() {
            panic!("desc and vec length do not match");
        }
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let datum_desc = &datum_desc[i];
            let value = t.to_value(datum_desc.dat_type())?;
            vec.push(value)
        }
        Ok(vec)
    }

    fn to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
        if desc.len() != self.len() {
            panic!("desc and vec length do not match");
        }
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let datum_desc = &desc[i];
            let binary = t.to_binary(datum_desc.dat_type())?;
            vec.push(binary.into())
        }
        Ok(vec)
    }

    fn tuple_desc(&self, field_name: &[String]) -> RS<TupleFieldDesc> {
        let mut vec = Vec::with_capacity(self.len());
        for (i, t) in self.iter().enumerate() {
            let id = t.dat_type_id()?;
            let dat_type = DatType::default_for(id);
            let name = if self.len() == field_name.len() {
                field_name[i].clone()
            } else {
                format!("v_{}", i)
            };
            let datum_desc = DatumDesc::new(name, dat_type);
            vec.push(datum_desc)
        }
        Ok(TupleFieldDesc::new(vec))
    }
}

impl VecDynDatum for [&dyn DatumDyn] {}
