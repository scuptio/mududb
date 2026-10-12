#[cfg(target_os = "linux")]
use mudu::common::result::RS;
#[cfg(target_os = "linux")]
use mudu::error::ErrorCode;
#[cfg(target_os = "linux")]
use mudu::mudu_error;

/// OS-specific subsystem - native implementation.
pub struct SysOs;

impl Default for SysOs {
    fn default() -> Self {
        Self::new()
    }
}

impl SysOs {
    /// Creates the OS subsystem handle.
    pub fn new() -> Self {
        Self
    }

    /// Creates a new io_uring instance with a submission queue of `entries`
    /// entries (Linux only).
    #[cfg(target_os = "linux")]
    pub fn io_uring(
        &self,
        entries: u32,
    ) -> RS<crate::imp::native::linux::io_uring::iouring::IoUring> {
        crate::imp::native::linux::io_uring::iouring::IoUring::new(entries)
            .map_err(|e| mudu_error!(ErrorCode::Io, format!("create io_uring error: {e}")))
    }
}
