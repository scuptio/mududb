use crate::common::result::RS;
use crate::common::serde_utils::Sizer;
use crate::error::ec::EC;
use crate::m_error;
use std::io::Cursor;

pub type MsgPackInteger = rmpv::Integer;
pub type MsgPackValue = rmpv::Value;
pub type MsgPackUtf8String = rmpv::Utf8String;

pub fn msg_pack_value_to_binary(value: &MsgPackValue) -> RS<Vec<u8>> {
    let mut sizer = Sizer::new();
    rmpv::encode::write_value(&mut sizer, value).unwrap();
    let mut vec = Vec::with_capacity(sizer.size());
    vec.resize(sizer.size(), 0u8);
    rmpv::encode::write_value(&mut vec, value).unwrap();
    Ok(vec)
}

pub fn msg_pack_binary_to_value(binary: &[u8]) -> RS<(MsgPackValue, u64)> {
    let mut cursor = Cursor::new(binary);
    let v = rmpv::decode::read_value(&mut cursor)
        .map_err(|e| m_error!(EC::DecodeErr, "cannot decode from msg pack binary", e))?;
    Ok((v, cursor.position()))
}
