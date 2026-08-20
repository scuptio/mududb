//! SQL parameter conversion and placeholder replacement helpers.

use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_type::DataType;
use mudu_type::data_type_function::{recv_binary, send_binary};
use mudu_type::data_type_param_kind::DataTypeParamKind;
use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
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
    } else if let Some(v) = value.as_numeric() {
        Ok(Value::Text(v.to_plain_string()))
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
    if normalized.contains("NUMERIC") || normalized.contains("DECIMAL") {
        TypeFamily::Numeric
    } else if normalized.contains("BIGINT") || normalized.contains("INT8") {
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
            TypeFamily::Numeric => Ok(DataValue::from_numeric(Numeric::from(v))),
            _ => Ok(DataValue::from_i64(v)),
        },
        ValueRef::Real(v) => match preferred {
            TypeFamily::F32 if v >= f32::MIN as f64 && v <= f32::MAX as f64 => {
                Ok(DataValue::from_f32(v as f32))
            }
            TypeFamily::Numeric => Ok(DataValue::from_numeric(
                Numeric::parse(v.to_string().as_str()).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "parse numeric error", e)
                })?,
            )),
            _ => Ok(DataValue::from_f64(v)),
        },
        ValueRef::Text(v) => match preferred {
            TypeFamily::Numeric => {
                let s = String::from_utf8_lossy(v);
                Ok(DataValue::from_numeric(
                    Numeric::parse(s.as_ref()).map_err(|e| {
                        mudu_error!(ErrorCode::TypeConversionFailed, "parse numeric error", e)
                    })?,
                ))
            }
            _ => Ok(DataValue::from_string(
                String::from_utf8_lossy(v).into_owned(),
            )),
        },
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
        | TypeFamily::Numeric
        | TypeFamily::String => DataType::default_for(id),
        _ => DataType::new_no_param(id),
    }
}

/// Maps a SQLite declared column type to the exact [`DataType`] used to
/// decode and encode relation datums, preserving `NUMERIC(precision, scale)`
/// parameters (the numeric binary encoding is scale-dependent).
pub fn sqlite_decl_data_type(decl_type: Option<&str>, idx: usize) -> DataType {
    if let Some(name) = decl_type {
        let normalized = name.to_ascii_uppercase();
        if (normalized.contains("NUMERIC") || normalized.contains("DECIMAL"))
            && let Some(param) = parse_numeric_param(&normalized)
        {
            return DataType::from_id_param(
                TypeFamily::Numeric,
                Some(DataTypeParamKind::Numeric(Box::new(param))),
            );
        }
    }
    datum_type_for_id(sqlite_decl_type_to_id(decl_type, idx))
}

/// Parses the `(precision, scale)` parameter of a `NUMERIC`/`DECIMAL`
/// declaration; absent parameters default to `(38, 0)`.
fn parse_numeric_param(normalized: &str) -> Option<DataTypeParamNumeric> {
    let open = normalized.find('(')?;
    let close = normalized[open..].find(')')? + open;
    let inner = &normalized[open + 1..close];
    let mut parts = inner.split(',');
    let precision = parts.next()?.trim().parse().ok()?;
    let scale = parts.next().map(str::trim).unwrap_or("0").parse().ok()?;
    Some(DataTypeParamNumeric::new(precision, scale))
}

/// Converts a [`DataValue`] into a SQLite [`Value`] for the relation datum
/// path (mirrors [`datum_to_sqlite_value`] for already-decoded values).
pub fn data_value_to_sqlite_value(value: &DataValue) -> RS<Value> {
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
    } else if let Some(v) = value.as_numeric() {
        Ok(Value::Text(v.to_plain_string()))
    } else if let Some(v) = value.as_binary() {
        Ok(Value::Blob(v.clone()))
    } else {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "unsupported sqlite relation datum type: {:?}",
                value.type_family()
            )
        ))
    }
}

/// Decodes a relation datum (the column binary format) into a SQLite bind
/// value.
pub fn datum_binary_to_sqlite_value(datum: &[u8], data_type: &DataType) -> RS<Value> {
    let value = recv_binary(datum, data_type).map_err(|e| e.to_m_err())?;
    data_value_to_sqlite_value(&value)
}

/// Encodes a SQLite column value back into the relation datum (column
/// binary) format; SQL `NULL` becomes `None`.
pub fn sqlite_value_to_datum_binary(
    raw: ValueRef<'_>,
    data_type: &DataType,
) -> RS<Option<Vec<u8>>> {
    match raw {
        ValueRef::Null => Ok(None),
        other => {
            let value = sqlite_value_to_data_value(other, data_type.type_family())?;
            let binary = send_binary(&value, data_type).map_err(|e| e.to_m_err())?;
            Ok(Some(binary))
        }
    }
}

