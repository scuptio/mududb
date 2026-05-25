pub mod debug;
mod init_log;
pub mod log;
pub mod md5;
pub mod notifier;
pub mod sync;
#[deprecated(note = "use mudu_utils::task_async or mudu_utils::task_sync instead")]
pub mod task;
pub mod task_async;
pub mod task_context;
pub mod task_id;
mod task_macros;
pub mod task_sync;
pub mod task_trace;
mod test_debug_server;
pub mod thread_trace;
pub mod ts_gram;

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_works() {
        let result = add(2, 2);
        assert_eq!(result, 4);
    }
}
