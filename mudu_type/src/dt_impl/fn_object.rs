use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_convert::FnBase;
use std::collections::HashMap;

use crate::dtp_object::DTPRecord;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::bin_size::BinSize;
use mudu::utils::bin_slot::BinSlot;
use mudu::utils::json::{from_json_str, JsonMap, JsonValue};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};

pub fn fn_object_in(s: &str, dat_type: &DatType) -> Result<DatValue, TyErr> {
    let json_value: JsonValue =
        from_json_str(s).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    let dat = fn_object_in_json(&json_value, dat_type)?;
    Ok(dat)
}

pub fn fn_object_out(v: &DatValue, dat_type: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_object_out_json(v, dat_type)?;
    Ok(DatTextual::from(json.to_string()))
}

pub fn fn_object_in_json(json: &JsonValue, ty: &DatType) -> Result<DatValue, TyErr> {
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
        let id = ty.dat_type_id();
        let dat_val = id.fn_input_json()(field_json, ty)?;
        object_fields.push(dat_val);
    }
    Ok(DatValue::from_record(object_fields))
}

pub fn fn_object_out_json(v: &DatValue, dt: &DatType) -> Result<DatJson, TyErr> {
    let param = object_param(dt);
    let datum_object: &Vec<DatValue> = v.expect_record();
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
    for (i, dat_value) in datum_object.iter().enumerate() {
        let (name, ty) = &param.fields()[i];
        let id = ty.dat_type_id();
        let field_json = id.fn_output_json()(dat_value, ty)?;
        json_map.insert(name.clone(), field_json.into_json_value());
    }
    Ok(DatJson::from(JsonValue::Object(json_map)))
}

pub fn fn_object_in_msgpack(msg_pack: &MsgPackValue, ty: &DatType) -> Result<DatValue, TyErr> {
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
                let v = ty.dat_type_id().fn_input_msg_pack()(*v, ty)?;
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
    Ok(DatValue::from_record(vec))
}

pub fn fn_object_out_msgpack(v: &DatValue, ty: &DatType) -> Result<MsgPackValue, TyErr> {
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
        let value_pack = ty_field.dat_type_id().fn_output_msg_pack()(value_field, ty_field)?;
        let key = MsgPackValue::String(MsgPackUtf8String::from(name.to_string()));
        vec.push((key, value_pack));
    }
    Ok(MsgPackValue::Map(vec))
}

pub fn fn_object_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(None)
}

fn header_size(num_field: usize) -> usize {
    BinSize::size_of() + BinSlot::size_of() * num_field
}

pub fn fn_object_dat_output_len(dat_value: &DatValue, dat_type: &DatType) -> Result<u32, TyErr> {
    let param = object_param(dat_type);
    let mut size = header_size(param.fields().len()) as u32;
    let datum_object: &Vec<DatValue> = dat_value.expect_record();
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
        let id = ty.dat_type_id();
        let field_dat_value = &datum_object[i];
        let n = id.fn_send_dat_len()(field_dat_value, ty)?;
        size += n;
    }
    Ok(size)
}

pub fn fn_object_send(value: &DatValue, dat_type: &DatType) -> Result<DatBinary, TyErr> {
    let size = fn_object_dat_output_len(value, dat_type)?;
    let mut vec = Vec::with_capacity(size as usize);
    unsafe { vec.set_len(size as usize) };
    fn_object_send_to(value, dat_type, &mut vec)?;
    Ok(DatBinary::from(vec))
}

pub fn fn_object_send_to(
    value: &DatValue,
    dat_type: &DatType,
    buf: &mut [u8],
) -> Result<u32, TyErr> {
    let param = object_param(dat_type);
    let datum_object: &Vec<DatValue> = value.expect_record();
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
        let _len = fn_object_dat_output_len(value, dat_type)?;
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let mut offset = hdr_size;
    for (i, field_value) in datum_object.iter().enumerate() {
        let (_, ty) = &param.fields()[i];
        let id = ty.dat_type_id();
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

pub fn fn_object_recv(binary: &[u8], dat_type: &DatType) -> Result<(DatValue, u32), TyErr> {
    let param = object_param(dat_type);
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
        let id = ty.dat_type_id();
        let slot_off = BinSize::size_of() + BinSlot::size_of() * i;
        let slot = BinSlot::from_slice(&binary[slot_off..slot_off + BinSlot::size_of()]);
        let (dat_value, _) = id.fn_recv()(
            &binary[slot.offset() as usize..(slot.offset() + slot.length()) as usize],
            ty,
        )?;
        vec_fields.push(dat_value);
    }
    Ok((DatValue::from_record(vec_fields), size))
}

pub fn fn_object_default(ty: &DatType) -> Result<DatValue, TyErr> {
    if ty.dat_type_id() != DatTypeID::Record {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "expected a object type".to_string(),
        ));
    }
    let mut fields = Vec::new();
    let param = object_param(ty);
    for (_field, field_ty) in param.fields() {
        let value = field_ty.dat_type_id().fn_default()(field_ty)?;
        fields.push(value);
    }

    Ok(DatValue::from_record(fields))
}

fn object_param(dat_type: &DatType) -> &DTPRecord {
    dat_type.expect_record_param()
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
