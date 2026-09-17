//! Frame codecs for the `batch` syscall.
//!
//! Thin adapter over the `mudu_gen`-generated codec in
//! [`super::generated::uni_syscall`]: the request body is `[argv]` with a
//! `UniCommandArgv` record, the result body is `[0, UniCommandResult]` or
//! `[1, UniError]` — the same shapes as `command`, carried under the `Batch`
//! message kind.

use super::generated::uni_syscall as gen_codec;
use super::payload;
use crate::error::ApiError;
use crate::types::UniCommandReturn;
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_command_return::UniCommandResult;

/// Serializes a `batch` request into a complete MSSP frame: the 16-byte
/// header (kind `Batch`) plus the MessagePack body `[argv]`.
pub fn serialize_batch(argv: &UniCommandArgv) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_batch_request(argv))
}

/// Deserializes a `batch` result MSSP frame: validates the header, then
/// decodes the `[ok_tag, value]` body into a [`UniCommandReturn`].
pub fn deserialize_batch_result(bytes: &[u8]) -> Result<UniCommandReturn, ApiError> {
    let result = gen_codec::decode_batch_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(super::command_return_from_uni(result))
}

/// Decodes a `batch` request MSSP frame into its argument record.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_batch_request(frame: &[u8]) -> Result<UniCommandArgv, ApiError> {
    gen_codec::decode_batch_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes a `batch` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_batch_response(result: &UniCommandReturn) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_batch_result(
        &super::command_return_to_uni(result),
    ))
}

/// Runs a `batch` syscall.
pub async fn sys_batch(argv: &UniCommandArgv) -> Result<UniCommandReturn, ApiError> {
    let request = serialize_batch(argv)?;
    let response = batch_raw(request).await?;
    deserialize_batch_result(&response)
}

/// Runs a `batch` syscall and flattens the result to the affected row count,
/// mirroring [`super::sys_command_affected_rows`].
pub async fn sys_batch_affected_rows(argv: &UniCommandArgv) -> Result<u64, ApiError> {
    match sys_batch(argv).await? {
        UniCommandReturn::Ok(UniCommandResult { affected_rows }) => Ok(affected_rows),
        UniCommandReturn::Err(error) => Err(ApiError::Decode(error.err_msg)),
    }
}

#[allow(unused_variables)]
pub async fn batch_raw(batch_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::batch_raw(batch_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return super::wasm_async::batch_raw(batch_in).await;
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
    use crate::universal::uni_oid::UniOid;
    use crate::universal::uni_sql_param::UniSqlParam;
    use crate::universal::uni_sql_stmt::UniSqlStmt;

    fn batch_argv() -> UniCommandArgv {
        UniCommandArgv {
            oid: UniOid { h: 0, l: 7 },
            command: UniSqlStmt {
                sql_string: "insert into t values (1)".to_string(),
            },
            param_list: UniSqlParam { params: Vec::new() },
        }
    }

    #[test]
    fn batch_request_frame_matches_golden_bytes() {
        let frame = serialize_batch(&batch_argv()).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, // magic "MSSP"
            0x00, 0x00, 0x00, 0x01, // version 1
            0x00, 0x00, 0x00, 0x00, // flags
            0x00, 0x00, 0x00, 0x03, // kind Batch
            0x81, 0x01, // {1: argv}
            0x83, // argv: a 3-field record map
            0x01, 0x82, 0x01, 0x00, 0x02, 0x07, // oid {1: 0, 2: 7}
            0x02, 0x81, 0x01, 0xB8, // command {1: fixstr len 24}
            0x69, 0x6E, 0x73, 0x65, 0x72, 0x74, 0x20, 0x69, 0x6E, 0x74, 0x6F, 0x20, 0x74, 0x20,
            0x76, 0x61, 0x6C, 0x75, 0x65, 0x73, 0x20, 0x28, 0x31,
            0x29, // "insert into t values (1)"
            0x03, 0x81, 0x01, 0x90, // param_list {1: []}
        ];
        assert_eq!(frame, expected);

        let decoded = decode_batch_request(&frame).unwrap();
        assert_eq!(decoded.oid.l, 7);
        assert_eq!(decoded.command.sql_string, "insert into t values (1)");
    }

    #[test]
    fn batch_result_frame_roundtrips_ok_and_err() {
        let ok = UniCommandReturn::from_ok(UniCommandResult { affected_rows: 3 });
        let frame = encode_batch_response(&ok).unwrap();
        let expected: &[u8] = &[
            0x4D, 0x53, 0x53, 0x50, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            0x00, 0x03, // kind Batch
            0x92, 0x00, 0x81, 0x01, 0x03, // body [0, {1: 3}]
        ];
        assert_eq!(frame, expected);
        match deserialize_batch_result(&frame).unwrap() {
            UniCommandReturn::Ok(result) => assert_eq!(result.affected_rows, 3),
            UniCommandReturn::Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let err: UniCommandReturn = UniCommandReturn::from_err(UniError {
            err_code: 2,
            err_msg: "boom".to_string(),
            ..Default::default()
        });
        let frame = encode_batch_response(&err).unwrap();
        match deserialize_batch_result(&frame).unwrap() {
            UniCommandReturn::Ok(_) => panic!("expected error variant"),
            UniCommandReturn::Err(error) => assert_eq!(error.err_msg, "boom"),
        }

        // A `command` frame is rejected by the `batch` decoder.
        let command_frame = super::super::serialize_command(&batch_argv()).unwrap();
        assert!(deserialize_batch_result(&command_frame).is_err());
    }
}
