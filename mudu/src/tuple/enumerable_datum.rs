use crate::common::result::RS;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_item_desc::TupleItemDesc;

pub trait EnumerableDatum {
    fn to_binary(&self, desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>>;

    fn tuple_desc(&self) -> RS<TupleItemDesc>;
}
