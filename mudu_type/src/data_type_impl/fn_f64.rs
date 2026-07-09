use crate::data_type::DataType;
use crate::data_type_fn_convert::FnBase;
use mudu::common::endian::Endian;

use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::json_value;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::MsgPackValue;

pub fn fn_f64_in_textual(v: &str, _dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_f64_in_json(&DataJson::from(json), _dt)
}

pub fn fn_f64_out_textual(v: &DataValue, _dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_f64_out_json(v, _dt)?;
    Ok(DataTextual::from(json.to_string()))
}

pub fn fn_f64_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    let opt_num = v.as_number();
    let opt_f64 = match opt_num {
        Some(num) => num.as_f64(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to f64", v),
            ));
        }
    };
    match opt_f64 {
        Some(num) => Ok(DataValue::from_f64(num)),
        None => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to f64", v),
        )),
    }
}

pub fn fn_f64_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    let i = v.to_f64();
    let json = json_value!(i);
    Ok(DataJson::from(json))
}

pub fn fn_f64_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
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
    Ok(DataValue::from_f64(v))
}

pub fn fn_f64_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    let i = v.to_f64();
    Ok(MsgPackValue::F64(i))
}
pub fn fn_f64_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<f64>() as u32))
}

pub fn fn_f64_dat_output_len(_: &DataValue, _ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_f64_len(_ty)?.unwrap())
}

pub fn fn_f64_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let i = v.to_f64();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_f64(&mut buf, i);
    Ok(DataBinary::from(buf))
}

pub fn fn_f64_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let i = v.to_f64();
    let len = size_of_val(&i) as u32;
    if buf.len() < size_of_val(&i) {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_f64(buf, i);
    Ok(len)
}

pub fn fn_f64_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<f64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let i = Endian::read_f64(buf);
    Ok((DataValue::from_f64(i), size_of::<f64>() as u32))
}

pub fn fn_f64_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_f64(f64::default()))
}

pub const FN_F64_CONVERT: FnBase = FnBase {
    input_textual: fn_f64_in_textual,
    output_textual: fn_f64_out_textual,
    input_json: fn_f64_in_json,
    output_json: fn_f64_out_json,
    input_msg_pack: fn_f64_in_msgpack,
    output_msg_pack: fn_f64_out_msgpack,
    type_len: fn_f64_len,
    data_len: fn_f64_dat_output_len,
    receive: fn_f64_recv,
    send: fn_f64_send,
    send_to: fn_f64_send_to,
    default: fn_f64_default,
};
