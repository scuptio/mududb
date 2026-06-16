#[cfg(target_os = "linux")]
mod imp {
    pub use crate::imp::native::linux::io_uring::iouring::IoUring;
}

#[cfg(not(target_os = "linux"))]
mod imp {
    pub struct IoUring;

    impl IoUring {
        pub fn new(_entries: u32) -> Result<Self, i32> {
            Err(-1)
        }
    }
}

pub use imp::IoUring;
