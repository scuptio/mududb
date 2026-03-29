use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;
use rusqlite::types::{Value, ValueRef};

pub fn to_sqlite_values(params: &dyn SQLParams) -> RS<Vec<Value>> {
    let mut values = Vec::with_capacity(params.size() as usize);
    for idx in 0..params.size() {
        let datum = params.get_idx(idx).ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("sql param index {} does not exist", idx)
            )
        })?;
        values.push(datum_to_sqlite_value(datum)?);
    }
    Ok(values)
}

pub fn datum_to_sqlite_value(datum: &dyn DatumDyn) -> RS<Value> {
    let type_id = datum.dat_type_id()?;
    let dat_type = datum_type_for_id(type_id);
    let value = datum.to_value(&dat_type)?;
    if let Some(v) = value.as_i32() {
        Ok(Value::Integer(*v as i64))
    } else if let Some(v) = value.as_i64() {
        Ok(Value::Integer(*v))
    } else if let Some(v) = value.as_f32() {
        Ok(Value::Real(*v as f64))
    } else if let Some(v) = value.as_f64() {
        Ok(Value::Real(*v))
    } else if let Some(v) = value.as_string() {
        Ok(Value::Text(v.clone()))
    } else if let Some(v) = value.as_binary() {
        Ok(Value::Blob(v.clone()))
    } else {
        Err(m_error!(
            EC::NotImplemented,
            format!("unsupported sqlite parameter type: {:?}", type_id)
        ))
    }
}

pub fn build_sqlite_desc(stmt: &rusqlite::Statement<'_>) -> TupleFieldDesc {
    let columns = stmt.columns();
    let fields = columns
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            let name = column.name().to_string();
            let type_id = sqlite_decl_type_to_id(column.decl_type(), idx);
            DatumDesc::new(name, datum_type_for_id(type_id))
        })
        .collect();
    TupleFieldDesc::new(fields)
}

pub fn sqlite_decl_type_to_id(decl_type: Option<&str>, idx: usize) -> DatTypeID {
    let Some(name) = decl_type else {
        return if idx == 0 {
            DatTypeID::I64
        } else {
            DatTypeID::String
        };
    };
    let normalized = name.to_ascii_uppercase();
    if normalized.contains("BIGINT") || normalized.contains("INT8") {
        DatTypeID::I64
    } else if normalized.contains("INT") {
        DatTypeID::I32
    } else if normalized.contains("REAL")
        || normalized.contains("FLOA")
        || normalized.contains("DOUB")
    {
        DatTypeID::F64
    } else if normalized.contains("BLOB") {
        DatTypeID::Binary
    } else {
        DatTypeID::String
    }
}

pub fn read_sqlite_row(row: &rusqlite::Row<'_>, desc: &TupleFieldDesc) -> RS<TupleValue> {
    let mut values = Vec::with_capacity(desc.fields().len());
    for (idx, field) in desc.fields().iter().enumerate() {
        let raw = row
            .get_ref(idx)
            .map_err(|e| m_error!(EC::DBInternalError, "read sqlite column error", e))?;
        values.push(sqlite_value_to_dat_value(raw, field.dat_type_id())?);
    }
    Ok(TupleValue::from(values))
}

pub fn sqlite_value_to_dat_value(raw: ValueRef<'_>, preferred: DatTypeID) -> RS<DatValue> {
    match raw {
        ValueRef::Null => Err(m_error!(EC::NotImplemented, "NULL value is not supported")),
        ValueRef::Integer(v) => match preferred {
            DatTypeID::I32 if i32::try_from(v).is_ok() => Ok(DatValue::from_i32(v as i32)),
            _ => Ok(DatValue::from_i64(v)),
        },
        ValueRef::Real(v) => match preferred {
            DatTypeID::F32 if v >= f32::MIN as f64 && v <= f32::MAX as f64 => {
                Ok(DatValue::from_f32(v as f32))
            }
            _ => Ok(DatValue::from_f64(v)),
        },
        ValueRef::Text(v) => Ok(DatValue::from_string(
            String::from_utf8_lossy(v).into_owned(),
        )),
        ValueRef::Blob(v) => Ok(DatValue::from_binary(v.to_vec())),
    }
}

pub fn datum_type_for_id(id: DatTypeID) -> DatType {
    match id {
        DatTypeID::Binary => DatType::new_no_param(id),
        DatTypeID::I32 | DatTypeID::I64 | DatTypeID::F32 | DatTypeID::F64 | DatTypeID::String => {
            DatType::default_for(id)
        }
        _ => DatType::new_no_param(id),
    }
}

pub fn replace_placeholders(sql_text: &str, params: &dyn SQLParams) -> RS<String> {
    if params.size() == 0 {
        return Ok(sql_text.to_string());
    }

    let pieces: Vec<_> = sql_text.match_indices('?').collect();
    if pieces.len() != params.size() as usize {
        return Err(m_error!(
            EC::ParseErr,
            "parameter and placeholder count mismatch"
        ));
    }

    let mut out = String::with_capacity(sql_text.len() + 32 * pieces.len());
    let mut start = 0;
    for (idx, (pos, _)) in pieces.iter().enumerate() {
        out.push_str(&sql_text[start..*pos]);
        let datum = params.get_idx(idx as u64).ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("sql param index {} does not exist", idx)
            )
        })?;
        let ty = datum_type_for_id(datum.dat_type_id()?);
        let textual = datum.to_textual(&ty)?;
        out.push_str(textual.as_str());
        start = *pos + 1;
    }
    out.push_str(&sql_text[start..]);
    Ok(out)
}
