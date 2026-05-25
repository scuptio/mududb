use crate::dat_json::DatJson;
use crate::dat_type::DatType;
use crate::dtp_time::DTPTime;
use crate::dtp_timestamp::DTPTimestamp;
use crate::dtp_timestamptz::DTPTimestampTz;
use crate::type_error::{TyEC, TyErr};
use mudu::utils::json::JsonValue;

pub fn parse_temporal_json_string(value: &JsonValue, name: &str) -> Result<String, TyErr> {
    value.as_str().map(ToOwned::to_owned).ok_or_else(|| {
        TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to {}", value, name),
        )
    })
}

pub fn temporal_json_output(text: String) -> Result<DatJson, TyErr> {
    Ok(DatJson::from(JsonValue::String(text)))
}

pub fn encode_sortable_i32(value: i32) -> u32 {
    (value as u32) ^ (1u32 << 31)
}

pub fn decode_sortable_i32(value: u32) -> i32 {
    (value ^ (1u32 << 31)) as i32
}

pub fn encode_sortable_i64(value: i64) -> u64 {
    (value as u64) ^ (1u64 << 63)
}

pub fn decode_sortable_i64(value: u64) -> i64 {
    (value ^ (1u64 << 63)) as i64
}

pub fn time_precision(dt: &DatType) -> u8 {
    dt.as_time_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn timestamp_precision(dt: &DatType) -> u8 {
    dt.as_timestamp_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn timestamptz_precision(dt: &DatType) -> u8 {
    dt.as_timestamptz_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn validate_time_param(param: &DTPTime) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub fn validate_timestamp_param(param: &DTPTimestamp) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub fn validate_timestamptz_param(param: &DTPTimestampTz) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}
