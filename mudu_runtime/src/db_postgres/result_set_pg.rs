use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSet;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
#[cfg(not(target_arch = "wasm32"))]
use postgres::Row;
use std::sync::{Arc, Mutex};
use mudu_contract::tuple::tuple_value::TupleValue;

pub struct ResultSetPG {
    desc: Arc<TupleFieldDesc>,
    rows: Mutex<Vec<Row>>,
}

impl ResultSetPG {
    pub fn new(desc: Arc<TupleFieldDesc>, rows: Vec<Row>) -> Self {
        Self {
            desc,
            rows: Mutex::new(rows),
        }
    }
}
impl ResultSet for ResultSetPG {
    fn next(&self) -> RS<Option<TupleValue>> {
        let opt_row = self.rows.lock().unwrap().pop();
        match opt_row {
            Some(row) => {
                let mut tuple_row = vec![];
                for (i, d) in self.desc.fields().iter().enumerate() {
                    let id = d.dat_type_id();
                    let datum = match id {
                        DatTypeID::I32 => {
                            let val: i32 = row.get(i);
                            DatValue::from_i32(val)
                        }
                        DatTypeID::I64 => {
                            let val: i64 = row.get(i);
                            DatValue::from_i64(val)
                        }
                        DatTypeID::F32 => {
                            let val: f32 = row.get(i);
                            DatValue::from_f32(val)
                        }
                        DatTypeID::F64 => {
                            let val: f64 = row.get(i);
                            DatValue::from_f64(val)
                        }
                        DatTypeID::String => {
                            let val: String = row.get(i);
                            DatValue::from_string(val)
                        }
                        _ => {
                            panic!("unsupported type {:?}", id);
                        }
                    };

                    tuple_row.push(datum);
                }
                Ok(Some(TupleValue::from(tuple_row)))
            }
            None => Ok(None),
        }
    }
}
