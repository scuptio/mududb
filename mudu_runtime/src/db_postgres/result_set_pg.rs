use mudu::common::result::RS;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::dt_impl::dat_typed::DatTyped;
use mudu::database::result_set::ResultSet;
use mudu::tuple::datum::DatumDyn;
use mudu::tuple::tuple_field::TupleField;
use mudu::tuple::tuple_field_desc::TupleFieldDesc;
#[cfg(not(target_arch = "wasm32"))]
use postgres::Row;
use std::sync::{Arc, Mutex};

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
    fn next(&self) -> RS<Option<TupleField>> {
        let opt_row = self.rows.lock().unwrap().pop();
        match opt_row {
            Some(row) => {
                let mut tuple_row = vec![];
                for (i, d) in self.desc.fields().iter().enumerate() {
                    let id = d.dat_type_id();
                    let datum = match id {
                        DatTypeID::I32 => {
                            let val: i32 = row.get(i);
                            DatTyped::I32(val)
                        }
                        DatTypeID::I64 => {
                            let val: i64 = row.get(i);
                            DatTyped::I64(val)
                        }
                        DatTypeID::F32 => {
                            let val: f32 = row.get(i);
                            DatTyped::F32(val)
                        }
                        DatTypeID::F64 => {
                            let val: f64 = row.get(i);
                            DatTyped::F64(val)
                        }
                        DatTypeID::CharFixedLen => {
                            let val: String = row.get(i);
                            DatTyped::String(val)
                        }
                        DatTypeID::CharVarLen => {
                            let val: String = row.get(i);
                            DatTyped::String(val)
                        }
                    };
                    let binary = datum.to_binary(d.param_obj())?;
                    tuple_row.push(binary.into());
                }
                Ok(Some(TupleField::new(tuple_row)))
            }
            None => Ok(None),
        }
    }
}
