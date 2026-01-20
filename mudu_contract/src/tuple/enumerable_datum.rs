use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use mudu_type::dat_value::DatValue;

pub trait EnumerableDatum {
    fn to_value(&self, datum_desc: &[DatumDesc]) -> RS<Vec<DatValue>>;
    
    fn to_binary(&self, datum_desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>>;

    fn tuple_desc(&self, field_name: &[String]) -> RS<TupleFieldDesc>;
}
