use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::type_error::TyErr;
use std::sync::Arc;

pub fn value_from_i32(value: i32) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_i32(value)))
}

pub fn value_from_i64(value: i64) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_i64(value)))
}

pub fn value_from_f32(value: f32) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_f32(value)))
}

pub fn value_from_f64(value: f64) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_f64(value)))
}

pub fn value_from_string(value: String) -> Result<Arc<DataValue>, TyErr> {
    Ok(Arc::new(DataValue::from_string(value)))
}

pub fn input_textual(textual: &str, ty: &DataType) -> Result<Arc<DataValue>, TyErr> {
    let id = ty.type_family();
    let value = id.fn_input()(textual, ty)?;
    Ok(Arc::new(value))
}

pub fn output_textual(value: &DataValue, ty: &DataType) -> Result<String, TyErr> {
    let id = ty.type_family();
    let value = id.fn_output()(value, ty)?;
    Ok(value.into())
}

pub fn send_binary(value: &DataValue, ty: &DataType) -> Result<Vec<u8>, TyErr> {
    let id = ty.type_family();
    let value = id.fn_send()(value, ty)?;
    Ok(value.into())
}

pub fn recv_binary(value: &[u8], ty: &DataType) -> Result<Arc<DataValue>, TyErr> {
    let id = ty.type_family();
    let (value, _) = id.fn_recv()(value, ty)?;
    Ok(Arc::new(value))
}
