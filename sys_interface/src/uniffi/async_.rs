use super::{SysInterfaceUniffiError, binding_error};
use crate::api_impl::async_ as api_impl_async;

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_query(query_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_query_bytes(&query_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_fetch(cursor: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_fetch_bytes(&cursor)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_command(command_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_command_bytes(&command_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_batch(batch_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_batch_bytes(&batch_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_open(open_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_open_bytes(&open_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_close(close_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_close_bytes(&close_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_get(get_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_get_bytes(&get_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_put(put_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_put_bytes(&put_in)
        .await
        .map_err(binding_error)
}

#[::uniffi::export(async_runtime = "tokio")]
pub async fn async_range(range_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_async::mudu_range_bytes(&range_in)
        .await
        .map_err(binding_error)
}
