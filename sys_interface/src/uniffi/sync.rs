use super::{SysInterfaceUniffiError, binding_error};
use crate::api_impl::sync as api_impl_sync;

#[::uniffi::export]
pub fn sync_query(query_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_query_bytes(&query_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_fetch(cursor: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_fetch_bytes(&cursor).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_command(command_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_command_bytes(&command_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_batch(batch_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_batch_bytes(&batch_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_open(open_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_open_bytes(&open_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_close(close_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_close_bytes(&close_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_get(get_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_get_bytes(&get_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_put(put_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_put_bytes(&put_in).map_err(binding_error)
}

#[::uniffi::export]
pub fn sync_range(range_in: Vec<u8>) -> Result<Vec<u8>, SysInterfaceUniffiError> {
    api_impl_sync::mudu_range_bytes(&range_in).map_err(binding_error)
}
