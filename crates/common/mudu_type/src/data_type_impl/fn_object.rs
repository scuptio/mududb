use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_convert::FnBase;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use std::collections::HashMap;

use crate::data_type_param_record::DataTypeParamRecord;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::bin_size::BinSize;
use mudu::utils::bin_slot::BinSlot;
use mudu::utils::json::{JsonMap, JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};

pub fn fn_object_in(s: &str, data_type: &DataType) -> Result<DataValue, TyErr> {
    let json_value: JsonValue =
        from_json_str(s).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    let dat = fn_object_in_json(&json_value, data_type)?;
    Ok(dat)
}

pub fn fn_object_out(v: &DataValue, data_type: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_object_out_json(v, data_type)?;
    Ok(DataTextual::from(json.to_string()))
}

pub fn fn_object_in_json(json: &JsonValue, ty: &DataType) -> Result<DataValue, TyErr> {
    let param = object_param(ty);
    let opt_object = json.as_object();
    let map = match opt_object {
        Some(map) => map,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "expected a object json".to_string(),
            ));
        }
    };
    let field_name_ty = param.fields();

    let mut object_fields = Vec::with_capacity(map.len());
    for (name, ty) in field_name_ty {
        let opt_v = map.get(name);
        let field_json = match opt_v {
            None => {
                return Err(TyErr::new(
                    TyEC::TypeConvertFailed,
                    format!("cannot find field name {}", name),
                ));
            }
            Some(v) => v,
        };
        let id = ty.type_family();
        let dat_val = id.fn_input_json()(field_json, ty)?;
        object_fields.push(dat_val);
    }
    Ok(DataValue::from_record(object_fields))
}

pub fn fn_object_out_json(v: &DataValue, dt: &DataType) -> Result<DataJson, TyErr> {
    let param = object_param(dt);
    let datum_object: &Vec<DataValue> = v.expect_record();
    if datum_object.len() != param.fields().len() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "output json, expected object fields size equal with its description {}",
                param.fields().len()
            ),
        ));
    }
    let mut json_map = JsonMap::with_capacity(datum_object.len());
    for (i, data_value) in datum_object.iter().enumerate() {
        let (name, ty) = &param.fields()[i];
        let id = ty.type_family();
        let field_json = id.fn_output_json()(data_value, ty)?;
        json_map.insert(name.clone(), field_json.into_json_value());
    }
    Ok(DataJson::from(JsonValue::Object(json_map)))
}

pub fn fn_object_in_msgpack(msg_pack: &MsgPackValue, ty: &DataType) -> Result<DataValue, TyErr> {
    let param = object_param(ty);
    let opt_object = msg_pack.as_map();
    let map = match opt_object {
        Some(map) => map,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "expected a map msg pack".to_string(),
            ));
        }
    };
    if map.len() != param.fields().len() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "input msg pack, expected object fields size equal with its description {}",
                param.fields().len()
            ),
        ));
    }
    let mut field_map = HashMap::with_capacity(map.len());
    for (k, v) in map.iter() {
        match k.as_str() {
            None => {
                return Err(TyErr::new(
                    TyEC::TypeConvertFailed,
                    "do not support non-string key".to_string(),
                ));
            }
            Some(name) => {
                field_map.insert(name, v);
            }
        }
    }
    let mut vec = Vec::with_capacity(param.fields().len());
    for (name, ty) in param.fields() {
        let opt_v = field_map.get(name.as_str());
        match opt_v {
            Some(v) => {
                let v = ty.type_family().fn_input_msg_pack()(v, ty)?;
                vec.push(v);
            }
            None => {
                return Err(TyErr::new(
                    TyEC::TypeConvertFailed,
                    format!("do not support non-string key {}", name),
                ));
            }
        }
    }
    Ok(DataValue::from_record(vec))
}

pub fn fn_object_out_msgpack(v: &DataValue, ty: &DataType) -> Result<MsgPackValue, TyErr> {
    let param = object_param(ty);
    let opt_object = v.as_record();
    let obj = match opt_object {
        Some(map) => map,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "expected a object value".to_string(),
            ));
        }
    };
    if obj.len() != param.fields().len() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "output msg pack, expected object fields size equal with its description {}",
                param.fields().len()
            ),
        ));
    }
    let mut vec = Vec::with_capacity(param.fields().len());
    for (i, value_field) in obj.iter().enumerate() {
        let (name, ty_field) = &param.fields()[i];
        let value_pack = ty_field.type_family().fn_output_msg_pack()(value_field, ty_field)?;
        let key = MsgPackValue::String(MsgPackUtf8String::from(name.to_string()));
        vec.push((key, value_pack));
    }
    Ok(MsgPackValue::Map(vec))
}

