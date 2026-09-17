//! Frame codecs for the KV syscall family: `get`, `put`, `delete`, `range`.
//!
//! Thin adapter over the `mudu_gen`-generated codec in
//! [`super::generated::uni_syscall`]: requests are positional MessagePack
//! arrays with byte strings encoded as MessagePack bin (`get` `[oid, key]`,
//! `put` `[oid, key, value]`, `delete` `[oid, key]`, `range`
//! `[oid, start, end]`); results are `[0, value]` / `[1, UniError]` with unit
//! results as `[0, 0]`.

use super::generated::uni_syscall as gen_codec;
use super::payload;
use crate::error::ApiError;
use crate::types::UniReturn;
use crate::universal::uni_oid::UniOid;

/// Key/value pair returned by a `range` scan.
pub type KvPair = (Vec<u8>, Vec<u8>);

/// Serializes a `get` request into a complete MSSP frame: body `[oid, key]`.
pub fn serialize_get(oid: &UniOid, key: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_get_request(oid, key))
}

/// Deserializes a `get` result MSSP frame into the optional value.
pub fn deserialize_get_result(bytes: &[u8]) -> Result<UniReturn<Option<Vec<u8>>>, ApiError> {
    gen_codec::decode_get_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `put` request into a complete MSSP frame: body
/// `[oid, key, value]`.
pub fn serialize_put(oid: &UniOid, key: &[u8], value: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_put_request(oid, key, value))
}

/// Deserializes a `put` result MSSP frame.
pub fn deserialize_put_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    gen_codec::decode_put_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `delete` request into a complete MSSP frame: body
/// `[oid, key]`.
pub fn serialize_delete(oid: &UniOid, key: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_delete_request(oid, key))
}

/// Deserializes a `delete` result MSSP frame.
pub fn deserialize_delete_result(bytes: &[u8]) -> Result<UniReturn<()>, ApiError> {
    gen_codec::decode_delete_result(bytes).map_err(payload::frame_error_to_api)
}

/// Serializes a `range` request into a complete MSSP frame: body
/// `[oid, start, end]`.
pub fn serialize_range(oid: &UniOid, start: &[u8], end: &[u8]) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_range_request(oid, start, end))
}

/// Deserializes a `range` result MSSP frame into the key/value pairs.
pub fn deserialize_range_result(bytes: &[u8]) -> Result<UniReturn<Vec<KvPair>>, ApiError> {
    gen_codec::decode_range_result(bytes).map_err(payload::frame_error_to_api)
}

/// Decodes a `get` request MSSP frame into `(oid, key)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_get_request(frame: &[u8]) -> Result<(UniOid, Vec<u8>), ApiError> {
    gen_codec::decode_get_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `put` request MSSP frame into `(oid, key, value)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_put_request(frame: &[u8]) -> Result<(UniOid, Vec<u8>, Vec<u8>), ApiError> {
    gen_codec::decode_put_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `delete` request MSSP frame into `(oid, key)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_delete_request(frame: &[u8]) -> Result<(UniOid, Vec<u8>), ApiError> {
    gen_codec::decode_delete_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `range` request MSSP frame into `(oid, start, end)`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_range_request(frame: &[u8]) -> Result<(UniOid, Vec<u8>, Vec<u8>), ApiError> {
    gen_codec::decode_range_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes a `get` result (or error) into a complete MSSP frame: the value
/// arm is a bin or nil.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_get_response(
    result: &UniReturn<Option<Vec<u8>>>,
) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_get_result(result))
}

/// Encodes a `put` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_put_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_put_result(result))
}

/// Encodes a `delete` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_delete_response(result: &UniReturn<()>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_delete_result(result))
}

/// Encodes a `range` result (or error) into a complete MSSP frame: the value
/// arm is an array of `[key, value]` bin pairs.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_range_response(result: &UniReturn<Vec<KvPair>>) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_range_result(result))
}

/// Runs a `get` syscall and returns the optional value.
pub async fn sys_get(oid: &UniOid, key: &[u8]) -> Result<UniReturn<Option<Vec<u8>>>, ApiError> {
    let request = serialize_get(oid, key)?;
    let response = get_raw(request).await?;
    deserialize_get_result(&response)
}

/// Runs a `put` syscall.
pub async fn sys_put(oid: &UniOid, key: &[u8], value: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_put(oid, key, value)?;
    let response = put_raw(request).await?;
    deserialize_put_result(&response)
}

/// Runs a `delete` syscall.
pub async fn sys_delete(oid: &UniOid, key: &[u8]) -> Result<UniReturn<()>, ApiError> {
    let request = serialize_delete(oid, key)?;
    let response = delete_raw(request).await?;
    deserialize_delete_result(&response)
}

/// Runs a `range` syscall and returns the key/value pairs in `[start, end)`.
pub async fn sys_range(
    oid: &UniOid,
    start: &[u8],
    end: &[u8],
) -> Result<UniReturn<Vec<KvPair>>, ApiError> {
    let request = serialize_range(oid, start, end)?;
    let response = range_raw(request).await?;
    deserialize_range_result(&response)
}

