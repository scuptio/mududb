use crate::common::result::RS;
use crate::database::record::Record;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;

pub fn record_from_tuple<R: Record, T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(
    row: T,
    desc: D,
) -> RS<R> {
    let mut s = R::new_empty();
    if row.as_ref().fields().len() != desc.as_ref().fields().len() {
        panic!("Users::from_tuple wrong length");
    }
    for (i, dat) in row.as_ref().fields().iter().enumerate() {
        let dd = &desc.as_ref().fields()[i];
        s.set_binary(dd.name(), dat)?;
    }
    Ok(s)
}

pub fn record_to_tuple<R: Record, D: AsRef<TupleFieldDesc>>(record: &R, desc: D) -> RS<TupleField> {
    let mut tuple = vec![];
    for d in desc.as_ref().fields() {
        let opt_datum = record.get_binary(d.name())?;
        if let Some(datum) = opt_datum {
            tuple.push(datum);
        }
    }
    Ok(TupleField::new(tuple))
}
