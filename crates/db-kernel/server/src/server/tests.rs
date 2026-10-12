#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use crate::frame_dispatch::try_decode_next_frame;
use mudu_contract::protocol::encode_get_request;
use mudu_contract::protocol::GetRequest;
use mudu_contract::protocol::HEADER_LEN;

#[test]
fn try_decode_next_frame_waits_for_full_payload() {
    let encoded = encode_get_request(1, &GetRequest::new(1, b"k".to_vec())).unwrap();
    assert!(try_decode_next_frame(&encoded[..HEADER_LEN - 1])
        .unwrap()
        .is_none());
    assert!(try_decode_next_frame(&encoded[..HEADER_LEN])
        .unwrap()
        .is_none());
    let decoded = try_decode_next_frame(&encoded).unwrap().unwrap();
    assert_eq!(decoded.0.header().request_id(), 1);
    assert_eq!(decoded.1, encoded.len());
}
