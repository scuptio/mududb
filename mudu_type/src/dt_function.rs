use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::type_error::TyErr;
use std::sync::Arc;

pub fn value_from_i32(value: i32) -> Result<Arc<DatValue>, TyErr> {
    Ok(Arc::new(DatValue::from_i32(value)))
}

pub fn value_from_i64(value: i64) -> Result<Arc<DatValue>, TyErr> {
    Ok(Arc::new(DatValue::from_i64(value)))
}

pub fn value_from_f32(value: f32) -> Result<Arc<DatValue>, TyErr> {
    Ok(Arc::new(DatValue::from_f32(value)))
}

pub fn value_from_f64(value: f64) -> Result<Arc<DatValue>, TyErr> {
    Ok(Arc::new(DatValue::from_f64(value)))
}

pub fn value_from_string(value: String) -> Result<Arc<DatValue>, TyErr> {
    Ok(Arc::new(DatValue::from_string(value)))
}

pub fn input_textual(textual: &str, ty: &DatType) -> Result<Arc<DatValue>, TyErr> {
    let id = ty.dat_type_id();
    let value = id.fn_input()(textual, ty)?;
    Ok(Arc::new(value))
}

pub fn output_textual(value: &DatValue, ty: &DatType) -> Result<String, TyErr> {
    let id = ty.dat_type_id();
    let value = id.fn_output()(value, ty)?;
    Ok(value.into())
}

pub fn send_binary(value: &DatValue, ty: &DatType) -> Result<Vec<u8>, TyErr> {
    let id = ty.dat_type_id();
    let value = id.fn_send()(value, ty)?;
    Ok(value.into())
}

pub fn recv_binary(value: &Vec<u8>, ty: &DatType) -> Result<Arc<DatValue>, TyErr> {
    let id = ty.dat_type_id();
    let (value, _) = id.fn_recv()(value, ty)?;
    Ok(Arc::new(value))
}
