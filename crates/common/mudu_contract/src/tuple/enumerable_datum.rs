//! `tuple::enumerable_datum` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::data_value::DataValue;

pub trait EnumerableDatum {
    fn to_value(&self, datum_desc: &[DatumDesc]) -> RS<Vec<DataValue>>;

    fn to_binary(&self, datum_desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>>;

    fn tuple_desc(&self, field_name: &[String]) -> RS<TupleFieldDesc>;
}
