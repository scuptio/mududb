use crate::common::buf::Buf;
use crate::common::result::RS;
use crate::tuple::datum::Datum;
use crate::tuple::tuple_desc::TupleDesc;
use crate::tuple::tuple_raw::TupleRaw;
use crate::tuple::write_value;

pub fn build_tuple_into(
    vec: &[Datum],
    tuple_desc: &TupleDesc,
    tuple: &mut TupleRaw,
) -> RS<Result<usize, usize>> {
    if vec.len() != tuple_desc.field_count() {
        panic!("value vector size must equal with tuple_desc field size");
    }
    if tuple.len() < tuple_desc.min_tuple_size() {
        return Ok(Err(tuple_desc.min_tuple_size()));
    }
    let mut offset = tuple_desc.meta_size();
    assert!(offset < tuple.len());
    for (i, v) in vec.iter().enumerate() {
        let field = tuple_desc.get_field_desc(i);
        let r = write_value::write_value_to_tuple(field, offset, v, tuple)?;
        let size = match &r {
            Ok(size) => *size,
            Err(_) => {
                return Ok(r);
            }
        };
        write_value::write_slot_to_tuple(field, offset, size, tuple)?;
        offset += size;
    }
    Ok(Ok(offset))
}

pub fn build_tuple(vec: &Vec<Datum>, tuple_desc: &TupleDesc) -> RS<Buf> {
    let mut tuple = vec![0; tuple_desc.min_tuple_size()];
    tuple.resize(tuple_desc.min_tuple_size(), 0);
    if vec.len() != tuple_desc.field_count() {
        panic!("value vector size must equal with tuple_desc field size");
    }
    if tuple.len() < tuple_desc.min_tuple_size() {
        panic!("low buffer capacity");
    }
    let mut offset = tuple_desc.meta_size();
    assert!(offset < tuple.len());
    for (i, v) in vec.iter().enumerate() {
        let field = tuple_desc.get_field_desc(i);
        let size = loop {
            let r = write_value::write_value_to_tuple(field, offset, v, &mut tuple)?;
            match &r {
                Ok(size) => break *size,
                Err(_size) => {
                    tuple.resize(tuple.len() * 2, 0);
                }
            };
        };
        write_value::write_slot_to_tuple(field, offset, size, &mut tuple)?;
        offset += size;
    }
    tuple.resize(offset, 0);
    Ok(tuple)
}
