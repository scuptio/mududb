
pub mod contract;
#[cfg(target_os = "linux")]
pub mod linux;
pub mod mode;
pub mod std_file;
pub mod tokio;



pub fn async_io_only() -> bool {
    true
}

