//! SQL parameter conversion and placeholder replacement helpers.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use rusqlite::types::{Value, ValueRef};

/// Converts SQL parameters into SQLite [`Value`]s.
pub fn to_sqlite_values(params: &dyn SQLParams) -> RS<Vec<Value>> {
    let mut values = Vec::with_capacity(params.size() as usize);
    for idx in 0..params.size() {
        let datum = params.get_idx(idx).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("sql param index {} does not exist", idx)
            )
        })?;
        values.push(datum_to_sqlite_value(datum)?);
    }
    Ok(values)
}

/// Converts a single datum into a SQLite [`Value`].
pub fn datum_to_sqlite_value(datum: &dyn DatumDyn) -> RS<Value> {
    let type_id = datum.type_family()?;
    let data_type = datum_type_for_id(type_id);
    let value = datum.to_value(&data_type)?;
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
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!("unsupported sqlite parameter type: {:?}", type_id)
        ))
    }
}

/// Builds a tuple field descriptor from a SQLite statement's columns.
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

/// Maps a SQLite declared column type to a [`TypeFamily`].
pub fn sqlite_decl_type_to_id(decl_type: Option<&str>, idx: usize) -> TypeFamily {
    let Some(name) = decl_type else {
        return if idx == 0 {
            TypeFamily::I64
        } else {
            TypeFamily::String
        };
    };
    let normalized = name.to_ascii_uppercase();
    if normalized.contains("BIGINT") || normalized.contains("INT8") {
        TypeFamily::I64
    } else if normalized.contains("INT") {
        TypeFamily::I32
    } else if normalized.contains("REAL")
        || normalized.contains("FLOA")
        || normalized.contains("DOUB")
    {
        TypeFamily::F64
    } else if normalized.contains("BLOB") {
        TypeFamily::Binary
    } else {
        TypeFamily::String
    }
}

/// Reads a single SQLite row into a [`TupleValue`].
pub fn read_sqlite_row(row: &rusqlite::Row<'_>, desc: &TupleFieldDesc) -> RS<TupleValue> {
    let mut values = Vec::with_capacity(desc.fields().len());
    for (idx, field) in desc.fields().iter().enumerate() {
        let raw = row
            .get_ref(idx)
            .map_err(|e| mudu_error!(ErrorCode::Database, "read sqlite column error", e))?;
        values.push(sqlite_value_to_data_value(raw, field.type_family())?);
    }
    Ok(TupleValue::from(values))
}

/// Converts a SQLite [`ValueRef`] into a [`DataValue`].
pub fn sqlite_value_to_data_value(raw: ValueRef<'_>, preferred: TypeFamily) -> RS<DataValue> {
    match raw {
        ValueRef::Null => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "NULL value is not supported"
        )),
        ValueRef::Integer(v) => match preferred {
            TypeFamily::I32 if i32::try_from(v).is_ok() => Ok(DataValue::from_i32(v as i32)),
            _ => Ok(DataValue::from_i64(v)),
        },
        ValueRef::Real(v) => match preferred {
            TypeFamily::F32 if v >= f32::MIN as f64 && v <= f32::MAX as f64 => {
                Ok(DataValue::from_f32(v as f32))
            }
            _ => Ok(DataValue::from_f64(v)),
        },
        ValueRef::Text(v) => Ok(DataValue::from_string(
            String::from_utf8_lossy(v).into_owned(),
        )),
        ValueRef::Blob(v) => Ok(DataValue::from_binary(v.to_vec())),
    }
}

/// Returns the default [`DataType`] for a given [`TypeFamily`].
pub fn datum_type_for_id(id: TypeFamily) -> DataType {
    match id {
        TypeFamily::Binary => DataType::new_no_param(id),
        TypeFamily::I32
        | TypeFamily::I64
        | TypeFamily::F32
        | TypeFamily::F64
        | TypeFamily::String => DataType::default_for(id),
        _ => DataType::new_no_param(id),
    }
}

/// Replaces `?` placeholders in `sql_text` with textual parameter values.
pub fn replace_placeholders(sql_text: &str, params: &dyn SQLParams) -> RS<String> {
    if params.size() == 0 {
        return Ok(sql_text.to_string());
    }

    let pieces: Vec<_> = sql_text.match_indices('?').collect();
    if pieces.len() != params.size() as usize {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "parameter and placeholder count mismatch"
        ));
    }

    let mut out = String::with_capacity(sql_text.len() + 32 * pieces.len());
    let mut start = 0;
    for (idx, (pos, _)) in pieces.iter().enumerate() {
        out.push_str(&sql_text[start..*pos]);
        let datum = params.get_idx(idx as u64).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("sql param index {} does not exist", idx)
            )
        })?;
        let ty = datum_type_for_id(datum.type_family()?);
        let textual = datum.to_textual(&ty)?;
        out.push_str(textual.as_str());
        start = *pos + 1;
    }
    out.push_str(&sql_text[start..]);
    Ok(out)
}
