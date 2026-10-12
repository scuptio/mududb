use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;

pub fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

pub fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("invalid utf8 in ycsb {label}"),
            e.to_string()
        )
    })
}
