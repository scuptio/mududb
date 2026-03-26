use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_primitive_value::UniPrimitiveValue;
use mudu::common::result::RS;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

impl UniDatValue {
    pub fn uni_to(self) -> RS<DatValue> {
        let value = match self {
            UniDatValue::Primitive(value) => {
                let v = match value {
                    UniPrimitiveValue::Bool(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::U8(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::I8(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::U16(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::I16(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::U32(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::I32(v) => DatValue::from_i32(v),
                    UniPrimitiveValue::U64(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::I64(v) => DatValue::from_i64(v),
                    UniPrimitiveValue::F32(v) => DatValue::from_f32(v),
                    UniPrimitiveValue::F64(v) => DatValue::from_f64(v),
                    UniPrimitiveValue::Char(_) => {
                        unimplemented!()
                    }
                    UniPrimitiveValue::String(v) => DatValue::from_string(v),
                };
                v
            }
            UniDatValue::Array(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for mu_v in inner {
                    let v = mu_v.uni_to()?;
                    vec.push(v);
                }
                DatValue::from_array(vec)
            }
            UniDatValue::Record(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for mu_v in inner {
                    let v = mu_v.uni_to()?;
                    vec.push(v);
                }
                DatValue::from_record(vec)
            }
            UniDatValue::Binary(_data) => {
                unimplemented!()
            }
        };
        Ok(value)
    }

    pub fn uni_from(dat_value: DatValue) -> RS<UniDatValue> {
        let id = dat_value.dat_type_id()?;
        let mu_v = match id {
            DatTypeID::I32 => {
                UniDatValue::from_primitive(UniPrimitiveValue::I32(dat_value.expect_i32().clone()))
            }
            DatTypeID::I64 => {
                UniDatValue::from_primitive(UniPrimitiveValue::I64(dat_value.expect_i64().clone()))
            }
            DatTypeID::F32 => {
                UniDatValue::from_primitive(UniPrimitiveValue::F32(dat_value.expect_f32().clone()))
            }
            DatTypeID::F64 => {
                UniDatValue::from_primitive(UniPrimitiveValue::F64(dat_value.expect_f64().clone()))
            }
            DatTypeID::String => UniDatValue::from_primitive(UniPrimitiveValue::String(
                dat_value.expect_string().clone(),
            )),
            DatTypeID::Array => {
                let array = dat_value.into_array();
                let mut vec = Vec::with_capacity(array.len());
                for v in array {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(mu_ve);
                }
                UniDatValue::from_array(vec)
            }
            DatTypeID::Record => {
                let object = dat_value.into_record();
                let mut vec = Vec::with_capacity(object.len());
                for v in object {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(mu_ve);
                }
                UniDatValue::from_record(vec)
            }
            DatTypeID::Binary => {
                let binary = dat_value.into_binary();
                UniDatValue::from_binary(binary)
            }
        };
        Ok(mu_v)
    }
}