pub fn fn_object_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(None)
}

fn header_size(num_field: usize) -> usize {
    BinSize::size_of() + BinSlot::size_of() * num_field
}

pub fn fn_object_dat_output_len(
    data_value: &DataValue,
    data_type: &DataType,
) -> Result<u32, TyErr> {
    let param = object_param(data_type);
    let mut size = header_size(param.fields().len()) as u32;
    let datum_object: &Vec<DataValue> = data_value.expect_record();
    if datum_object.len() != param.fields().len() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "output length, expected object fields size equal with its description {}",
                param.fields().len()
            ),
        ));
    }
    for (i, (_, ty)) in param.fields().iter().enumerate() {
        let id = ty.type_family();
        let field_data_value = &datum_object[i];
        let n = id.fn_send_data_len()(field_data_value, ty)?;
        size += n;
    }
    Ok(size)
}

pub fn fn_object_send(value: &DataValue, data_type: &DataType) -> Result<DataBinary, TyErr> {
    let size = fn_object_dat_output_len(value, data_type)?;
    let mut vec = vec![0; size as usize];
    fn_object_send_to(value, data_type, &mut vec)?;
    Ok(DataBinary::from(vec))
}

pub fn fn_object_send_to(
    value: &DataValue,
    data_type: &DataType,
    buf: &mut [u8],
) -> Result<u32, TyErr> {
    let param = object_param(data_type);
    let datum_object: &Vec<DataValue> = value.expect_record();
    if datum_object.len() != param.fields().len() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "expected object fields size equal with its description {}",
                param.fields().len()
            ),
        ));
    }
    let hdr_size = header_size(param.fields().len());
    if buf.len() < hdr_size {
        let _len = fn_object_dat_output_len(value, data_type)?;
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let mut offset = hdr_size;
    for (i, field_value) in datum_object.iter().enumerate() {
        let (_, ty) = &param.fields()[i];
        let id = ty.type_family();
        let write_n = id.fn_send_to()(field_value, ty, &mut buf[offset..])?;
        let bin_slot = BinSlot::new(offset as u32, write_n);
        let slot_off = BinSize::size_of() + BinSlot::size_of() * i;
        bin_slot.copy_to_slice(&mut buf[slot_off..]);
        offset += write_n as usize;
    }
    // write the total length of the send binary data
    let bin_size = BinSize::new(offset as u32);
    bin_size.copy_to_slice(&mut buf[..BinSize::size_of()]);
    Ok(offset as u32)
}

pub fn fn_object_recv(binary: &[u8], data_type: &DataType) -> Result<(DataValue, u32), TyErr> {
    let param = object_param(data_type);
    let hdr_size = header_size(param.fields().len());
    let size = BinSize::from_slice(binary).size();
    if size as usize > binary.len() || hdr_size > binary.len() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let mut vec_fields = Vec::with_capacity(param.fields().len());
    for (i, (_, ty)) in param.fields().iter().enumerate() {
        let id = ty.type_family();
        let slot_off = BinSize::size_of() + BinSlot::size_of() * i;
        let slot = BinSlot::from_slice(&binary[slot_off..slot_off + BinSlot::size_of()]);
        let (data_value, _) = id.fn_recv()(
            &binary[slot.offset() as usize..(slot.offset() + slot.length()) as usize],
            ty,
        )?;
        vec_fields.push(data_value);
    }
    Ok((DataValue::from_record(vec_fields), size))
}

pub fn fn_object_default(ty: &DataType) -> Result<DataValue, TyErr> {
    if ty.type_family() != TypeFamily::Record {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "expected a object type".to_string(),
        ));
    }
    let mut fields = Vec::new();
    let param = object_param(ty);
    for (_field, field_ty) in param.fields() {
        let value = field_ty.type_family().fn_default()(field_ty)?;
        fields.push(value);
    }

    Ok(DataValue::from_record(fields))
}

fn object_param(data_type: &DataType) -> &DataTypeParamRecord {
    data_type.expect_record_param()
}

pub const FN_OBJECT_CONVERT: FnBase = FnBase {
    input_textual: fn_object_in,
    output_textual: fn_object_out,
    input_json: fn_object_in_json,
    output_json: fn_object_out_json,
    input_msg_pack: fn_object_in_msgpack,
    output_msg_pack: fn_object_out_msgpack,
    type_len: fn_object_len,
    data_len: fn_object_dat_output_len,
    receive: fn_object_recv,
    send: fn_object_send,
    send_to: fn_object_send_to,
    default: fn_object_default,
};

#[cfg(test)]
#[path = "fn_object_test.rs"]
mod fn_object_test;
