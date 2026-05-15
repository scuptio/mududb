use mududb::common::result::RS;
use mududb::error::ec::EC;
use mududb::m_error;

pub fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

pub fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        m_error!(
            EC::DecodeErr,
            format!("invalid utf8 in ycsb {label}"),
            e.to_string()
        )
    })
}
