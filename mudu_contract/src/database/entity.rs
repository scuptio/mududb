use mudu::common::result::RS;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::{Datum, DatumDyn};
use crate::database::entity_utils;
use mudu::m_error;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::error::ec::EC;
use paste::paste;

pub trait Entity: private::Sealed + Datum {
    fn new_empty() -> Self;

    fn tuple_desc() -> &'static TupleFieldDesc;

    fn object_name() -> &'static str;

    fn get_field_binary(&self, field_name: &str) -> RS<Option<Vec<u8>>>;

    fn set_field_binary<B: AsRef<[u8]>>(&mut self, field_name: &str, binary: B) -> RS<()>;

    fn get_field_value(&self, field_name: &str) -> RS<Option<DatValue>>;

    fn set_field_value<D: AsRef<DatValue>>(&mut self, field_name: &str, value: D) -> RS<()>;

    fn from_tuple(tuple_row: &TupleField) -> RS<Self> {
        entity_utils::entity_from_tuple_field(tuple_row)
    }

    fn to_tuple(&self) -> RS<TupleField> {
        entity_utils::entity_to_tuple(self)
    }
}


mod private {
    pub trait Sealed {}
}
impl<U: Entity> private::Sealed for U {}

const OBJECT_NAME_PREFIX:&str = "object";
const OBJECT_FIELD_PREFIX:&str = "field";


macro_rules! impl_entity_trait {
    ($(($variant_upper:ident, $variant_lower:ident, $datum_type:ty)),+ $(,)?) => {
        $(
            impl Entity for $datum_type {
                paste! {
                    fn new_empty() -> Self {
                        Self::default()
                    }

                    fn tuple_desc() -> &'static TupleFieldDesc {
                        lazy_static::lazy_static! {
                            static ref TUPLE_DESC:TupleFieldDesc = TupleFieldDesc::new(vec![
                                DatumDesc::new(format!("{}_{}", OBJECT_FIELD_PREFIX, stringify!($variant_lower)), $datum_type::dat_type().clone())]);
                        }
                        &TUPLE_DESC
                    }

                    fn object_name() -> &'static str {
                        lazy_static::lazy_static! {
                            static ref OBJECT_NAME:String = {
                                format!("{}_{}", OBJECT_NAME_PREFIX,  stringify!($variant_lower))
                            };
                        }
                        &OBJECT_NAME
                    }

                    fn get_field_binary(&self, _: &str) -> RS<Option<Vec<u8>>> {
                        let dat_binary = self.to_binary(Self::dat_type())?;
                        Ok(Some(dat_binary.into()))
                    }

                    fn set_field_binary<B: AsRef<[u8]>>(&mut self, _: &str, binary: B) -> RS<()> {
                        let i:Self = Self::from_binary(binary.as_ref())?;
                        *self = i;
                        Ok(())
                    }

                    fn get_field_value(&self, _: &str) -> RS<Option<DatValue>> {
                        Ok(Some(DatValue::[<from_$variant_lower>](self.clone())))
                    }

                    fn set_field_value<D: AsRef<DatValue>>(&mut self, _: &str, value: D) -> RS<()> {
                        let field_value = value.as_ref().[<as_$variant_lower>]()
                            .map_or_else( ||{ Err(m_error!(EC::TypeErr, "")) }, |v|{Ok(v.clone())},)?;
                        *self = field_value;
                        Ok(())
                    }
                }
            }
        )+
    };
}


impl_entity_trait!(
    (I32, i32, i32),
    (I64, i64, i64),
    (F32, f32, f32),
    (F64, f64, f64),
    (String, string, String)
);
