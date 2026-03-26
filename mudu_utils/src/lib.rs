pub mod debug;
mod init_log;
pub mod log;
pub mod md5;
pub mod notifier;
pub mod sync;
pub mod task;
pub mod task_context;
pub mod task_id;
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
