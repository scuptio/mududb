use crate::data_json::DataJson;
use crate::data_type::DataType;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
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

pub fn temporal_json_output(text: String) -> Result<DataJson, TyErr> {
    Ok(DataJson::from(JsonValue::String(text)))
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

pub fn time_precision(dt: &DataType) -> u8 {
    dt.as_time_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn timestamp_precision(dt: &DataType) -> u8 {
    dt.as_timestamp_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn timestamptz_precision(dt: &DataType) -> u8 {
    dt.as_timestamptz_param()
        .map(|param| param.precision())
        .unwrap_or(6)
}

pub fn validate_time_param(param: &DataTypeParamTime) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub fn validate_timestamp_param(param: &DataTypeParamTimestamp) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}

pub fn validate_timestamptz_param(param: &DataTypeParamTimestampTz) -> Result<(), TyErr> {
    param
        .validate()
        .map_err(|message| TyErr::new(TyEC::ParamParseError, message))
}
