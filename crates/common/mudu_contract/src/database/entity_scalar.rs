//! `database::entity_scalar` module.
//!
//! Scalar [`Entity`] implementations: a scalar value is treated as a
//! single-column row, so `mudu_query::<i64>` and friends decode the first
//! column of each result row directly.
//!
//! The synthetic object/field names are a **public contract**: the object
//! name is `object_<type>` (e.g. `object_i64`) and the single column name is
//! `field_<type>` (e.g. `field_i64`). User SQL may rely on these names with
//! `AS field_i64`-style aliases, though decoding is positional and does not
//! check column names.
#![allow(missing_docs)]

use crate::database::entity::{
    Entity, expect_field_count, field_from_tuple_binary, field_to_tuple_binary,
};
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu_type::data_value::DataValue;
use mudu_type::datum::Datum;
use paste::paste;

const OBJECT_NAME_PREFIX: &str = "object";
const OBJECT_FIELD_PREFIX: &str = "field";

macro_rules! impl_entity_trait {
    ($(($variant_upper:ident, $variant_lower:ident, $datum_type:ty)),+ $(,)?) => {
        $(
            impl Entity for $datum_type {
                paste! {
                    fn tuple_desc() -> &'static TupleFieldDesc {
                        static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
                        ONCE_LOCK.get_or_init(|| {
                            TupleFieldDesc::new(vec![
                                crate::tuple::datum_desc::DatumDesc::new(
                                    format!("{}_{}", OBJECT_FIELD_PREFIX, stringify!($variant_lower)),
                                    <$datum_type>::data_type().clone(),
                                ),
                            ])
                        })
                    }

                    fn table_name() -> &'static str {
                        lazy_static::lazy_static! {
                            static ref OBJECT_NAME:String = {
                                format!("{}_{}", OBJECT_NAME_PREFIX,  stringify!($variant_lower))
                            };
                        }
                        &OBJECT_NAME
                    }

                    fn from_tuple(row: &TupleField) -> RS<Self> {
                        let fields = row.fields();
                        expect_field_count(fields.len(), 1)?;
                        field_from_tuple_binary(
                            Self::table_name(),
                            &Self::tuple_desc().fields()[0].name().to_string(),
                            &fields[0],
                        )
                    }

                    fn from_tuple_value(row: &TupleValue) -> RS<Self> {
                        let values = row.values();
                        expect_field_count(values.len(), 1)?;
                        Self::from_value(&values[0])
                    }

                    fn to_tuple(&self) -> RS<TupleField> {
                        Ok(TupleField::new_nullable(vec![field_to_tuple_binary(
                            self,
                        )?]))
                    }

                    fn to_tuple_value(&self) -> RS<TupleValue> {
                        Ok(TupleValue::from(vec![DataValue::[<from_$variant_lower>](
                            self.clone(),
                        )]))
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
    (String, string, String),
    (Numeric, numeric, Numeric)
);
