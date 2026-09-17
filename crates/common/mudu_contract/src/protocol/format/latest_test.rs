//! Tests for the latest protocol frame wire format.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

#[cfg(test)]
mod tests {
    use super::super::latest;
    use crate::protocol::{Frame, MessageType};
    use mudu_sys_contract::perf::TraceContext;

    // Re-export constants so the tests read like `latest::HEADER_LEN`.
    // (latest is already in scope via the `use` above.)

    #[test]
    fn decode_rejects_incomplete_header() {
        let err = latest::decode(&[0u8; 5]).unwrap_err();
        assert!(err.to_string().contains("frame header is incomplete"));

        let err = latest::decode_header_bytes(&[0u8; 5]).unwrap_err();
        assert!(err.to_string().contains("frame header is incomplete"));
    }

    #[test]
    fn decode_rejects_unknown_flag_bits() {
        let frame = Frame::new_with_trace(
            MessageType::Response,
            1,
            TraceContext::new(7),
            b"hi".to_vec(),
        );
        let mut buf = latest::encode(&frame);
        // Set an unknown flag bit in the low byte of the flags field (bytes 12..20).
        buf[19] |= 0x04;

        let err = latest::decode(&buf).unwrap_err();
        assert!(err.to_string().contains("unknown flag bits"));

        let err_header = latest::decode_header_bytes(&buf[..latest::HEADER_LEN]).unwrap_err();
        assert!(err_header.to_string().contains("unknown flag bits"));
    }
}
