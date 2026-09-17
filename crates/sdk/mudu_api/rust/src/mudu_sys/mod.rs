use crate::error::ApiError;
use crate::types::{UniCommandReturn, UniQueryReturn};
use crate::{UniCommandArgv, UniCommandResult, UniQueryArgv, UniQueryResult};

pub(crate) mod generated;
pub(crate) mod payload;

pub mod batch;
pub mod fs;
pub mod kv;
pub mod relation;
pub mod session;

use generated::uni_syscall as gen_codec;

#[cfg(all(target_arch = "wasm32", feature = "wasm-async"))]
mod wasm_async;

#[allow(unused_variables)]
pub async fn query_raw(query_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::query_raw(query_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return wasm_async::query_raw(query_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn command_raw(command_in: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::command_raw(command_in).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return wasm_async::command_raw(command_in).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

#[allow(unused_variables)]
pub async fn fetch_raw(query_result: Vec<u8>) -> Result<Vec<u8>, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return crate::mock::MockSqliteMuduSysCall::fetch_raw(query_result).await;
    }

    #[cfg(all(
        target_arch = "wasm32",
        feature = "wasm-async",
        not(feature = "mock-sqlite")
    ))]
    {
        return wasm_async::fetch_raw(query_result).await;
    }

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable(
        "no mudu backend configured; enable `mock-sqlite` or build wasm32 with `wasm-async`",
    ))
}

/// Converts a [`UniCommandReturn`] into the generated codec's
/// `UniResult<UniCommandResult>` (`Result<UniCommandResult, UniError>`).
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn command_return_to_uni(
    result: &UniCommandReturn,
) -> gen_codec::UniResult<UniCommandResult> {
    match result {
        UniCommandReturn::Ok(value) => Ok(value.clone()),
        UniCommandReturn::Err(error) => Err(error.clone()),
    }
}

/// Converts the generated codec's `UniResult<UniCommandResult>` into a
/// [`UniCommandReturn`].
pub(crate) fn command_return_from_uni(
    result: gen_codec::UniResult<UniCommandResult>,
) -> UniCommandReturn {
    match result {
        Ok(value) => UniCommandReturn::Ok(value),
        Err(error) => UniCommandReturn::Err(error),
    }
}

/// Converts a [`UniQueryReturn`] into the generated codec's
/// `UniResult<UniQueryResult>`.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn query_return_to_uni(result: &UniQueryReturn) -> gen_codec::UniResult<UniQueryResult> {
    match result {
        UniQueryReturn::Ok(value) => Ok(value.clone()),
        UniQueryReturn::Err(error) => Err(error.clone()),
    }
}

/// Converts the generated codec's `UniResult<UniQueryResult>` into a
/// [`UniQueryReturn`].
pub(crate) fn query_return_from_uni(
    result: gen_codec::UniResult<UniQueryResult>,
) -> UniQueryReturn {
    match result {
        Ok(value) => UniQueryReturn::Ok(value),
        Err(error) => UniQueryReturn::Err(error),
    }
}

/// Serializes a `command` request into a complete MSSP frame: the 16-byte
/// header (kind `Command`) plus the MessagePack body `[argv]`.
pub fn serialize_command(argv: &UniCommandArgv) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_command_request(argv))
}

/// Serializes a `query` request into a complete MSSP frame: the 16-byte
/// header (kind `Query`) plus the MessagePack body `[argv]`.
pub fn serialize_query(argv: &UniQueryArgv) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_query_request(argv))
}

/// Deserializes a `command` result MSSP frame: validates the header, then
/// decodes the `[ok_tag, value]` body into a [`UniCommandReturn`].
pub fn deserialize_command_result(bytes: &[u8]) -> Result<UniCommandReturn, ApiError> {
    let result = gen_codec::decode_command_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(command_return_from_uni(result))
}

/// Deserializes a `query` result MSSP frame: validates the header, then
/// decodes the `[ok_tag, value]` body into a [`UniQueryReturn`].
pub fn deserialize_query_result(bytes: &[u8]) -> Result<UniQueryReturn, ApiError> {
    let result = gen_codec::decode_query_result(bytes).map_err(payload::frame_error_to_api)?;
    Ok(query_return_from_uni(result))
}

/// Decodes a `query` request MSSP frame into its argument record.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_query_request(frame: &[u8]) -> Result<UniQueryArgv, ApiError> {
    gen_codec::decode_query_request(frame).map_err(payload::frame_error_to_api)
}

/// Decodes a `command` request MSSP frame into its argument record.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_command_request(frame: &[u8]) -> Result<UniCommandArgv, ApiError> {
    gen_codec::decode_command_request(frame).map_err(payload::frame_error_to_api)
}

/// Encodes a `query` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_query_response(result: &UniQueryReturn) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_query_result(&query_return_to_uni(result)))
}

/// Encodes a `command` result (or error) into a complete MSSP frame.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn encode_command_response(result: &UniCommandReturn) -> Result<Vec<u8>, ApiError> {
    Ok(gen_codec::encode_command_result(&command_return_to_uni(
        result,
    )))
}

