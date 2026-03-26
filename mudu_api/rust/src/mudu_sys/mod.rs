use crate::error::ApiError;
use crate::types::{UniCommandReturn, UniQueryReturn};
use crate::{UniCommandArgv, UniCommandResult, UniQueryArgv, UniQueryResult};

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

pub fn serialize_command(argv: &UniCommandArgv) -> Result<Vec<u8>, ApiError> {
    Ok(rmp_serde::to_vec(argv)?)
}

pub fn serialize_query(argv: &UniQueryArgv) -> Result<Vec<u8>, ApiError> {
    Ok(rmp_serde::to_vec(argv)?)
}

pub fn deserialize_command_result(bytes: &[u8]) -> Result<UniCommandReturn, ApiError> {
    Ok(rmp_serde::from_slice(bytes)?)
}

pub fn deserialize_query_result(bytes: &[u8]) -> Result<UniQueryReturn, ApiError> {
    Ok(rmp_serde::from_slice(bytes)?)
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
