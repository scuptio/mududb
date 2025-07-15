use mudu::common::result::RS;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID;
use mudu::data_type::dt_impl::dat_typed::DatTyped;
use mudu::database::result_set::ResultSet;
use mudu::database::row_desc::RowDesc;
use mudu::database::tuple_row::TupleRow;
use mudu::tuple::datum::Datum;
#[cfg(not(target_arch = "wasm32"))]
use postgres::Row;
use std::sync::Mutex;

pub struct ResultSetPG {
    desc: RowDesc,
    rows:Mutex<Vec<Row>>
}



impl ResultSetPG {
    pub fn new(desc:RowDesc, rows:Vec<Row>) -> Self {
        Self {
            desc,
            rows:Mutex::new(rows)
        }
    }
}
impl ResultSet for ResultSetPG {
    fn next(&self) -> RS<Option<TupleRow>> {
        let opt_row = self.rows.lock().unwrap().pop();
        match opt_row {
            Some(row) => {
                let mut tuple_row = vec![];
                for (i, d) in self.desc.desc().iter().enumerate() {
                    let id = d.data_type_id();
                    let datum = match id {
                        DatTypeID::I32 => {
                            let val: i32 = row.get(i);
                            Datum::Typed(DatTyped::I32(val))
                        }
                        DatTypeID::I64 => {
                            let val: i64 = row.get(i);
                            Datum::Typed(DatTyped::I64(val))
                        }
                        DatTypeID::F32 => {
                            let val: f32 = row.get(i);
                            Datum::Typed(DatTyped::F32(val))
                        }
                        DatTypeID::F64 => {
                            let val: f64 = row.get(i);
                            Datum::Typed(DatTyped::F64(val))
                        }
                        DatTypeID::FixedLenString => {
                            let val: String = row.get(i);
                            Datum::Typed(DatTyped::String(val))
                        }
                        DatTypeID::VarLenString => {
                            let val: String = row.get(i);
                            Datum::Typed(DatTyped::String(val))
                        }
                    };
                    tuple_row.push(datum);
                }
                Ok(Some(TupleRow::new(tuple_row)))
            }
            None => {
                Ok(None)
            }
        }
    }
}