use crate::error;
use crate::exports::mududb::component_shim::types;
use crate::ids;
use mududb::types::data_value::DataValue;

pub fn null() -> types::Value {
    types::Value::Null
}

pub fn from_boolean(input: bool) -> types::Value {
    types::Value::Boolean(input)
}

pub fn from_int64(input: i64) -> types::Value {
    types::Value::Int64(input)
}

pub fn from_float64(input: f64) -> types::Value {
    types::Value::Float64(input)
}

pub fn from_text(input: String) -> types::Value {
    types::Value::Text(input)
}

pub fn from_binary(input: Vec<u8>) -> types::Value {
    types::Value::Binary(input)
}

pub fn from_oid(input: types::Oid) -> types::Value {
    types::Value::ObjectId(input)
}

pub fn is_null(input: &types::Value) -> bool {
    matches!(input, types::Value::Null)
}

pub fn as_boolean(input: types::Value) -> Result<bool, types::Error> {
    match input {
        types::Value::Boolean(value) => Ok(value),
        _ => Err(error::type_error("boolean")),
    }
}

pub fn as_int64(input: types::Value) -> Result<i64, types::Error> {
    match input {
        types::Value::Int64(value) => Ok(value),
        _ => Err(error::type_error("int64")),
    }
}

pub fn as_float64(input: types::Value) -> Result<f64, types::Error> {
    match input {
        types::Value::Float64(value) => Ok(value),
        _ => Err(error::type_error("float64")),
    }
}

pub fn as_text(input: types::Value) -> Result<String, types::Error> {
    match input {
        types::Value::Text(value) => Ok(value),
        _ => Err(error::type_error("text")),
    }
}

pub fn as_binary(input: types::Value) -> Result<Vec<u8>, types::Error> {
    match input {
        types::Value::Binary(value) => Ok(value),
        _ => Err(error::type_error("binary")),
    }
}

pub fn as_oid(input: types::Value) -> Result<types::Oid, types::Error> {
    match input {
        types::Value::ObjectId(value) => Ok(value),
        _ => Err(error::type_error("oid")),
    }
}

pub fn into_data_value(input: types::Value) -> DataValue {
    match input {
        types::Value::Null => DataValue::null(),
        types::Value::Boolean(value) => DataValue::from_i32(i32::from(value)),
        types::Value::Int64(value) => DataValue::from_i64(value),
        types::Value::Float64(value) => DataValue::from_f64(value),
        types::Value::Text(value) => DataValue::from_string(value),
        types::Value::Binary(value) => DataValue::from_binary(value),
        types::Value::ObjectId(value) => DataValue::from_u128(ids::to_facade(value)),
    }
}

pub fn from_data_value(input: &DataValue) -> types::Value {
    if input.is_null() {
        return types::Value::Null;
    }

    if let Some(value) = input.as_i32() {
        return types::Value::Int64(i64::from(*value));
    }
    if let Some(value) = input.as_i64() {
        return types::Value::Int64(*value);
    }
    if let Some(value) = input.as_f32() {
        return types::Value::Float64(f64::from(*value));
    }
    if let Some(value) = input.as_f64() {
        return types::Value::Float64(*value);
    }
    if let Some(value) = input.as_string() {
        return types::Value::Text(value.clone());
    }
    if let Some(value) = input.as_binary() {
        return types::Value::Binary(value.clone());
    }
    if let Some(value) = input.as_u128() {
        return types::Value::ObjectId(ids::from_facade(*value));
    }

    types::Value::Text(format!("{input:?}"))
}
