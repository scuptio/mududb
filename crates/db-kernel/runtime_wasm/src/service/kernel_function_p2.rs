use crate::interface::kernel_sync;
use ::conn::worker_local::WorkerLocalRef;

/// Handles the guest SQL `query` syscall.
#[cfg(test)]
pub fn host_query(query_in: Vec<u8>) -> Vec<u8> {
    kernel_sync::query_internal(&query_in)
}

/// Handles the guest SQL `command` syscall.
pub fn host_command(command_in: Vec<u8>) -> Vec<u8> {
    kernel_sync::command_internal(&command_in)
}

/// Handles the guest SQL `batch` syscall.
pub fn host_batch(batch_in: Vec<u8>) -> Vec<u8> {
    kernel_sync::batch_internal(&batch_in)
}

/// Handles the guest `open` syscall, minting a worker session.
pub fn host_open(open_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::open_internal_with_worker_local(&open_in, worker_local)
}

/// Handles the guest `close` syscall.
pub fn host_close(close_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::close_internal_with_worker_local(&close_in, worker_local)
}

/// Handles the guest `fetch` syscall for the next batch of result rows.
pub fn host_fetch(result_cursor: Vec<u8>) -> Vec<u8> {
    kernel_sync::fetch_internal(&result_cursor)
}

/// Handles the guest KV `get` syscall.
pub fn host_get(get_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::get_internal_with_worker_local(&get_in, worker_local)
}

/// Handles the guest KV `put` syscall.
pub fn host_put(put_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::put_internal_with_worker_local(&put_in, worker_local)
}

/// Handles the guest KV `delete` syscall.
pub fn host_delete(delete_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::delete_internal_with_worker_local(&delete_in, worker_local)
}

/// Handles the guest KV `range` syscall.
pub fn host_range(range_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::range_internal_with_worker_local(&range_in, worker_local)
}

/// Handles the guest relation `get` syscall.
pub fn host_relation_get(
    relation_get_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_sync::relation_get_internal_with_worker_local(&relation_get_in, worker_local)
}

/// Handles the guest relation `update` syscall.
pub fn host_relation_update(
    relation_update_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_sync::relation_update_internal_with_worker_local(&relation_update_in, worker_local)
}

/// Handles the guest relation `insert` syscall.
pub fn host_relation_insert(
    relation_insert_in: Vec<u8>,
    worker_local: Option<WorkerLocalRef>,
) -> Vec<u8> {
    kernel_sync::relation_insert_internal_with_worker_local(&relation_insert_in, worker_local)
}

/// Handles the guest fs `open` syscall.
pub fn host_fs_open(fs_open_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_open_internal_with_worker_local(&fs_open_in, worker_local)
}

/// Handles the guest fs `close` syscall.
pub fn host_fs_close(fs_close_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_close_internal_with_worker_local(&fs_close_in, worker_local)
}

/// Handles the guest fs `read` syscall.
pub fn host_fs_read(fs_read_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_read_internal_with_worker_local(&fs_read_in, worker_local)
}

/// Handles the guest fs `write` syscall.
pub fn host_fs_write(fs_write_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_write_internal_with_worker_local(&fs_write_in, worker_local)
}

/// Handles the guest fs `pread` syscall.
pub fn host_fs_pread(fs_pread_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_pread_internal_with_worker_local(&fs_pread_in, worker_local)
}

/// Handles the guest fs `pwrite` syscall.
pub fn host_fs_pwrite(fs_pwrite_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_pwrite_internal_with_worker_local(&fs_pwrite_in, worker_local)
}

/// Handles the guest fs `lseek` syscall.
pub fn host_fs_lseek(fs_lseek_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_lseek_internal_with_worker_local(&fs_lseek_in, worker_local)
}

/// Handles the guest fs `fstat` syscall.
pub fn host_fs_fstat(fs_fstat_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_fstat_internal_with_worker_local(&fs_fstat_in, worker_local)
}

/// Handles the guest fs `stat` syscall.
pub fn host_fs_stat(fs_stat_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_stat_internal_with_worker_local(&fs_stat_in, worker_local)
}

/// Handles the guest fs `fsync` syscall.
pub fn host_fs_fsync(fs_fsync_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_fsync_internal_with_worker_local(&fs_fsync_in, worker_local)
}

/// Handles the guest fs `readdir` syscall.
pub fn host_fs_readdir(fs_readdir_in: Vec<u8>, worker_local: Option<WorkerLocalRef>) -> Vec<u8> {
    kernel_sync::fs_readdir_internal_with_worker_local(&fs_readdir_in, worker_local)
}
