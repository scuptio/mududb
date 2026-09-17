//! Frame codecs for the `open-session` and `close-session` syscalls.
//!
//! Thin adapter over the `mudu_gen`-generated codec in
//! [`super::generated::uni_syscall`]: `open-session` request `[worker_id]` /
//! result `[0, UniOid]` or `[1, UniError]`; `close-session` request `[oid]` /
//! result `[0, 0]` or `[1, UniError]`.

use super::generated::uni_syscall as gen_codec;
use super::payload;
use crate::error::ApiError;
use crate::types::UniReturn;
use crate::universal::uni_oid::UniOid;

/// Serializes an `open-session` request into a complete MSSP frame: the
/// 16-byte header (kind `Open`) plus the MessagePack body `[worker_id]`.
pub fn serialize_open(worker_id: &UniOid) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_open_session_request(worker_id))
}

/// Deserializes an `open-session` result MSSP frame into the new session OID.
pub fn deserialize_open_result(bytes: &[u8]) -> Result<UniReturn<UniOid>, ApiError> {
    gen_codec::decode_open_session_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `close-session` request into a complete MSSP frame: the
/// 16-byte header (kind `Close`) plus the MessagePack body `[oid]`.
pub fn serialize_close(oid: &UniOid) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_close_session_request(oid))
}

/// Deserializes a `close-session` result MSSP frame.
pub fn deserialize_close_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    gen_codec::decode_close_session_result(bytes).map_err(payload::frame_error_to_api)
}

/// Decodes an `open-session` request MSSP frame into the worker OID.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_open_request(frame: &[u8]) -> Result<UniOid, ApiError> {
    gen_codec::decode_open_session_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `close-session` request MSSP frame into the session OID.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_close_request(frame: &[u8]) -> Result<UniOid, ApiError> {
    gen_codec::decode_close_session_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes an `open-session` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_open_response(result: &UniReturn<UniOid>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_open_session_result(result))
}

/// Encodes a `close-session` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_close_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_close_session_result(result))
}

/// Runs an `open-session` syscall and returns the new session OID.
pub async fn sys_open(worker_id: &UniOid) -> Result<UniReturn<UniOid>, ApiError> {
    let request = serialize_open(worker_id)?;
    let response = open_raw(request).await?;
    deserialize_open_result(&response)
}

/// Runs a `close-session` syscall.
pub async fn sys_close(oid: &UniOid) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_close(oid)?;
    let response = close_raw(request).await?;
    deserialize_close_result(&response)
}

#[allow(unused_variables)]
pub async fn open_raw(open_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::open_raw(open_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::open_raw(open_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn close_raw(close_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::close_raw(close_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::close_raw(close_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal::uni_error::UniError;

    fn worker_id() -> UniOid {
        UniOid { h: 1, l: 2 }
    }

    #[test]
    fn open_request_frame_matches_golden_bytes() {
        let frame = serialize_open(&worker_id()).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, // magic "MSSP"
            0x00, 0x00, 0x00, 0x01, // version 1
            0x00, 0x00, 0x00, 0x00, // flags
            0x00, 0x00, 0x00, 0x04, // kind Open
            0x81, 0x01, 0x82, 0x01, 0x01, 0x02, 0x02, // body {1: {1: h, 2: l}}
        ];
        assert_eq!(frame, expected);

        let decoded = decode_open_request(&frame).unwrap();
        assert_eq!(decoded.h, 1);
        assert_eq!(decoded.l, 2);
    }

    #[test]
    fn close_request_frame_matches_golden_bytes() {
        let frame = serialize_close(&UniOid { h: 0, l: 7 }).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x05, // kind Close
            0x81, 0x01, 0x82, 0x01, 0x00, 0x02, 0x07, // body {1: {1: 0, 2: 7}}
        ];
        assert_eq!(frame, expected);

        let decoded = decode_close_request(&frame).unwrap();
        assert_eq!(decoded.l, 7);
    }

    #[test]
    fn open_result_frame_roundtrips_ok_and_err() {
        let ok: UniReturn<UniOid> = Ok(UniOid { h: 0, l: 9 });
        let frame = encode_open_response(&ok).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x04, // kind Open
            0x92, 0x00, 0x82, 0x01, 0x00, 0x02, 0x09, // body [0, {1: 0, 2: 9}]
        ];
        assert_eq!(frame, expected);
        match deserialize_open_result(&frame).unwrap() {
            Ok(oid) => assert_eq!(oid.l, 9),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let err: UniReturn<UniOid> = Err(UniError {
            err_code: 2,
            err_msg: "boom".to_string(),
            ..Default::default()
        });
        let frame = encode_open_response(&err).unwrap();
        match deserialize_open_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => {
                assert_eq!(error.err_code, 2);
                assert_eq!(error.err_msg, "boom");
            }
        }
    }

    #[test]
    fn close_result_frame_roundtrips_unit_and_err() {
        let frame = encode_close_response(&Ok(())).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x05, // kind Close
            0x92, 0x00, 0x00, // body [0, 0]
        ];
        assert_eq!(frame, expected);
        assert!(deserialize_close_result(&frame).unwrap().is_ok());

        let frame = encode_close_response(&Err(UniError {
            err_code: 9,
            err_msg: "unknown session".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_close_result(&frame).unwrap() {
            Ok(()) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_code, 9),
        }

        // Trailing bytes after the body are rejected.
        let mut trailing = frame.clone();
        trailing.push(0x00);
        assert!(deserialize_close_result(&trailing).is_err());
    }
}
