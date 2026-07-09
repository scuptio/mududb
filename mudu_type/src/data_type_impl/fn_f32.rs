use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_convert::FnBase;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use mudu::common::endian;
use mudu::json_value;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::MsgPackValue;

pub fn fn_f32_in_textual(v: &str, _dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_f32_in_json(&DataJson::from(json), _dt)
}

pub fn fn_f32_out_textual(v: &DataValue, _dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_f32_out_json(v, _dt)?;
    Ok(DataTextual::from(json.to_string()))
}

pub fn fn_f32_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    let opt_num = v.as_number();
    let opt_f64 = match opt_num {
        Some(num) => num.as_f64(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to f32", v),
            ));
        }
    };
    match opt_f64 {
        Some(num) => Ok(DataValue::from_f32(num as f32)),
        None => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to f32", v),
        )),
    }
}

pub fn fn_f32_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    let i = v.to_f32();
    let json = json_value!(i);
    Ok(DataJson::from(json))
}

pub fn fn_f32_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
    let opt_value = msg_pack.as_f64();
    let v = match opt_value {
        Some(v) => v,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "cannot convert msg pack to dat value".to_string(),
            ));
        }
    };
    Ok(DataValue::from_f32(v as f32))
}

pub fn fn_f32_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    let i = v.to_f32();
    Ok(MsgPackValue::F32(i))
}

pub fn fn_f32_type_output_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<f32>() as u32))
}

pub fn fn_f32_dat_output_len(_: &DataValue, _ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_f32_type_output_len(_ty)?.unwrap())
}

pub fn fn_f32_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let i = v.to_f32();
    let mut buf = vec![0; size_of_val(&i)];
    endian::write_f32(&mut buf, i);
    Ok(DataBinary::from(buf))
}

pub fn fn_f32_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let i = v.to_f32();
    let len = size_of_val(&i) as u32;
    if buf.len() < size_of_val(&i) {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    endian::write_f32(buf, i);
    Ok(len)
}

pub fn fn_f32_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<f32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let i = endian::read_f32(buf);
    Ok((DataValue::from_f32(i), size_of::<f32>() as u32))
}

pub fn fn_f32_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_f32(f32::default()))
}

pub const FN_F32_CONVERT: FnBase = FnBase {
    input_textual: fn_f32_in_textual,
    output_textual: fn_f32_out_textual,
    input_json: fn_f32_in_json,
    output_json: fn_f32_out_json,
    input_msg_pack: fn_f32_in_msgpack,
    output_msg_pack: fn_f32_out_msgpack,
    type_len: fn_f32_type_output_len,
    data_len: fn_f32_dat_output_len,
    receive: fn_f32_recv,
    send: fn_f32_send,
    send_to: fn_f32_send_to,
    default: fn_f32_default,
};
