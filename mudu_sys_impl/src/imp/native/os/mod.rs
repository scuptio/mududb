#![allow(missing_docs)]
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// OS-specific subsystem - native implementation.
pub struct SysOs;

impl Default for SysOs {
    fn default() -> Self {
        Self::new()
    }
}

impl SysOs {
    pub fn new() -> Self {
        Self
    }

    pub fn io_uring(
        &self,
        entries: u32,
    ) -> RS<crate::imp::native::linux::io_uring::iouring::IoUring> {
        crate::imp::native::linux::io_uring::iouring::IoUring::new(entries)
            .map_err(|e| mudu_error!(ErrorCode::Io, format!("create io_uring error: {e}")))
    }
}
