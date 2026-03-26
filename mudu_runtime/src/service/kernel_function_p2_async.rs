use crate::interface::kernel;
use mudu_kernel::server_ur::worker_local::WorkerLocalRef;

pub async fn async_host_query(query_in: Vec<u8>) -> Vec<u8> {
    kernel::async_query_internal(query_in).await
}

pub async fn async_host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel::async_command_internal(command_in).await
}

pub async fn async_host_open(open_in: Vec<u8>, worker_local: Option<&WorkerLocalRef>) -> Vec<u8> {
    kernel::async_open_internal_with_worker_local(open_in, worker_local).await
}

pub async fn async_host_close(close_in: Vec<u8>, worker_local: Option<&WorkerLocalRef>) -> Vec<u8> {
    kernel::async_close_internal_with_worker_local(close_in, worker_local).await
}

pub async fn async_host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel::async_fetch_internal(result_cursor).await
}

pub async fn async_host_get(get_in: Vec<u8>, worker_local: Option<&WorkerLocalRef>) -> Vec<u8> {
    kernel::async_get_internal_with_worker_local(get_in, worker_local).await
}

pub async fn async_host_put(put_in: Vec<u8>, worker_local: Option<&WorkerLocalRef>) -> Vec<u8> {
    kernel::async_put_internal_with_worker_local(put_in, worker_local).await
}

pub async fn async_host_range(range_in: Vec<u8>, worker_local: Option<&WorkerLocalRef>) -> Vec<u8> {
    kernel::async_range_internal_with_worker_local(range_in, worker_local).await
}
