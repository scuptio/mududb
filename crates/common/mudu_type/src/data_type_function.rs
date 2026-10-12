use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::type_error::TyErr;
use std::sync::Arc;

/// Wraps an `i32` into a shared value.
pub fn value_from_i32(value: i32) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_i32(value)))
}

/// Wraps an `i64` into a shared value.
pub fn value_from_i64(value: i64) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_i64(value)))
}

/// Wraps an `f32` into a shared value.
pub fn value_from_f32(value: f32) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_f32(value)))
}

/// Wraps an `f64` into a shared value.
pub fn value_from_f64(value: f64) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_f64(value)))
}

/// Wraps a string into a shared value.
pub fn value_from_string(value: String) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_string(value)))
}

/// Parses a textual representation into a shared value of type `ty`.
pub fn input_textual(textual: &str, ty: &DataType) -> Result<Arc<DataValue>, TyErr> {
    let id = ty.type_family();
    let value = id.fn_input()(textual, ty)?;
    Ok(Arc::new(value))
}

/// Renders a value in the textual representation of type `ty`.
pub fn output_textual(value: &DataValue, ty: &DataType) -> Result<String, TyErr> {
    let id = ty.type_family();
    let value = id.fn_output()(value, ty)?;
    Ok(value.into())
}

/// Encodes a value into the binary representation of type `ty`.
pub fn send_binary(value: &DataValue, ty: &DataType) -> Result<Vec<u8>, TyErr> {
    let id = ty.type_family();
    let value = id.fn_send()(value, ty)?;
    Ok(value.into())
}

/// Decodes a binary representation into a shared value of type `ty`.
pub fn recv_binary(value: &[u8], ty: &DataType) -> Result<Arc<DataValue>, TyErr> {
    let id = ty.type_family();
    let (value, _) = id.fn_recv()(value, ty)?;
    Ok(Arc::new(value))
}
