use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_tuple_row::UniTupleRow;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_value::TupleValue;

impl UniTupleRow {
    pub fn uni_to(self) -> RS<TupleValue> {
        let mut vec = Vec::with_capacity(self.fields.len());
        for value in self.fields {
            let v = value.uni_to()?;
            vec.push(v);
        }
        Ok(TupleValue::from(vec))
    }

    pub fn uni_from(tuple_value: TupleValue) -> RS<UniTupleRow> {
        let mut vec = Vec::with_capacity(tuple_value.values().len());
        for value in tuple_value.into() {
            let value = UniDatValue::uni_from(value)?;
            vec.push(value);
        }
        Ok(UniTupleRow { fields: vec })
    }
}
