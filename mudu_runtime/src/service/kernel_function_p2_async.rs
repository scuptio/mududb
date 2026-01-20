use crate::interface::kernel;

pub async fn async_host_query(query_in: Vec<u8>) -> Vec<u8> {
    kernel::async_query_internal(query_in).await
}

pub async fn async_host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel::async_command_internal(command_in).await
}

pub async fn async_host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel::async_fetch_internal(result_cursor).await
}
