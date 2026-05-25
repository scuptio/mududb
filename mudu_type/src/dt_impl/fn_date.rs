use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_compare::{ErrCompare, FnCompare};
use crate::dt_fn_convert::FnBase;
use crate::dt_impl::temporal::{
    decode_sortable_i32, encode_sortable_i32, parse_temporal_json_string, temporal_json_output,
};
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::date::DateValue;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_date_str(value: &str) -> Result<DateValue, TyErr> {
    DateValue::parse(value)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, format!("invalid date {}", e)))
}

fn fn_date_in_textual(v: &str, dt: &DatType) -> Result<DatValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_date_in_json(&json, dt)
}

fn fn_date_out_textual(v: &DatValue, dt: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_date_out_json(v, dt)?;
    Ok(DatTextual::from(json.to_string()))
}

fn fn_date_in_json(v: &JsonValue, _: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_date(parse_date_str(
        parse_temporal_json_string(v, "date")?.as_str(),
    )?))
}

fn fn_date_out_json(v: &DatValue, _: &DatType) -> Result<DatJson, TyErr> {
    temporal_json_output(v.expect_date().format())
}

fn fn_date_in_msgpack(msg_pack: &MsgPackValue, _: &DatType) -> Result<DatValue, TyErr> {
    let Some(s) = msg_pack.as_str() else {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "cannot convert msg pack to date".to_string(),
        ));
    };
    Ok(DatValue::from_date(parse_date_str(s)?))
}

fn fn_date_out_msgpack(v: &DatValue, _: &DatType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.expect_date().format(),
    )))
}

fn fn_date_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i32>() as u32))
}

fn fn_date_dat_output_len(_: &DatValue, _: &DatType) -> Result<u32, TyErr> {
    Ok(size_of::<i32>() as u32)
}

fn fn_date_send(v: &DatValue, _: &DatType) -> Result<DatBinary, TyErr> {
    let mut buf = vec![0u8; size_of::<i32>()];
    Endian::write_u32(
        &mut buf,
        encode_sortable_i32(v.expect_date().days_since_epoch()),
    );
    Ok(DatBinary::from(buf))
}

fn fn_date_send_to(v: &DatValue, _: &DatType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_u32(buf, encode_sortable_i32(v.expect_date().days_since_epoch()));
    Ok(size_of::<i32>() as u32)
}

fn fn_date_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    if buf.len() < size_of::<i32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let days = decode_sortable_i32(Endian::read_u32(buf));
    Ok((
        DatValue::from_date(DateValue::from_days_since_epoch(days)),
        size_of::<i32>() as u32,
    ))
}

fn fn_date_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_date(DateValue::from_days_since_epoch(0)))
}

fn fn_date_order(v1: &DatValue, v2: &DatValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_date().cmp(v2.expect_date()))
}

fn fn_date_equal(v1: &DatValue, v2: &DatValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_date() == v2.expect_date())
}

fn fn_date_hash(v: &DatValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i32(v.expect_date().days_since_epoch());
    Ok(())
}

pub const FN_DATE_COMPARE: FnCompare = FnCompare {
    order: fn_date_order,
    equal: fn_date_equal,
    hash: fn_date_hash,
};

pub const FN_DATE_CONVERT: FnBase = FnBase {
    input_textual: fn_date_in_textual,
    output_textual: fn_date_out_textual,
    input_json: fn_date_in_json,
    output_json: fn_date_out_json,
    input_msg_pack: fn_date_in_msgpack,
    output_msg_pack: fn_date_out_msgpack,
    type_len: fn_date_len,
    data_len: fn_date_dat_output_len,
    receive: fn_date_recv,
    send: fn_date_send,
    send_to: fn_date_send_to,
    default: fn_date_default,
};

#[cfg(test)]
mod tests {
    use super::{fn_date_recv, fn_date_send};
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dat_value::DatValue;
    use mudu::data_type::date::DateValue;

    #[test]
    fn date_binary_roundtrip_preserves_value() {
        let ty = DatType::default_for(DatTypeID::Date);
        let value = DatValue::from_date(DateValue::parse("2026-05-20").unwrap());
        let binary = fn_date_send(&value, &ty).unwrap();
        let (decoded, used) = fn_date_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(used as usize, size_of::<i32>());
        assert_eq!(decoded.expect_date().format(), "2026-05-20");
    }
}
