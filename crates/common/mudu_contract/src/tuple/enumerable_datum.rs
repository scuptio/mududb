//! Datum types enumerable as a sequence of fields.

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::data_value::DataValue;

/// A value that can be enumerated as a sequence of typed fields.
pub trait EnumerableDatum {
    /// Encodes the fields into memory values typed by `datum_desc`.
    fn to_value(&self, datum_desc: &[DatumDesc]) -> RS<Vec<DataValue>>;

    /// Encodes the fields into binary values typed by `datum_desc`.
    fn to_binary(&self, datum_desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>>;

    /// Builds the field description of this value using `field_name` for the field names.
    fn tuple_desc(&self, field_name: &[String]) -> RS<TupleFieldDesc>;
}
