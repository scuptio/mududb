pub mod debug;
mod init_log;
pub mod log;
pub mod notifier;
pub mod task;
mod test_debug_server;
pub mod md5;


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
