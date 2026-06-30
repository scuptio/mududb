//! MessagePack serialization helpers.

use crate::common::result::RS;
use crate::common::serde_utils::Sizer;
use crate::error::ErrorCode;
use crate::mudu_error;
use std::io::Cursor;

/// MessagePack integer type alias.
pub type MsgPackInteger = rmpv::Integer;

/// MessagePack value type alias.
pub type MsgPackValue = rmpv::Value;

/// MessagePack UTF-8 string type alias.
pub type MsgPackUtf8String = rmpv::Utf8String;

/// Encodes a MessagePack value into its binary representation.
pub fn msg_pack_value_to_binary(value: &MsgPackValue) -> RS<Vec<u8>> {
    let mut vec = Vec::with_capacity({
        let mut sizer = Sizer::new();
        rmpv::encode::write_value(&mut sizer, value).unwrap();
        sizer.size()
    });
    rmpv::encode::write_value(&mut vec, value).unwrap();
    Ok(vec)
}

/// Decodes a MessagePack value from `binary` and returns the value plus the
/// number of bytes consumed.
pub fn msg_pack_binary_to_value(binary: &[u8]) -> RS<(MsgPackValue, u64)> {
    let mut cursor = Cursor::new(binary);
    let v = rmpv::decode::read_value(&mut cursor)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "cannot decode from msg pack binary", e))?;
    Ok((v, cursor.position()))
}

#[cfg(test)]
mod tests {
    use super::{MsgPackValue, msg_pack_binary_to_value, msg_pack_value_to_binary};

    #[test]
    fn msg_pack_roundtrip_preserves_value_and_position() {
        let value = MsgPackValue::Array(vec![MsgPackValue::from(7), MsgPackValue::from("neo")]);

        let binary = msg_pack_value_to_binary(&value).unwrap();
        let (decoded, used) = msg_pack_binary_to_value(&binary).unwrap();

        assert_eq!(decoded, value);
        assert_eq!(used as usize, binary.len());
    }

    #[test]
    fn msg_pack_decode_invalid_binary_returns_error() {
        assert!(msg_pack_binary_to_value(&[0x92, 0x01]).is_err());
    }
}