pub async fn sys_command(argv: &UniCommandArgv) -> Result<UniCommandReturn, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return Ok(crate::mock::MockSqliteMuduSysCall::sys_command(argv.clone()).await);
    }

    #[cfg(not(feature = "mock-sqlite"))]
    let request = serialize_command(argv)?;
    #[cfg(not(feature = "mock-sqlite"))]
    let response = command_raw(request).await?;
    #[cfg(not(feature = "mock-sqlite"))]
    return deserialize_command_result(&response);

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable("unreachable backend branch"))
}

pub async fn sys_query(argv: &UniQueryArgv) -> Result<UniQueryReturn, ApiError> {
    #[cfg(feature = "mock-sqlite")]
    {
        return Ok(crate::mock::MockSqliteMuduSysCall::sys_query(argv.clone()).await);
    }

    #[cfg(not(feature = "mock-sqlite"))]
    let request = serialize_query(argv)?;
    #[cfg(not(feature = "mock-sqlite"))]
    let response = query_raw(request).await?;
    #[cfg(not(feature = "mock-sqlite"))]
    return deserialize_query_result(&response);

    #[allow(unreachable_code)]
    Err(ApiError::backend_unavailable("unreachable backend branch"))
}

pub async fn sys_command_affected_rows(argv: &UniCommandArgv) -> Result<u64, ApiError> {
    match sys_command(argv).await? {
        UniCommandReturn::Ok(UniCommandResult { affected_rows }) => Ok(affected_rows),
        UniCommandReturn::Err(error) => Err(ApiError::Decode(error.err_msg)),
    }
}

pub async fn sys_query_ok(argv: &UniQueryArgv) -> Result<UniQueryResult, ApiError> {
    match sys_query(argv).await? {
        UniQueryReturn::Ok(result) => Ok(result),
        UniQueryReturn::Err(error) => Err(ApiError::Decode(error.err_msg)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal::uni_sql_param::UniSqlParam;
    use crate::universal::uni_sql_stmt::UniSqlStmt;
    use crate::{UniError, UniOid};

    fn command_argv() -> UniCommandArgv {
        UniCommandArgv {
            oid: UniOid { h: 0, l: 7 },
            command: UniSqlStmt {
                sql_string: "update t set a = 1".to_string(),
            },
            param_list: UniSqlParam {
                params: Vec::new(),
                param_names: None,
            },
        }
    }

    #[test]
    fn command_request_frame_roundtrips_header_and_argv() {
        let frame = serialize_command(&command_argv()).unwrap();
        assert_eq!(&frame[0..4], b"MSSP");
        assert_eq!(
            u32::from_be_bytes(frame[12..16].try_into().unwrap()),
            payload::KIND_COMMAND
        );

        let decoded = decode_command_request(&frame).unwrap();
        assert_eq!(decoded.oid.l, 7);
        assert_eq!(decoded.command.sql_string, "update t set a = 1");

        let query_frame = serialize_query(&UniQueryArgv {
            oid: UniOid { h: 0, l: 7 },
            query: UniSqlStmt {
                sql_string: "select 1".to_string(),
            },
            param_list: UniSqlParam {
                params: Vec::new(),
                param_names: None,
            },
        })
        .unwrap();
        assert!(decode_command_request(&query_frame).is_err());
    }

    #[test]
    fn command_result_frame_roundtrips_ok_and_err() {
        let ok = UniCommandReturn::from_ok(UniCommandResult { affected_rows: 3 });
        let frame = encode_command_response(&ok).unwrap();
        match deserialize_command_result(&frame).unwrap() {
            UniCommandReturn::Ok(result) => assert_eq!(result.affected_rows, 3),
            UniCommandReturn::Err(error) => panic!("unexpected error: {}", error.err_msg),
        }

        let err = UniCommandReturn::from_err(UniError {
            err_code: 2,
            err_msg: "boom".to_string(),
            ..Default::default()
        });
        let frame = encode_command_response(&err).unwrap();
        match deserialize_command_result(&frame).unwrap() {
            UniCommandReturn::Ok(_) => panic!("expected error variant"),
            UniCommandReturn::Err(error) => assert_eq!(error.err_msg, "boom"),
        }

        // A bare MessagePack body without the MSSP header is rejected.
        let bare = crate::universal::mp_wire::encode_value(
            &crate::universal::mp_wire::ToValue::to_value(&ok),
        );
        assert!(deserialize_command_result(&bare).is_err());
    }

    #[test]
    fn query_request_frame_uses_query_kind() {
        let argv = UniQueryArgv {
            oid: UniOid { h: 1, l: 2 },
            query: UniSqlStmt {
                sql_string: "select 1".to_string(),
            },
            param_list: UniSqlParam {
                params: Vec::new(),
                param_names: None,
            },
        };
        let frame = serialize_query(&argv).unwrap();
        assert_eq!(
            u32::from_be_bytes(frame[12..16].try_into().unwrap()),
            payload::KIND_QUERY
        );
        let decoded = decode_query_request(&frame).unwrap();
        assert_eq!(decoded.oid.h, 1);
        assert_eq!(decoded.query.sql_string, "select 1");

        let ok = UniQueryReturn::from_ok(crate::UniQueryResult {
            tuple_desc: Default::default(),
            result_set: Default::default(),
        });
        let frame = encode_query_response(&ok).unwrap();
        assert!(matches!(
            deserialize_query_result(&frame).unwrap(),
            UniQueryReturn::Ok(_)
        ));
    }
}