#[allow(unused_variables)]
pub async fn get_raw(get_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::get_raw(get_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::get_raw(get_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn put_raw(put_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::put_raw(put_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::put_raw(put_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn delete_raw(delete_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::delete_raw(delete_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::delete_raw(delete_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn range_raw(range_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::range_raw(range_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::range_raw(range_in).await;
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

    fn oid() -> UniOid {
        UniOid { h: 0, l: 7 }
    }

    #[test]
    fn get_request_frame_matches_golden_bytes() {
        let frame = serialize_get(&oid(), b"k1").unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x06, // kind Get
            0x82, 0x01, 0x82, 0x01, 0x00, 0x02, 0x07, 0x02, 0xC4, 0x02, 0x6B,
            0x31, // body {1: {1: 0, 2: 7}, 2: bin "k1"}
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, key) = decode_get_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(key, b"k1");
    }

    #[test]
    fn put_request_frame_matches_golden_bytes() {
        let frame = serialize_put(&oid(), b"k1", b"v1").unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x07, // kind Put
            0x83, 0x01, 0x82, 0x01, 0x00, 0x02, 0x07, 0x02, 0xC4, 0x02, 0x6B, 0x31, 0x03, 0xC4,
            0x02, 0x76, 0x31, // body {1: oid, 2: bin "k1", 3: bin "v1"}
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, key, value) = decode_put_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(key, b"k1");
        assert_eq!(value, b"v1");
    }

    #[test]
    fn delete_request_frame_matches_golden_bytes() {
        let frame = serialize_delete(&oid(), b"k1").unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x08, // kind Delete
            0x82, 0x01, 0x82, 0x01, 0x00, 0x02, 0x07, 0x02, 0xC4, 0x02, 0x6B, 0x31,
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, key) = decode_delete_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(key, b"k1");
    }

    #[test]
    fn range_request_frame_matches_golden_bytes() {
        let frame = serialize_range(&oid(), b"a", b"z").unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x09, // kind Range
            0x83, 0x01, 0x82, 0x01, 0x00, 0x02, 0x07, 0x02, 0xC4, 0x01, 0x61, 0x03, 0xC4, 0x01,
            0x7A, // body {1: oid, 2: bin "a", 3: bin "z"}
        ];
        assert_eq!(frame, expected);

        let (decoded_oid, start, end) = decode_range_request(&frame).unwrap();
        assert_eq!(decoded_oid.l, 7);
        assert_eq!(start, b"a");
        assert_eq!(end, b"z");
    }

    #[test]
    fn get_result_frame_roundtrips_some_none_and_err() {
        let some: UniReturn<Option<Vec<u8>>> = Ok(Some(b"v1".to_vec()));
        let frame = encode_get_response(&some).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x06, // kind Get
            0x92, 0x00, 0xC4, 0x02, 0x76, 0x31, // body [0, bin "v1"]
        ];
        assert_eq!(frame, expected);
        match deserialize_get_result(&frame).unwrap() {
            Ok(value) => assert_eq!(value, Some(b"v1".to_vec())),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let none: UniReturn<Option<Vec<u8>>> = Ok(None);
        let frame = encode_get_response(&none).unwrap();
        assert_eq!(&frame[16..], &[0x92, 0x00, 0xC0]);
        match deserialize_get_result(&frame).unwrap() {
            Ok(value) => assert_eq!(value, None),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let err: UniReturn<Option<Vec<u8>>> = Err(UniError {
            err_code: 2,
            err_msg: "missing".to_string(),
            ..Default::default()
        });
        let frame = encode_get_response(&err).unwrap();
        match deserialize_get_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_code, 2),
        }
    }

    #[test]
    fn unit_result_frames_roundtrip_for_put_and_delete() {
        for (encode, deserialize, kind) in [
            (
                encode_put_response as fn(&UniReturn<()>) -> Result<Vec<u8>, ApiError>,
                deserialize_put_result as fn(&[u8]) -> Result<UniReturn<()>, ApiError>,
                payload::KIND_PUT,
            ),
            (
                encode_delete_response,
                deserialize_delete_result,
                payload::KIND_DELETE,
            ),
        ] {
            let frame = encode(&Ok(())).unwrap();
            assert_eq!(u32::from_be_bytes(frame[12..16].try_into().unwrap()), kind);
            assert_eq!(&frame[16..], &[0x92, 0x00, 0x00]);
            assert!(deserialize(&frame).unwrap().is_ok());

            let frame = encode(&Err(UniError {
                err_code: 1,
                err_msg: "nope".to_string(),
                ..Default::default()
            }))
            .unwrap();
            match deserialize(&frame).unwrap() {
                Ok(()) => panic!("expected error variant"),
                Err(error) => assert_eq!(error.err_msg, "nope"),
            }
        }
    }

    #[test]
    fn range_result_frame_roundtrips_pairs_and_err() {
        let ok: UniReturn<Vec<KvPair>> = Ok(vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec()),
        ]);
        let frame = encode_range_response(&ok).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x09, // kind Range
            0x92, 0x00, 0x92, // [0, 2 pairs]
            0x92, 0xC4, 0x01, 0x61, 0xC4, 0x01, 0x31, // [bin "a", bin "1"]
            0x92, 0xC4, 0x01, 0x62, 0xC4, 0x01, 0x32, // [bin "b", bin "2"]
        ];
        assert_eq!(frame, expected);
        match deserialize_range_result(&frame).unwrap() {
            Ok(items) => assert_eq!(
                items,
                vec![
                    (b"a".to_vec(), b"1".to_vec()),
                    (b"b".to_vec(), b"2".to_vec())
                ]
            ),
            Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let frame = encode_range_response(&Err(UniError {
            err_code: 1,
            err_msg: "bad range".to_string(),
            ..Default::default()
        }))
        .unwrap();
        match deserialize_range_result(&frame).unwrap() {
            Ok(_) => panic!("expected error variant"),
            Err(error) => assert_eq!(error.err_msg, "bad range"),
        }

        // A `get` frame is rejected by the `range` decoder.
        let get_frame = serialize_get(&oid(), b"k1").unwrap();
        assert!(deserialize_range_result(&get_frame).is_err());
    }
}
