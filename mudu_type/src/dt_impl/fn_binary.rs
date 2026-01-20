use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_convert::FnBase;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::bin_size::BinSize;
use mudu::utils::json::{from_json_str, JsonNumber, JsonValue};
use mudu::utils::msg_pack::{MsgPackInteger, MsgPackValue};

pub fn fn_binary_in(s: &str, dat_type: &DatType) -> Result<DatValue, TyErr> {
    let json_value: JsonValue =
        from_json_str(s).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    let dat = fn_binary_in_json(&json_value, dat_type)?;
    Ok(dat)
}

pub fn fn_binary_out(v: &DatValue, dat_type: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_binary_out_json(v, dat_type)?;
    Ok(DatTextual::from(json.to_string()))
}

pub fn fn_binary_in_json(json: &JsonValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_binary = json.as_array();
    let value_array = match opt_binary {
        Some(binary) => binary,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "expected a binary in JSON".to_string(),
            ));
        }
    };
    let mut binary = Vec::with_capacity(value_array.len());
    for v in value_array.iter() {
        let n = v.as_u64().map_or_else(
            ||{ Err(TyErr::new( TyEC::TypeConvertFailed, "expected a number".to_string())) },
            |e| { Ok(e) }
        )?;
        binary.push(n as u8);
    }
    Ok(DatValue::from_binary(binary))
}

pub fn fn_binary_out_json(v: &DatValue, _: &DatType) -> Result<DatJson, TyErr> {
    let datum_binary: &Vec<u8> = v.expect_binary();
    let mut vec_json_value = Vec::with_capacity(datum_binary.len());
    for v in datum_binary.iter() {
        vec_json_value.push(JsonValue::Number(JsonNumber::from(*v)));
    }
    Ok(DatJson::from(JsonValue::Array(vec_json_value)))
}

pub fn fn_binary_in_msgpack(msg_pack: &MsgPackValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_binary = msg_pack.as_array();
    let value_array = match opt_binary {
        Some(binary) => binary,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("expected a binary in msgpack, got {:?}", opt_binary),
            ));
        }
    };

    let mut binary = Vec::with_capacity(value_array.len());
    for v in value_array.iter() {
        let n = v.as_u64().map_or_else(
            ||{ Err(TyErr::new( TyEC::TypeConvertFailed, "in msgpack, expected a number".to_string())) },
            |e| { Ok(e) }
        )?;
        binary.push(n as u8);
    }
    Ok(DatValue::from_binary(binary))
}

pub fn fn_binary_out_msgpack(v: &DatValue, _: &DatType) -> Result<MsgPackValue, TyErr> {
    let opt_binary = v.as_binary();
    let binary = match opt_binary {
        Some(binary) => binary,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("expected a binary in value, got {:?}", opt_binary),
            ));
        }
    };
    let mut vec = Vec::with_capacity(binary.len());
    for v in binary.iter() {

        vec.push(MsgPackValue::Integer(MsgPackInteger::from(*v)));
    }
    Ok(MsgPackValue::Array(vec))
}

pub fn fn_type_output_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(None)
}

fn header_size() -> usize {
    BinSize::size_of()
}

pub fn fn_dat_output_len(dat_value: &DatValue, _: &DatType) -> Result<u32, TyErr> {
    let datum_binary = dat_value.expect_binary();
    let mut size = header_size() as u32;
    size += datum_binary.len() as u32;
    Ok(size)
}



pub fn fn_binary_send(dat_value: &DatValue, dat_type: &DatType) -> Result<DatBinary, TyErr> {
    let len = fn_dat_output_len(dat_value, dat_type)?;
    let mut vec = Vec::with_capacity(len as usize);
    unsafe {
        vec.set_len(len as usize);
    }
    let _ = fn_binary_send_to(dat_value, dat_type, &mut vec)?;
    Ok(DatBinary::from(vec))
}

pub fn fn_binary_send_to(
    object: &DatValue,
    _: &DatType,
    buf: &mut [u8],
) -> Result<u32, TyErr> {
    let datum_binary: &Vec<u8> = object.expect_binary();
    let hdr_size = header_size();
    let offset = hdr_size as u32;
    buf[offset as usize..].copy_from_slice(datum_binary);
    let binary_bytes = BinSize::new(offset);
    binary_bytes.copy_to_slice(&mut buf[0..BinSize::size_of()]);
    Ok(offset)
}

pub fn fn_binary_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    if buf.len() < header_size() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "space insufficient error".to_string(),
        ));
    }

    let binary_bytes = BinSize::from_slice(&buf[0..BinSize::size_of()]).size();
    if buf.len() < binary_bytes as usize {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "space insufficient error".to_string(),
        ));
    }

    let mut binary = Vec::with_capacity(binary_bytes as usize);
    binary.resize(binary_bytes as usize, 0);
    binary.copy_from_slice(&buf[header_size()..]);
    Ok((DatValue::from_binary(binary), binary_bytes))
}

pub fn fn_binary_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_binary(vec![]))
}



pub const FN_BINARY_CONVERT: FnBase = FnBase {
    input_textual: fn_binary_in,
    output_textual: fn_binary_out,
    input_json: fn_binary_in_json,
    output_json: fn_binary_out_json,
    input_msg_pack: fn_binary_in_msgpack,
    output_msg_pack: fn_binary_out_msgpack,
    type_len: fn_type_output_len,
    data_len: fn_dat_output_len,
    receive: fn_binary_recv,
    send: fn_binary_send,
    send_to: fn_binary_send_to,
    default: fn_binary_default,
};