/// Converts SQL parameters into wire [`DataValue`]s for the mudud protocol.
///
/// Values keep the order of the `?` placeholders they bind to. NUMERIC values
/// are sent in their plain string form; the server coerces them back to the
/// target NUMERIC type during bind.
pub fn to_data_values(params: &dyn SQLParams) -> RS<Vec<DataValue>> {
    let mut values = Vec::with_capacity(params.size() as usize);
    for idx in 0..params.size() {
        let datum = params.get_idx(idx).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("sql param index {} does not exist", idx)
            )
        })?;
        let data_type = datum_type_for_id(datum.type_family()?);
        let value = datum.to_value(&data_type)?;
        let value = if let Some(numeric) = value.as_numeric() {
            DataValue::from_string(numeric.to_plain_string())
        } else {
            value
        };
        values.push(value);
    }
    Ok(values)
}

/// Replaces `?` placeholders in `sql_text` with SQL literal values.
///
/// String values are emitted as single-quoted SQL string literals (with embedded
/// single quotes escaped), which is accepted by SQLite, MySQL, and PostgreSQL.
/// NUMERIC values are emitted as bare numeric literals (their textual form is a
/// JSON string, which PostgreSQL would read as a quoted identifier). Date/time
/// values, whose textual form is also a JSON string, are re-quoted as SQL string
/// literals. JSON `null` is normalized to SQL `NULL` for the same reason.
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
        let family = datum.type_family()?;
        let ty = datum_type_for_id(family);
        let literal = if family == TypeFamily::String {
            let value = datum.to_value(&ty)?;
            let s = value.as_string().ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidType,
                    format!(
                        "placeholder {}: expected String, got {:?} in SQL: {}",
                        idx, value, sql_text
                    )
                )
            })?;
            format!("'{}'", s.replace('\'', "''"))
        } else if family == TypeFamily::Numeric {
            // NUMERIC's textual form is a JSON string ("5820"), which
            // PostgreSQL reads as a quoted identifier; emit a bare numeric
            // literal instead.
            let value = datum.to_value(&ty)?;
            let numeric = value.as_numeric().ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidType,
                    format!(
                        "placeholder {}: expected Numeric, got {:?} in SQL: {}",
                        idx, value, sql_text
                    )
                )
            })?;
            numeric.to_plain_string()
        } else {
            let textual = datum.to_textual(&ty)?;
            if textual.as_str().eq_ignore_ascii_case("null") {
                "NULL".to_string()
            } else if let Some(inner) = textual
                .as_str()
                .strip_prefix('"')
                .and_then(|s| s.strip_suffix('"'))
            {
                // Date/time textual output is a JSON string; re-quote it as a
                // SQL string literal so PostgreSQL accepts it.
                format!("'{}'", inner.replace('\'', "''"))
            } else {
                textual.to_string()
            }
        };
        out.push_str(&literal);
        start = *pos + 1;
    }
    out.push_str(&sql_text[start..]);
    Ok(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use mudu_contract::database::sql_param_value::SQLParamValue;

    #[test]
    fn to_data_values_preserves_order_and_stringifies_numeric() {
        let params = SQLParamValue::from_vec(vec![
            DataValue::from_i32(7),
            DataValue::from_string("abc".to_string()),
            DataValue::from_numeric(Numeric::parse("3.00").unwrap()),
            DataValue::from_f64(2.5),
        ]);
        let values = to_data_values(&params).unwrap();
        assert_eq!(values.len(), 4);
        assert_eq!(values[0].expect_i32(), &7);
        assert_eq!(values[1].expect_string(), "abc");
        // NUMERIC goes on the wire in plain string form; the server coerces
        // it back to the column NUMERIC type during bind.
        assert_eq!(values[2].expect_string(), "3.00");
        assert_eq!(values[3].expect_f64(), &2.5);
    }

    #[test]
    fn to_data_values_empty_params() {
        assert!(to_data_values(&()).unwrap().is_empty());
    }

    #[test]
    fn replace_placeholders_emits_bare_numeric_and_quoted_strings() {
        let params = SQLParamValue::from_vec(vec![
            DataValue::from_i32(2),
            DataValue::from_numeric(Numeric::parse("5820.00").unwrap()),
            DataValue::from_string("it's".to_string()),
        ]);
        let rendered = replace_placeholders("INSERT INTO t VALUES (?, ?, ?)", &params).unwrap();
        // The NUMERIC literal must not be double-quoted: PostgreSQL would read
        // `"5820.00"` as an identifier and reject the statement.
        assert_eq!(rendered, "INSERT INTO t VALUES (2, 5820.00, 'it''s')");
    }
}
