use crate::interface::kernel;
use mudu_kernel::server::worker_local::WorkerLocalRef;

pub fn host_query(query_in: Vec<u8>) -> Vec<u8> {
    kernel::query_internal(&query_in)
}

pub fn host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel::command_internal(&command_in)
}

pub fn host_batch(batch_in: Vec<u8>) -> Vec<u8> {
    kernel::batch_internal(&batch_in)
}

pub fn host_open(open_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::open_internal_with_worker_local(&open_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}

pub fn host_close(close_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::close_internal_with_worker_local(&close_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}

pub fn host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel::fetch_internal(&result_cursor)
}

pub fn host_get(get_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::get_internal_with_worker_local(&get_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}

pub fn host_put(put_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::put_internal_with_worker_local(&put_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}

pub fn host_delete(delete_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::delete_internal_with_worker_local(&delete_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}

pub fn host_range(range_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel::range_internal_with_worker_local(&range_in, worker_local)
        .unwrap_or_else(mudu_binding::codec::handle_sys_session::serialize_error_result)
}
