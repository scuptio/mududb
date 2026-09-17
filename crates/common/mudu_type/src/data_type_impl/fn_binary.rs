use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_convert::FnBase;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::bin_size::BinSize;
use mudu::utils::json::{JsonNumber, JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackInteger, MsgPackValue};

pub fn fn_binary_in(s: &str, data_type: &DataType) -> Result<DataValue, TyErr> {
    let json_value: JsonValue =
        from_json_str(s).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    let dat = fn_binary_in_json(&json_value, data_type)?;
    Ok(dat)
}

pub fn fn_binary_out(v: &DataValue, data_type: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_binary_out_json(v, data_type)?;
    Ok(DataTextual::from(json.to_string()))
}

pub fn fn_binary_in_json(json: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
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
            || {
                Err(TyErr::new(
                    TyEC::TypeConvertFailed,
                    "expected a number".to_string(),
                ))
            },
            Ok,
        )?;
        binary.push(n as u8);
    }
    Ok(DataValue::from_binary(binary))
}

pub fn fn_binary_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    let datum_binary: &Vec<u8> = v.expect_binary();
    let mut vec_json_value = Vec::with_capacity(datum_binary.len());
    for v in datum_binary.iter() {
        vec_json_value.push(JsonValue::Number(JsonNumber::from(*v)));
    }
    Ok(DataJson::from(JsonValue::Array(vec_json_value)))
}

pub fn fn_binary_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
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
            || {
                Err(TyErr::new(
                    TyEC::TypeConvertFailed,
                    "in msgpack, expected a number".to_string(),
                ))
            },
            Ok,
        )?;
        binary.push(n as u8);
    }
    Ok(DataValue::from_binary(binary))
}

pub fn fn_binary_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
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

pub fn fn_type_output_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(None)
}

fn header_size() -> usize {
    BinSize::size_of()
}

pub fn fn_dat_output_len(data_value: &DataValue, _: &DataType) -> Result<u32, TyErr> {
    let datum_binary = data_value.expect_binary();
    let mut size = header_size() as u32;
    size += datum_binary.len() as u32;
    Ok(size)
}

pub fn fn_binary_send(data_value: &DataValue, data_type: &DataType) -> Result<DataBinary, TyErr> {
    let len = fn_dat_output_len(data_value, data_type)?;
    let mut vec = vec![0; len as usize];
    let _ = fn_binary_send_to(data_value, data_type, &mut vec)?;
    Ok(DataBinary::from(vec))
}

pub fn fn_binary_send_to(object: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let datum_binary: &Vec<u8> = object.expect_binary();
    let hdr_size = header_size();
    let total_len = hdr_size + datum_binary.len();
    if buf.len() < total_len {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let offset = hdr_size as u32;
    buf[offset as usize..offset as usize + datum_binary.len()].copy_from_slice(datum_binary);
    let binary_bytes = BinSize::new(total_len as u32);
    binary_bytes.copy_to_slice(&mut buf[0..BinSize::size_of()]);
    Ok(total_len as u32)
}

pub fn fn_binary_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < header_size() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "space insufficient error".to_string(),
        ));
    }

    let binary_bytes = BinSize::from_slice(&buf[0..BinSize::size_of()]).size();
    if buf.len() < binary_bytes as usize || (binary_bytes as usize) < header_size() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "space insufficient error".to_string(),
        ));
    }

    let data_len = binary_bytes as usize - header_size();
    let mut binary = vec![0; data_len];
    binary.copy_from_slice(&buf[header_size()..binary_bytes as usize]);
    Ok((DataValue::from_binary(binary), binary_bytes))
}

pub fn fn_binary_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_binary(vec![]))
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
