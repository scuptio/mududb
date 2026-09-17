use crate::interface::kernel_async;
use mudu_kernel::server::worker_local::WorkerLocalRef;

pub async fn async_host_query(query_in: Vec<u8>) -> Vec<u8> {
    kernel_async::async_query_internal(query_in).await
}

pub async fn async_host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel_async::async_command_internal(command_in).await
}

pub async fn async_host_batch(batch_in: Vec<u8>) -> Vec<u8> {
    kernel_async::async_batch_internal(batch_in).await
}

pub async fn async_host_open(open_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_async::async_open_internal_with_worker_local(open_in, worker_local).await
}

pub async fn async_host_close(close_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_async::async_close_internal_with_worker_local(close_in, worker_local).await
}

pub async fn async_host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel_async::async_fetch_internal(result_cursor).await
}

pub async fn async_host_get(get_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_async::async_get_internal_with_worker_local(get_in, worker_local).await
}

pub async fn async_host_put(put_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_async::async_put_internal_with_worker_local(put_in, worker_local).await
}

pub async fn async_host_delete(
    delete_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_delete_internal_with_worker_local(delete_in, worker_local).await
}

pub async fn async_host_range(range_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_async::async_range_internal_with_worker_local(range_in, worker_local).await
}

pub async fn async_host_relation_get(
    relation_get_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_relation_get_internal_with_worker_local(relation_get_in, worker_local).await
}

pub async fn async_host_relation_update(
    relation_update_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_relation_update_internal_with_worker_local(relation_update_in, worker_local)
        .await
}

pub async fn async_host_relation_insert(
    relation_insert_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_relation_insert_internal_with_worker_local(relation_insert_in, worker_local)
        .await
}

pub async fn async_host_fs_open(
    fs_open_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_open_internal_with_worker_local(fs_open_in, worker_local).await
}

pub async fn async_host_fs_close(
    fs_close_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_close_internal_with_worker_local(fs_close_in, worker_local).await
}

pub async fn async_host_fs_read(
    fs_read_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_read_internal_with_worker_local(fs_read_in, worker_local).await
}

pub async fn async_host_fs_write(
    fs_write_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_write_internal_with_worker_local(fs_write_in, worker_local).await
}

pub async fn async_host_fs_pread(
    fs_pread_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_pread_internal_with_worker_local(fs_pread_in, worker_local).await
}

pub async fn async_host_fs_pwrite(
    fs_pwrite_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_pwrite_internal_with_worker_local(fs_pwrite_in, worker_local).await
}

pub async fn async_host_fs_lseek(
    fs_lseek_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_lseek_internal_with_worker_local(fs_lseek_in, worker_local).await
}

pub async fn async_host_fs_fstat(
    fs_fstat_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_fstat_internal_with_worker_local(fs_fstat_in, worker_local).await
}

pub async fn async_host_fs_stat(
    fs_stat_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_stat_internal_with_worker_local(fs_stat_in, worker_local).await
}

pub async fn async_host_fs_fsync(
    fs_fsync_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_fsync_internal_with_worker_local(fs_fsync_in, worker_local).await
}

pub async fn async_host_fs_readdir(
    fs_readdir_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_async::async_fs_readdir_internal_with_worker_local(fs_readdir_in, worker_local).await
}
