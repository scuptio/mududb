//! Tests for the protocol frame format dispatcher.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(test)]
mod tests {
    use super::super::{
        HEADER_LEN, PROTOCOL_FRAME_CURRENT_VERSION, decode, decode_header_bytes,
        ensure_latest_protocol_frame,
    };
    use crate::protocol::{Frame, MessageType};
    use mudu::compat::FormatKind;
    use mudu_compat_migrate::{CompatibilityRouter, global};
    use std::borrow::Cow;

    #[test]
    fn decode_header_bytes_roundtrip() {
        let frame = Frame::new(MessageType::Query, 7, b"payload".to_vec());
        let encoded = frame.encode();
        let header = decode_header_bytes(&encoded[..HEADER_LEN]).unwrap();
        assert_eq!(header.magic(), frame.header().magic());
        assert_eq!(header.version(), frame.header().version());
        assert_eq!(header.message_type(), MessageType::Query);
        assert_eq!(header.request_id(), 7);
        assert_eq!(header.payload_len(), 7);
    }

    #[test]
    fn decode_rejects_incomplete_header() {
        let err = decode(&[]).unwrap_err();
        assert!(err.to_string().contains("frame header is incomplete"));
    }

    #[test]
    fn ensure_latest_protocol_frame_borrows_current_and_upgrades_legacy() {
        let mut router = CompatibilityRouter::new();
        router.set_supported_window(FormatKind::ProtocolFrame, 1, 2);
        let _ = global::install(router);

        let buf = vec![0u8; HEADER_LEN];
        let current = ensure_latest_protocol_frame(&buf, PROTOCOL_FRAME_CURRENT_VERSION).unwrap();
        assert!(matches!(current, Cow::Borrowed(_)));

        let legacy = ensure_latest_protocol_frame(&buf, 2).unwrap();
        assert!(matches!(legacy, Cow::Owned(_)));
        assert_eq!(legacy.into_owned(), buf);
    }

    #[test]
    fn decode_header_bytes_rejects_bad_magic() {
        // Covers the error path of `compat::check_magic_and_version` inside the
        // format decode entry point.
        let err = decode_header_bytes(&[0u8; HEADER_LEN]).unwrap_err();
        assert!(err.to_string().contains("invalid protocol frame magic"));
    }
}
